//! Windows local path semantics ported from FileZilla's `CLocalPath`.
//!
//! The UI mostly uses `std::path`, but FileZilla's rules for drive roots, UNC
//! shares and `..` clamping differ subtly from `Path::canonicalize` (which also
//! touches the disk).  Keeping a pure-string implementation makes the rules
//! testable and safe for paths that do not exist yet.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalPath {
    path: String,
}

#[allow(dead_code)]
impl LocalPath {
    pub fn new(input: &str) -> Self {
        Self {
            path: normalize(input),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.path
    }

    pub fn is_empty(&self) -> bool {
        self.path.is_empty()
    }

    pub fn has_parent(&self) -> bool {
        let (_, segments) = split_root(&self.path);
        !segments.is_empty()
    }

    pub fn has_logical_parent(&self) -> bool {
        if self.path.is_empty() || self.path == "\\" {
            return false;
        }
        if is_unc_root(&self.path) {
            return false;
        }
        true
    }

    /// Replace the path with an absolute path, or apply a relative path.
    pub fn change_path(&mut self, input: &str) -> bool {
        let normalized = normalize_separators(input);
        let absolute = normalized.starts_with('\\')
            || normalized.starts_with("\\\\")
            || has_drive_prefix(&normalized);
        if absolute {
            self.path = normalize(&normalized);
            return true;
        }
        if self.path.is_empty() {
            return false;
        }
        self.path = apply_relative(&self.path, &normalized);
        true
    }

    pub fn add_segment(&mut self, segment: &str) {
        if segment.is_empty() {
            return;
        }
        if self.path.is_empty() {
            self.path = normalize(segment);
        } else {
            self.path = append_segment(&self.path, segment);
        }
    }

    pub fn parent(&self) -> Option<LocalPath> {
        if !self.has_parent() {
            return None;
        }
        let (root, mut segments) = split_root(&self.path);
        segments.pop();
        Some(LocalPath {
            path: join_root(&root, &segments),
        })
    }
}

fn normalize_separators(input: &str) -> String {
    input.replace('/', "\\")
}

fn has_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn is_unc_root(value: &str) -> bool {
    if !value.starts_with("\\\\") {
        return false;
    }
    let rest = value.trim_start_matches('\\').trim_end_matches('\\');
    !rest.is_empty() && !rest.contains('\\')
}

/// Split a normalized path into its root (including a trailing separator) and
/// its non-empty child segments.
fn split_root(input: &str) -> (String, Vec<String>) {
    if input.is_empty() {
        return (String::new(), Vec::new());
    }
    if let Some(rest) = input.strip_prefix("\\\\") {
        let mut parts = rest.split('\\').filter(|part| !part.is_empty());
        let server = parts.next().unwrap_or_default();
        let root = if server.is_empty() {
            "\\".to_string()
        } else {
            format!("\\\\{server}\\")
        };
        let segments = parts.map(str::to_string).collect();
        return (root, segments);
    }
    if let Some(rest) = input.strip_prefix('\\') {
        let segments = rest
            .split('\\')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect();
        return ("\\".to_string(), segments);
    }
    if has_drive_prefix(input) {
        let drive = &input[..2];
        let rest = &input[2..];
        let segments = rest
            .split('\\')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect();
        return (format!("{drive}\\"), segments);
    }
    (String::new(), Vec::new())
}

fn join_root(root: &str, segments: &[String]) -> String {
    if segments.is_empty() {
        return root.to_string();
    }
    let mut out = String::from(root);
    out.push_str(&segments.join("\\"));
    out.push('\\');
    out
}

fn normalize(input: &str) -> String {
    let normalized = normalize_separators(input);
    if normalized.is_empty() {
        return String::new();
    }
    let (root, segments) = split_root(&normalized);
    if root.is_empty() {
        return String::new();
    }
    let mut cleaned: Vec<String> = Vec::new();
    for segment in segments {
        match segment.as_str() {
            "" | "." => {}
            ".." => {
                cleaned.pop();
            }
            other => cleaned.push(other.to_string()),
        }
    }
    join_root(&root, &cleaned)
}

fn apply_relative(base: &str, relative: &str) -> String {
    let (root, mut segments) = split_root(base);
    if root.is_empty() {
        return normalize(relative);
    }
    for segment in relative.split('\\') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other.to_string()),
        }
    }
    join_root(&root, &segments)
}

