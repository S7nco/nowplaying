use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Storage::Streams::{
    Buffer, DataReader, IRandomAccessStreamReference, InputStreamOptions,
};
use windows::core::{HRESULT, Interface};
use windows_future::{AsyncStatus, IAsyncInfo};

// How long to wait for Windows to answer about a media app
const WINDOWS_TIMEOUT: Duration = Duration::from_secs(2);
const TIMED_OUT: HRESULT = HRESULT(0x800705B4u32 as i32); // Windows' "operation timed out" code

/// Waits for a Windows operation to finish, but not forever: a frozen player can't hang the app.
/// Use it before .join(), which then returns right away.
fn within_time<O: Interface>(operation: O) -> windows::core::Result<O> {
    let info: IAsyncInfo = operation.cast()?;
    let started = Instant::now();
    while info.Status()? == AsyncStatus::Started {
        if started.elapsed() >= WINDOWS_TIMEOUT {
            let _ = info.Cancel();
            return Err(TIMED_OUT.into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(operation)
}

// Created once, then reused. Making a new one on every request leaks memory.
static MANAGER: OnceLock<Manager> = OnceLock::new();

fn manager() -> windows::core::Result<&'static Manager> {
    if let Some(m) = MANAGER.get() {
        return Ok(m);
    }
    let m = within_time(Manager::RequestAsync()?)?.join()?;
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

// The current song's cover. Players can report a placeholder first (Apple Music does for about a second when it starts)
// So the cover is checked again for a while after each song starts.
const COVER_RECHECK: Duration = Duration::from_secs(15);

struct Cover {
    song: (String, String), // title and artist
    url: Option<String>,    // what the overlay loads, once a cover has been saved
    fingerprint: u64,       // of the saved image, to notice when the player reports a different one
    since: Instant,         // when this song was first seen
}

static COVER: Mutex<Option<Cover>> = Mutex::new(None);

/// A cover is final once it's been saved and the song has been playing for a while
fn cover_settled(cover: &Option<Cover>, song: &(String, String)) -> bool {
    matches!(cover, Some(c) if c.song == *song && c.url.is_some() && c.since.elapsed() >= COVER_RECHECK)
}

/// Save this image unless it's the one already saved
fn cover_changed(cover: &Cover, fingerprint: u64) -> bool {
    cover.url.is_none() || cover.fingerprint != fingerprint
}

fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

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

// Held while reading from Windows, so a slow player can't pile up waiting threads
static READING: Mutex<()> = Mutex::new(());
// The last good answer: used during another read, or if Windows stops answering
static LAST_RESULT: Mutex<Option<TrackInfo>> = Mutex::new(None);

pub fn read_track() -> TrackInfo {
    let Ok(_reading) = READING.try_lock() else {
        return LAST_RESULT.lock().unwrap().clone().unwrap_or_default();
    };
    match read_track_inner() {
        Ok(track) => {
            let track = track.unwrap_or_default();
            LAST_ERROR.lock().unwrap().clear(); // working again: a new failure gets logged
            *LAST_RESULT.lock().unwrap() = Some(track.clone());
            track
        }
        Err(e) => {
            let text = format!("Reading media sessions failed: {e}");
            let mut last = LAST_ERROR.lock().unwrap();
            if *last != text {
                crate::log(&text);
                *last = text;
            }
            // A slow player keeps its song on screen; any other failure shows nothing
            if e.code() == TIMED_OUT {
                LAST_RESULT.lock().unwrap().clone().unwrap_or_default()
            } else {
                TrackInfo::default()
            }
        }
    }
}

/// A media app Windows can see right now, for the settings page's "Detected right now" list.
/// No song title: with bind_address = "0.0.0.0" other devices can read this list
#[derive(Serialize)]
pub struct DetectedSession {
    pub id: String,
    pub playing: bool,
    pub known: bool, // one of the players in config.toml already matches it
}

pub fn detected_sessions() -> Vec<DetectedSession> {
    let Ok(manager) = manager() else {
        return Vec::new();
    };
    let Ok(sessions) = manager.GetSessions() else {
        return Vec::new();
    };
    let wanted = split_list(&crate::config::get().app_id_match);
    sessions
        .into_iter()
        .filter_map(|s| {
            let id = s.SourceAppUserModelId().ok()?.to_string();
            let lower = id.to_lowercase();
            let playing = s
                .GetPlaybackInfo()
                .and_then(|info| info.PlaybackStatus())
                .is_ok_and(|status| status == PlaybackStatus::Playing);
            let known = wanted.iter().any(|w| lower.contains(w.as_str()));
            Some(DetectedSession { id, playing, known })
        })
        .collect()
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

    let props = within_time(session.TryGetMediaPropertiesAsync()?)?.join()?;
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

    // "Artist - Album" -> artist and album
    // Only Apple Music combines them; in other players a dash is part of the artist's name
    let is_apple_music = session
        .SourceAppUserModelId()
        .is_ok_and(|id| id.to_string().to_lowercase().contains("applemusic"));
    let (artist, album_in_artist) = if is_apple_music {
        let mut parts = artist_raw.split(" \u{2014} "); // em dash
        (
            parts.next().unwrap_or("").to_string(),
            parts.next().map(|s| s.to_string()),
        )
    } else {
        (artist_raw.clone(), None)
    };
    let album = album_in_artist.or_else(|| {
        props
            .AlbumTitle()
            .ok()
            .map(|a| a.to_string())
            .filter(|a| !a.is_empty())
    });

    // Fetch the cover when the song changes, and keep checking it for a while in case it was a placeholder
    let key = (title.clone(), artist_raw.clone());
    let mut cover = COVER.lock().unwrap();
    if !cover_settled(&cover, &key) {
        if cover.as_ref().is_none_or(|c| c.song != key) {
            *cover = Some(Cover {
                song: key.clone(),
                url: None,
                fingerprint: 0,
                since: Instant::now(),
            });
        }
        let current = cover.as_mut().unwrap();
        match props.Thumbnail() {
            Ok(thumb) => match read_artwork(&thumb) {
                Ok(Some(bytes)) if looks_like_image(&bytes) => {
                    let print = fingerprint(&bytes);
                    // the same image as the one already saved needs nothing
                    if cover_changed(current, print) {
                        match crate::files::write_atomic("static/cover.jpg", &bytes) {
                            Ok(()) => {
                                let secs = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs_f64();
                                current.url = Some(format!("/cover.jpg?timestamp={secs}"));
                                current.fingerprint = print;
                            }
                            // e.g. a read-only folder; it's tried again on the next read, but logged once per song
                            Err(e) => log_artwork_once(
                                &key,
                                format!("Artwork: couldn't save the cover: {e}"),
                            ),
                        }
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
        artwork: cover.as_ref().and_then(|c| c.url.clone()),
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
    let stream = within_time(thumb.OpenReadAsync()?)?.join()?;
    let size = stream.Size()? as u32;
    if size == 0 {
        return Ok(None);
    }
    let buffer = Buffer::Create(size)?;
    let reader =
        within_time(stream.ReadAsync(&buffer, size, InputStreamOptions::ReadAhead)?)?.join()?;
    let reader = DataReader::FromBuffer(&reader)?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes)?;
    Ok(Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_lists_are_split_and_lowercased() {
        assert_eq!(
            split_list("AmpLibraryAgent.exe, Spotify.exe"),
            ["amplibraryagent.exe", "spotify.exe"]
        );
        assert_eq!(split_list(" a ,, B ,"), ["a", "b"]);
        assert!(split_list("").is_empty());
    }

    #[test]
    fn only_real_images_count_as_covers() {
        let mut jpeg = vec![0xFF, 0xD8];
        jpeg.resize(300, 0);
        let mut png = vec![0x89, 0x50, 0x4E, 0x47];
        png.resize(300, 0);
        assert!(looks_like_image(&jpeg));
        assert!(looks_like_image(&png));
        assert!(!looks_like_image(&[0xFF, 0xD8, 0, 0]));
        assert!(!looks_like_image(&vec![0u8; 300]));
    }
}

#[cfg(test)]
mod cover_tests {
    use super::*;

    fn song(t: &str) -> (String, String) {
        (t.to_string(), "Artist".to_string())
    }

    fn cover(t: &str, url: Option<&str>, print: u64, age_secs: u64) -> Option<Cover> {
        Some(Cover {
            song: song(t),
            url: url.map(String::from),
            fingerprint: print,
            since: Instant::now() - Duration::from_secs(age_secs),
        })
    }

    // 0.3.1: Apple Music's placeholder at startup stayed for the whole first song
    #[test]
    fn a_new_cover_replaces_the_placeholder_while_the_song_is_new() {
        let c = cover(
            "First",
            Some("/cover.jpg?timestamp=1"),
            fingerprint(b"placeholder"),
            1,
        );
        assert!(!cover_settled(&c, &song("First")));
        assert!(cover_changed(
            c.as_ref().unwrap(),
            fingerprint(b"real cover")
        ));
        assert!(!cover_changed(
            c.as_ref().unwrap(),
            fingerprint(b"placeholder")
        ));
    }

    #[test]
    fn the_cover_settles_after_a_while() {
        let c = cover("First", Some("/cover.jpg?timestamp=1"), 1, 20);
        assert!(cover_settled(&c, &song("First")));
        assert!(!cover_settled(&c, &song("Second")));
    }

    #[test]
    fn a_song_without_a_cover_keeps_trying() {
        assert!(!cover_settled(&cover("First", None, 0, 60), &song("First")));
        assert!(!cover_settled(&None, &song("First")));
    }
}
