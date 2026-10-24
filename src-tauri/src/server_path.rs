//! Port of FileZilla's `CServerPath` (src/engine/serverpath.cpp).
//!
//! A server path is not a local path: FTP servers can use UNIX syntax (and
//! lookalikes such as ZVM or Cygwin), VMS notation, DOS drive letters, MVS
//! datasets, VxWorks device prefixes, HP NonStop volume names or the virtual
//! DOS tree, and each of those has its own separator, enclosure and prefix
//! rules. This module keeps the original representation so that the exact
//! string formatting and equality semantics can be preserved:
//!
//! * [`ServerType`] - the flavour of path syntax.
//! * an optional prefix (for example the `//server` part of a Cygwin UNC path,
//!   a VMS node name or a DOS drive),
//! * a list of path segments.
//!
//! The public methods mirror the C++ API: `path`, `has_parent`, `parent`,
//! `format_subdir`, `common_parent`, `format_filename` and `change_path`.
//! Rust has no overloading, so the two C++ `ChangePath` overloads are exposed
//! as [`ServerPath::change_path`] (change to a directory) and
//! [`ServerPath::change_path_file`] (change to a file, updating the passed
//! subdir to the bare filename).
//!
//! The type names follow Rust conventions; the C++ spellings map as follows:
//! UNIX -> `Unix`, VMS -> `Vms`, DOS -> `Dos`, MVS -> `Mvs`, VXWORKS ->
//! `VxWorks`, ZVM -> `Zvm`, HPNONSTOP -> `HpNonStop`, DOS_VIRTUAL ->
//! `DosVirtual`, CYGWIN -> `Cygwin`, DOS_FWD_SLASHES -> `DosFwdSlashes`.
#![allow(dead_code)]

use std::fmt;

/// The kind of path syntax used by a server.
///
/// The discriminants intentionally match the C++ `ServerType` enum since the
/// corresponding traits table is indexed by them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(u8)]
pub enum ServerType {
    #[default]
    Unix = 0,
    Vms = 1,
    /// DOS with backslashes as preferred separator.
    Dos = 2,
    Mvs = 3,
    VxWorks = 4,
    /// Same as UNIX, only differs in the directory listing parser.
    Zvm = 5,
    HpNonStop = 6,
    DosVirtual = 7,
    Cygwin = 8,
    /// DOS with forward slashes as preferred separator.
    DosFwdSlashes = 9,
}

const NO_ENCLOSURE: char = '\0';

struct TypeTraits {
    /// All characters that separate segments. The first one is preferred.
    separators: &'static str,
    /// Root is simply the separator and nothing else.
    has_root: bool,
    left_enclosure: char,
    right_enclosure: char,
    /// MVS: the filename is placed inside the enclosure, e.g. `'FOO.BAR(BAZ)'`.
    filename_inside_enclosure: bool,
    /// 0 = normal prefix, 1 = suffix (MVS).
    prefixmode: u8,
    /// VMS escapes a dot in a segment as `^.`.
    separator_escape: Option<char>,
    /// `..` (parent) and `.` (self) have a special meaning.
    has_dots: bool,
    /// Cygwin: the prefix `//server` is followed by another separator.
    separator_after_prefix: bool,
}

impl TypeTraits {
    fn preferred_separator(&self) -> char {
        self.separators
            .chars()
            .next()
            .expect("a path type has a separator")
    }
}

/// Traits table, indexed by [`ServerType`]'s discriminant.
const TRAITS: [TypeTraits; 10] = [
    // UNIX
    TypeTraits {
        separators: "/",
        has_root: true,
        left_enclosure: NO_ENCLOSURE,
        right_enclosure: NO_ENCLOSURE,
        filename_inside_enclosure: false,
        prefixmode: 0,
        separator_escape: None,
        has_dots: true,
        separator_after_prefix: false,
    },
    // VMS
    TypeTraits {
        separators: ".",
        has_root: false,
        left_enclosure: '[',
        right_enclosure: ']',
        filename_inside_enclosure: false,
        prefixmode: 0,
        separator_escape: Some('^'),
        has_dots: false,
        separator_after_prefix: false,
    },
    // DOS, backslashes preferred
    TypeTraits {
        separators: "\\/",
        has_root: false,
        left_enclosure: NO_ENCLOSURE,
        right_enclosure: NO_ENCLOSURE,
        filename_inside_enclosure: false,
        prefixmode: 0,
        separator_escape: None,
        has_dots: true,
        separator_after_prefix: false,
    },
    // MVS
    TypeTraits {
        separators: ".",
        has_root: false,
        left_enclosure: '\'',
        right_enclosure: '\'',
        filename_inside_enclosure: true,
        prefixmode: 1,
        separator_escape: None,
        has_dots: false,
        separator_after_prefix: false,
    },
    // VxWorks
    TypeTraits {
        separators: "/",
        has_root: true,
        left_enclosure: NO_ENCLOSURE,
        right_enclosure: NO_ENCLOSURE,
        filename_inside_enclosure: false,
        prefixmode: 0,
        separator_escape: None,
        has_dots: true,
        separator_after_prefix: false,
    },
    // ZVM (same as UNIX)
    TypeTraits {
        separators: "/",
        has_root: true,
        left_enclosure: NO_ENCLOSURE,
        right_enclosure: NO_ENCLOSURE,
        filename_inside_enclosure: false,
        prefixmode: 0,
        separator_escape: None,
        has_dots: true,
        separator_after_prefix: false,
    },
    // HPNONSTOP
    TypeTraits {
        separators: ".",
        has_root: false,
        left_enclosure: NO_ENCLOSURE,
        right_enclosure: NO_ENCLOSURE,
        filename_inside_enclosure: false,
        prefixmode: 0,
        separator_escape: None,
        has_dots: false,
        separator_after_prefix: false,
    },
    // DOS_VIRTUAL
    TypeTraits {
        separators: "\\",
        has_root: true,
        left_enclosure: NO_ENCLOSURE,
        right_enclosure: NO_ENCLOSURE,
        filename_inside_enclosure: false,
        prefixmode: 0,
        separator_escape: None,
        has_dots: true,
        separator_after_prefix: false,
    },
    // CYGWIN
    TypeTraits {
        separators: "/",
        has_root: true,
        left_enclosure: NO_ENCLOSURE,
        right_enclosure: NO_ENCLOSURE,
        filename_inside_enclosure: false,
        prefixmode: 0,
        separator_escape: None,
        has_dots: true,
        separator_after_prefix: true,
    },
    // DOS with forward slashes preferred
    TypeTraits {
        separators: "/\\",
        has_root: false,
        left_enclosure: NO_ENCLOSURE,
        right_enclosure: NO_ENCLOSURE,
        filename_inside_enclosure: false,
        prefixmode: 0,
        separator_escape: None,
        has_dots: true,
        separator_after_prefix: false,
    },
];

impl ServerType {
    fn traits(self) -> &'static TypeTraits {
        &TRAITS[self as usize]
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ServerPathData {
    /// Prefix, if the representation has one (drive, node, `//server`, ...).
    prefix: Option<String>,
    segments: Vec<String>,
}

/// A path on a remote server.
///
/// An empty `ServerPath` stores the path type but no path data, matching the
/// C++ class.  Paths are parsed eagerly; `change_path` / `change_path_file`
/// mutate them, while every other accessor is read-only.
#[derive(Clone, Debug)]
pub struct ServerPath {
    data: Option<ServerPathData>,
    kind: ServerType,
}

impl Default for ServerPath {
    fn default() -> Self {
        Self {
            data: None,
            kind: ServerType::Unix,
        }
    }
}

impl PartialEq for ServerPath {
    fn eq(&self, other: &Self) -> bool {
        self.is_empty() == other.is_empty() && self.kind == other.kind && self.data == other.data
    }
}

impl Eq for ServerPath {}

impl fmt::Display for ServerPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.path())
    }
}

#[inline]
fn is_separator(t: &TypeTraits, c: char) -> bool {
    t.separators.contains(c)
}

/// Escape every separator character with the type's escape character.
///
/// Only VMS uses this (`.` becomes `^.`); all other types return the input
/// unchanged, just like `CServerPath::FormatSubdir`.
fn escape_separators(t: &TypeTraits, value: &str) -> String {
    let Some(escape) = t.separator_escape else {
        return value.to_string();
    };

    let preferred = t.preferred_separator();
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if is_separator(t, c) {
            out.push(escape);
            out.push(preferred);
        } else {
            out.push(c);
        }
    }
    out
}