fn append_segment(base: &str, segment: &str) -> String {
    let (root, mut segments) = split_root(base);
    if root.is_empty() {
        return normalize(segment);
    }
    segments.push(segment.to_string());
    join_root(&root, &segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpp_set_path_cases() {
        assert_eq!(LocalPath::new("\\").as_str(), "\\");

        assert_eq!(LocalPath::new("C:").as_str(), "C:\\");
        assert_eq!(LocalPath::new("C:\\").as_str(), "C:\\");
        assert_eq!(LocalPath::new("C:\\.").as_str(), "C:\\");
        assert_eq!(LocalPath::new("C:\\.\\").as_str(), "C:\\");
        assert_eq!(LocalPath::new("C:\\..").as_str(), "C:\\");
        assert_eq!(LocalPath::new("C:\\..\\").as_str(), "C:\\");
        assert_eq!(LocalPath::new("C:\\foo").as_str(), "C:\\foo\\");
        assert_eq!(LocalPath::new("C:\\..\\foo\\").as_str(), "C:\\foo\\");
        assert_eq!(LocalPath::new("C:\\foo\\..\\bar").as_str(), "C:\\bar\\");

        assert_eq!(LocalPath::new("\\\\foo").as_str(), "\\\\foo\\");
        assert_eq!(LocalPath::new("\\\\foo\\").as_str(), "\\\\foo\\");
        assert_eq!(LocalPath::new("\\\\foo/").as_str(), "\\\\foo\\");
        assert_eq!(LocalPath::new("\\\\foo/..").as_str(), "\\\\foo\\");
        assert_eq!(LocalPath::new("\\\\foo\\.").as_str(), "\\\\foo\\");
        assert_eq!(LocalPath::new("\\\\foo\\.\\").as_str(), "\\\\foo\\");
        assert_eq!(LocalPath::new("\\\\foo\\bar\\").as_str(), "\\\\foo\\bar\\");
        assert_eq!(LocalPath::new("\\\\foo\\bar\\.\\..").as_str(), "\\\\foo\\");
    }

    #[test]
    fn cpp_change_path_cases() {
        let mut path = LocalPath::new("C:\\");
        assert!(path.change_path("\\"));
        assert_eq!(path.as_str(), "\\");
        assert!(path.change_path("C:"));
        assert_eq!(path.as_str(), "C:\\");
        assert!(path.change_path("C:\\."));
        assert_eq!(path.as_str(), "C:\\");
        assert!(path.change_path("C:\\.."));
        assert_eq!(path.as_str(), "C:\\");
        assert!(path.change_path("foo"));
        assert_eq!(path.as_str(), "C:\\foo\\");
        assert!(path.change_path(".."));
        assert_eq!(path.as_str(), "C:\\");
        assert!(path.change_path(".."));
        assert_eq!(path.as_str(), "C:\\");
        assert!(path.change_path("C:\\foo"));
        assert_eq!(path.as_str(), "C:\\foo\\");
        assert!(path.change_path("."));
        assert_eq!(path.as_str(), "C:\\foo\\");
        assert!(path.change_path("..\\bar"));
        assert_eq!(path.as_str(), "C:\\bar\\");

        let mut unc = LocalPath::new("\\\\foo");
        assert_eq!(unc.as_str(), "\\\\foo\\");
        assert!(unc.change_path("."));
        assert_eq!(unc.as_str(), "\\\\foo\\");
        assert!(unc.change_path(".."));
        assert_eq!(unc.as_str(), "\\\\foo\\");
        assert!(unc.change_path("..\\bar\\.\\baz\\.."));
        assert_eq!(unc.as_str(), "\\\\foo\\bar\\");
    }

    #[test]
    fn cpp_parent_cases() {
        assert!(!LocalPath::new("\\").has_parent());
        assert!(!LocalPath::new("C:\\").has_parent());
        assert!(LocalPath::new("C:\\foo").has_parent());
        assert!(LocalPath::new("c:\\foo\\bar\\").has_parent());
        assert!(!LocalPath::new("\\\\foo").has_parent());
        assert!(LocalPath::new("\\\\foo\\bar").has_parent());

        assert!(!LocalPath::new("\\").has_logical_parent());
        assert!(LocalPath::new("C:\\").has_logical_parent());
        assert!(LocalPath::new("C:\\foo").has_logical_parent());
        assert!(LocalPath::new("c:\\foo\\bar\\").has_logical_parent());
        assert!(!LocalPath::new("\\\\foo").has_logical_parent());
        assert!(LocalPath::new("\\\\foo\\bar").has_logical_parent());
    }

    #[test]
    fn cpp_add_segment_cases() {
        let mut path = LocalPath::new("c:\\foo");
        let before = path.clone();
        path.add_segment("");
        assert_eq!(path, before);
    }

    #[test]
    fn parent_walks_to_root() {
        assert_eq!(
            LocalPath::new("C:\\foo\\bar").parent().unwrap().as_str(),
            "C:\\foo\\"
        );
        assert_eq!(LocalPath::new("C:\\foo").parent().unwrap().as_str(), "C:\\");
        assert!(LocalPath::new("C:\\").parent().is_none());
    }
}

/// Validate a single file-name component received from a remote server before
/// it is joined onto a local download directory.
///
/// A malicious server can send names such as `..\..\windows\x`, `C:\evil`,
/// `\\server\share`, `CON` or names containing control characters to escape
/// the chosen directory (zip-slip) or to hit Windows device paths.  Returns
/// `None` when the name cannot be used safely anywhere under a directory.
pub fn sanitize_remote_name(name: &str) -> Option<String> {
    // Windows silently strips trailing dots/spaces; normalise explicitly so
    // "a." and "a" cannot silently alias different remote entries.
    let trimmed = name.trim_end_matches([' ', '.']);
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return None;
    }
    // Separators, drive prefixes, NTFS alternate data streams and shell
    // glob/redirection characters.
    if trimmed.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return None;
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return None;
    }
    if trimmed.len() > 255 {
        return None;
    }
    // Windows reserved device names apply to the first dot-separated part
    // ("CON.txt" still opens the console device).
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = trimmed.split('.').next().unwrap_or_default().to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        return None;
    }
    Some(trimmed.to_string())
}

