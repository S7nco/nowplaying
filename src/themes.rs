// Theme files: which names are allowed, and listing the .css files in each folder

use std::collections::HashSet;
use std::sync::Mutex;

/// Every folder in static/themes, in the order the overlay searches them:
/// custom first, examples last, the rest alphabetical order
pub fn folders() -> Vec<String> {
    let Ok(dir) = std::fs::read_dir("static/themes") else {
        return Vec::new();
    };
    let mut names: Vec<String> = dir
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| valid_name(name))
        .collect();
    let rank = |name: &str| match name {
        "custom" => 0,
        "examples" => 2,
        _ => 1,
    };
    names.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
    names
}

/// Letters, numbers, - and _ only (the overlay strips anything else from ?theme=)
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

// Files already reported as skipped, so the log gets each one once
static REPORTED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Theme names (file names without .css) in one folder, sorted. Bad names are skipped and logged.
pub fn list(folder: &str) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(dir) = std::fs::read_dir(format!("static/themes/{folder}")) else {
        return names;
    };
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("css") {
            continue;
        }
        let Some(name) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
            continue;
        };
        if valid_name(&name) {
            names.push(name);
        } else {
            let file = format!("{folder}/{name}.css");
            let mut reported = REPORTED.lock().unwrap();
            if reported
                .get_or_insert_with(HashSet::new)
                .insert(file.clone())
            {
                crate::log(&format!(
                    "Skipped theme \"{file}\": use only letters, numbers, - and _ in the name"
                ));
            }
        }
    }
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// Every installed theme name, from all folders
pub fn installed() -> Vec<String> {
    folders().iter().flat_map(|f| list(f)).collect()
}
