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
// checks every request: right host, own pages only, changes from this PC only
mod guard;

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

/// Asks the copy that's already running for its version (GET /version on this PC).
/// None if it doesn't answer within a second, e.g. an older version without /version.
fn running_version() -> Option<String> {
    use std::io::{Read, Write};
    use std::time::Duration;

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], config::get().port));
    let mut stream = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(1)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(1))).ok()?;
    stream
        .write_all(b"GET /version HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    // the reply is "HTTP/1.1 200 OK ... headers ... (blank line) 0.2.0"
    let (head, body) = response.split_once("\r\n\r\n")?;
    let version = body.trim();
    (head.contains(" 200 ") && !version.is_empty() && version.len() <= 20)
        .then(|| version.to_string())
}

/// Shows a Windows message box. The app has no console window, so this is how it tells the user something.
pub fn show_message(text: &str, error: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{
        MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MessageBoxW,
    };
    use windows::core::{HSTRING, w};

    let icon = if error {
        MB_ICONERROR
    } else {
        MB_ICONINFORMATION
    };
    unsafe {
        MessageBoxW(None, &HSTRING::from(text), w!("Now Playing"), MB_OK | icon);
    }
}

/// Tells the user another copy is already running, instead of closing without a word
fn show_already_running() {
    let this = env!("CARGO_PKG_VERSION");
    let text = match running_version() {
        Some(running) if running != this => format!(
            "Now Playing {running} is already running, so this copy ({this}) won't start.\n\n\
            To switch, right-click its tray icon, choose Quit, then start this one again."
        ),
        _ => "Now Playing is already running.\n\n\
            Look for its icon in the system tray (you may need to click the ^ arrow)."
            .to_string(),
    };
    show_message(&text, false);
}

/// The app's folder for the log, with the user's own folder shown as %USERPROFILE%,
/// so a log pasted into a public issue doesn't show the Windows user name
fn folder_for_log() -> String {
    let dir = std::env::current_dir()
        .unwrap_or_default()
        .display()
        .to_string();
    match std::env::var("USERPROFILE") {
        Ok(home)
            if !home.is_empty()
                && dir
                    .get(..home.len())
                    .is_some_and(|start| start.eq_ignore_ascii_case(&home))
                && matches!(
                    dir[home.len()..].chars().next(),
                    None | Some('\\') | Some('/')
                ) =>
        {
            format!("%USERPROFILE%{}", &dir[home.len()..])
        }
        _ => dir,
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
            // a normal start found another copy: say so. After Restart, the old copy just
            // took too long to close; that's rare, and a message there would only confuse
            if !restarted {
                show_already_running();
            }
            return; // another copy owns the tray icon and the port
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Keep the last run's log as nowplaying.old.log (so a crash isn't wiped by starting again), then start fresh
    // if renaming fails (e.g. the old log is open in another program), empty the log instead.
    if std::fs::rename("nowplaying.log", "nowplaying.old.log").is_err() {
        let _ = std::fs::write("nowplaying.log", "");
    }
    std::panic::set_hook(Box::new(|info| {
        log(&format!("panic: {info}"));
        show_message(
            "Now Playing stopped because of an unexpected error.\n\n\
            The details are in nowplaying.log, in the app's folder. If it keeps happening, \
            please report it on GitHub with that file.",
            true,
        );
        std::process::exit(1);
    }));

    // Record that we started, and which settings and folder we're using
    log("started");
    let c = config::get();
    log(&format!(
        "config: port={} floor={} ceil={} tilt={} (folder: {})",
        c.port,
        c.eq_floor_db,
        c.eq_ceil_db,
        c.eq_tilt_db,
        folder_for_log()
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
