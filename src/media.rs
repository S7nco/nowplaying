use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Storage::Streams::{
    Buffer, DataReader, IRandomAccessStreamReference, InputStreamOptions,
};

// Created once, then reused. Making a new one on every request leaks memory.
static MANAGER: OnceLock<Manager> = OnceLock::new();

fn manager() -> windows::core::Result<&'static Manager> {
    if let Some(m) = MANAGER.get() {
        return Ok(m);
    }
    let m = Manager::RequestAsync()?.join()?;
    Ok(MANAGER.get_or_init(|| m))
}

// Option<T> means "a value, or nothing". Nothing is sent as null
#[derive(Serialize, Default, Clone)]
pub struct TrackInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    album: Option<String>,
    artwork: Option<String>,
    position: Option<f64>,
    duration: Option<f64>,
    percent: Option<f64>,
    pub playing: Option<bool>,
}

// Remembered between requests
static LAST_TRACK: Mutex<Option<(String, String)>> = Mutex::new(None);
static LAST_ARTWORK: Mutex<Option<String>> = Mutex::new(None);

// Which player is active: a position in the app_id_match list (audio.rs uses it to pick the process)
pub static ACTIVE_APP: AtomicUsize = AtomicUsize::new(0);

// "a. B" -> ["a", "b"]
pub fn split_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim().to_lowercase())
        .filter(|x| !x.is_empty())
        .collect()
}

/// Always returns something. If anything fails, you get a TrackInfo full of nulls
static LAST_ERROR: Mutex<String> = Mutex::new(String::new());

pub fn read_track() -> TrackInfo {
    match read_track_inner() {
        Ok(track) => track.unwrap_or_default(),
        Err(e) => {
            let text = format!("Reading media sessions failed: {e}");
            let mut last = LAST_ERROR.lock().unwrap();
            if *last != text {
                crate::log(&text);
                *last = text;
            }
            TrackInfo::default()
        }
    }
}

// The track we last logged an artwork problem for, so each track logs it only once
static LAST_ART_LOG: Mutex<Option<(String, String)>> = Mutex::new(None);

fn log_artwork_once(key: &(String, String), msg: String) {
    let mut last = LAST_ART_LOG.lock().unwrap();
    if last.as_ref() != Some(key) {
        *last = Some(key.clone());
        crate::log(&msg);
    }
}

// Result<..> means "this can fail". The ? after a call means "if it failed, stop and return the error".
fn read_track_inner() -> windows::core::Result<Option<TrackInfo>> {
    let manager = manager()?;

    // Find the Apple Music session.
    // Find a session from any of the listed players; prefer one that is playing
    let wanted = split_list(&crate::config::get().app_id_match);
    let mut found: Option<(_, usize, bool)> = None;
    for s in manager.GetSessions()? {
        // A session that can't report its app (e.g. one that's closing) is skipped, not the whole read
        let Ok(id) = s.SourceAppUserModelId() else {
            continue;
        };
        let id = id.to_string().to_lowercase();
        let Some(index) = wanted.iter().position(|w| id.contains(w.as_str())) else {
            continue;
        };
        let is_playing = s.GetPlaybackInfo()?.PlaybackStatus()? == PlaybackStatus::Playing;
        let better = match &found {
            None => true,
            Some((_, _, was_playing)) => is_playing && !*was_playing,
        };
        if better {
            found = Some((s, index, is_playing));
        }
    }
    let Some((session, index, _)) = found else {
        return Ok(None); // None of the players are running
    };
    ACTIVE_APP.store(index, Ordering::Relaxed);

    let props = session.TryGetMediaPropertiesAsync()?.join()?;
    let title = props.Title()?.to_string();
    let artist_raw = props.Artist()?.to_string();
    if title.is_empty() {
        return Ok(None);
    }

    // Times come in 100-nanoseconds units, so divide to get seconds
    let timeline = session.GetTimelineProperties()?;
    let position = timeline.Position()?.Duration as f64 / 10_000_000.0;
    let duration = timeline.EndTime()?.Duration as f64 / 10_000_000.0;
    let percent = if duration > 0.0 {
        position / duration * 100.0
    } else {
        0.0
    };
    let playing = session.GetPlaybackInfo()?.PlaybackStatus()? == PlaybackStatus::Playing;

    // "Aritst - Album" -> artist and album
    let mut parts = artist_raw.split(" \u{2014} "); // em dash
    let artist = parts.next().unwrap_or("").to_string();
    let album = parts.next().map(|s| s.to_string()).or_else(|| {
        props
            .AlbumTitle()
            .ok()
            .map(|a| a.to_string())
            .filter(|a| !a.is_empty())
    });

    // Only fetch artwork when the song changes
    let key = (title.clone(), artist_raw.clone());
    let mut last_track = LAST_TRACK.lock().unwrap();
    let mut last_art = LAST_ARTWORK.lock().unwrap();
    if last_track.as_ref() != Some(&key) {
        *last_track = None;
        *last_art = None;
        match props.Thumbnail() {
            Ok(thumb) => match read_artwork(&thumb) {
                Ok(Some(bytes)) if looks_like_image(&bytes) => {
                    if std::fs::write("static/cover.jpg", &bytes).is_ok() {
                        let secs = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs_f64();
                        *last_art = Some(format!("/cover.jpg?timestamp={secs}"));
                        *last_track = Some(key);
                    }
                }
                Ok(Some(bytes)) => log_artwork_once(
                    &key,
                    format!(
                        "Artwork: {} bytes isn't an image yet, retrying",
                        bytes.len()
                    ),
                ),
                Ok(None) => log_artwork_once(&key, "Artwork: empty thumbnail".to_string()),
                Err(e) => log_artwork_once(&key, format!("Artwork: read failed: {e}")),
            },
            Err(e) => log_artwork_once(&key, format!("Artwork: no thumbnail: {e}")),
        }
    }

    Ok(Some(TrackInfo {
        title: Some(title),
        artist: Some(artist).filter(|a| !a.is_empty()), // no artist (e.g. some podcasts): null
        album,
        artwork: last_art.clone(),
        position: Some(position),
        duration: Some(duration),
        percent: Some(percent),
        playing: Some(playing),
    }))
}

// JPEG, PNG, BMP, or WebP files start with these bytes. Anything else isn't a cover yet.
fn looks_like_image(b: &[u8]) -> bool {
    b.len() > 200
        && (b.starts_with(&[0xFF, 0xD8])
            || b.starts_with(&[0x89, 0x50, 0x4E, 0x47])
            || b.starts_with(b"BM")
            || b.starts_with(b"RIFF"))
}

fn read_artwork(thumb: &IRandomAccessStreamReference) -> windows::core::Result<Option<Vec<u8>>> {
    let stream = thumb.OpenReadAsync()?.join()?;
    let size = stream.Size()? as u32;
    if size == 0 {
        return Ok(None);
    }
    let buffer = Buffer::Create(size)?;
    let reader = stream
        .ReadAsync(&buffer, size, InputStreamOptions::ReadAhead)?
        .join()?;
    let reader = DataReader::FromBuffer(&reader)?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes)?;
    Ok(Some(bytes))
}
