use std::path::PathBuf;

use url::Url;

fn drive_path(drive: char, rest: &str) -> PathBuf {
    let drive = drive.to_ascii_uppercase();
    let rest = rest.trim_start_matches(['/', '\\']);
    if cfg!(windows) {
        PathBuf::from(format!("{}:\\{}", drive, rest.replace('/', "\\")))
    } else {
        PathBuf::from(format!(
            "/mnt/{}/{}",
            drive.to_ascii_lowercase(),
            rest.replace('\\', "/")
        ))
    }
}

fn parse_windows_like(raw: &str) -> Option<PathBuf> {
    let text = raw.trim();
    if text.len() >= 3 {
        let bytes = text.as_bytes();
        if bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'/' || bytes[2] == b'\\')
        {
            let drive = bytes[0] as char;
            return Some(drive_path(drive, &text[3..]));
        }
    }
    if text.len() >= 4 {
        let bytes = text.as_bytes();
        if bytes[0] == b'/'
            && bytes[1].is_ascii_alphabetic()
            && bytes[2] == b':'
            && (bytes[3] == b'/' || bytes[3] == b'\\')
        {
            let drive = bytes[1] as char;
            return Some(drive_path(drive, &text[4..]));
        }
    }
    None
}

/// Resolve ACP `file://` URIs and raw paths into a local filesystem path.
///
/// This keeps path parsing logic centralized and testable across OS variants.
pub fn path_from_file_uri(uri: &str) -> Option<PathBuf> {
    let raw = uri.trim();
    if raw.is_empty() {
        return None;
    }
    if !raw.contains("://") {
        return Some(PathBuf::from(raw));
    }

    // Fast path for common Windows shapes that `Url::parse` may normalize unexpectedly.
    if let Some(file_tail) = raw.strip_prefix("file://") {
        if let Some(path) = parse_windows_like(file_tail) {
            return Some(path);
        }
        if let Some(local_tail) = file_tail.strip_prefix("localhost/") {
            if let Some(path) = parse_windows_like(local_tail) {
                return Some(path);
            }
        }
    }

    let parsed = Url::parse(raw).ok()?;
    if parsed.scheme() != "file" {
        return None;
    }
    if let Some(host) = parsed.host_str() {
        if host != "localhost" && !host.is_empty() {
            if host.len() == 1 && host.chars().all(|c| c.is_ascii_alphabetic()) {
                return Some(drive_path(host.chars().next()?, parsed.path()));
            }
            return None;
        }
    }

    let mut path_text = parsed.path().to_string();
    if path_text.starts_with("/%3A") {
        path_text = path_text.replacen("/%3A", ":", 1);
    }
    if path_text.len() >= 3 {
        let bytes = path_text.as_bytes();
        if bytes[0] == b'/' && bytes[2] == b':' && bytes[1].is_ascii_alphabetic() {
            let drive = bytes[1] as char;
            return Some(drive_path(drive, &path_text[3..]));
        }
    }
    if path_text.len() >= 2 {
        let bytes = path_text.as_bytes();
        if bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
            let drive = bytes[0] as char;
            return Some(drive_path(drive, &path_text[2..]));
        }
    }

    Some(PathBuf::from(path_text))
}

#[cfg(test)]
mod tests {
    use super::path_from_file_uri;

    #[test]
    fn rejects_non_file_uri() {
        assert!(path_from_file_uri("https://example.com/a.txt").is_none());
    }

    #[test]
    fn rejects_remote_file_host() {
        assert!(path_from_file_uri("file://server/share/file.txt").is_none());
    }

    #[test]
    fn keeps_raw_path_without_scheme() {
        let out = path_from_file_uri("relative/path.txt").expect("path");
        assert!(out.to_string_lossy().contains("relative"));
    }

    #[cfg(windows)]
    #[test]
    fn resolves_windows_file_uri_matrix() {
        let cases = [
            ("file://C:/Users/test/a.txt", r"C:\Users\test\a.txt"),
            ("file:///C:/Users/test/a.txt", r"C:\Users\test\a.txt"),
            ("file://localhost/C:/Users/test/a.txt", r"C:\Users\test\a.txt"),
            ("file://C:\\Users\\test\\a.txt", r"C:\Users\test\a.txt"),
            ("file:///C:\\Users\\test\\a.txt", r"C:\Users\test\a.txt"),
        ];
        for (input, expected) in cases {
            let got = path_from_file_uri(input).expect(input);
            assert_eq!(got.to_string_lossy(), expected);
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn resolves_windows_file_uri_to_wsl_style_on_non_windows() {
        let got = path_from_file_uri("file://C:/Users/test/a.txt").expect("path");
        assert_eq!(got.to_string_lossy(), "/mnt/c/Users/test/a.txt");
    }
}