#[cfg(test)]
mod sanitize_tests {
    use super::sanitize_remote_name;

    #[test]
    fn accepts_ordinary_names() {
        assert_eq!(sanitize_remote_name("report.pdf").as_deref(), Some("report.pdf"));
        assert_eq!(
            sanitize_remote_name("my file (1).tar.gz").as_deref(),
            Some("my file (1).tar.gz")
        );
        assert_eq!(sanitize_remote_name("报告 2024").as_deref(), Some("报告 2024"));
    }

    #[test]
    fn rejects_traversal_and_absolute_forms() {
        assert_eq!(sanitize_remote_name(".."), None);
        assert_eq!(sanitize_remote_name("."), None);
        assert_eq!(sanitize_remote_name(""), None);
        assert_eq!(sanitize_remote_name("..\\..\\windows\\x"), None);
        assert_eq!(sanitize_remote_name("../../etc/passwd"), None);
        assert_eq!(sanitize_remote_name("C:\\evil"), None);
        assert_eq!(sanitize_remote_name("\\\\server\\share"), None);
        assert_eq!(sanitize_remote_name("/absolute"), None);
        assert_eq!(sanitize_remote_name("stream:evil"), None);
    }

    #[test]
    fn rejects_reserved_and_control_names() {
        assert_eq!(sanitize_remote_name("CON"), None);
        assert_eq!(sanitize_remote_name("con.txt"), None);
        assert_eq!(sanitize_remote_name("LPT9"), None);
        assert_eq!(sanitize_remote_name("nul.dat"), None);
        assert_eq!(sanitize_remote_name("bad\x00name"), None);
        assert_eq!(sanitize_remote_name("bad\nname"), None);
        assert_eq!(sanitize_remote_name("x<>|y"), None);
        assert_eq!(sanitize_remote_name("x*y?z"), None);
    }

    #[test]
    fn trims_windows_hostile_trailing() {
        // Windows would create "a." as "a" anyway; be explicit and consistent.
        assert_eq!(sanitize_remote_name("a. "), Some("a".to_string()));
        assert_eq!(sanitize_remote_name("a. . ."), Some("a".to_string()));
        assert_eq!(sanitize_remote_name(" . "), None);
    }
}
