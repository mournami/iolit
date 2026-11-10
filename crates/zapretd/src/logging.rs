pub fn log_tail(path: &std::path::Path) -> Vec<String> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        return vec![];
    };
    let start = file
        .metadata()
        .map(|m| m.len().saturating_sub(128 * 1024))
        .unwrap_or(0);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return vec![];
    }
    let mut bytes = Vec::new();
    if file.take(128 * 1024).read_to_end(&mut bytes).is_err() {
        return vec![];
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<_> = text.lines().rev().take(80).map(str::to_owned).collect();
    lines.into_iter().rev().collect()
}
