// Start with Windows: a value in the current user's "Run" list in the registry.
// It lives there rather than in config.toml, so Windows and the app can't disagree about it.

use axum::{Json, http::StatusCode, response::IntoResponse, response::Response};
use serde::{Deserialize, Serialize};
use serde_json::json;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW,
    RegSetKeyValueW,
};
use windows::core::{HSTRING, PCWSTR, w};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
// Where Windows remembers startup apps turned off in Settings > Apps > Startup or Task Manager
const APPROVED_KEY: PCWSTR =
    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
const VALUE: PCWSTR = w!("NowPlaying");

#[derive(Serialize)]
pub struct Status {
    pub enabled: bool, // this copy of the app starts with Windows
    other_copy: bool,  // the entry points at a different folder (the app was moved?)
    blocked: bool,     // on, but turned off in Windows' own startup settings
    can_enable: bool, // false for a development build, which can't find static\ when started by Windows
}

/// The quoted path of this exe, as Windows should run it
fn command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\"", exe.display()))
}

fn read_entry() -> Option<String> {
    let mut size = 0u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
        .ok()
        .ok()?;
        let mut text = vec![0u16; size as usize / 2 + 1];
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            Some(text.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
        let len = text.iter().position(|&c| c == 0).unwrap_or(text.len());
        Some(String::from_utf16_lossy(&text[..len]))
    }
}

/// Windows stores an odd first byte when the user turned this startup app off
fn blocked_by_windows() -> bool {
    let mut data = [0u8; 12];
    let mut size = data.len() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            APPROVED_KEY,
            VALUE,
            RRF_RT_REG_BINARY,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    result.is_ok() && data[0] & 1 == 1
}

/// Windows starts apps from another folder; main() only finds static\ if it's next to the exe
fn can_enable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("static").is_dir()))
        .unwrap_or(false)
}

pub fn status() -> Status {
    let entry = read_entry();
    let enabled = match (&entry, command()) {
        (Some(entry), Some(ours)) => entry.eq_ignore_ascii_case(&ours),
        _ => false,
    };
    Status {
        enabled,
        other_copy: entry.is_some() && !enabled,
        blocked: enabled && blocked_by_windows(),
        can_enable: can_enable(),
    }
}

pub fn set(enabled: bool) -> Result<(), String> {
    if enabled {
        if !can_enable() {
            return Err("This is a development build. Start with Windows only works from the packaged app, with the static folder next to nowplaying.exe.".into());
        }
        let command = command().ok_or("Couldn't find this app's location")?;
        let text = HSTRING::from(command);
        let bytes = (text.len() as u32 + 1) * 2; // including the ending 0
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE,
                REG_SZ.0,
                Some(text.as_ptr().cast()),
                bytes,
            )
        }
        .ok()
        .map_err(|e| format!("Couldn't turn it on: {e}"))?;
    } else if status().enabled {
        // only this copy's entry: never remove one that belongs to another copy
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE) }
            .ok()
            .map_err(|e| format!("Couldn't turn it off: {e}"))?;
    }
    crate::log(if enabled {
        "start with Windows: on"
    } else {
        "start with Windows: off"
    });
    Ok(())
}

#[derive(Deserialize)]
pub struct Change {
    enabled: bool,
}

/// GET /startup
pub async fn get_startup() -> Json<Status> {
    Json(status())
}

/// PUT /startup {"enabled": true}. Only from this PC (see guard.rs)
pub async fn put_startup(Json(change): Json<Change>) -> Response {
    match set(change.enabled) {
        Ok(()) => Json(status()).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response(),
    }
}
