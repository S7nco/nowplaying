// Checks every request before it reaches the server's routes:
// - Host: only localhost or an IP address (blocks "DNS rebinding", where a website pretends to be this PC)
// - Origin: /ws and changes only from the app's own pages (other websites can't listen in or change things)
// - Changes (anything but GET/HEAD) only from this PC, even when bind_address lets other devices connect

use axum::extract::{ConnectInfo, Request};
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::net::{IpAddr, SocketAddr};

pub async fn guard(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .map(|v| v.to_str().unwrap_or("").to_string());
    let changes = !matches!(*req.method(), Method::GET | Method::HEAD);
    let websocket = req.uri().path() == "/ws";

    if !host_allowed(&host) {
        return refuse("Unknown host");
    }
    // Browsers always send Origin for WebSockets and changes. It must be this server's own address.
    if changes || websocket {
        if let Some(origin) = origin {
            if origin != format!("http://{host}") {
                return refuse("Not allowed from other websites");
            }
        }
    }
    if changes && !peer.ip().is_loopback() {
        return refuse("Changes can only be made on the PC running Now Playing");
    }
    next.run(req).await
}

fn refuse(why: &'static str) -> Response {
    (StatusCode::FORBIDDEN, why).into_response()
}

/// "localhost:5000", "127.0.0.1:5000", "192.168.1.20:5000" and "[::1]:5000" are fine;
/// this PC's own name (e.g. "DESKTOP-ABC:5000" or "desktop-abc.local:5000"), plus any names
/// listed in allowed_hosts. Other names like "evil.com", aren't
fn host_allowed(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(""), // [::1]:5000
        None => host.rsplit_once(':').map_or(host, |(name, _)| name), // name:5000
    };
    if name.eq_ignore_ascii_case("localhost") || name.parse::<IpAddr>().is_ok() {
        return true;
    }
    // This PC's own name, for OBS on a second PC. A website can't use it as its address,
    // so allowing it doesn't weaken the DNS rebinding protection.
    if let Ok(pc) = std::env::var("COMPUTERNAME") {
        if name.eq_ignore_ascii_case(&pc) || name.eq_ignore_ascii_case(&format!("{pc}.local")) {
            return true;
        }
    }
    // Any other names the user listed in config.toml
    crate::config::get()
        .allowed_hosts
        .iter()
        .any(|allowed| name.eq_ignore_ascii_case(allowed.trim()))
}
