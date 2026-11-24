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
    let restart_needed =
        settings.port != running.port || settings.bind_address != running.bind_address;
    json!({
        "settings": settings,
        "defaults": Config::default(),
        "restart_needed": restart_needed,
        "unknown": unknown,
        "error": error,
        // What other devices open, or null when only this PC can connect (uses the running settings)
        "network_url": crate::tray::network_host().map(|h| crate::tray::overlay_url(&h, "default")),
    })
}

fn one_error(name: &str, message: impl Into<String>) -> Errors {
    let mut errors = Errors::new();
    errors.insert(name.into(), Value::String(message.into()));
    errors
}

/// Checks the changes, writes them into config.toml, and uses them. Nothing is saved if anything is wrong.
fn apply(changes: &Map<String, Value>) -> Result<(), Errors> {
    // Start from what config.toml says now, with the changes on top
    let (current, _) = config::read_file().map_err(|e| {
        one_error(
            "config.toml",
            format!("config.toml has an error; fix it first: {e}"),
        )
    })?;
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
    let candidate: Config =
        serde_json::from_value(merged).map_err(|e| one_error("settings", e.to_string()))?;
    check(&candidate, &mut errors);
    if !errors.is_empty() {
        return Err(errors);
    }

    // A new port or address must really work, or the next Restart would close the app
    if changes.contains_key("port") || changes.contains_key("bind_address") {
        if let Some((name, message)) = network_problem(candidate.port, &candidate.bind_address) {
            return Err(one_error(name, message));
        }
    }

    write(changes)
        .map_err(|e| one_error("config.toml", format!("couldn't save config.toml: {e}")))?;
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
        bad(
            "bind_address",
            "Must be an IP address, like 127.0.0.1 (this PC only) or 0.0.0.0 (other devices too)",
        );
    }
    if !(-150.0..=0.0).contains(&c.eq_floor_db) {
        bad("eq_floor_db", "Must be between -150 and 0");
    }
    if !(-150.0..=0.0).contains(&c.eq_ceil_db) {
        bad("eq_ceil_db", "Must be between -150 and 0");
    } else if c.eq_ceil_db <= c.eq_floor_db {
        bad("eq_ceil_db", "Ceiling must be higher than the floor");
    }
    if !(-10.0..=10.0).contains(&c.eq_tilt_db) {
        bad("eq_tilt_db", "Must be between -10 and 10");
    }
    let players = crate::media::split_list(&c.process_name).len();
    if crate::media::split_list(&c.app_id_match).len() != players {
        bad(
            "app_id_match",
            "Needs one entry for each process_name entry, in the same order",
        );
    }
    if c.tray_themes.iter().any(|t| !crate::themes::valid_name(t)) {
        bad(
            "tray_themes",
            "Theme names use only letters, numbers, - and _",
        );
    }
    if c.allowed_hosts.iter().any(|h| h.contains('*')) {
        bad(
            "allowed_hosts",
            "Wildcards aren't supported; list each name",
        );
    }
}

/// Ports browsers refuse to open, even when the app can listen on them (the "bad ports" in the Fetch standard)
const BROWSER_BLOCKED_PORTS: &[u16] = &[
    1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102,
    103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465,
    512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993,
    995, 1719, 1720, 1723, 2049, 3659, 4045, 4190, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668,
    6669, 6679, 6697, 10080,
];

/// Why the app couldn't run on this port and address: (setting name, message). None = it can
fn network_problem(port: u16, bind: &str) -> Option<(&'static str, String)> {
    let running = config::get();
    if port == running.port && bind == running.bind_address {
        return None; // what the app is using right now
    }
    if BROWSER_BLOCKED_PORTS.contains(&port) {
        return Some((
            "port",
            format!("Browsers and OBS refuse to open port {port}. Try another, like 5001."),
        ));
    }
    // This app already holds its own port, so for that one only the address can be tested
    let test_port = if port == running.port { 0 } else { port };
    let error = std::net::TcpListener::bind((bind, test_port)).err()?;
    Some(match error.kind() {
        std::io::ErrorKind::AddrInUse => (
            "port",
            format!("Port {port} is already used by another program. Try another."),
        ),
        std::io::ErrorKind::PermissionDenied => (
            "port",
            format!("Windows has reserved port {port}. Try another, like 5001."),
        ),
        std::io::ErrorKind::AddrNotAvailable => (
            "bind_address",
            format!("{bind} isn't an address of this PC."),
        ),
        _ => (
            "bind_address",
            format!("Can't listen on {bind}:{port}: {error}"),
        ),
    })
}

/// Why a Restart would fail, checked first so it can't close the app and leave nothing running
pub fn restart_problem() -> Option<String> {
    match config::read_file() {
        Ok((file, _)) => network_problem(file.port, &file.bind_address).map(|(_, message)| message),
        Err(e) => Some(format!("config.toml has an error. Fix it first: {e}")),
    }
}

/// Writes only the changed settings into config.toml. Everything else in the file stays exactly as it was.
fn write(changes: &Map<String, Value>) -> std::io::Result<()> {
    let text = std::fs::read_to_string(config::FILE).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(std::io::Error::other)?;
    let defaults = toml::Table::try_from(Config::default()).unwrap_or_default();
    let mut missing = Vec::new(); // settings not in the file yet: (name, "name = value")
    for (name, value) in changes {
        // numbers with decimals stay that way in the file ("-30.0", not "-30")
        let decimal = matches!(defaults.get(name), Some(toml::Value::Float(_)));
        let Some(new) = to_toml(value, decimal) else {
            continue;
        };
        match doc.get_mut(name).and_then(|item| item.as_value_mut()) {
            // keep the spacing and any comment after the value on that line
            Some(old) => {
                let decor = old.decor().clone();
                *old = new;
                *old.decor_mut() = decor;
            }
            None => missing.push((name.clone(), format!("{name} = {}", new.to_string().trim()))),
        }
    }
    let mut out = doc.to_string();
    for (name, line) in &missing {
        out = insert_line(&out, name, line);
    }
    // Make sure the result still reads back correctly before saving it
    out.parse::<toml::Table>().map_err(std::io::Error::other)?;
    crate::files::write_atomic(config::FILE, out.as_bytes())
}

/// Puts a new setting's line right after the last "# name = ..." comment, so it lands next to its
/// explanation (and the examples stay as they are). No such comment: it goes at the end.
fn insert_line(text: &str, name: &str, line: &str) -> String {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let is_comment_for = |l: &&str| {
        l.trim_start()
            .strip_prefix('#')
            .and_then(|rest| rest.trim_start().strip_prefix(name))
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    };
    let mut lines: Vec<&str> = text.lines().collect();
    match lines.iter().rposition(is_comment_for) {
        Some(i) => lines.insert(i + 1, line),
        None => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push("");
            }
            lines.push(line);
        }
    }
    lines.join(newline) + newline
}

fn to_toml(value: &Value, decimal: bool) -> Option<toml_edit::Value> {
    Some(match value {
        Value::Bool(b) => (*b).into(),
        Value::Number(n) if decimal => n.as_f64()?.into(),
        Value::Number(n) => n.as_i64()?.into(),
        Value::String(s) => s.as_str().into(),
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .collect::<toml_edit::Array>()
            .into(),
        _ => return None,
    })
}