fn find_separator_from(value: &str, t: &TypeTraits, from: usize) -> Option<usize> {
    value[from..]
        .char_indices()
        .find(|(_, c)| is_separator(t, *c))
        .map(|(i, _)| from + i)
}

/// Split `str` into segments, handling `.`/`..` and VMS escaping.
///
/// Returns `false` if a segment ends with the escape character but has no
/// following segment to append to (an incomplete `^` escape).
fn segmentize(t: &TypeTraits, value: &str, segments: &mut Vec<String>) -> bool {
    let mut append = false;
    let mut start = 0;

    while let Some(pos) = find_separator_from(value, t, start) {
        if start == pos {
            // Skip consecutive separators, like the C++ code does.
            start += 1;
            continue;
        }

        let segment = &value[start..pos];
        start = pos + 1;
        if !segmentize_add_segment(t, segment, segments, &mut append) {
            return false;
        }
    }

    if start < value.len() {
        let segment = &value[start..];
        if !segmentize_add_segment(t, segment, segments, &mut append) {
            return false;
        }
    }

    !append
}

fn segmentize_add_segment(
    t: &TypeTraits,
    segment: &str,
    segments: &mut Vec<String>,
    append: &mut bool,
) -> bool {
    if t.has_dots {
        if segment == "." {
            return true;
        }
        if segment == ".." {
            segments.pop();
            return true;
        }
    }

    let mut append_next = false;
    let mut escaped = String::new();
    let segment = match t.separator_escape {
        Some(escape) if segment.ends_with(escape) => {
            // The separator is escaped, so this segment continues in the next
            // one. Replace the trailing escape with the actual separator.
            append_next = true;
            escaped.push_str(&segment[..segment.len() - escape.len_utf8()]);
            escaped.push(t.preferred_separator());
            escaped.as_str()
        }
        _ => segment,
    };

    if *append {
        if let Some(last) = segments.last_mut() {
            last.push_str(segment);
        }
    } else {
        segments.push(segment.to_string());
    }

    *append = append_next;
    true
}

/// Split the last segment off `dir` and store it in `file`.
///
/// Returns `false` when `dir` ends with a separator, which means there is no
/// filename to extract.
fn extract_file(t: &TypeTraits, dir: &mut String, file: &mut String) -> bool {
    let pos = dir.rfind(|c: char| is_separator(t, c));

    match pos {
        Some(pos) if pos == dir.len() - 1 => false,
        Some(pos) => {
            *file = dir[pos + 1..].to_string();
            dir.truncate(pos + 1);
            true
        }
        None => {
            *file = std::mem::take(dir);
            true
        }
    }
}

fn segments_equal(a: &str, b: &str, ignore_case: bool) -> bool {
    if ignore_case {
        a.to_lowercase() == b.to_lowercase()
    } else {
        a == b
    }
}

impl ServerPath {
    /// Parse `value` using the syntax of `kind`.
    ///
    /// An invalid path produces an empty `ServerPath` of `kind`, exactly like
    /// the C++ constructor.
    pub fn new(value: &str, kind: ServerType) -> Self {
        let mut path = Self { data: None, kind };
        if !value.is_empty() {
            let _ = path.change_path(value);
        }
        path
    }

    /// Create an empty path for `kind`.
    pub fn empty(kind: ServerType) -> Self {
        Self { data: None, kind }
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_none()
    }

    pub fn clear(&mut self) {
        self.data = None;
    }

    /// The path type used by this object.
    pub fn kind(&self) -> ServerType {
        self.kind
    }

    /// Set the path type. Fails if the object is non-empty and has another
    /// type already, matching `CServerPath::SetType`.
    pub fn set_type(&mut self, kind: ServerType) -> bool {
        if !self.is_empty() && self.kind != kind {
            return false;
        }
        self.kind = kind;
        true
    }

    /// Number of path segments (0 for empty paths).
    pub fn segment_count(&self) -> usize {
        self.data.as_ref().map_or(0, |data| data.segments.len())
    }

    /// Render the path in server syntax (`CServerPath::GetPath`).
    pub fn path(&self) -> String {
        let Some(data) = self.data.as_ref() else {
            return String::new();
        };
        let t = self.kind.traits();
        let separator = t.preferred_separator();
        let mut path = String::new();

        if t.prefixmode == 0 {
            if let Some(prefix) = &data.prefix {
                path.push_str(prefix);
            }
        }

        if t.left_enclosure != NO_ENCLOSURE {
            path.push(t.left_enclosure);
        }

        if data.segments.is_empty()
            && (!t.has_root || data.prefix.is_none() || t.separator_after_prefix)
        {
            path.push(separator);
        }

        for (index, segment) in data.segments.iter().enumerate() {
            if index != 0 {
                path.push(separator);
            } else if t.has_root && (data.prefix.is_none() || t.separator_after_prefix) {
                path.push(separator);
            }

            if t.separator_escape.is_some() {
                path.push_str(&escape_separators(t, segment));
            } else {
                path.push_str(segment);
            }
        }

        if t.prefixmode == 1 {
            if let Some(prefix) = &data.prefix {
                path.push_str(prefix);
            }
        }

        if t.right_enclosure != NO_ENCLOSURE {
            path.push(t.right_enclosure);
        }

        // DOS is strange: "C:" is the current working directory on drive C,
        // while "C:\" is the drive root.
        if matches!(self.kind, ServerType::Dos | ServerType::DosFwdSlashes)
            && data.segments.len() == 1
        {
            path.push(separator);
        }

        path
    }

    /// Whether this path has a parent directory that can be navigated to.
    pub fn has_parent(&self) -> bool {
        let Some(data) = self.data.as_ref() else {
            return false;
        };

        if !self.kind.traits().has_root {
            return data.segments.len() > 1;
        }

        !data.segments.is_empty()
    }

    /// The parent directory. An empty path is returned when the path has no
    /// parent (for example a root directory).
    pub fn parent(&self) -> ServerPath {
        let mut parent = self.clone();
        parent.make_parent();
        parent
    }

    /// In-place version of [`ServerPath::parent`].
    pub fn make_parent(&mut self) -> &mut Self {
        if self.is_empty() || !self.has_parent() {
            self.clear();
            return self;
        }

        let kind = self.kind;
        if let Some(data) = self.data.as_mut() {
            data.segments.pop();

            // MVS parents are partitioned data sets and use the `.` suffix.
            if kind == ServerType::Mvs {
                data.prefix = Some(".".to_string());
            }
        }

        self
    }

    /// Format a subdirectory name for use in a path (`FormatSubdir`).
    ///
    /// Only VMS escapes anything; all other path types return the argument
    /// unchanged.
    pub fn format_subdir(&self, subdir: &str) -> String {
        escape_separators(self.kind.traits(), subdir)
    }

    /// Whether this path is a subdirectory of `path`.
    pub fn is_subdir_of(&self, path: &ServerPath, ignore_case: bool, allow_equal: bool) -> bool {
        if self.is_empty() || path.is_empty() {
            return false;
        }
        if self.kind != path.kind {
            return false;
        }
        if !self.has_parent() {
            return false;
        }

        let ld = self.data.as_ref().expect("non-empty paths have data");
        let rd = path.data.as_ref().expect("non-empty paths have data");
        let t = self.kind.traits();

        if t.prefixmode != 1 {
            if ignore_case {
                match (&ld.prefix, &rd.prefix) {
                    (Some(left), Some(right)) => {
                        if !segments_equal(left, right, true) {
                            return false;
                        }
                    }
                    (None, None) => {}
                    _ => return false,
                }
            } else if ld.prefix != rd.prefix {
                return false;
            }
        }

        // On MVS, directories like 'FOO.BAR' without a trailing dot cannot
        // have subdirectories.
        if t.prefixmode == 1 && rd.prefix.is_none() {
            return false;
        }

        let mut index = 0;
        while index < ld.segments.len() {
            if index >= rd.segments.len() {
                return true;
            }
            if !segments_equal(&ld.segments[index], &rd.segments[index], ignore_case) {
                return false;
            }
            index += 1;
        }

        allow_equal && index == rd.segments.len()
    }

