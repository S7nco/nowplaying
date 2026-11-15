// Favorite themes: picked with the star in the gallery, shown in the tray menu.
// Saved in favorites.json next to the .exe. Until you star something, the
// list from config.toml (tray_themes) is used as the default.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

const FILE: &str = "favorites.json";

static LIST: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

// Goes up every time the list changes, so the tray knows when to refresh
static VERSION: AtomicU64 = AtomicU64::new(0);

// True once a favorites.json exists (read at startup, or written by text)
static SAVED: AtomicBool = AtomicBool::new(false);

fn list() -> &'static Mutex<Vec<String>> {
    LIST.get_or_init(|| {
        let saved = std::fs::read_to_string(FILE)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok());
        SAVED.store(saved.is_some(), Ordering::Relaxed);
        Mutex::new(saved.unwrap_or_else(|| crate::config::get().tray_themes.clone()))
    })
}

/// The favorite theme names, in the order they were added
pub fn get() -> Vec<String> {
    list().lock().unwrap().clone()
}

/// Changes whenever the list changes
pub fn version() -> u64 {
    VERSION.load(Ordering::Relaxed)
}

/// Whether the list came from a saved favorites.json (false = just the tray_menu defaults)
pub fn is_saved() -> bool {
    list(); // make sure the file has been read
    SAVED.load(Ordering::Relaxed)
}

/// Replaces the list and saves it. Names are cleaned: letters, numbers, _ and - only, no repeats
pub fn set(names: Vec<String>) -> Vec<String> {
    let mut clean: Vec<String> = Vec::new();
    for name in names {
        let ok = !name.is_empty()
            && name.len() <= 64
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if ok && !clean.contains(&name) {
            clean.push(name);
        }
    }
    if let Ok(text) = serde_json::to_string_pretty(&clean) {
        let _ = std::fs::write(FILE, text);
    }
    *list().lock().unwrap() = clean.clone();
    SAVED.store(true, Ordering::Relaxed);
    VERSION.fetch_add(1, Ordering::Relaxed);
    clean
}

/// Adds or removes a single theme, so two gallery tabs can't overwrite each other's changes
pub fn set_one(name: &str, on: bool) -> Vec<String> {
    let mut names = get();
    names.retain(|n| n != name);
    if on {
        names.push(name.to_string());
    }
    set(names)
}
