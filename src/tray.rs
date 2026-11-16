use crate::{audio, config, favorites, media, themes};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tray_icon::{
    Icon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu},
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, SetTimer, TranslateMessage, WM_TIMER,
};

// ============================================================
// Constants and types
// ============================================================

// What a tray menu item does when clicked
#[derive(Clone)]
enum Action {
    Open(String),
    Copy(String),
    Restart,
    Quit,
}

/// The status text shared between threads: (menu line, tooltip).
pub type Status = Arc<Mutex<(String, String)>>;

// ============================================================
// Small helpers
// ============================================================

/// Shortens text to 'max' characters, adding "..." if it was cut
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max - 1).collect::<String>())
    }
}

/// "time-above" -> "Time Above", for showing theme names in the menu
fn display_name(name: &str) -> String {
    name.replace(|c| c == '-' || c == '_', " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Builds the overlay address for a theme, using the port from config.toml
fn overlay_url(theme: &str) -> String {
    let port = config::get().port;
    if theme == "default" {
        format!("http://localhost:{port}/overlay.html")
    } else {
        format!("http://localhost:{port}/overlay.html?theme={theme}")
    }
}

// Adds one item per theme to a submenu
fn fill_submenu(
    sub: &Submenu,
    names: &[String],
    action: fn(String) -> Action,
    actions: &mut HashMap<MenuId, Action>,
    items: &mut Vec<MenuItem>,
) {
    if names.is_empty() {
        let none = MenuItem::new("(none)", false, None); // greyed out)
        sub.append(&none).unwrap();
        items.push(none);
    }
    for name in names {
        let item = MenuItem::new(display_name(&name), true, None);
        sub.append(&item).unwrap();
        actions.insert(item.id().clone(), action(overlay_url(&name)));
        items.push(item);
    }
}

// Fills (or refills) the two favorites submenus from the favorites list
// The list is picked in the gallery; only themese that are installed are shown.
fn rebuild_favorites(
    open_menu: &Submenu,
    copy_menu: &Submenu,
    actions: &Arc<Mutex<HashMap<MenuId, Action>>>,
    items: &mut Vec<MenuItem>,
) {
    let installed = themes::installed();
    let names: Vec<String> = favorites::get()
        .into_iter()
        .filter(|n| installed.contains(n))
        .collect();

    let mut map = actions.lock().unwrap();
    // forget the old items
    for item in items.iter() {
        map.remove(item.id());
    }
    items.clear();
    while open_menu.remove_at(0).is_some() {}
    while copy_menu.remove_at(0).is_some() {}

    fill_submenu(open_menu, &names, Action::Open, &mut map, items);
    fill_submenu(copy_menu, &names, Action::Copy, &mut map, items);
}

/// Makes the tray menu follow Windows' dark mode setting.
/// Uses unoffical uxtheme.dll functions, so it could stop working in a future Windows update.
fn enable_dark_menus() {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::core::{PCSTR, w};

    unsafe {
        if let Ok(lib) = LoadLibraryW(w!("uxtheme.dll")) {
            // Unoffical ordinals: 135 = SetPreferredAppMOde, 136 = FlushMenuThemes
            if let Some(f) = GetProcAddress(lib, PCSTR(135 as *const u8)) {
                let set_mode: extern "system" fn(i32) -> i32 = std::mem::transmute(f);
                set_mode(1); // 1 = follow system setting, 2 = always dark
            }
            if let Some(f) = GetProcAddress(lib, PCSTR(136 as *const u8)) {
                let flush: extern "system" fn() = std::mem::transmute(f);
                flush();
            }
        }
    }
}

/// Starts a backgound thread that reads the current song every 3 seconds
pub fn start_status_thread() -> Status {
    // Background thread that keeps the status text up to date
    let shared = std::sync::Arc::new(std::sync::Mutex::new((
        "Starting...".to_string(),
        "Now Playing Server:".to_string(),
    )));
    let writer = shared.clone();
    std::thread::spawn(move || {
        loop {
            let info = media::read_track();
            let text = match (&info.title, &info.artist) {
                (Some(t), a) => {
                    let state = if info.playing == Some(true) {
                        "Playing"
                    } else {
                        "Paused"
                    };
                    // "Title - Artist", or just the title when there's no artist
                    let song = match a {
                        Some(a) => format!("{t} - {a}"),
                        None => t.clone(),
                    };
                    (
                        truncate(&format!("{state}: {song}"), 60),
                        truncate(&format!("Now Playing Server\n{song}"), 120),
                    )
                }
                _ => {
                    let running = media::split_list(&config::get().process_name)
                        .iter()
                        .any(|name| audio::find_pid(name).is_some());
                    let msg = if running {
                        "No Song Detected"
                    } else {
                        "Application Not Running"
                    };
                    (msg.to_string(), format!("Now Playing Server\n{msg}"))
                }
            };
            *writer.lock().unwrap() = text;
            std::thread::sleep(Duration::from_secs(3));
        }
    });
    shared
}

/// Builds the tray icon and menu, then runs the Windows message loop forever.
pub fn run(shared: Status) {
    // Enable dark menus
    enable_dark_menus();

    // the tray icon and its menu live on the main thread
    let menu = Menu::new();
    let status_item = MenuItem::new("Starting...", false, None); // false = greyed out

    // shared with the click handler, because the favorites can change while the app runs
    let actions: Arc<Mutex<HashMap<MenuId, Action>>> = Arc::new(Mutex::new(HashMap::new()));

    // the two favorites submenus: filled from the favorites list (picked in the gallery)
    let open_menu = Submenu::new("Open in Browser", true);
    let copy_menu = Submenu::new("Copy URL", true);
    let mut fav_items: Vec<MenuItem> = Vec::new();
    let mut seen_version = favorites::version();
    rebuild_favorites(&open_menu, &copy_menu, &actions, &mut fav_items);

    // Opens the theme gallary page in the browser
    let gallery = MenuItem::new("Theme Gallery", true, None);
    actions.lock().unwrap().insert(
        gallery.id().clone(),
        Action::Open(format!(
            "http://localhost:{}/gallery.html",
            config::get().port
        )),
    );

    let restart = MenuItem::new("Restart", true, None);
    actions
        .lock()
        .unwrap()
        .insert(restart.id().clone(), Action::Restart);

    let quit = MenuItem::new("Quit", true, None);
    actions
        .lock()
        .unwrap()
        .insert(quit.id().clone(), Action::Quit);

    // the app's version (from Cargo.toml at build time); clicking it opens the releases page
    let version = MenuItem::new(format!("Version {}", env!("CARGO_PKG_VERSION")), true, None);
    actions.lock().unwrap().insert(
        version.id().clone(),
        Action::Open(format!("{}/releases", env!("CARGO_PKG_REPOSITORY"))),
    );

    // Opens nowplaying.log (the app runs from its own folder, so the log is there)
    let log_path = std::env::current_dir().unwrap_or_default().join("nowplaying.log");
    let open_log = MenuItem::new("Open Log", true, None);
    actions.lock().unwrap().insert(
        open_log.id().clone(),
        Action::Open(log_path.to_string_lossy().into_owned()),
    );

    menu.append(&status_item).unwrap();
    menu.append(&PredefinedMenuItem::separator()).unwrap();
    menu.append(&gallery).unwrap();
    menu.append(&open_menu).unwrap();
    menu.append(&copy_menu).unwrap();
    menu.append(&PredefinedMenuItem::separator()).unwrap();
    menu.append(&open_log).unwrap();
    menu.append(&version).unwrap();
    menu.append(&restart).unwrap();
    menu.append(&quit).unwrap();

    let handler_actions = actions.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let action = handler_actions.lock().unwrap().get(&event.id).cloned();
        match action {
            Some(Action::Open(url)) => {
                let _ = open::that(url);
            }
            Some(Action::Copy(url)) => {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(url.clone());
                }
            }
            Some(Action::Restart) => {
                // Start a new copy (same folder, so static/ and config.toml are found), then exit
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).arg("--restarted").spawn();
                }
                std::process::exit(0);
            }
            Some(Action::Quit) => std::process::exit(0),
            None => {}
        }
    }));

    let icon = Icon::from_path("icon.ico", Some((32, 32))).unwrap();
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false) // Disable left-click menu
        .with_tooltip("Now Playing Server")
        .with_icon(icon)
        .build()
        .unwrap();

    // the Windows message loop: keeps the tray icon alive and responsive
    unsafe {
        SetTimer(None, 1, 2000, None); // a WIM_TIMER message every 2 seconds
        let mut last = (String::new(), String::new());
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_TIMER {
                // the favorites changed in the gallery: refill the two submenus
                let version = favorites::version();
                if version != seen_version {
                    seen_version = version;
                    rebuild_favorites(&open_menu, &copy_menu, &actions, &mut fav_items);
                }

                let current = shared.lock().unwrap().clone();
                if current != last {
                    status_item.set_text(&current.0);
                    let _ = tray.set_tooltip(Some(&current.1));
                    last = current;
                }
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
