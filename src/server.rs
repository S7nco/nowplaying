use crate::{audio, config, favorites, log, media, themes};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, header};
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer};

// ============================================================
// Web server: routes and startup
// ============================================================

// Everything the handlers share
#[derive(Clone)]
struct AppState {
    bands: audio::Bands,
    // messages for every connected overlya: song updates, and spectrum levels
    track_tx: broadcast::Sender<String>,
    spectrum_tx: broadcast::Sender<String>,
}

static LAST_TRACK: OnceLock<Mutex<Option<(Instant, media::TrackInfo)>>> = OnceLock::new();

// The current song. Reads from Windows at most once a second, however many ask.
async fn current_track() -> media::TrackInfo {
    let cache = LAST_TRACK.get_or_init(|| Mutex::new(None));
    if let Some((time, info)) = cache.lock().unwrap().as_ref() {
        if time.elapsed() < Duration::from_millis(1000) {
            return info.clone();
        }
    }
    // the Windows calls wait for an answer, so run them off the main asysnc threads
    let info = tokio::task::spawn_blocking(media::read_track)
        .await
        .unwrap();
    *cache.lock().unwrap() = Some((Instant::now(), info.clone()));
    info
}

// GET /nowplaying: the current song as JSON (overlays use /ws, this is the fallback)
async fn nowplaying() -> Json<media::TrackInfo> {
    Json(current_track().await)
}

/// GET /spectrum: the latest equalizer band levels as JSON (fallback for /ws)
async fn spectrum(State(state): State<AppState>) -> Json<Value> {
    let levels = state.bands.lock().unwrap().clone();
    Json(json!({ "levels": levels }))
}

#[derive(serde::Serialize)]
struct Themes {
    examples: Vec<String>,
    styles: Vec<String>,
    #[serde(rename = "game-inspired")]
    game_inspired: Vec<String>,
    custom: Vec<String>,
}

async fn themes() -> Json<Themes> {
    axum::Json(Themes {
        examples: themes::list("examples"),
        styles: themes::list("styles"),
        game_inspired: themes::list("game-inspired"),
        custom: themes::list("custom"),
    })
}

// =============================================================================
// Favorites: GET /favorites (returns { saved, list }: the favorite, themes, and whether
//                           they came from a saved favorites.json)
//            PUT /favorites (updates the list of favorite themes)
// =============================================================================
#[derive(serde::Serialize)]
struct FavoriteState {
    saved: bool,
    list: Vec<String>,
}

async fn get_favorites() -> Json<FavoriteState> {
    Json(FavoriteState {
        saved: favorites::is_saved(),
        list: favorites::get(),
    })
}

async fn put_favorites(Json(names): Json<Vec<String>>) -> Json<Vec<String>> {
    Json(favorites::set(names))
}

async fn add_favorite(Path(name): Path<String>) -> Json<Vec<String>> {
    Json(favorites::set_one(&name, true))
}

async fn remove_favorite(Path(name): Path<String>) -> Json<Vec<String>> {
    Json(favorites::set_one(&name, false))
}

// =============================================================================
// Live updates: GET /ws (WebSocket)
// The server sends a message when the song changes, and a stream of spectrum
// levels to overlays that ask for them with /ws?spectrum=1
// =============================================================================

#[derive(Deserialize)]
struct WsParams {
    spectrum: Option<u8>,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsParams>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let want_spectrum = params.spectrum == Some(1);
    ws.on_upgrade(move |socket| client_loop(socket, state, want_spectrum))
}