    /// Lowest common ancestor of two paths (`GetCommonParent`).
    ///
    /// Returns an empty path when there is no common parent. The returned
    /// path keeps this path's type.
    pub fn common_parent(&self, path: &ServerPath) -> ServerPath {
        if self == path {
            return self.clone();
        }
        if self.is_empty() || path.is_empty() {
            return ServerPath::default();
        }

        let ld = self.data.as_ref().expect("non-empty paths have data");
        let rd = path.data.as_ref().expect("non-empty paths have data");
        let t = self.kind.traits();

        if self.kind != path.kind || (t.prefixmode != 1 && ld.prefix != rd.prefix) {
            return ServerPath::default();
        }

        if !self.has_parent() {
            return if path.is_subdir_of(self, false, false) {
                self.clone()
            } else {
                ServerPath::default()
            };
        }
        if !path.has_parent() {
            return if self.is_subdir_of(path, false, false) {
                path.clone()
            } else {
                ServerPath::default()
            };
        }

        // On MVS the prefix of the common parent comes from this path's
        // parent (which sets the trailing dot); other types keep the prefix
        // they already have.
        let parent_prefix = if t.prefixmode == 1 {
            self.parent().data.and_then(|data| data.prefix)
        } else {
            ld.prefix.clone()
        };

        // On MVS a path without a trailing dot ends in a data set name, which
        // cannot itself be a directory; exclude that last segment.
        let last = if t.prefixmode == 1 && ld.prefix.is_none() {
            ld.segments.len() - 1
        } else {
            ld.segments.len()
        };
        let last2 = if t.prefixmode == 1 && rd.prefix.is_none() {
            rd.segments.len() - 1
        } else {
            rd.segments.len()
        };

        let mut segments = Vec::new();
        let mut left = 0;
        let mut right = 0;
        while left < last && right < last2 {
            if ld.segments[left] != rd.segments[right] {
                // Paths without a root separator need at least one shared
                // segment; otherwise there is no common parent.
                if !t.has_root && segments.is_empty() {
                    return ServerPath::default();
                }
                return Self::from_parts(parent_prefix, segments, self.kind);
            }

            segments.push(ld.segments[left].clone());
            left += 1;
            right += 1;
        }

        Self::from_parts(parent_prefix, segments, self.kind)
    }

    fn from_parts(prefix: Option<String>, segments: Vec<String>, kind: ServerType) -> Self {
        Self {
            data: Some(ServerPathData { prefix, segments }),
            kind,
        }
    }

    /// Format a filename for this directory (`FormatFilename`).
    ///
    /// `omit_path` is only a hint: MVS data set member names always use
    /// absolute filenames including the full path.
    pub fn format_filename(&self, filename: &str, omit_path: bool) -> String {
        if self.is_empty() || filename.is_empty() {
            return filename.to_string();
        }

        let data = self.data.as_ref().expect("non-empty paths have data");
        let t = self.kind.traits();

        if omit_path && (t.prefixmode != 1 || data.prefix.as_deref() == Some(".")) {
            return filename.to_string();
        }

        let mut result = self.path();
        if t.left_enclosure != NO_ENCLOSURE && t.filename_inside_enclosure {
            // MVS: remove the closing quote so the filename can be placed
            // inside the enclosure.
            result.pop();
        }

        match self.kind {
            ServerType::VxWorks => {
                if !result.is_empty()
                    && !result.chars().last().is_some_and(|c| is_separator(t, c))
                    && !data.segments.is_empty()
                {
                    result.push(t.preferred_separator());
                }
            }
            ServerType::Vms | ServerType::Mvs => {}
            _ => {
                if !result.is_empty() && !result.chars().last().is_some_and(|c| is_separator(t, c))
                {
                    result.push(t.preferred_separator());
                }
            }
        }

        if t.prefixmode == 1 && data.prefix.is_none() {
            // MVS partitioned data set member: 'FOO.BAR(BAZ)'.
            result.push('(');
            result.push_str(filename);
            result.push(')');
        } else {
            result.push_str(filename);
        }

        if t.left_enclosure != NO_ENCLOSURE && t.filename_inside_enclosure {
            result.push(t.right_enclosure);
        }

        result
    }

    /// Change to a subdirectory (`ChangePath(subdir)`).
    ///
    /// On failure the path is cleared, matching the C++ contract.
    pub fn change_path(&mut self, subdir: &str) -> bool {
        match self.do_change_path(subdir, false) {
            Ok(_) => true,
            Err(()) => {
                self.clear();
                false
            }
        }
    }

    /// Change to a file (`ChangePath(subdir, true)`).
    ///
    /// On success `subdir` is replaced by the bare filename. On failure the
    /// path is cleared and `subdir` is left untouched.
    pub fn change_path_file(&mut self, subdir: &mut String) -> bool {
        match self.do_change_path(subdir, true) {
            Ok(file) => {
                *subdir = file;
                true
            }
            Err(()) => {
                self.clear();
                false
            }
        }
    }

    fn do_change_path(&mut self, subdir: &str, is_file: bool) -> Result<String, ()> {
        let mut dir = subdir.to_string();
        let mut file = String::new();

        if dir.is_empty() {
            if self.is_empty() || is_file {
                return Err(());
            }
            return Ok(file);
        }

        let was_empty = self.is_empty();
        if self.data.is_none() {
            self.data = Some(ServerPathData::default());
        }

        let kind = self.kind;
        let t = kind.traits();
        let data = self.data.as_mut().expect("data was just created");
        let first = dir.chars().next().expect("dir is not empty");

        match kind {
            ServerType::Vms => {
                let Some(left) = dir.find(t.left_enclosure) else {
                    if dir.rfind(t.right_enclosure).is_some() {
                        return Err(());
                    }

                    if is_file {
                        if was_empty {
                            return Err(());
                        }

                        // A bare filename: there is nothing to segmentize,
                        // but the resulting path still needs segments.
                        if !t.has_root && data.segments.is_empty() {
                            return Err(());
                        }
                        return Ok(dir);
                    }

                    if !segmentize(t, &dir, &mut data.segments) {
                        return Err(());
                    }
                    if data.segments.is_empty() && was_empty {
                        return Err(());
                    }
                    return finish(is_file, t, data, file);
                };

                let Some(right) = dir.rfind(t.right_enclosure) else {
                    return Err(());
                };
                if right <= left + 1 {
                    return Err(());
                }

                let enclosure_is_last = right == dir.len() - 1;
                // A file needs a name after the enclosure; a directory must
                // end with the enclosure.
                if is_file == enclosure_is_last {
                    return Err(());
                }

                if is_file {
                    file = dir[right + 1..].to_string();
                }
                dir.truncate(right);

                if left > 0 {
                    data.prefix = Some(dir[..left].to_string());
                }
                dir = dir[left + 1..].to_string();
                data.segments.clear();

                if !segmentize(t, &dir, &mut data.segments) {
                    return Err(());
                }
                if data.segments.is_empty() && was_empty {
                    return Err(());
                }
            }
            ServerType::Dos | ServerType::DosFwdSlashes => {
                let mut is_absolute = false;
                let sep = dir.find(|c: char| is_separator(t, c)).unwrap_or(dir.len());
                let colon = dir.find(':');

                if colon == Some(0) {
                    return Err(());
                }
                if let Some(colon) = colon {
                    if colon + 1 == sep {
                        is_absolute = true;
                    } else if colon > sep {
                        // Malformed, e.g. `c\bad:`.
                        return Err(());
                    }

                    if colon + 1 < sep {
                        // Relative to the last working directory of the given
                        // drive, but we do not know that directory.
                        if data.segments.is_empty()
                            || data.segments[0].as_str() != &dir[..colon + 1]
                        {
                            return Err(());
                        }
                        dir = dir[colon + 1..].to_string();
                    }
                }

                if is_absolute {
                    data.segments.clear();
                } else if is_separator(t, first) {
                    // Relative to the root of the current drive.
                    if data.segments.is_empty() {
                        return Err(());
                    }
                    let first_segment = data.segments[0].clone();
                    data.segments.clear();
                    data.segments.push(first_segment);
                    dir = dir[1..].to_string();
                }

                if is_file && !extract_file(t, &mut dir, &mut file) {
                    return Err(());
                }
                if !segmentize(t, &dir, &mut data.segments) {
                    return Err(());
                }
                if data.segments.is_empty() && was_empty {
                    return Err(());
                }
            }
            ServerType::Mvs => {
                // Remove the double quotation marks some servers send in a
                // PWD reply. 0xDC is the EBCDIC representation of `"`.
                while dir.starts_with('\u{00DC}') {
                    dir.remove(0);
                }
                while dir.ends_with('\u{00DC}') {
                    dir.pop();
                }
                if dir.is_empty() {
                    return Err(());
                }

                if dir.starts_with(t.left_enclosure) {
                    if !dir.ends_with(t.right_enclosure) {
                        return Err(());
                    }
                    dir = if dir.len() >= 2 {
                        dir[1..dir.len() - 1].to_string()
                    } else {
                        String::new()
                    };
                    data.segments.clear();
                } else if dir.ends_with(t.right_enclosure) {
                    return Err(());
                } else if was_empty {
                    return Err(());
                }

                if !dir.is_empty() && dir.ends_with(')') {
                    // Partitioned data set member.
                    if !is_file {
                        return Err(());
                    }

                    let Some(pos) = dir.find('(') else {
                        return Err(());
                    };
                    dir.pop();
                    file = dir[pos + 1..].to_string();
                    dir.truncate(pos);

                    if !was_empty && data.prefix.is_none() && !dir.is_empty() {
                        return Err(());
                    }

                    data.prefix = None;
                } else {
                    if !was_empty && data.prefix.is_none() {
                        if dir.contains('.') || !is_file {
                            return Err(());
                        }
                    }

                    if is_file {
                        if !extract_file(t, &mut dir, &mut file) {
                            return Err(());
                        }
                        data.prefix = Some(".".to_string());
                    } else if !dir.is_empty() && dir.ends_with('.') {
                        data.prefix = Some(".".to_string());
                    } else {
                        data.prefix = None;
                    }
                }

                if !segmentize(t, &dir, &mut data.segments) {
                    return Err(());
                }
            }
            ServerType::HpNonStop => {
                if dir.starts_with('\\') {
                    data.segments.clear();
                }

                if is_file && !extract_file(t, &mut dir, &mut file) {
                    return Err(());
                }
                if !segmentize(t, &dir, &mut data.segments) {
                    return Err(());
                }
                if data.segments.is_empty() && was_empty {
                    return Err(());
                }
            }
            ServerType::VxWorks => {
                if dir.starts_with(':') {
                    let Some(colon2) = dir[1..].find(':').map(|pos| pos + 1) else {
                        return Err(());
                    };
                    if colon2 == 1 {
                        return Err(());
                    }

                    data.prefix = Some(dir[..colon2 + 1].to_string());
                    dir = dir[colon2 + 1..].to_string();
                    data.segments.clear();
                } else if was_empty {
                    return Err(());
                }

                if is_file && !extract_file(t, &mut dir, &mut file) {
                    return Err(());
                }
                if !segmentize(t, &dir, &mut data.segments) {
                    return Err(());
                }
            }
            ServerType::Cygwin => {
                if is_separator(t, first) {
                    data.segments.clear();
                    data.prefix = None;
                } else if was_empty {
                    return Err(());
                }

                // Optional UNC-style prefix `//server`.
                if dir.starts_with("//") {
                    data.prefix = Some(t.preferred_separator().to_string());
                    dir = dir[1..].to_string();
                }

                if is_file && !extract_file(t, &mut dir, &mut file) {
                    return Err(());
                }
                if !segmentize(t, &dir, &mut data.segments) {
                    return Err(());
                }
            }
            // UNIX and ZVM.
            _ => {
                if is_separator(t, first) {
                    data.segments.clear();
                } else if was_empty {
                    return Err(());
                }

                if is_file && !extract_file(t, &mut dir, &mut file) {
                    return Err(());
                }
                if !segmentize(t, &dir, &mut data.segments) {
                    return Err(());
                }
            }
        }

        finish(is_file, t, data, file)
    }
}

