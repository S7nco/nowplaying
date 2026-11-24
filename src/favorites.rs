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

/// The favorite theme names, in alphabetical order
pub fn get() -> Vec<String> {
    let mut names = list().lock().unwrap().clone();
    names.sort_by_key(|n| n.to_lowercase());
    names
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

/// Cleans the names (letters, numbers, _ and - only, no repeats), saves them to favorites.json
/// and makes them the current list. The caller holds the lock, so nothing can change in between.
fn store(current: &mut Vec<String>, names: Vec<String>) -> Vec<String> {
    let mut clean: Vec<String> = Vec::new();
    for name in names {
        if crate::themes::valid_name(&name) && !clean.contains(&name) {
            clean.push(name);
        }
    }
    if let Ok(text) = serde_json::to_string_pretty(&clean) {
        if let Err(e) = crate::files::write_atomic(FILE, text.as_bytes()) {
            crate::log(&format!("Couldn't save favorites.json: {e}"));
        }
    }
    *current = clean.clone();
    SAVED.store(true, Ordering::Relaxed);
    VERSION.fetch_add(1, Ordering::Relaxed);
    clean
}

/// Replaces the whole list and saves it
pub fn set(names: Vec<String>) -> Vec<String> {
    let mut current = list().lock().unwrap();
    store(&mut current, names)
}

/// Adds or removes a single theme, so two gallery tabs can't overwrite each other's changes
/// Reading the list and saving the change happen under one lock.
pub fn set_one(name: &str, on: bool) -> Vec<String> {
    let mut current = list().lock().unwrap();
    let mut names = current.clone();
    names.retain(|n| n != name);
    if on {
        names.push(name.to_string());
    }
    store(&mut current, names)
}
