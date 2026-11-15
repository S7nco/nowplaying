// ============================================================
// Setup: hide the console, declare modules, import
// ============================================================

// Hide the console window in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Other source files
// audio capture + FFT for the EQ
mod audio;
// reads config.toml
mod config;
// the list of favorite themes
mod favorites;
// handles logging to nowplaying.log
mod logging;
// song info from the media players (Apple Music, Spotify)
mod media;
// handles the HTTP server and routes
mod server;
// theme names and listing the theme folders
mod themes;
// handles the system tray icon and menu
mod tray;

// Lets the other files keep calling crate::log(...)
pub use logging::log;

/// Returns true if another copy of the app is already running.
/// Uses a named mutex, which Windows releases automatically when the app exits
fn already_running() -> bool {
    use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::core::w;

    unsafe {
        match CreateMutexW(None, false, w!("Local\\NowPlayingServer")) {
            Ok(handle) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    // Another copy owns it: close our handle so we don't keep its mutex alive
                    let _ = CloseHandle(handle);
                    true
                } else {
                    false // we own it now; the handle stays open until we exit
                }
            }
            Err(_) => false,
        }
    }
}

// ============================================================
// Main: starts the server, the status thread, and the tray icon
// ============================================================
fn main() {
    // Run from the exe's own folder, so static/ and config.toml are found no matter how the app is launched
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join("static").is_dir() {
                let _ = std::env::set_current_dir(dir);
            }
        }
    }

    // After "Restart", the old copy is still shutting down: wait up to ~3 seconds for it
    let restarted = std::env::args().any(|a| a == "--restarted");
    let mut tries = if restarted { 30 } else { 1 };
    while already_running() {
        tries -= 1;
        if tries == 0 {
            return; // another copy owns the tray icon and the port
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Clear last run's log, and make any crash get logged and end the app
    let _ = std::fs::write("nowplaying.log", ""); // Clear the old log
    std::panic::set_hook(Box::new(|info| {
        log(&format!("panic: {info}"));
        std::process::exit(1);
    }));

    // Record that we started, and which settings and folder we're using
    log("started");
    let c = config::get();
    log(&format!(
        "config: port={} floor={} ceil={} tilt={} (folder: {:?})",
        c.port,
        c.eq_floor_db,
        c.eq_ceil_db,
        c.eq_tilt_db,
        std::env::current_dir().unwrap_or_default()
    ));

    // the web sever and audio run on their own thead
    std::thread::spawn(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(server::run());
    });

    // the tray icon runs on the main thread and never returns
    let status = tray::start_status_thread();
    tray::run(status);
}
