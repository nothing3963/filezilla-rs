//! POSIX path helpers for remote file systems.
//!
//! FTP always uses `/` as separator and SFTP uses POSIX paths as well, so the
//! whole remote side is normalised here and never touches `std::path` (which
//! would use `\` on Windows).

/// Normalise a remote path: absolute, single slashes, `.`/`..` resolved.
pub fn normalize(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return "/".to_string();
    }
    let mut parts: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

/// Join a directory and a child name (or absolute path).
pub fn join(base: &str, name: &str) -> String {
    if name.starts_with('/') {
        return normalize(name);
    }
    if base.is_empty() || base == "/" {
        normalize(&format!("/{name}"))
    } else {
        normalize(&format!("{base}/{name}"))
    }
}

pub fn parent(path: &str) -> Option<String> {
    let normalized = normalize(path);
    if normalized == "/" {
        return None;
    }
    let trimmed = normalized.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) | None => Some("/".to_string()),
        Some(idx) => Some(trimmed[..idx].to_string()),
    }
}

pub fn file_name(path: &str) -> String {
    let normalized = normalize(path);
    if normalized == "/" {
        return "/".to_string();
    }
    normalized
        .rsplit('/')
        .next()
        .unwrap_or(&normalized)
        .to_string()
}

/// Lowest common ancestor of two absolute POSIX paths.
pub fn common_parent(a: &str, b: &str) -> Option<String> {
    let a = normalize(a);
    let b = normalize(b);
    let ap: Vec<&str> = a
        .trim_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    let bp: Vec<&str> = b
        .trim_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    let mut shared = Vec::new();
    for (left, right) in ap.iter().zip(bp.iter()) {
        if left == right {
            shared.push(*left);
        } else {
            break;
        }
    }
    if shared.is_empty() {
        Some("/".to_string())
    } else {
        Some(format!("/{}", shared.join("/")))
    }
}

/// Build a child path (same semantics as `CServerPath::FormatFilename`).
pub fn format_filename(base: &str, name: &str) -> String {
    join(base, name)
}

/// Apply a relative path to a base, rejecting attempts to leave the root.
pub fn change_path(base: &str, relative: &str) -> Option<String> {
    if relative.starts_with('/') {
        return Some(normalize(relative));
    }
    let normalized = normalize(base);
    let mut parts: Vec<&str> = normalized
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        Some("/".to_string())
    } else {
        Some(format!("/{}", parts.join("/")))
    }
}

/// Convert a Unix mode to a `ls -l` style permission string.
pub fn mode_to_string(mode: u32, is_dir: bool) -> String {
    let mut out = String::with_capacity(10);
    out.push(if is_dir { 'd' } else { '-' });
    let flags = [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ];
    for (bit, ch) in flags {
        out.push(if mode & bit != 0 { ch } else { '-' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_paths() {
        assert_eq!(normalize(""), "/");
        assert_eq!(normalize("foo/bar"), "/foo/bar");
        assert_eq!(normalize("/foo//bar/./baz/"), "/foo/bar/baz");
        assert_eq!(normalize("/foo/../bar"), "/bar");
        assert_eq!(normalize("/../.."), "/");
    }

    #[test]
    fn joins_and_walks() {
        assert_eq!(join("/", "etc"), "/etc");
        assert_eq!(join("/var/www", "index.html"), "/var/www/index.html");
        assert_eq!(join("/var/www", "/abs/path"), "/abs/path");
        assert_eq!(parent("/"), None);
        assert_eq!(parent("/etc"), Some("/".into()));
        assert_eq!(parent("/var/www"), Some("/var".into()));
        assert_eq!(file_name("/var/www/index.html"), "index.html");
    }

    #[test]
    fn cpp_posix_path_cases() {
        assert_eq!(
            common_parent("/foo/baz", "/foo/bar").as_deref(),
            Some("/foo")
        );
        assert_eq!(common_parent("/foo", "/foo").as_deref(), Some("/foo"));
        assert_eq!(common_parent("/foo", "/foo/bar").as_deref(), Some("/foo"));
        assert_eq!(format_filename("/", "bar"), "/bar");
        assert_eq!(format_filename("/foo", "bar"), "/foo/bar");
        assert_eq!(
            change_path("/foo/bar", "baz").as_deref(),
            Some("/foo/bar/baz")
        );
        assert_eq!(
            change_path("/foo/bar/baz", "../../baz").as_deref(),
            Some("/foo/baz")
        );
        assert_eq!(
            change_path("/foo/baz", "/foo/bar/baz").as_deref(),
            Some("/foo/bar/baz")
        );
        assert_eq!(change_path("/foo/bar", ".").as_deref(), Some("/foo/bar"));
        assert_eq!(change_path("/", ".."), None);
    }

    #[test]
    fn formats_modes() {
        assert_eq!(mode_to_string(0o755, true), "drwxr-xr-x");
        assert_eq!(mode_to_string(0o644, false), "-rw-r--r--");
    }
}
