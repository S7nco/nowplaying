use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub port: u16,
    pub process_name: String,
    pub app_id_match: String,
    pub eq_floor_db: f32,           // level shown as an empty bar
    pub eq_ceil_db: f32,            // level shown as a full bar
    pub eq_tilt_db: f32,            // boost for higher bands, per band
    pub tray_themes: Vec<String>,   // themes listed in the tray menu (the gallery shows all)
    pub bind_address: String, // who can connect: "0.0.0.0" = any device on your network, "127.0.0.1" = only your own machine
    pub allowed_hosts: Vec<String>, // extra names the overlay may be opened by (this PC's own name always works)
}

// The port/bind_address from config.toml already reported as "needs a restart", so it's said once, not on every save
static RESTART_NOTED: Mutex<Option<(u16, String)>> = Mutex::new(None);

// The current settings. A reload swaps in a new copy; code still using the old one finishes safely
static CONFIG: OnceLock<RwLock<Arc<Config>>> = OnceLock::new();
const FILE: &str = "config.toml";

fn slot() -> &'static RwLock<Arc<Config>> {
    CONFIG.get_or_init(|| {
        let cfg = read_file().unwrap_or_else(|e| {
            crate::log(&format!("config.toml error, using defaults: {e}"));
            Config::default()
        });
        RwLock::new(Arc::new(cfg))
    })
}

impl Default for Config {
    fn default() -> Self {
        Config {
            port: 5000,
            process_name: "AmpLibraryAgent.exe, Spotify.exe".into(),
            app_id_match: "applemusic, spotify".into(),
            eq_floor_db: -73.0, //-65
            eq_ceil_db: -27.0,  //-15
            eq_tilt_db: 3.0,
            tray_themes: vec!["default".into()],
            bind_address: "127.0.0.1".into(),
            allowed_hosts: Vec::new(),
        }
    }
}

// The current settings. Use it and let it go; don't keep it for long, or it will miss changes
pub fn get() -> Arc<Config> {
    slot().read().unwrap().clone()
}

// Reads config.toml. No file means all defaults; a broken file will result in an error
fn read_file() -> Result<Config, String> {
    match std::fs::read_to_string(FILE) {
        Ok(text) => {
            let cfg: Config = toml::from_str(&text).map_err(|e| e.to_string())?;
            warn_unknown_settings(&text);
            Ok(cfg)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("Couldn't read config.toml: {e}")),
    }
}

// Logs settings in config.toml the app doesn't know (usually a typo, like "eq_floor_dgb").
fn warn_unknown_settings(text: &str) {
    let Ok(file) = text.parse::<toml::Table>() else {
        return;
    };
    let Ok(known) = toml::Table::try_from(Config::default()) else {
        return;
    };
    for name in file.keys().filter(|name| !known.contains_key(*name)) {
        crate::log(&format!(
            "config.toml: unknown setting \"{name}\" is ignored. Check its spelling against config.example.toml"
        ));
    }
}

// Reads config.toml again and uses it from now. If it has an error, current settings stay
pub fn reload() -> Result<(), String> {
    let mut new = read_file()?;
    let old = get();
    let wanted = (new.port, new.bind_address.clone());
    let mut noted = RESTART_NOTED.lock().unwrap();
    if new.port != old.port || new.bind_address != old.bind_address {
        if noted.as_ref() != Some(&wanted) {
            crate::log(
                "config: port or bind_address changed; this takes effect after a Restart (tray menu)",
            );
            *noted = Some(wanted);
        }
        new.port = old.port;
        new.bind_address = old.bind_address.clone();
    } else {
        *noted = None;
    }
    drop(noted);

    if new == *old {
        return Ok(()); // saved without changes: nothing to do
    }
    crate::log(&format!(
        "config reloaded: floor={} ceil={} tilt={}",
        new.eq_floor_db, new.eq_ceil_db, new.eq_tilt_db
    ));
    *slot().write().unwrap() = Arc::new(new);
    Ok(())
}

// Reloads config.toml whenever it changes (checked every 2 seconds)
pub fn watch() {
    std::thread::spawn(|| {
        let modified = || std::fs::metadata(FILE).and_then(|m| m.modified()).ok();
        let mut last = modified();
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let now = modified();
            if now != last {
                last = now;
                if let Err(e) = reload() {
                    crate::log(&format!(
                        "config.toml error, keeping the current settings: {e}"
                    ));
                }
            }
        }
    });
}