// One connected overlay
async fn client_loop(mut socket: WebSocket, state: AppState, want_spectrum: bool) {
    let mut track_rx = state.track_tx.subscribe();
    // only overlays that want the spectrum listen for it (a spare channel stands in for the rest)
    let mut spectrum_rx = if want_spectrum {
        state.spectrum_tx.subscribe()
    } else {
        broadcast::channel::<String>(1).1
    };

    // show the current song straight away
    let first = json!({ "type": "nowplaying", "data": current_track().await }).to_string();
    if socket.send(Message::Text(first.into())).await.is_err() {
        return;
    }

    loop {
        // wait for the next thing to send (or for the overlay to go away)
        let outgoing: Option<String> = tokio::select! {
            msg = track_rx.recv() => match msg {
                Ok(text) => Some(text),
                Err(broadcast::error::RecvError::Lagged(_)) => None,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            msg = spectrum_rx.recv(), if want_spectrum => match msg {
                Ok(text) => Some(text),
                Err(broadcast::error::RecvError::Lagged(_)) => None,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => None,
            },
        };
        if let Some(text) = outgoing {
            if socket.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    }
}

// Background jobs that watch for changes and broadcast them
fn start_broadcasters(state: AppState) {
    // The song: look once a second, send only when something changed
    let track_tx = state.track_tx.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        let mut last_key = Value::Null;
        let mut last_seen: Option<(f64, Instant, bool)> = None; // position, when, playing
        let mut last_sent: Option<Instant> = None;
        loop {
            tick.tick().await;
            if track_tx.receiver_count() == 0 {
                continue; // nobody is watching
            }
            let info = current_track().await;
            let value = serde_json::to_value(&info).unwrap();
            let position = value["position"].as_f64();
            let playing = value["playing"].as_bool().unwrap_or(false);

            // everything except the position (it chanes every second)
            let mut key = value.clone();
            if let Some(obj) = key.as_object_mut() {
                obj.remove("position");
                obj.remove("percent");
            }

            // did the position jump, like when you skip around in a song?
            let jumped = match (position, last_seen) {
                (Some(p), Some((lp, when, was_playing))) => {
                    let expected = if was_playing {
                        lp + when.elapsed().as_secs_f64()
                    } else {
                        lp
                    };
                    (p - expected).abs() > 1.5
                }
                _ => false,
            };
            last_seen = position.map(|p| (p, Instant::now(), playing));

            // also resend every 10 seconds so the overlay stays in step
            let due = last_sent.map_or(true, |t| t.elapsed() >= Duration::from_secs(10));
            if key != last_key || jumped || due {
                let _ = track_tx.send(json!({ "type": "nowplaying", "data": value }).to_string());
                last_key = key;
                last_sent = Some(Instant::now());
            }
        }
    });

    // The spectrum: about 14 times a second, but only while an overlay is listening
    let spectrum_tx = state.spectrum_tx.clone();
    let bands = state.bands.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(70));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if spectrum_tx.receiver_count() == 0 {
                continue;
            }
            let levels: Vec<f64> = bands
                .lock()
                .unwrap()
                .iter()
                .map(|v| ((*v as f64) * 1000.0).round() / 1000.0)
                .collect();
            let _ = spectrum_tx.send(json!({ "type": "spectrum", "levels": levels }).to_string());
        }
    });
}

/// Starts the audio capture, then the HTTP server, and handles incoming requests
pub async fn run() {
    let bands: audio::Bands = Arc::new(Mutex::new(vec![0.0; 8]));
    audio::start(bands.clone());

    let (track_tx, _) = broadcast::channel::<String>(16);
    let (spectrum_tx, _) = broadcast::channel::<String>(16);
    let state = AppState {
        bands,
        track_tx,
        spectrum_tx,
    };
    start_broadcasters(state.clone());

    let app = Router::new()
        // get() is used to tie URL to a function
        .route("/nowplaying", get(nowplaying))
        .route("/spectrum", get(spectrum))
        .route("/themes.json", get(themes))
        .route("/ws", get(ws_handler))
        .route("/favorites", get(get_favorites).put(put_favorites))
        .route(
            "/favorites/{name}",
            axum::routing::put(add_favorite).delete(remove_favorite),
        )
        .route("/version", get(|| async { env!("CARGO_PKG_VERSION") }))
        .with_state(state)
        // Serve static files from the "static" directory.
        .fallback_service(ServeDir::new("static"))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache"),
        ))
        // every request passes the security checks first (see guard.rs)
        .layer(axum::middleware::from_fn(crate::guard::guard));

    // if the port can't be bound, log the error and exit
    let port = config::get().port;
    let addr = format!("{}:{port}", config::get().bind_address);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            log(&format!("Could not start server on {addr}: {e}"));
            std::process::exit(1);
        }
    };
    log(&format!(
        "Server running on http://localhost:{port} (listening on {addr})"
    ));
    if let Err(e) = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    {
        log(&format!("Server stopped: {e}"));
        std::process::exit(1);
    }
}