/// Common post-processing at the end of `DoChangePath`.
fn finish(
    is_file: bool,
    t: &TypeTraits,
    data: &ServerPathData,
    file: String,
) -> Result<String, ()> {
    if !t.has_root && data.segments.is_empty() {
        return Err(());
    }

    if is_file && t.has_dots && (file == ".." || file == ".") {
        return Err(());
    }

    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::{ServerPath, ServerType};

    fn unix(path: &str) -> ServerPath {
        ServerPath::new(path, ServerType::Unix)
    }

    #[test]
    fn get_path() {
        assert_eq!(unix("/").path(), "/");
        assert_eq!(unix("/foo").path(), "/foo");
        assert_eq!(unix("/foo/bar").path(), "/foo/bar");

        assert_eq!(
            ServerPath::new("FOO:[BAR]", ServerType::Vms).path(),
            "FOO:[BAR]"
        );
        assert_eq!(
            ServerPath::new("FOO:[BAR.TEST]", ServerType::Vms).path(),
            "FOO:[BAR.TEST]"
        );
        assert_eq!(
            ServerPath::new("FOO:[BAR^.TEST]", ServerType::Vms).path(),
            "FOO:[BAR^.TEST]"
        );
        assert_eq!(
            ServerPath::new("FOO:[BAR^.TEST.SOMETHING]", ServerType::Vms).path(),
            "FOO:[BAR^.TEST.SOMETHING]"
        );

        assert_eq!(ServerPath::new("C:\\", ServerType::Dos).path(), "C:\\");
        assert_eq!(
            ServerPath::new("C:\\FOO", ServerType::Dos).path(),
            "C:\\FOO"
        );
        assert_eq!(ServerPath::new("md:\\", ServerType::Dos).path(), "md:\\");
        assert_eq!(ServerPath::new("C:", ServerType::Dos).path(), "C:\\");
        assert_eq!(
            ServerPath::new("C:\\FOO\\", ServerType::Dos).path(),
            "C:\\FOO"
        );
        assert!(ServerPath::new("C:NOTABSOLUTE\\", ServerType::Dos).is_empty());

        assert_eq!(ServerPath::new("'FOO'", ServerType::Mvs).path(), "'FOO'");
        assert_eq!(ServerPath::new("'FOO.'", ServerType::Mvs).path(), "'FOO.'");
        assert_eq!(
            ServerPath::new("'FOO.BAR'", ServerType::Mvs).path(),
            "'FOO.BAR'"
        );
        assert_eq!(
            ServerPath::new("'FOO.BAR.'", ServerType::Mvs).path(),
            "'FOO.BAR.'"
        );

        assert_eq!(
            ServerPath::new(":foo:", ServerType::VxWorks).path(),
            ":foo:"
        );
        assert_eq!(
            ServerPath::new(":foo:bar", ServerType::VxWorks).path(),
            ":foo:bar"
        );
        assert_eq!(
            ServerPath::new(":foo:bar/test", ServerType::VxWorks).path(),
            ":foo:bar/test"
        );

        // ZVM is same as Unix, only makes difference in directory listing
        // parser.
        assert_eq!(ServerPath::new("/", ServerType::Zvm).path(), "/");
        assert_eq!(ServerPath::new("/foo", ServerType::Zvm).path(), "/foo");
        assert_eq!(
            ServerPath::new("/foo/bar", ServerType::Zvm).path(),
            "/foo/bar"
        );

        assert_eq!(
            ServerPath::new("\\mysys", ServerType::HpNonStop).path(),
            "\\mysys"
        );
        assert_eq!(
            ServerPath::new("\\mysys.$myvol", ServerType::HpNonStop).path(),
            "\\mysys.$myvol"
        );
        assert_eq!(
            ServerPath::new("\\mysys.$myvol.mysubvol", ServerType::HpNonStop).path(),
            "\\mysys.$myvol.mysubvol"
        );

        assert_eq!(ServerPath::new("\\", ServerType::DosVirtual).path(), "\\");
        assert_eq!(
            ServerPath::new("\\foo", ServerType::DosVirtual).path(),
            "\\foo"
        );
        assert_eq!(
            ServerPath::new("\\foo\\bar", ServerType::DosVirtual).path(),
            "\\foo\\bar"
        );

        assert_eq!(ServerPath::new("/", ServerType::Cygwin).path(), "/");
        assert_eq!(ServerPath::new("/foo", ServerType::Cygwin).path(), "/foo");
        assert_eq!(ServerPath::new("//", ServerType::Cygwin).path(), "//");
        assert_eq!(ServerPath::new("//foo", ServerType::Cygwin).path(), "//foo");

        assert_eq!(
            ServerPath::new("C:\\", ServerType::DosFwdSlashes).path(),
            "C:/"
        );
        assert_eq!(
            ServerPath::new("C:\\FOO", ServerType::DosFwdSlashes).path(),
            "C:/FOO"
        );
        assert_eq!(
            ServerPath::new("md:\\", ServerType::DosFwdSlashes).path(),
            "md:/"
        );
        assert_eq!(
            ServerPath::new("C:", ServerType::DosFwdSlashes).path(),
            "C:/"
        );
        assert_eq!(
            ServerPath::new("C:\\FOO\\", ServerType::DosFwdSlashes).path(),
            "C:/FOO"
        );
    }

    #[test]
    fn has_parent() {
        assert!(!unix("/").has_parent());
        assert!(unix("/foo").has_parent());
        assert!(unix("/foo/bar").has_parent());

        assert!(!ServerPath::new("FOO:[BAR]", ServerType::Vms).has_parent());
        assert!(ServerPath::new("FOO:[BAR.TEST]", ServerType::Vms).has_parent());
        assert!(!ServerPath::new("FOO:[BAR^.TEST]", ServerType::Vms).has_parent());
        assert!(ServerPath::new("FOO:[BAR^.TEST.SOMETHING]", ServerType::Vms).has_parent());

        assert!(!ServerPath::new("C:\\", ServerType::Dos).has_parent());
        assert!(ServerPath::new("C:\\FOO", ServerType::Dos).has_parent());

        assert!(!ServerPath::new("'FOO'", ServerType::Mvs).has_parent());
        assert!(!ServerPath::new("'FOO.'", ServerType::Mvs).has_parent());
        assert!(ServerPath::new("'FOO.BAR'", ServerType::Mvs).has_parent());
        assert!(ServerPath::new("'FOO.BAR.'", ServerType::Mvs).has_parent());

        assert!(!ServerPath::new(":foo:", ServerType::VxWorks).has_parent());
        assert!(ServerPath::new(":foo:bar", ServerType::VxWorks).has_parent());
        assert!(ServerPath::new(":foo:bar/test", ServerType::VxWorks).has_parent());

        // ZVM is same as Unix, only makes difference in directory listing
        // parser.
        assert!(!ServerPath::new("/", ServerType::Zvm).has_parent());
        assert!(ServerPath::new("/foo", ServerType::Zvm).has_parent());
        assert!(ServerPath::new("/foo/bar", ServerType::Zvm).has_parent());

        assert!(!ServerPath::new("\\mysys", ServerType::HpNonStop).has_parent());
        assert!(ServerPath::new("\\mysys.$myvol", ServerType::HpNonStop).has_parent());
        assert!(ServerPath::new("\\mysys.$myvol.mysubvol", ServerType::HpNonStop).has_parent());

        assert!(!ServerPath::new("\\", ServerType::DosVirtual).has_parent());
        assert!(ServerPath::new("\\foo", ServerType::DosVirtual).has_parent());
        assert!(ServerPath::new("\\foo\\bar", ServerType::DosVirtual).has_parent());

        assert!(!ServerPath::new("/", ServerType::Cygwin).has_parent());
        assert!(ServerPath::new("/foo", ServerType::Cygwin).has_parent());
        assert!(ServerPath::new("/foo/bar", ServerType::Cygwin).has_parent());
        assert!(!ServerPath::new("//", ServerType::Cygwin).has_parent());
        assert!(ServerPath::new("//foo", ServerType::Cygwin).has_parent());
        assert!(ServerPath::new("//foo/bar", ServerType::Cygwin).has_parent());

        assert!(!ServerPath::new("C:\\", ServerType::DosFwdSlashes).has_parent());
        assert!(ServerPath::new("C:\\FOO", ServerType::DosFwdSlashes).has_parent());
    }

    #[test]
    fn get_parent() {
        let unix1 = unix("/");
        let unix2 = unix("/foo");
        let unix3 = unix("/foo/bar");
        assert_eq!(unix2.parent(), unix1);
        assert_eq!(unix3.parent(), unix2);

        let vms1 = ServerPath::new("FOO:[BAR]", ServerType::Vms);
        let vms2 = ServerPath::new("FOO:[BAR.TEST]", ServerType::Vms);
        let vms3 = ServerPath::new("FOO:[BAR^.TEST]", ServerType::Vms);
        let vms4 = ServerPath::new("FOO:[BAR^.TEST.SOMETHING]", ServerType::Vms);
        assert_eq!(vms2.parent(), vms1);
        assert_eq!(vms4.parent(), vms3);

        let dos1 = ServerPath::new("C:\\", ServerType::Dos);
        let dos2 = ServerPath::new("C:\\FOO", ServerType::Dos);
        assert_eq!(dos2.parent(), dos1);

        let _mvs1 = ServerPath::new("'FOO'", ServerType::Mvs);
        let mvs2 = ServerPath::new("'FOO.'", ServerType::Mvs);
        let mvs3 = ServerPath::new("'FOO.BAR'", ServerType::Mvs);
        let mvs4 = ServerPath::new("'FOO.BAR.'", ServerType::Mvs);
        assert_eq!(mvs3.parent(), mvs2);
        assert_eq!(mvs4.parent(), mvs2);

        // The C++ test deliberately types these as DOS_VIRTUAL; relative
        // paths of that type cannot be parsed, so the three paths are empty
        // and their (empty) parents compare equal.
        let vxworks1 = ServerPath::new(":foo:", ServerType::DosVirtual);
        let vxworks2 = ServerPath::new(":foo:bar", ServerType::DosVirtual);
        let vxworks3 = ServerPath::new(":foo:bar/test", ServerType::DosVirtual);
        assert_eq!(vxworks2.parent(), vxworks1);
        assert_eq!(vxworks3.parent(), vxworks2);

        // ZVM is same as Unix, only makes difference in directory listing
        // parser.
        let zvm1 = ServerPath::new("/", ServerType::Zvm);
        let zvm2 = ServerPath::new("/foo", ServerType::Zvm);
        let zvm3 = ServerPath::new("/foo/bar", ServerType::Zvm);
        assert_eq!(zvm2.parent(), zvm1);
        assert_eq!(zvm3.parent(), zvm2);

        let hpnonstop1 = ServerPath::new("\\mysys", ServerType::HpNonStop);
        let hpnonstop2 = ServerPath::new("\\mysys.$myvol", ServerType::HpNonStop);
        let hpnonstop3 = ServerPath::new("\\mysys.$myvol.mysubvol", ServerType::HpNonStop);
        assert_eq!(hpnonstop2.parent(), hpnonstop1);
        assert_eq!(hpnonstop3.parent(), hpnonstop2);

        let dos_virtual1 = ServerPath::new("\\", ServerType::DosVirtual);
        let dos_virtual2 = ServerPath::new("\\foo", ServerType::DosVirtual);
        let dos_virtual3 = ServerPath::new("\\foo\\bar", ServerType::DosVirtual);
        assert_eq!(dos_virtual2.parent(), dos_virtual1);
        assert_eq!(dos_virtual3.parent(), dos_virtual2);

        let cygwin1 = ServerPath::new("/", ServerType::Cygwin);
        let cygwin2 = ServerPath::new("/foo", ServerType::Cygwin);
        let cygwin3 = ServerPath::new("/foo/bar", ServerType::Cygwin);
        let cygwin4 = ServerPath::new("//", ServerType::Cygwin);
        let cygwin5 = ServerPath::new("//foo", ServerType::Cygwin);
        let cygwin6 = ServerPath::new("//foo/bar", ServerType::Cygwin);
        assert_eq!(cygwin2.parent(), cygwin1);
        assert_eq!(cygwin3.parent(), cygwin2);
        assert_eq!(cygwin5.parent(), cygwin4);
        assert_eq!(cygwin6.parent(), cygwin5);

        let dos_slashes1 = ServerPath::new("C:\\", ServerType::DosFwdSlashes);
        let dos_slashes2 = ServerPath::new("C:/FOO", ServerType::DosFwdSlashes);
        assert_eq!(dos_slashes2.parent(), dos_slashes1);
    }

    #[test]
    fn format_subdir() {
        let mut path = ServerPath::default();
        assert!(path.set_type(ServerType::Vms));

        assert_eq!(path.format_subdir("FOO.BAR"), "FOO^.BAR");
    }

    #[test]
    fn get_common_parent() {
        let unix1 = unix("/foo");
        let unix2 = unix("/foo/baz");
        let unix3 = unix("/foo/bar");
        assert_eq!(unix2.common_parent(&unix3), unix1);
        assert_eq!(unix1.common_parent(&unix1), unix1);
        assert_eq!(unix1.common_parent(&unix3), unix1);

        let vms1 = ServerPath::new("FOO:[BAR]", ServerType::Vms);
        let vms2 = ServerPath::new("FOO:[BAR.TEST]", ServerType::Vms);
        let vms3 = ServerPath::new("GOO:[BAR", ServerType::Vms);
        let vms4 = ServerPath::new("GOO:[BAR^.TEST.SOMETHING]", ServerType::Vms);
        assert_eq!(vms2.common_parent(&vms1), vms1);
        assert_eq!(vms3.common_parent(&vms1), ServerPath::default());
        // The C++ test creates vms4 but never uses it; keep the binding so
        // the port mirrors the original test.
        let _ = &vms4;

        {
            let dos1 = ServerPath::new("C:\\", ServerType::Dos);
            let dos2 = ServerPath::new("C:\\FOO", ServerType::Dos);
            let dos3 = ServerPath::new("D:\\FOO", ServerType::Dos);
            assert_eq!(dos1.common_parent(&dos2), dos1);
            assert_eq!(dos2.common_parent(&dos3), ServerPath::default());
        }

        let mvs1 = ServerPath::new("'FOO'", ServerType::Mvs);
        let mvs2 = ServerPath::new("'FOO.'", ServerType::Mvs);
        let mvs3 = ServerPath::new("'FOO.BAR'", ServerType::Mvs);
        let mvs4 = ServerPath::new("'FOO.BAR.'", ServerType::Mvs);
        let mvs5 = ServerPath::new("'BAR.'", ServerType::Mvs);
        let mvs6 = ServerPath::new("'FOO.BAR.BAZ'", ServerType::Mvs);
        assert_eq!(mvs1.common_parent(&mvs2), ServerPath::default());
        assert_eq!(mvs3.common_parent(&mvs1), ServerPath::default());
        assert_eq!(mvs4.common_parent(&mvs3), mvs2);
        assert_eq!(mvs3.common_parent(&mvs4), mvs2);
        assert_eq!(mvs3.common_parent(&mvs2), mvs2);
        assert_eq!(mvs4.common_parent(&mvs2), mvs2);
        assert_eq!(mvs5.common_parent(&mvs2), ServerPath::default());
        assert_eq!(mvs6.common_parent(&mvs4), mvs4);
        assert_eq!(mvs6.common_parent(&mvs3), mvs2);

        let vxworks1 = ServerPath::new(":foo:", ServerType::VxWorks);
        let vxworks2 = ServerPath::new(":foo:bar", ServerType::VxWorks);
        let vxworks3 = ServerPath::new(":foo:baz", ServerType::VxWorks);
        let vxworks4 = ServerPath::new(":baz:bar", ServerType::VxWorks);
        assert_eq!(vxworks1.common_parent(&vxworks2), vxworks1);
        assert_eq!(vxworks2.common_parent(&vxworks3), vxworks1);
        assert_eq!(vxworks4.common_parent(&vxworks2), ServerPath::default());

        // ZVM is same as Unix, only makes difference in directory listing
        // parser.
        let zvm1 = ServerPath::new("/foo", ServerType::Zvm);
        let zvm2 = ServerPath::new("/foo/baz", ServerType::Zvm);
        let zvm3 = ServerPath::new("/foo/bar", ServerType::Zvm);
        assert_eq!(zvm2.common_parent(&zvm3), zvm1);
        assert_eq!(zvm1.common_parent(&zvm1), zvm1);

        let hpnonstop1 = ServerPath::new("\\mysys", ServerType::HpNonStop);
        let hpnonstop2 = ServerPath::new("\\mysys.$myvol", ServerType::HpNonStop);
        let hpnonstop3 = ServerPath::new("\\mysys.$theirvol", ServerType::HpNonStop);
        let hpnonstop4 = ServerPath::new("\\theirsys.$myvol", ServerType::HpNonStop);
        assert_eq!(hpnonstop2.common_parent(&hpnonstop3), hpnonstop1);
        assert_eq!(hpnonstop1.common_parent(&hpnonstop1), hpnonstop1);
        assert_eq!(hpnonstop2.common_parent(&hpnonstop4), ServerPath::default());

        let dos_virtual1 = ServerPath::new("\\foo", ServerType::DosVirtual);
        let dos_virtual2 = ServerPath::new("\\foo\\baz", ServerType::DosVirtual);
        let dos_virtual3 = ServerPath::new("\\foo\\bar", ServerType::DosVirtual);
        assert_eq!(dos_virtual2.common_parent(&dos_virtual3), dos_virtual1);
        assert_eq!(dos_virtual1.common_parent(&dos_virtual1), dos_virtual1);

        let cygwin1 = ServerPath::new("/foo", ServerType::Cygwin);
        let cygwin2 = ServerPath::new("/foo/baz", ServerType::Cygwin);
        let cygwin3 = ServerPath::new("/foo/bar", ServerType::Cygwin);
        let cygwin4 = ServerPath::new("//foo", ServerType::Cygwin);
        let cygwin5 = ServerPath::new("//foo/baz", ServerType::Cygwin);
        let cygwin6 = ServerPath::new("//foo/bar", ServerType::Cygwin);
        assert_eq!(cygwin2.common_parent(&cygwin3), cygwin1);
        assert_eq!(cygwin1.common_parent(&cygwin1), cygwin1);
        assert_eq!(cygwin1.common_parent(&cygwin3), cygwin1);
        assert_eq!(cygwin5.common_parent(&cygwin6), cygwin4);
        assert_eq!(cygwin4.common_parent(&cygwin4), cygwin4);
        assert_eq!(cygwin4.common_parent(&cygwin6), cygwin4);
        assert_eq!(cygwin4.common_parent(&cygwin1), ServerPath::default());

        {
            let dos_slashes1 = ServerPath::new("C:\\", ServerType::DosFwdSlashes);
            let dos_slashes2 = ServerPath::new("C:\\FOO", ServerType::DosFwdSlashes);
            let dos_slashes3 = ServerPath::new("D:\\FOO", ServerType::DosFwdSlashes);
            assert_eq!(dos_slashes1.common_parent(&dos_slashes2), dos_slashes1);
            assert_eq!(
                dos_slashes2.common_parent(&dos_slashes3),
                ServerPath::default()
            );
        }
    }

    #[test]
    fn format_filename() {
        let unix1 = unix("/");
        let unix2 = unix("/foo");
        assert_eq!(unix1.format_filename("bar", false), "/bar");
        assert_eq!(unix2.format_filename("bar", false), "/foo/bar");

        let vms1 = ServerPath::new("FOO:[BAR]", ServerType::Vms);
        assert_eq!(vms1.format_filename("BAZ", false), "FOO:[BAR]BAZ");

        {
            let dos1 = ServerPath::new("C:\\", ServerType::Dos);
            let dos2 = ServerPath::new("C:\\foo", ServerType::Dos);
            assert_eq!(dos1.format_filename("bar", false), "C:\\bar");
            assert_eq!(dos2.format_filename("bar", false), "C:\\foo\\bar");
        }

        let mvs1 = ServerPath::new("'FOO.BAR'", ServerType::Mvs);
        let mvs2 = ServerPath::new("'FOO.BAR.'", ServerType::Mvs);
        assert_eq!(mvs1.format_filename("BAZ", false), "'FOO.BAR(BAZ)'");
        assert_eq!(mvs2.format_filename("BAZ", false), "'FOO.BAR.BAZ'");

        let vxworks1 = ServerPath::new(":foo:", ServerType::VxWorks);
        let vxworks2 = ServerPath::new(":foo:bar", ServerType::VxWorks);
        let vxworks3 = ServerPath::new(":foo:bar/test", ServerType::VxWorks);
        assert_eq!(vxworks1.format_filename("baz", false), ":foo:baz");
        assert_eq!(vxworks2.format_filename("baz", false), ":foo:bar/baz");
        assert_eq!(vxworks3.format_filename("baz", false), ":foo:bar/test/baz");

        // ZVM is same as Unix, only makes difference in directory listing
        // parser.
        let zvm1 = ServerPath::new("/", ServerType::Zvm);
        let zvm2 = ServerPath::new("/foo", ServerType::Zvm);
        assert_eq!(zvm1.format_filename("bar", false), "/bar");
        assert_eq!(zvm2.format_filename("bar", false), "/foo/bar");

        let hpnonstop1 = ServerPath::new("\\mysys", ServerType::HpNonStop);
        let hpnonstop2 = ServerPath::new("\\mysys.$myvol", ServerType::HpNonStop);
        let hpnonstop3 = ServerPath::new("\\mysys.$myvol.mysubvol", ServerType::HpNonStop);
        assert_eq!(hpnonstop1.format_filename("foo", false), "\\mysys.foo");
        assert_eq!(
            hpnonstop2.format_filename("foo", false),
            "\\mysys.$myvol.foo"
        );
        assert_eq!(
            hpnonstop3.format_filename("foo", false),
            "\\mysys.$myvol.mysubvol.foo"
        );

        let dos_virtual1 = ServerPath::new("\\", ServerType::DosVirtual);
        let dos_virtual2 = ServerPath::new("\\foo", ServerType::DosVirtual);
        assert_eq!(dos_virtual1.format_filename("bar", false), "\\bar");
        assert_eq!(dos_virtual2.format_filename("bar", false), "\\foo\\bar");

        let cygwin1 = ServerPath::new("/", ServerType::Cygwin);
        let cygwin2 = ServerPath::new("/foo", ServerType::Cygwin);
        let cygwin3 = ServerPath::new("//", ServerType::Cygwin);
        let cygwin4 = ServerPath::new("//foo", ServerType::Cygwin);
        assert_eq!(cygwin1.format_filename("bar", false), "/bar");
        assert_eq!(cygwin2.format_filename("bar", false), "/foo/bar");
        assert_eq!(cygwin3.format_filename("bar", false), "//bar");
        assert_eq!(cygwin4.format_filename("bar", false), "//foo/bar");

        {
            let dos_slashes1 = ServerPath::new("C:\\", ServerType::DosFwdSlashes);
            let dos_slashes2 = ServerPath::new("C:\\foo", ServerType::DosFwdSlashes);
            assert_eq!(dos_slashes1.format_filename("bar", false), "C:/bar");
            assert_eq!(dos_slashes2.format_filename("bar", false), "C:/foo/bar");
        }
    }

    #[test]
    fn change_path() {
        let mut unix1 = unix("/foo/bar");
        let mut unix2 = unix("/foo/bar/baz");
        let mut unix3 = unix("/foo/baz");
        let mut unix4 = unix("/foo/bar/baz");
        let unix5 = unix("/foo");
        assert!(unix1.change_path("baz") && unix1 == unix2);
        assert!(unix2.change_path("../../baz") && unix2 == unix3);
        assert!(unix3.change_path("/foo/bar/baz") && unix3 == unix4);
        assert!(unix3.change_path(".") && unix3 == unix4);
        let mut sub = String::from("../../bar");
        assert!(unix4.change_path_file(&mut sub) && unix4 == unix5 && sub == "bar");
        sub = String::from("bar/");
        assert!(!unix4.change_path_file(&mut sub));

        let vms1 = ServerPath::new("FOO:[BAR]", ServerType::Vms);
        let mut vms2 = ServerPath::new("FOO:[BAR]", ServerType::Vms);
        let vms3 = ServerPath::new("FOO:[BAR.BAZ]", ServerType::Vms);
        let mut vms4 = ServerPath::new("FOO:[BAR]", ServerType::Vms);
        let vms5 = ServerPath::new("FOO:[BAR.BAZ.BAR]", ServerType::Vms);
        let mut vms6 = ServerPath::new("FOO:[BAR]", ServerType::Vms);
        let mut vms7 = ServerPath::new("FOO:[BAR]", ServerType::Vms);
        let mut vms8 = ServerPath::new("FOO:[BAR]", ServerType::Vms);
        let vms9 = ServerPath::new("FOO:[BAZ.BAR]", ServerType::Vms);
        let mut vms10 = ServerPath::new("DOO:[BAZ.BAR]", ServerType::Vms);
        assert!(vms2.change_path("BAZ") && vms2 == vms3);
        assert!(vms4.change_path("BAZ.BAR") && vms4 == vms5);
        sub = String::from("BAZ.BAR");
        assert!(vms6.change_path_file(&mut sub) && vms6 == vms1 && sub == "BAZ.BAR");
        sub = String::from("[BAZ.BAR]FOO");
        assert!(!vms7.change_path(&sub));
        assert!(vms8.change_path_file(&mut sub) && vms8 == vms9 && sub == "FOO");
        assert!(vms10.change_path("FOO:[BAR]") && vms10 == vms1);

        {
            let dos1 = ServerPath::new("c:\\bar", ServerType::Dos);
            let mut dos2 = ServerPath::new("c:\\bar", ServerType::Dos);
            let dos3 = ServerPath::new("c:\\bar\\baz", ServerType::Dos);
            let mut dos4 = ServerPath::new("c:\\bar", ServerType::Dos);
            let mut dos5 = ServerPath::new("c:\\bar\\baz\\bar", ServerType::Dos);
            let mut dos6 = ServerPath::new("c:\\bar\\baz\\", ServerType::Dos);
            let mut dos7 = ServerPath::new("d:\\bar", ServerType::Dos);
            let mut dos8 = ServerPath::new("c:\\bar\\baz", ServerType::Dos);
            let mut dos9 = ServerPath::new("c:\\bar\\", ServerType::Dos);
            let dos10 = ServerPath::new("md:\\bar\\", ServerType::Dos);
            assert!(dos2.change_path("baz") && dos2 == dos3);
            assert!(dos4.change_path("baz\\bar") && dos4 == dos5);
            assert!(dos5.change_path("\\bar\\") && dos5 == dos1);
            assert!(dos6.change_path("..\\..\\.\\foo\\..\\bar") && dos6 == dos1);
            assert!(dos7.change_path("c:\\bar") && dos7 == dos1);
            sub = String::from("\\bar\\foo");
            assert!(dos8.change_path_file(&mut sub) && dos8 == dos1 && sub == "foo");
            sub = String::from("baz\\foo");
            assert!(dos9.change_path_file(&mut sub) && dos9 == dos3 && sub == "foo");
            assert!(dos9.change_path("md:\\bar\\") && dos9 == dos10);
        }
        {
            // Test drive-relative paths.
            let dos = ServerPath::new("c:\\bar", ServerType::Dos);
            let mut p = dos.clone();
            assert!(!p.change_path("d:wrongdrive"));

            p = dos;
            assert!(p.change_path("c:correctDrive"));
            assert_eq!(p.path(), "c:\\bar\\correctDrive");
        }

        let _mvs1 = ServerPath::new("'BAR.'", ServerType::Mvs);
        let mut mvs2 = ServerPath::new("'BAR.'", ServerType::Mvs);
        let mvs3 = ServerPath::new("'BAR.BAZ.'", ServerType::Mvs);
        let mut mvs4 = ServerPath::new("'BAR.'", ServerType::Mvs);
        let mut mvs5 = ServerPath::new("'BAR.BAZ.BAR'", ServerType::Mvs);
        let mut mvs6 = ServerPath::new("'BAR.'", ServerType::Mvs);
        let mut mvs7 = ServerPath::new("'BAR.'", ServerType::Mvs);
        let mut mvs8 = ServerPath::new("'BAR.'", ServerType::Mvs);
        let mut mvs9 = ServerPath::new("'BAR.BAZ'", ServerType::Mvs);
        let mut mvs10 = ServerPath::new("'BAR.BAZ.'", ServerType::Mvs);
        let mut mvs11 = ServerPath::new("'BAR.BAZ'", ServerType::Mvs);
        let mvs12 = ServerPath::new("'BAR.BAZ'", ServerType::Mvs);
        assert!(mvs2.change_path("BAZ.") && mvs2 == mvs3);
        assert!(mvs4.change_path("BAZ.BAR") && mvs4 == mvs5);
        sub = String::from("BAZ.BAR");
        assert!(mvs6.change_path_file(&mut sub) && mvs6 == mvs3 && sub == "BAR");
        sub = String::from("BAZ.BAR.");
        assert!(!mvs7.change_path_file(&mut sub));
        sub = String::from("BAZ(BAR)");
        assert!(mvs8.change_path_file(&mut sub) && mvs8 == mvs9 && sub == "BAR");
        assert!(!mvs5.change_path("(FOO)"));
        assert!(!mvs9.change_path("FOO"));
        sub = String::from("(FOO)");
        assert!(!mvs10.change_path(&sub));
        sub = String::from("(FOO)");
        assert!(mvs11.change_path_file(&mut sub) && mvs11 == mvs12 && sub == "FOO");

        let vxworks1 = ServerPath::new(":foo:bar", ServerType::VxWorks);
        let mut vxworks2 = ServerPath::new(":foo:bar", ServerType::VxWorks);
        let vxworks3 = ServerPath::new(":foo:bar/baz", ServerType::VxWorks);
        let mut vxworks4 = ServerPath::new(":foo:bar", ServerType::VxWorks);
        let vxworks5 = ServerPath::new(":foo:bar/baz/bar", ServerType::VxWorks);
        let mut vxworks6 = ServerPath::new(":foo:bar/baz/", ServerType::VxWorks);
        let mut vxworks7 = ServerPath::new(":bar:bar", ServerType::VxWorks);
        let mut vxworks8 = ServerPath::new(":foo:bar/baz", ServerType::VxWorks);
        let vxworks9 = ServerPath::new(":foo:bar/baz/bar", ServerType::VxWorks);
        assert!(vxworks2.change_path("baz") && vxworks2 == vxworks3);
        assert!(vxworks4.change_path("baz/bar") && vxworks4 == vxworks5);
        assert!(vxworks6.change_path("../.././foo/../bar") && vxworks6 == vxworks1);
        assert!(vxworks7.change_path(":foo:bar") && vxworks7 == vxworks1);
        sub = String::from("bar/foo");
        assert!(vxworks8.change_path_file(&mut sub) && vxworks8 == vxworks9 && sub == "foo");

        // ZVM is same as Unix, only makes difference in directory listing
        // parser.
        let mut zvm1 = ServerPath::new("/foo/bar", ServerType::Zvm);
        let mut zvm2 = ServerPath::new("/foo/bar/baz", ServerType::Zvm);
        let mut zvm3 = ServerPath::new("/foo/baz", ServerType::Zvm);
        let mut zvm4 = ServerPath::new("/foo/bar/baz", ServerType::Zvm);
        let zvm5 = ServerPath::new("/foo", ServerType::Zvm);
        assert!(zvm1.change_path("baz") && zvm1 == zvm2);
        assert!(zvm2.change_path("../../baz") && zvm2 == zvm3);
        assert!(zvm3.change_path("/foo/bar/baz") && zvm3 == zvm4);
        assert!(zvm3.change_path(".") && zvm3 == zvm4);
        sub = String::from("../../bar");
        assert!(zvm4.change_path_file(&mut sub) && zvm4 == zvm5 && sub == "bar");
        sub = String::from("bar/");
        assert!(!zvm4.change_path_file(&mut sub));

        let mut hpnonstop1 = ServerPath::new("\\mysys.$myvol", ServerType::HpNonStop);
        let mut hpnonstop2 = ServerPath::new("\\mysys.$myvol.mysubvol", ServerType::HpNonStop);
        let mut hpnonstop3 = ServerPath::new("\\mysys.$myvol2", ServerType::HpNonStop);
        let mut hpnonstop4 = ServerPath::new("\\mysys.$myvol.mysubvol", ServerType::HpNonStop);
        let mut hpnonstop5 = ServerPath::new("\\mysys", ServerType::HpNonStop);
        assert!(hpnonstop1.change_path("mysubvol") && hpnonstop1 == hpnonstop2);
        assert!(hpnonstop3.change_path("\\mysys.$myvol.mysubvol") && hpnonstop3 == hpnonstop4);
        sub = String::from("bar");
        assert!(hpnonstop2.change_path_file(&mut sub) && hpnonstop2 == hpnonstop4 && sub == "bar");
        sub = String::from("$myvol.mysubvol.bar");
        assert!(hpnonstop5.change_path_file(&mut sub) && hpnonstop5 == hpnonstop4 && sub == "bar");
        sub = String::from("bar.");
        assert!(!hpnonstop4.change_path_file(&mut sub));

        let mut dos_virtual1 = ServerPath::new("\\foo\\bar", ServerType::DosVirtual);
        let mut dos_virtual2 = ServerPath::new("\\foo\\bar\\baz", ServerType::DosVirtual);
        let mut dos_virtual3 = ServerPath::new("\\foo\\baz", ServerType::DosVirtual);
        let mut dos_virtual4 = ServerPath::new("\\foo\\bar\\baz", ServerType::DosVirtual);
        let dos_virtual5 = ServerPath::new("\\foo", ServerType::DosVirtual);
        assert!(dos_virtual1.change_path("baz") && dos_virtual1 == dos_virtual2);
        assert!(dos_virtual2.change_path("..\\..\\baz") && dos_virtual2 == dos_virtual3);
        assert!(dos_virtual3.change_path("\\foo\\bar\\baz") && dos_virtual3 == dos_virtual4);
        assert!(dos_virtual3.change_path(".") && dos_virtual3 == dos_virtual4);
        sub = String::from("..\\..\\bar");
        assert!(
            dos_virtual4.change_path_file(&mut sub) && dos_virtual4 == dos_virtual5 && sub == "bar"
        );
        sub = String::from("bar\\");
        assert!(!dos_virtual4.change_path_file(&mut sub));

        let mut cygwin1 = ServerPath::new("/foo/bar", ServerType::Cygwin);
        let mut cygwin2 = ServerPath::new("/foo/bar/baz", ServerType::Cygwin);
        let mut cygwin3 = ServerPath::new("/foo/baz", ServerType::Cygwin);
        let mut cygwin4 = ServerPath::new("/foo/bar/baz", ServerType::Cygwin);
        let mut cygwin5 = ServerPath::new("/foo", ServerType::Cygwin);
        let cygwin6 = ServerPath::new("//foo", ServerType::Cygwin);
        let cygwin7 = ServerPath::new("//", ServerType::Cygwin);
        assert!(cygwin1.change_path("baz") && cygwin1 == cygwin2);
        assert!(cygwin2.change_path("../../baz") && cygwin2 == cygwin3);
        assert!(cygwin3.change_path("/foo/bar/baz") && cygwin3 == cygwin4);
        assert!(cygwin3.change_path(".") && cygwin3 == cygwin4);
        sub = String::from("../../bar");
        assert!(cygwin4.change_path_file(&mut sub) && cygwin4 == cygwin5 && sub == "bar");
        sub = String::from("bar/");
        assert!(!cygwin4.change_path_file(&mut sub));
        assert!(cygwin5.change_path("//foo") && cygwin5 == cygwin6);
        sub = String::from("//foo");
        assert!(cygwin1.change_path_file(&mut sub) && cygwin1 == cygwin7 && sub == "foo");

        {
            let dos_slashes1 = ServerPath::new("c:\\bar", ServerType::DosFwdSlashes);
            let mut dos_slashes2 = ServerPath::new("c:\\bar", ServerType::DosFwdSlashes);
            let dos_slashes3 = ServerPath::new("c:\\bar/baz", ServerType::DosFwdSlashes);
            let mut dos_slashes4 = ServerPath::new("c:\\bar", ServerType::DosFwdSlashes);
            let mut dos_slashes5 = ServerPath::new("c:\\bar\\baz\\bar", ServerType::DosFwdSlashes);
            let mut dos_slashes6 = ServerPath::new("c:\\bar\\baz\\", ServerType::DosFwdSlashes);
            let mut dos_slashes7 = ServerPath::new("d:/bar", ServerType::DosFwdSlashes);
            let mut dos_slashes8 = ServerPath::new("c:\\bar\\baz", ServerType::DosFwdSlashes);
            let mut dos_slashes9 = ServerPath::new("c:\\bar\\", ServerType::DosFwdSlashes);
            let dos_slashes10 = ServerPath::new("md:\\bar\\", ServerType::DosFwdSlashes);
            assert!(dos_slashes2.change_path("baz") && dos_slashes2 == dos_slashes3);
            assert!(dos_slashes4.change_path("baz\\bar") && dos_slashes4 == dos_slashes5);
            assert!(dos_slashes5.change_path("\\bar\\") && dos_slashes5 == dos_slashes1);
            assert!(
                dos_slashes6.change_path("..\\..\\.\\foo\\..\\bar") && dos_slashes6 == dos_slashes1
            );
            assert!(dos_slashes7.change_path("c:\\bar") && dos_slashes7 == dos_slashes1);
            sub = String::from("\\bar\\foo");
            assert!(
                dos_slashes8.change_path_file(&mut sub)
                    && dos_slashes8 == dos_slashes1
                    && sub == "foo"
            );
            sub = String::from("baz\\foo");
            assert!(
                dos_slashes9.change_path_file(&mut sub)
                    && dos_slashes9 == dos_slashes3
                    && sub == "foo"
            );
            assert!(dos_slashes9.change_path("md:\\bar\\") && dos_slashes9 == dos_slashes10);
        }
    }
}
