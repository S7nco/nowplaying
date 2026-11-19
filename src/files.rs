// Writing files safely: readers never see a half-written file

/// Writes the file to "<path>.tmp" first, then renames it over the real file.
/// The rename replaces the old file in one step, so anyone reading it gets either the
/// complete old file or the complete new one, and a crash mid-write can't leave it broken.
pub fn write_atomic(path: &str, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp); // don't leave the temporary file behind
    })
}
