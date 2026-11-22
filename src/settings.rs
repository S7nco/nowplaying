// The settings API, used by the settings page:
//  GET /settings what config.toml says, and whether a Restart is waiting
//  PUT /settings change some settings, e.g. {"eq_ceil_db": -40}
// Changes are checked, written into config.toml (keeping its comments and layout), then used straight away

use crate::config::{self, Config};
use axum::{Json, http::StatusCode, response::IntoResponse, response::Response};
use serde_json::{Map, Value, json};

/// Setting name -> what's wrong with it
type Errors = Map<String, Value>;

/// GET /settings
pub async fn get_settings() -> Json<Value> {
    Json(state())
}

/// PUT /settings: answers with the new state, or 400 and {"errors": {"setting": "what's wrong"}}
pub async fn put_settings(Json(changes): Json<Map<String, Value>>) -> Response {
    match apply(&changes) {
        Ok(()) => Json(state()).into_response(),
        Err(errors) => (StatusCode::BAD_REQUEST, Json(json!({ "errors": errors }))).into_response(),
    }
}

/// What config.toml says now. If it has an error, the running settings are shown with the error
fn state() -> Value {
    let running = config::get();
    let (settings, unknown, error) = match config::read_file() {
        Ok((cfg, unknown)) => (cfg, unknown, None),
        Err(e) => ((*running).clone(), Vec::new(), Some(e)),
    };
    let restart_needed = settings.port != running.port || settings.bind_address != running.bind_address;
    json!({ "settings": settings, "restart_needed": restart_needed, "unknown": unknown, "error": error })
}

fn one_error(name: &str, message: impl Into<String>) -> Errors {
    let mut errors = Errors::new();
    errors.insert(name.into(), Value::String(message.into()));
    errors
}

/// Checks the changes, writes them into config.toml, and uses them. Nothing is saved if anything is wrong.
fn apply(changes: &Map<String, Value>) -> Result<(), Errors> {
    // Start from what config.toml says now, with the changes on top
    let (current, _) = config::read_file()
        .map_err(|e| one_error("config.toml", format!("config.toml has an error; fix it first: {e}")))?;
    let mut merged = serde_json::to_value(&current).unwrap_or_default();
    let mut errors = Errors::new();
    for (name, value) in changes {
        match merged.get_mut(name) {
            Some(slot) => *slot = value.clone(),
            None => {
                errors.insert(name.clone(), "Unknown setting".into());
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    // The right kind of value (a number, text, a list), then sensible values
    let candidate: Config = serde_json::from_value(merged).map_err(|e| one_error("settings", e.to_string()))?;
    check(&candidate, &mut errors);
    if !errors.is_empty(){
        return Err(errors);
    }

    write(changes).map_err(|e| one_error("config.toml", format!("couldn't save config.toml: {e}")))?;
    // Use them now. (The file watcher also notices, a moment later, but finds nothing new)
    config::reload().map_err(|e| one_error("config.toml", e))
}

/// Values that would break things, with a message the settings page can show next to the setting
fn check(c: &Config, errors: &mut Errors) {
    let mut bad = |name: &str, message: &str| {
        errors.insert(name.into(), message.into());
    };
    if c.port == 0 {
        bad("port", "Must be between 1 and 65535");
    }
    let bind = c.bind_address.trim();
    if bind != "localhost" && bind.parse::<std::net::IpAddr>().is_err() {
        bad("bind_address", "Must be an IP address, like 127.0.0.1 (this PC only) or 0.0.0.0 (other devices too)");
    }
    if !(-150.0..=0.0).contains(&c.eq_floor_db) {
        bad("eq_floor_db", "Must be between -150 and 0");
    }
    if !(-150.0..=0.0).contains(&c.eq_ceil_db) {
        bad("eq_ceil_db", "Must be between -150 and 0");
    }
    if !(-10.0..=10.0).contains(&c.eq_tilt_db) {
        bad("eq_tilt_db", "Must be between -10 and 10");
    }
    let players = crate::media::split_list(&c.process_name).len();
    if crate::media::split_list(&c.app_id_match).len() != players {
        bad("app_id_match", "Needs one entry for each process_name entry, in the same order");
    }
    if c.tray_themes.iter().any(|t| !crate::themes::valid_name(t)) {
        bad("tray_themes", "Theme names use only letters, numbers, - and _");
    }
    if c.allowed_hosts.iter().any(|h| h.contains('*')) {
        bad("allowed_hosts", "Wildcards aren't supported; list each name");
    }
}

/// Writes only the changed settings into config.toml. Everything else in the file stays exactly as it was.
fn write(changes: &Map<String, Value>) -> std::io::Result<()> {
    let text = std::fs::read_to_string(config::FILE).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(std::io::Error::other)?;
    let defaults = toml::Table::try_from(Config::default()).unwrap_or_default();
    for (name, value) in changes {
        // numbers with decimals stay that way in the file ("-30.0", not "-30")
        let decimal = matches!(defaults.get(name), Some(toml::Value::Float(_)));
        let Some(new) = to_toml(value, decimal) else { continue };
        match doc.get_mut(name).and_then(|item| item.as_value_mut()) {
            // keep the spacing and any comment after the value on that line
            Some(old) => {
                let decor = old.decor().clone();
                *old = new;
                *old.decor_mut() = decor;
            }
            // not in the file yet (or only as a # comment): added at the end
            None => {
                doc.insert(name, toml_edit::Item::Value(new));
            }
        }
    }
    crate::files::write_atomic(config::FILE, doc.to_string().as_bytes())
}

fn to_toml(value: &Value, decimal: bool) -> Option<toml_edit::Value> {
    Some(match value {
        Value::Bool(b) => (*b).into(),
        Value::Number(n) if decimal => n.as_f64()?.into(),
        Value::Number(n) => n.as_i64()?.into(),
        Value::String(s) => s.as_str().into(),
        Value::Array(items) => items.iter().filter_map(Value::as_str).collect::<toml_edit::Array>().into(),
        _ => return None,
    })
}