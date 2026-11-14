use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
#[serde(default)]
pub struct Config {
    pub port: u16,
    pub process_name: String,
    pub app_id_match: String,
    pub eq_floor_db: f32,   // level shown as an empty bar
    pub eq_ceil_db: f32,    // level shown as a full bar
    pub eq_tilt_db: f32,    // boost for higher bands, per band
    pub tray_themes: Vec<String>,   // themes listed in the tray menu (the gallery shows all)
}

impl Default for Config {
    fn default() -> Self {
        Config {
            port: 5000,
            process_name: "AmpLibraryAgent.exe, Spotify.exe".into(),
            app_id_match:"applemusic, spotify".into(),
            eq_floor_db: -73.0, //-65
            eq_ceil_db: -27.0,   //-15
            eq_tilt_db: 3.0,
            tray_themes: vec!["default".into()],
        }
    }
}

static CONFIG: OnceLock<Config> = OnceLock::new();

// Loaded once, the first time anything asks for it
pub fn get() -> &'static Config {
    CONFIG.get_or_init(load)
}

fn load() -> Config {
    match std::fs::read_to_string("config.toml") {
        Ok(text) => match toml::from_str(&text) {
            Ok(cfg) => cfg,
            Err(e) => {
                crate::log(&format!("config.toml error, using defaults: {e}"));
                Config::default()
            }
        },
        Err(_) => Config::default(), // no file: all defaults
    }
}