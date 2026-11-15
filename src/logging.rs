// ============================================================
// Logging
// ============================================================

/// Appends a line to nowplaying.log
pub fn log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("nowplaying.log")
    {
        let _ = writeln!(f, "{msg}");
    }
}
