use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Git prints paths with forward slashes; turn them into this platform's native form.
pub fn native_path(p: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(p.replace('/', "\\"))
    } else {
        PathBuf::from(p)
    }
}

/// A path in a form two paths can be compared by: forward slashes, no trailing slash, and
/// lowercase on the platforms whose file systems ignore case by default.
pub fn path_key(p: impl AsRef<Path>) -> String {
    let s = p.as_ref().to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if cfg!(any(windows, target_os = "macos")) {
        s.to_lowercase()
    } else {
        s
    }
}

/// The last `max` characters of `s`, cut on a character boundary.
pub fn tail(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut start = s.len() - max;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

/// Progress output rewrites one line with `\r`; keep only what the line finally said.
pub fn collapse_carriage_returns(s: &str) -> String {
    s.lines()
        .map(|l| l.rsplit('\r').find(|seg| !seg.trim().is_empty()).unwrap_or(""))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_progress_lines() {
        let s = "Receiving objects:  10%\rReceiving objects: 100%, done.\nResolving deltas: 100%\n";
        assert_eq!(collapse_carriage_returns(s), "Receiving objects: 100%, done.\nResolving deltas: 100%");
    }

    #[test]
    fn tail_respects_char_boundaries() {
        assert_eq!(tail("çğüşö", 3), "ö");
    }
}
