// ============================================================
// Logging
// ============================================================

/// Appends a line to nowplaying.log
pub fn log(msg: &str) {
    use std::io::Write;
    use windows::Win32::System::SystemInformation::GetLocalTime;

    let t = unsafe { GetLocalTime() };
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("nowplaying.log")
    {
        let _ = writeln!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {msg}",
            t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
        );
    }
}
