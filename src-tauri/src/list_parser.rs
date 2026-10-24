//! Parsers for the two directory listing dialects FTP servers speak:
//! RFC 3659 `MLSD` facts and the ubiquitous `LIST` output.
//!
//! The `LIST` parser is a Rust port of FileZilla's
//! `src/engine/directorylistingparser.cpp`.  It intentionally mirrors the C++
//! tokenizer (`CLine`/`CToken`) and the per-format parsers, including their
//! quirks, because real servers emit all of these formats in the wild:
//!
//! * Unix `ls -l` (many localised date and owner/group variants, NetWare,
//!   NetPresenz, VShell, OS/2, OS/9, WFTP, IBM AS/400, ...)
//! * EPLF, VMS (including multi-line entries), MVS (plain/PDS/PDS2/Migrated/
//!   Tape), ZVM and HP NonStop
//! * DOS/IIS
//!
//! The public API (`parse_list`, `parse_mlsd`, `parse_mlsd_line`) is unchanged.

use crate::models::RemoteEntry;
use crate::remote_path::{join, normalize};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Utc};

/// `CDirentry::flag_dir`
const FLAG_DIR: u8 = 0x01;
/// `CDirentry::flag_link`
const FLAG_LINK: u8 = 0x02;

/// Server dialect selector, mirroring FileZilla's `ServerType` where it
/// changes the parser (some formats are only unambiguous for these servers).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ServerType {
    Unix,
    Mvs,
    Vms,
    Zvm,
    HpNonStop,
}

impl ServerType {
    fn from_name(name: &str) -> Self {
        match name.to_ascii_uppercase().as_str() {
            "MVS" => ServerType::Mvs,
            "VMS" => ServerType::Vms,
            "ZVM" => ServerType::Zvm,
            "HPNONSTOP" | "HP_NONSTOP" => ServerType::HpNonStop,
            _ => ServerType::Unix,
        }
    }
}

/// Mutable working entry, the equivalent of `CDirentry` plus the C++ parser's
/// `-1` "unknown size" convention.
#[derive(Debug, Default, Clone)]
struct Entry {
    name: String,
    size: i64,
    permissions: String,
    owner_group: String,
    time: Option<NaiveDateTime>,
    flags: u8,
}

impl Entry {
    fn new() -> Self {
        Entry {
            name: String::new(),
            size: -1,
            permissions: String::new(),
            owner_group: String::new(),
            time: None,
            flags: 0,
        }
    }

    fn is_dir(&self) -> bool {
        self.flags & FLAG_DIR != 0
    }

    fn is_link(&self) -> bool {
        self.flags & FLAG_LINK != 0
    }
}

/// A line split into whitespace separated tokens, following `CLine`:
/// only spaces and tabs separate tokens, leading whitespace is skipped and
/// the last token is stored without trailing whitespace.
struct TokenLine<'a> {
    line: &'a str,
    tokens: Vec<(usize, usize)>,
    trimmed_len: usize,
}

impl<'a> TokenLine<'a> {
    fn new(line: &'a str) -> Self {
        let bytes = line.as_bytes();
        let mut tokens = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            let start = i;
            while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'\t' {
                i += 1;
            }
            tokens.push((start, i));
        }
        TokenLine {
            line,
            tokens,
            trimmed_len: line.trim_end_matches([' ', '\t']).len(),
        }
    }

    fn token(&self, n: usize) -> &'a str {
        match self.tokens.get(n) {
            Some(&(a, b)) => &self.line[a..b],
            None => "",
        }
    }

    /// `CLine::GetEndToken(n)` - from token `n` to the end of the line,
    /// without trailing whitespace.
    fn end_token(&self, n: usize) -> &'a str {
        match self.tokens.get(n) {
            Some(&(a, _)) => &self.line[a..self.trimmed_len],
            None => "",
        }
    }

    /// `CLine::GetEndToken(n, true)` - from the character right after token
    /// `n - 1` to the end of the line, preserving whitespace.  The C++ code
    /// assumes exactly one separator character, which matters for MLSD
    /// filenames that begin or end with whitespace.
    fn end_token_with_ws(&self, n: usize) -> &'a str {
        let prev = n.saturating_sub(1);
        match self.tokens.get(prev) {
            Some(&(_, b)) => {
                let p = b + 1;
                if p >= self.line.len() {
                    ""
                } else {
                    &self.line[p..]
                }
            }
            None => "",
        }
    }
}

fn split_raw_lines(raw: &str) -> impl Iterator<Item = &str> {
    raw.split(['\r', '\n']).filter(|line| !line.is_empty())
}

// ---------------------------------------------------------------------------
// Token helpers (ported from CToken)
// ---------------------------------------------------------------------------

fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn char_count(s: &str) -> usize {
    s.chars().count()
}

fn is_left_numeric(s: &str) -> bool {
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(first), Some(_)) => first.is_ascii_digit(),
        _ => false,
    }
}

fn is_right_numeric(s: &str) -> bool {
    let mut chars = s.chars().rev();
    match (chars.next(), chars.next()) {
        (Some(last), Some(_)) => last.is_ascii_digit(),
        _ => false,
    }
}

fn is_non_ascii(c: char) -> bool {
    c as u32 > 127
}

/// `CToken::GetNumber(s, decimal, false)`: parse leading digits, stop at the
/// first non-digit.  Returns -1 if there are no leading digits or on overflow.
fn parse_leading_number(s: &str) -> i64 {
    let mut value: i64 = 0;
    let mut any = false;
    for b in s.bytes() {
        if !b.is_ascii_digit() {
            break;
        }
        any = true;
        if value > i64::MAX / 10 {
            return -1;
        }
        value *= 10;
        let digit = (b - b'0') as i64;
        if i64::MAX - digit < value {
            return -1;
        }
        value += digit;
    }
    if any {
        value
    } else {
        -1
    }
}

fn parse_trailing_number(s: &str) -> i64 {
    let bytes = s.as_bytes();
    let mut start = bytes.len();
    while start > 0 && bytes[start - 1].is_ascii_digit() {
        start -= 1;
    }
    parse_leading_number(&s[start..])
}

/// `CToken::GetNumber(t_numberBase)`.
fn get_number(s: &str) -> i64 {
    if is_numeric(s) || is_left_numeric(s) {
        parse_leading_number(s)
    } else if is_right_numeric(s) {
        parse_trailing_number(s)
    } else {
        i64::MIN
    }
}

/// `CToken::GetNumber(start, len)`.
fn number_range(s: &str, start: usize, len: isize) -> i64 {
    let chars: Vec<char> = s.chars().collect();
    if start >= chars.len() {
        return -1;
    }
    let take = if len == -1 {
        chars.len() - start
    } else if len < 1 {
        return -1;
    } else {
        len as usize
    };
    if chars.len() - start < take {
        return -1;
    }
    let slice: String = chars[start..start + take].iter().collect();
    parse_leading_number(&slice)
}

fn parse_strict_i64(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value = parse_leading_number(s);
    if value < 0 {
        None
    } else {
        Some(value)
    }
}

fn parse_hex_strict(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    let mut value: i64 = 0;
    for c in s.chars() {
        let digit = match c {
            '0'..='9' => c as i64 - '0' as i64,
            'a'..='f' => c as i64 - 'a' as i64 + 10,
            'A'..='F' => c as i64 - 'A' as i64 + 10,
            _ => return None,
        };
        if value > i64::MAX / 16 {
            return None;
        }
        value *= 16;
        if i64::MAX - digit < value {
            return None;
        }
        value += digit;
    }
    Some(value)
}

fn is_hex_numeric(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c) || ('A'..='F').contains(&c))
}

fn find_any(s: &str, needles: &[char], start: usize) -> Option<usize> {
    s.char_indices()
        .skip_while(|(i, _)| *i < start)
        .find(|(_, c)| needles.contains(c))
        .map(|(i, _)| i)
}

/// Character index based search inside an already split token.
fn find_char_idx(chars: &[char], needles: &[char], start: usize) -> Option<usize> {
    chars
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, c)| needles.contains(c))
        .map(|(i, _)| i)
}

fn all_digits_chars(chars: &[char]) -> bool {
    !chars.is_empty() && chars.iter().all(|c| c.is_ascii_digit())
}

/// `CToken::GetNumber(start, len)` on a character slice (leading digits are
/// parsed, trailing data is ignored).
fn number_range_chars(chars: &[char], start: usize, len: isize) -> i64 {
    if start >= chars.len() {
        return -1;
    }
    let take = if len == -1 {
        chars.len() - start
    } else if len < 1 {
        return -1;
    } else {
        len as usize
    };
    if chars.len() - start < take {
        return -1;
    }
    let mut value: i64 = 0;
    let mut any = false;
    for c in &chars[start..start + take] {
        if !c.is_ascii_digit() {
            break;
        }
        any = true;
        if value > i64::MAX / 10 {
            return -1;
        }
        value *= 10;
        value += *c as i64 - '0' as i64;
    }
    if any {
        value
    } else {
        -1
    }
}

fn leading_number_chars(chars: &[char]) -> i64 {
    number_range_chars(chars, 0, -1)
}

/// `ParseComplexFileSize`: `123`, `33.5k`, `2KB`, `2/8`, ...
fn parse_complex_file_size(token: &str, blocksize: Option<i64>) -> Option<i64> {
    if is_numeric(token) {
        let mut size = parse_leading_number(token);
        if size < 0 {
            return None;
        }
        if let Some(bs) = blocksize {
            if bs > 0 {
                size = size.checked_mul(bs)?;
            }
        }
        return Some(size);
    }

    let bytes = token.as_bytes();
    let mut len = bytes.len();
    if len == 0 {
        return None;
    }

    let mut last = bytes[len - 1];
    if last == b'B' || last == b'b' {
        if len < 2 {
            return None;
        }
        len -= 1;
        let c = bytes[len - 1];
        if !c.is_ascii_digit() {
            len -= 1;
            last = c;
        } else {
            last = 0;
        }
    } else if last.is_ascii_digit() {
        last = 0;
    } else {
        len -= 1;
        if len == 0 {
            return None;
        }
    }

    let mut size: i64 = 0;
    let mut dot: usize = 0;
    for (i, &c) in bytes.iter().enumerate().take(len) {
        if c.is_ascii_digit() {
            size = size.checked_mul(10)?;
            size = size.checked_add((c - b'0') as i64)?;
        } else if c == b'.' {
            if i == 0 || i + 1 == len || dot != 0 {
                return None;
            }
            dot = len - i;
        } else {
            return None;
        }
    }

    let mult: i64 = match last {
        b'k' | b'K' => 1024,
        b'm' | b'M' => 1024 * 1024,
        b'g' | b'G' => 1024 * 1024 * 1024,
        b't' | b'T' => 1024i64 * 1024 * 1024 * 1024,
        b'b' | b'B' => 1,
        0 => blocksize.unwrap_or(1),
        _ => return None,
    };
    size = size.checked_mul(mult)?;

    // The C++ code accumulates all digits and then drops the decimal places.
    let mut d = dot;
    while d > 1 {
        size /= 10;
        d -= 1;
    }

    Some(size)
}

// ---------------------------------------------------------------------------
// Dates and times
// ---------------------------------------------------------------------------

fn month_base(name: &str) -> Option<u32> {
    Some(match name {
        // English (and the numeric months used by many servers)
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" | "june" => 6,
        "jul" | "july" => 7,
        "aug" => 8,
        "sep" | "sept" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        "1" | "01" => 1,
        "2" | "02" => 2,
        "3" | "03" => 3,
        "4" | "04" => 4,
        "5" | "05" => 5,
        "6" | "06" => 6,
        "7" | "07" => 7,
        "8" | "08" => 8,
        "9" | "09" => 9,
        "10" => 10,
        "11" => 11,
        "12" => 12,
        // German / Austrian
        "mrz" | "mär" | "märz" => 3,
        "mai" => 5,
        "juni" => 6,
        "juli" => 7,
        "okt" => 10,
        "dez" => 12,
        "jän" => 1,
        // French
        "janv" => 1,
        "fév" | "fev" | "févr" | "fevr" => 2,
        "mars" | "mrs" => 3,
        "avr" | "avril" => 4,
        "juin" => 6,
        "juil" | "jui" => 7,
        "aoû" | "août" | "aout" => 8,
        "déc" => 12,
        // Italian
        "gen" => 1,
        "mag" => 5,
        "giu" => 6,
        "lug" => 7,
        "ago" => 8,
        "set" => 9,
        "ott" => 10,
        "dic" => 12,
        // Spanish
        "ene" => 1,
        "fbro" => 2,
        "mzo" => 3,
        "ab" | "abr" => 4,
        "agto" => 8,
        "sbre" => 9,
        "obre" => 10,
        "nbre" => 11,
        "dbre" => 12,
        // Polish
        "sty" => 1,
        "lut" => 2,
        "kwi" => 4,
        "maj" => 5,
        "cze" => 6,
        "lip" => 7,
        "sie" => 8,
        "wrz" => 9,
        "paż" | "paź" | "paz" => 10,
        "lis" => 11,
        "gru" => 12,
        // Dutch
        "mrt" => 3,
        "mei" => 5,
        // Portuguese
        "out" => 10,
        // Finnish
        "tammi" => 1,
        "helmi" => 2,
        "maalis" => 3,
        "huhti" => 4,
        "touko" => 5,
        "kesä" => 6,
        "heinä" => 7,
        "elo" => 8,
        "syys" => 9,
        "loka" => 10,
        "marras" => 11,
        "joulu" => 12,
        // Slovenian
        "avg" => 8,
        // Icelandic
        "maí" => 5,
        "jún" => 6,
        "júl" => 7,
        "ág" => 8,
        "nóv" => 11,
        "des" => 12,
        // Lithuanian
        "sau" => 1,
        "vas" => 2,
        "kov" => 3,
        "bal" => 4,
        "geg" => 5,
        "bir" => 6,
        "lie" => 7,
        "rgp" => 8,
        "rgs" => 9,
        "spa" => 10,
        "lap" => 11,
        "grd" => 12,
        // Hungarian
        "szept" => 9,
        _ => return None,
    })
}

fn combo_matches(month: u32, suffix: &str) -> bool {
    let m = month as i64;
    let pad2 = |v: i64| format!("{v:02}");
    if suffix == pad2(m) || suffix == pad2(m - 1) {
        return true;
    }
    let short = if m < 10 { m } else { m % 10 };
    if suffix == short.to_string() {
        return true;
    }
    let short_alt = if m <= 10 { m - 1 } else { (m - 1) % 10 };
    suffix == short_alt.to_string()
}

/// `GetMonthFromName`: month names in many languages, numeric months and the
/// "month name plus number" combinations some servers send.
fn month_from_name(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    if let Some(month) = month_base(&lower) {
        return Some(month);
    }
    let chars: Vec<char> = lower.chars().collect();
    for split in 1..chars.len() {
        if !chars[split].is_ascii_digit() {
            continue;
        }
        let prefix: String = chars[..split].iter().collect();
        let suffix: String = chars[split..].iter().collect();
        if !suffix.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if let Some(month) = month_base(&prefix) {
            if combo_matches(month, &suffix) {
                return Some(month);
            }
        }
    }
    None
}

fn epoch_to_utc(seconds: i64) -> Option<NaiveDateTime> {
    chrono::DateTime::<Utc>::from_timestamp(seconds, 0).map(|dt| dt.naive_utc())
}

fn set_date(entry: &mut Entry, year: i64, month: i64, day: i64) -> bool {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return false;
    }
    let date = NaiveDate::from_ymd_opt(year as i32, month as u32, day as u32);
    match date.and_then(|d| d.and_hms_opt(0, 0, 0)) {
        Some(dt) => {
            entry.time = Some(dt);
            true
        }
        None => false,
    }
}

/// `fz::datetime::set(utc, y, m, d, h, min)`; a negative component means
/// "use zero", and 24:00 is normalised to the following midnight.
fn set_datetime(
    entry: &mut Entry,
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
) -> bool {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return false;
    }
    let (hour, extra_day) = if hour < 0 {
        (0u32, 0u64)
    } else if hour == 24 && minute == 0 {
        (0u32, 1u64)
    } else {
        (hour as u32, 0u64)
    };
    let minute = if minute < 0 { 0u32 } else { minute as u32 };
    if hour > 23 || minute > 59 {
        return false;
    }
    let date = NaiveDate::from_ymd_opt(year as i32, month as u32, day as u32);
    match date.and_then(|d| d.and_hms_opt(hour, minute, 0)) {
        Some(dt) => {
            entry.time = Some(dt + Duration::days(extra_day as i64));
            true
        }
        None => false,
    }
}

/// `ParseTime`: set the time of day on an already parsed date.
fn parse_time(token: &str, entry: &mut Entry) -> bool {
    if entry.time.is_none() {
        return false;
    }
    let Some(pos) = token.find(':') else {
        return false;
    };
    if pos == 0 || pos >= token.len() - 1 {
        return false;
    }
    let hour = parse_leading_number(&token[..pos]);
    if !(0..=24).contains(&hour) {
        return false;
    }
    let rest = &token[pos + 1..];
    let pos2 = rest.find(':');
    let (minute_str, seconds) = match pos2 {
        Some(p) => {
            // `GetNumber` tolerates trailing data such as ".27".
            let sec = parse_leading_number(&rest[p + 1..]);
            if !(0..=60).contains(&sec) {
                return false;
            }
            (&rest[..p], Some(sec))
        }
        None => (rest, None),
    };
    if minute_str.is_empty() {
        return false;
    }
    let minute = parse_leading_number(minute_str);
    if !(0..=59).contains(&minute) {
        return false;
    }
    let mut hour = hour;
    if !is_right_numeric(token) {
        let chars: Vec<char> = token.chars().collect();
        let marker = chars[chars.len() - 2];
        if marker == 'P' || marker == 'p' {
            if hour < 12 {
                hour += 12;
            }
        } else if marker == 'A' || marker == 'a' {
            if hour == 12 {
                hour = 0;
            }
        } else {
            return false;
        }
    }
    if hour > 23 {
        if hour != 24 || minute != 0 {
            return false;
        }
    }
    let date = entry.time.unwrap().date();
    let (hour, extra_day) = if hour == 24 {
        (0u32, 1i64)
    } else {
        (hour as u32, 0i64)
    };
    let second = match seconds {
        Some(v) if v == 60 => 59,
        Some(v) => v as u32,
        None => 0,
    };
    match date.and_hms_opt(hour, minute as u32, second) {
        Some(dt) => {
            entry.time = Some(dt + Duration::days(extra_day));
            true
        }
        None => false,
    }
}

/// `ParseShortDate`: `yyyy-mm-dd`, `mm/dd/yy`, `dd.mm.yyyy`, `01-jun-99`, ...
fn parse_short_date(token: &str, entry: &mut Entry, sane_field_order: bool) -> bool {
    let chars: Vec<char> = token.chars().collect();
    if chars.is_empty() {
        return false;
    }

    let mut got_year = false;
    let mut got_month = false;
    let mut got_day = false;
    let mut got_month_name = false;

    let mut year: i64 = 0;
    let mut month: i64 = 0;
    let mut day: i64 = 0;

    let Some(pos) = find_char_idx(&chars, &['-', '.', '/'], 0) else {
        return false;
    };
    if pos < 1 {
        return false;
    }

    if !all_digits_chars(&chars[..pos]) {
        // monthname-dd-yy
        let date_month: String = chars[..pos].iter().collect();
        match month_from_name(&date_month) {
            Some(m) => month = m as i64,
            None => return false,
        }
        got_month = true;
        got_month_name = true;
    } else if pos == 4 {
        // yyyy-mm-dd
        year = leading_number_chars(&chars[..pos]);
        if !(1900..=3000).contains(&year) {
            return false;
        }
        got_year = true;
    } else if pos <= 2 {
        let value = leading_number_chars(&chars[..pos]);
        if chars[pos] == '.' {
            // Maybe dd.mm.yyyy
            if !(1..=31).contains(&value) {
                return false;
            }
            day = value;
            got_day = true;
        } else if sane_field_order {
            year = value;
            if year < 50 {
                year += 2000;
            } else {
                year += 1900;
            }
            got_year = true;
        } else {
            if value < 1 {
                return false;
            }
            if value > 12 {
                if value > 31 {
                    return false;
                }
                day = value;
                got_day = true;
            } else {
                month = value;
                got_month = true;
            }
        }
    } else {
        return false;
    }

    let Some(pos2) = find_char_idx(&chars, &['-', '.', '/'], pos + 1) else {
        return false;
    };
    if pos2 - pos == 1 || pos2 == chars.len() - 1 {
        return false;
    }

    let second_numeric = all_digits_chars(&chars[pos + 1..pos2]);

    // If we already got the month and the second field is not numeric,
    // change the old month into the day and use the new token as month.
    if !second_numeric && got_month {
        if got_month_name || got_day {
            return false;
        }
        got_day = true;
        got_month = false;
        day = month;
    }

    if got_year || got_day {
        let date_month: String = chars[pos + 1..pos2].iter().collect();
        match month_from_name(&date_month) {
            Some(m) => month = m as i64,
            None => return false,
        }
        got_month = true;
    } else {
        let value = leading_number_chars(&chars[pos + 1..pos2]);
        if !(1..=31).contains(&value) {
            return false;
        }
        day = value;
        got_day = true;
    }

    let tail = leading_number_chars(&chars[pos2 + 1..]);
    if got_year {
        if !(1..=31).contains(&tail) {
            return false;
        }
        day = tail;
        got_day = true;
    } else {
        if !(0..=9999).contains(&tail) {
            return false;
        }
        let mut value = tail;
        if value < 50 {
            value += 2000;
        } else if value < 1000 {
            value += 1900;
        }
        year = value;
        got_year = true;
    }

    if !(got_year && got_month && got_day) {
        return false;
    }

    set_date(entry, year, month, day)
}

/// `ParseUnixDateTime`: the date/time part of a Unix style listing.
fn parse_unix_datetime(line: &TokenLine, index: &mut usize, entry: &mut Entry) -> bool {
    let mut may_have_time = true;
    let mut has_year_and_time = false;

    *index += 1;
    let token = line.token(*index);
    if token.is_empty() {
        return false;
    }

    let mut year: i64 = -1;
    let mut month: i64 = -1;
    let mut day: i64 = -1;
    let mut hour: i64 = -1;
    let mut minute: i64 = -1;

    let chars: Vec<char> = token.chars().collect();
    let mut date_month: Vec<char>;

    // Some servers use 26-05 2002, 2002-10-14, 01-jun-99 or 2004.07.15.
    if let Some(pos) = find_char_idx(&chars, &['-', '/', '.'], 0) {
        let pos2 = find_char_idx(&chars, &['-', '/', '.'], pos + 1);
        match pos2 {
            None => {
                if chars[pos] != '.' {
                    // something like 26-05 2002
                    day = number_range_chars(&chars, pos + 1, (chars.len() - pos - 1) as isize);
                    if !(1..=31).contains(&day) {
                        return false;
                    }
                    date_month = chars[..pos].to_vec();
                } else {
                    date_month = chars.clone();
                }
            }
            Some(pos2) => {
                if chars[pos] != chars[pos2] {
                    return false;
                }
                if !parse_short_date(token, entry, false) {
                    return false;
                }
                if chars[pos] == '.' {
                    return true;
                }
                let t = match entry.time {
                    Some(t) => t,
                    None => return false,
                };
                year = t.year() as i64;
                month = t.month() as i64;
                day = t.day() as i64;
                date_month = chars.clone();
                let _ = pos2;
            }
        }
    } else if is_numeric(token) {
        let number = parse_leading_number(token);
        if (1001..10000).contains(&number) {
            // 2005 3 13 or 2005 13 3 - assume the first one
            year = number;
            *index += 1;
            let next = line.token(*index);
            if next.is_empty() {
                return false;
            }
            date_month = next.chars().collect();
            may_have_time = false;
        } else {
            date_month = chars.clone();
        }
    } else {
        let last = chars.last().copied().unwrap_or('\0');
        if is_left_numeric(token) && is_non_ascii(last) && parse_leading_number(token) > 1000 {
            let number = parse_leading_number(token);
            if number > 10000 {
                return false;
            }
            // Asian date format: 2005년 5월 20일
            year = number;
            *index += 1;
            let next = line.token(*index);
            if next.is_empty() {
                return false;
            }
            date_month = next.chars().collect();
            may_have_time = false;
        } else {
            date_month = chars.clone();
        }
    }

    if day < 1 {
        *index += 1;
        let day_token = line.token(*index);
        if day_token.is_empty() {
            return false;
        }
        let day_chars: Vec<char> = day_token.chars().collect();

        if !is_numeric(day_token) && !is_left_numeric(day_token) {
            // Non-numeric day: the month name is in a different field.
            let mut offset = 0;
            if date_month.last() == Some(&'.') {
                offset = 1;
            }
            if offset > date_month.len() {
                return false;
            }
            let numeric_len = date_month.len() - offset;
            if numeric_len == 0 || !all_digits_chars(&date_month[..numeric_len]) {
                return false;
            }
            day = number_range_chars(&date_month, 0, numeric_len as isize);
            date_month = day_chars;
        } else if day_chars.len() == 5 && day_chars[2] == ':' && is_right_numeric(day_token) {
            // This is a time. We consumed too much already.
            return false;
        } else {
            day = get_number(day_token);
            if day_chars.last() == Some(&',') {
                has_year_and_time = true;
            }
        }

        if !(1..=31).contains(&day) {
            return false;
        }
    }

    if month < 1 {
        let last = date_month.last().copied().unwrap_or('\0');
        if is_left_numeric(&date_month.iter().collect::<String>()) && is_non_ascii(last) {
            // Asian servers append a language specific suffix to the month.
            let mut keep = 0;
            for i in (0..date_month.len()).rev() {
                if date_month[i].is_ascii_digit() {
                    keep = i + 1;
                    break;
                }
            }
            if keep == 0 {
                keep = 1.min(date_month.len());
            }
            date_month.truncate(keep);
        }
        let month_name: String = date_month.iter().collect();
        let trimmed = month_name.trim_end_matches([',', '.']);
        match month_from_name(trimmed) {
            Some(m) => month = m as i64,
            None => return false,
        }
    }

    *index += 1;
    let time_or_year = line.token(*index);
    if time_or_year.is_empty() {
        return false;
    }
    let time_chars: Vec<char> = time_or_year.chars().collect();

    if let Some(pos) = find_char_idx(&time_chars, &[':', '.', '-'], 0) {
        if may_have_time {
            if pos == 0 || pos == time_chars.len() - 1 {
                return false;
            }
            hour = leading_number_chars(&time_chars[..pos]);
            minute = leading_number_chars(&time_chars[pos + 1..]);
            if hour < 0 || minute < 0 {
                return false;
            }
            if !(0..=23).contains(&hour) {
                // Allow alternate midnight representation
                if hour != 24 || minute != 0 {
                    return false;
                }
            } else if !(0..=59).contains(&minute) {
                return false;
            }

            // Some servers use times only for files newer than 6 months.
            if year <= 0 {
                if month == -1 || day == -1 {
                    return false;
                }
                let now = Utc::now();
                year = now.year() as i64;
                let current_day_of_year = now.day() as i64 + 31 * now.month0() as i64;
                let file_day_of_year = day + 31 * (month - 1);
                // Compare with an offset of one: the server's timezone can be
                // up to 24 hours ahead of the client.
                if current_day_of_year + 1 < file_day_of_year {
                    year -= 1;
                }
            }
        } else if year <= 0 {
            return false;
        } else {
            *index = index.saturating_sub(1);
        }
    } else if year <= 0 {
        if !is_numeric(time_or_year) && !is_left_numeric(time_or_year) {
            return false;
        }
        year = parse_leading_number(time_or_year);
        if year > 3000 {
            return false;
        }
        if year < 1000 {
            year += 1900;
        }

        if has_year_and_time {
            *index += 1;
            let time_token = line.token(*index);
            if time_token.is_empty() {
                return false;
            }
            let tc: Vec<char> = time_token.chars().collect();
            if tc.len() == 5
                && tc[2] == ':'
                && tc[..2].iter().all(|c| c.is_ascii_digit())
                && tc[3..].iter().all(|c| c.is_ascii_digit())
            {
                hour = leading_number_chars(&tc[..2]);
                minute = leading_number_chars(&tc[3..]);
                if !(0..=23).contains(&hour) {
                    if hour != 24 || minute != 0 {
                        return false;
                    }
                } else if !(0..=59).contains(&minute) {
                    return false;
                }
            } else {
                *index = index.saturating_sub(1);
            }
        }
    } else {
        *index = index.saturating_sub(1);
    }

    set_datetime(entry, year, month, day, hour, minute)
}

// ---------------------------------------------------------------------------
// Format parsers
// ---------------------------------------------------------------------------

fn parse_as_unix(line: &TokenLine, entry: &mut Entry, expect_date: bool) -> bool {
    let permission_token = line.token(0);
    if permission_token.is_empty() {
        return false;
    }
    let first = match permission_token.chars().next() {
        Some(c) => c,
        None => return false,
    };
    if !matches!(first, 'b' | 'c' | 'd' | 'l' | 'p' | 's' | '-') {
        return false;
    }

    let mut permissions = permission_token.to_string();
    entry.flags = 0;
    if first == 'd' || first == 'l' {
        entry.flags |= FLAG_DIR;
    }
    if first == 'l' {
        entry.flags |= FLAG_LINK;
    }

    let mut index = 0usize;

    // NetWare servers split the permissions into two tokens.
    let mut netware = false;
    if char_count(permission_token) == 1 {
        index += 1;
        let cont = line.token(index);
        if cont.is_empty() {
            return false;
        }
        permissions.push(' ');
        permissions.push_str(cont);
        netware = true;
    }

    let mut num_owner_group: i64 = 3;
    if !netware {
        // Filter out the link count, we don't need it.
        index += 1;
        let link_count = line.token(index);
        if link_count.is_empty() {
            return false;
        }
        if !is_numeric(link_count) {
            index = index.saturating_sub(1);
        }
    }

    let startindex = index;
    let mut first_attempt = true;
    loop {
        if !first_attempt {
            if num_owner_group == 0 {
                return false;
            }
            num_owner_group -= 1;
        }
        first_attempt = false;

        index = startindex;

        let mut owner_group = String::new();
        let mut i: i64 = 0;
        while i < num_owner_group {
            index += 1;
            let owner_group_token = line.token(index);
            if owner_group_token.is_empty() {
                return false;
            }
            if i != 0 {
                owner_group.push(' ');
            }
            owner_group.push_str(owner_group_token);
            i += 1;
        }

        index += 1;
        let size_token = line.token(index);
        if size_token.is_empty() {
            return false;
        }
        if let Some(size) = parse_complex_file_size(size_token, None) {
            entry.size = size;
        } else if is_right_numeric(size_token) {
            entry.size = get_number(size_token);
            // Append the missing group to the owner group.
            if !owner_group.is_empty() {
                owner_group.push(' ');
            }
            owner_group.push_str(size_token.trim_end_matches(|c: char| c.is_ascii_digit()));
        } else {
            continue;
        }

        if expect_date {
            entry.time = None;
            if !parse_unix_datetime(line, &mut index, entry) {
                continue;
            }
        }

        index += 1;
        let name_token = line.end_token(index);
        if name_token.is_empty() {
            continue;
        }

        let mut name = name_token.to_string();
        // Filter out special characters at the end of the filename.
        if let Some(last) = name.chars().last() {
            if last == '/' || last == '|' || last == '*' {
                name.pop();
            }
        }

        if entry.is_link() {
            if let Some(pos) = name.find(" -> ") {
                name.truncate(pos);
            }
        }

        entry.name = name;
        entry.permissions = permissions;
        entry.owner_group = owner_group;
        return true;
    }
}

fn parse_as_dos(line: &TokenLine, entry: &mut Entry) -> bool {
    entry.flags = 0;

    let Some(date_token) = Some(line.token(0)).filter(|t| !t.is_empty()) else {
        return false;
    };
    if !parse_short_date(date_token, entry, false) {
        return false;
    }

    let time_token = line.token(1);
    if time_token.is_empty() || !parse_time(time_token, entry) {
        return false;
    }

    let size_token = line.token(2);
    if size_token.is_empty() {
        return false;
    }
    if size_token == "<DIR>" {
        entry.flags |= FLAG_DIR;
        entry.size = -1;
    } else if is_numeric(size_token) || is_left_numeric(size_token) {
        let mut size: i64 = 0;
        for c in size_token.chars() {
            if c == ',' || c == '.' {
                continue;
            }
            if !c.is_ascii_digit() {
                return false;
            }
            size = match size
                .checked_mul(10)
                .and_then(|v| v.checked_add(c as i64 - '0' as i64))
            {
                Some(v) => v,
                None => return false,
            };
        }
        entry.size = size;
    } else {
        return false;
    }

    let name = line.end_token(3);
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }
    entry.name = name.to_string();
    entry.permissions.clear();
    entry.owner_group.clear();
    true
}

fn parse_as_eplf(line: &TokenLine, entry: &mut Entry) -> bool {
    let token = line.end_token(0);
    if token.is_empty() || !token.starts_with('+') {
        return false;
    }
    let Some(pos) = token.find('\t') else {
        return false;
    };
    if pos == token.len() - 1 {
        return false;
    }

    entry.name = token[pos + 1..].to_string();
    entry.flags = 0;
    entry.size = -1;
    entry.permissions.clear();
    entry.owner_group.clear();

    let facts = &token[..pos];
    let mut fact = 1usize;
    while fact < facts.len() {
        let separator = facts[fact..].find(',').map(|p| fact + p);
        let len = match separator {
            Some(sep) => sep - fact,
            None => facts.len() - fact,
        };
        if len == 0 {
            fact += 1;
            continue;
        }
        let kind = facts.as_bytes()[fact];
        match kind {
            b'/' => entry.flags |= FLAG_DIR,
            b's' => {
                entry.size = number_range(facts, fact + 1, (len - 1) as isize);
                if entry.size < 0 {
                    return false;
                }
            }
            b'm' => {
                let number = number_range(facts, fact + 1, (len - 1) as isize);
                if number < 0 {
                    return false;
                }
                entry.time = epoch_to_utc(number);
            }
            b'u' if len > 2 && facts.as_bytes().get(fact + 1) == Some(&b'p') => {
                entry.permissions = facts[fact + 2..fact + len].to_string();
            }
            _ => {}
        }
        fact += len + 1;
    }

    true
}

fn unescape_vms(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '^' {
            match chars.next() {
                Some(next) => out.push(next),
                None => break,
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_as_vms(line: &TokenLine, entry: &mut Entry) -> bool {
    let first = line.token(0);
    if first.is_empty() {
        return false;
    }
    let Some(pos) = first.find(';') else {
        return false;
    };

    entry.flags = 0;
    let mut name = if pos > 4 && first.as_bytes().get(pos - 4..pos) == Some(b".DIR") {
        entry.flags |= FLAG_DIR;
        let base = &first[..pos - 4];
        if &first[pos..] == ";1" {
            base.to_string()
        } else {
            format!("{base}{}", &first[pos..])
        }
    } else {
        first.to_string()
    };
    name = unescape_vms(&name);

    let mut index = 1usize;
    let mut token = line.token(index);
    if token.is_empty() {
        return false;
    }

    let mut owner_group = String::new();
    let mut permissions = String::new();

    // Either the size, a `[username]` field or a date.
    if !is_numeric(token) && !is_left_numeric(token) {
        let len = char_count(token);
        let chars: Vec<char> = token.chars().collect();
        if len < 3 || chars[0] != '[' || chars[len - 1] != ']' {
            return false;
        }
        owner_group = chars[1..len - 1].iter().collect();

        index += 1;
        token = line.token(index);
        if token.is_empty() {
            return false;
        }
        if !is_numeric(token) && !is_left_numeric(token) {
            return false;
        }
    }

    let mut got_size = false;
    let pos = token.find('/');
    if pos == Some(0) {
        return false;
    }
    if is_numeric(token) || (pos.is_some() && token[pos.unwrap() + 1..].find('/').is_none()) {
        let size_token = match pos {
            Some(p) => &token[..p],
            None => token,
        };
        match parse_complex_file_size(size_token, Some(512)) {
            Some(size) => entry.size = size,
            None => return false,
        }
        got_size = true;

        index += 1;
        token = line.token(index);
        if token.is_empty() {
            return false;
        }
    } else if pos.is_none() && is_left_numeric(token) {
        if let Some(size) = parse_complex_file_size(token, Some(512)) {
            entry.size = size;
            got_size = true;
            index += 1;
            token = line.token(index);
            if token.is_empty() {
                return false;
            }
        }
    }

    if !parse_short_date(token, entry, false) {
        return false;
    }

    index += 1;
    token = line.token(index);
    if token.is_empty() {
        entry.name = name;
        entry.permissions = permissions;
        entry.owner_group = owner_group;
        return true;
    }

    if !parse_time(token, entry) {
        let chars: Vec<char> = token.chars().collect();
        let len = chars.len();
        let first_c = chars[0];
        let last_c = chars[len - 1];
        if first_c == '[' && last_c != ']' {
            return false;
        }
        if first_c == '(' && last_c != ')' {
            return false;
        }
        if first_c != '[' && last_c == ']' {
            return false;
        }
        if first_c != '(' && last_c == ')' {
            return false;
        }
        index = index.saturating_sub(1);
    }

    if !got_size {
        index += 1;
        token = line.token(index);
        if token.is_empty() {
            return false;
        }
        if !is_numeric(token) && !is_left_numeric(token) {
            return false;
        }
        let pos = token.find('/');
        if pos == Some(0) {
            return false;
        }
        let size_token = match pos {
            Some(p) => &token[..p],
            None => token,
        };
        match parse_complex_file_size(size_token, Some(512)) {
            Some(size) => entry.size = size,
            None => return false,
        }
    }

    index += 1;
    loop {
        let owner_token = line.token(index);
        if owner_token.is_empty() {
            break;
        }
        let chars: Vec<char> = owner_token.chars().collect();
        let len = chars.len();
        if len > 2 && chars[0] == '(' && chars[len - 1] == ')' {
            if !permissions.is_empty() {
                permissions.push(' ');
            }
            permissions.extend(&chars[1..len - 1]);
        } else if len > 2 && chars[0] == '[' && chars[len - 1] == ']' {
            if !owner_group.is_empty() {
                owner_group.push(' ');
            }
            owner_group.extend(&chars[1..len - 1]);
        } else {
            if !owner_group.is_empty() {
                owner_group.push(' ');
            }
            owner_group.push_str(owner_token);
        }
        index += 1;
    }

    entry.name = name;
    entry.permissions = permissions;
    entry.owner_group = owner_group;
    true
}

fn parse_as_ibm(line: &TokenLine, entry: &mut Entry) -> bool {
    let owner_group_token = line.token(0);
    if owner_group_token.is_empty() {
        return false;
    }

    let size_token = line.token(1);
    if size_token.is_empty() || !is_numeric(size_token) {
        return false;
    }
    entry.size = parse_leading_number(size_token);

    let date_token = line.token(2);
    if date_token.is_empty() {
        return false;
    }
    entry.flags = 0;
    if !parse_short_date(date_token, entry, false) {
        return false;
    }

    let time_token = line.token(3);
    if time_token.is_empty() || !parse_time(time_token, entry) {
        return false;
    }

    // Skip the object type token (`*FILE`, `*DIR`, ...).
    let name_token = line.end_token(5);
    if name_token.is_empty() {
        return false;
    }
    let mut name = name_token.to_string();
    if name.ends_with('/') {
        name.pop();
        entry.flags |= FLAG_DIR;
    }
    entry.name = name;
    entry.owner_group = owner_group_token.to_string();
    entry.permissions.clear();
    true
}

fn parse_other(line: &TokenLine, entry: &mut Entry, maybe_multiline_vms: bool) -> bool {
    let mut index = 0usize;
    let first_token = line.token(index);
    if first_token.is_empty() || !is_numeric(first_token) {
        return false;
    }

    index += 1;
    let mut token = line.token(index);
    if token.is_empty() {
        return false;
    }

    entry.flags = 0;

    if is_numeric(token) {
        // Numerical Unix style
        let chars: Vec<char> = first_token.chars().collect();
        if chars.len() >= 2 && chars[1] == '4' {
            entry.flags |= FLAG_DIR;
        }

        let mut owner_group = token.to_string();

        index += 1;
        token = line.token(index);
        if token.is_empty() {
            return false;
        }
        owner_group.push(' ');
        owner_group.push_str(token);

        index += 1;
        token = line.token(index);
        if token.is_empty() || !is_numeric(token) {
            return false;
        }
        entry.size = parse_leading_number(token);

        index += 1;
        token = line.token(index);
        if token.is_empty() {
            return false;
        }
        let number = get_number(token);
        if number < 0 {
            return false;
        }
        entry.time = epoch_to_utc(number);

        index += 1;
        let name_token = line.end_token(index);
        if name_token.is_empty() {
            return false;
        }
        entry.name = name_token.to_string();
        entry.permissions = first_token.to_string();
        entry.owner_group = owner_group;
        return true;
    }

    // Possible conflict with multiline VMS listings
    if maybe_multiline_vms {
        return false;
    }

    // VShell, OS/2 or Nortel VxWorks style format
    entry.size = parse_leading_number(first_token);

    let date_month = token;
    if month_from_name(date_month).is_none() {
        // OS/2 or nortel.VxWorks
        let mut skipped_count = 0;
        loop {
            if token == "DIR" {
                entry.flags |= FLAG_DIR;
            } else if find_any(token, &['-', '/', '.'], 0).is_some() {
                break;
            }

            skipped_count += 1;
            index += 1;
            token = line.token(index);
            if token.is_empty() {
                return false;
            }
        }

        if !parse_short_date(token, entry, false) {
            return false;
        }

        index += 1;
        token = line.token(index);
        if token.is_empty() || !parse_time(token, entry) {
            return false;
        }

        index += 1;
        let name_token = line.end_token(index);
        if name_token.is_empty() {
            return false;
        }
        let mut name = name_token.to_string();
        let char_len = name.chars().count();
        if char_len >= 5 {
            let suffix: String = name.chars().skip(char_len - 5).collect();
            if skipped_count == 0 && suffix.to_ascii_lowercase() == "<dir>" {
                entry.flags |= FLAG_DIR;
                let keep: String = name.chars().take(char_len - 5).collect();
                name = keep.trim_end_matches(' ').to_string();
            }
        }
        entry.name = name;
    } else {
        let month = month_from_name(date_month).unwrap() as i64;

        // Get day
        index += 1;
        token = line.token(index);
        if token.is_empty() {
            return false;
        }
        if !is_numeric(token) && !is_left_numeric(token) {
            return false;
        }
        let day = get_number(token);
        if !(0..=31).contains(&day) {
            return false;
        }

        // Get year
        index += 1;
        token = line.token(index);
        if token.is_empty() || !is_numeric(token) {
            return false;
        }
        let mut year = parse_leading_number(token);
        if year < 50 {
            year += 2000;
        } else if year < 1000 {
            year += 1900;
        }

        if !set_date(entry, year, month, day) {
            return false;
        }

        index += 1;
        token = line.token(index);
        if token.is_empty() || !parse_time(token, entry) {
            return false;
        }

        index += 1;
        let name_token = line.end_token(index);
        if name_token.is_empty() {
            return false;
        }
        let mut name = name_token.to_string();
        if let Some(last) = name.chars().last() {
            if last == '/' || last == '\\' {
                entry.flags |= FLAG_DIR;
                name.pop();
            }
        }
        entry.name = name;
    }

    entry.owner_group.clear();
    entry.permissions.clear();
    true
}

fn parse_as_wfftp(line: &TokenLine, entry: &mut Entry) -> bool {
    let name_token = line.token(0);
    if name_token.is_empty() {
        return false;
    }
    entry.name = name_token.to_string();

    let size_token = line.token(1);
    if size_token.is_empty() || !is_numeric(size_token) {
        return false;
    }
    entry.size = parse_leading_number(size_token);
    entry.flags = 0;

    let date_token = line.token(2);
    if date_token.is_empty() || !parse_short_date(date_token, entry, false) {
        return false;
    }

    let unused = line.token(3);
    if unused.is_empty() || !unused.ends_with('.') {
        return false;
    }

    let time_token = line.end_token(4);
    if time_token.is_empty() || !parse_time(time_token, entry) {
        return false;
    }

    entry.owner_group.clear();
    entry.permissions.clear();
    true
}

fn parse_as_ibm_mvs(line: &TokenLine, entry: &mut Entry) -> bool {
    let mut index = 0usize;

    // volume
    if line.token(index).is_empty() {
        return false;
    }
    index += 1;
    // unit
    if line.token(index).is_empty() {
        return false;
    }
    index += 1;
    // referred date
    let date_token = line.token(index);
    if date_token.is_empty() {
        return false;
    }
    index += 1;

    entry.flags = 0;
    if date_token != "**NONE**" && !parse_short_date(date_token, entry, false) {
        // Perhaps TSO004 3390 VSAM FOO.BAR
        if date_token != "VSAM" {
            return false;
        }
        let name_token = line.token(index);
        if name_token.is_empty() {
            return false;
        }
        let name = name_token.to_string();
        if name.contains(' ') {
            return false;
        }
        entry.name = name;
        entry.size = -1;
        entry.owner_group.clear();
        entry.permissions.clear();
        return true;
    }

    // ext
    let ext_token = line.token(index);
    if ext_token.is_empty() || !is_numeric(ext_token) {
        return false;
    }
    let prev_len = char_count(ext_token);
    index += 1;

    // used
    let used_token = line.token(index);
    if used_token.is_empty() {
        return false;
    }
    index += 1;
    if is_numeric(used_token) || used_token == "????" || used_token == "++++" {
        // recfm
        let recfm_token = line.token(index);
        if recfm_token.is_empty() || is_numeric(recfm_token) {
            return false;
        }
        index += 1;
    } else if prev_len < 6 {
        return false;
    }

    // lrecl
    let lrecl = line.token(index);
    if lrecl.is_empty() || !is_numeric(lrecl) {
        return false;
    }
    index += 1;
    // blksize
    let blksize = line.token(index);
    if blksize.is_empty() || !is_numeric(blksize) {
        return false;
    }
    index += 1;
    // dsorg
    let dsorg = line.token(index);
    if dsorg.is_empty() {
        return false;
    }
    index += 1;
    if dsorg == "PO" || dsorg == "PO-E" {
        entry.flags |= FLAG_DIR;
        entry.size = -1;
    } else {
        entry.size = 100;
    }

    let name_token = line.end_token(index);
    if name_token.is_empty() {
        return false;
    }
    entry.name = name_token.to_string();
    entry.owner_group.clear();
    entry.permissions.clear();
    true
}

fn parse_as_ibm_mvs_pds(line: &TokenLine, entry: &mut Entry) -> bool {
    let name_token = line.token(0);
    if name_token.is_empty() {
        return false;
    }
    entry.name = name_token.to_string();

    // vv.mm (unused)
    if line.token(1).is_empty() {
        return false;
    }

    entry.flags = 0;

    // creation date
    let creation = line.token(2);
    if creation.is_empty() || !parse_short_date(creation, entry, false) {
        return false;
    }
    // modification date
    let modification = line.token(3);
    if modification.is_empty() || !parse_short_date(modification, entry, false) {
        return false;
    }
    // modification time
    let time_token = line.token(4);
    if time_token.is_empty() || !parse_time(time_token, entry) {
        return false;
    }
    // size
    let size_token = line.token(5);
    if size_token.is_empty() || !is_numeric(size_token) {
        return false;
    }
    entry.size = parse_leading_number(size_token);

    // init
    let init = line.token(6);
    if init.is_empty() || !is_numeric(init) {
        return false;
    }
    // mod
    let modified = line.token(7);
    if modified.is_empty() || !is_numeric(modified) {
        return false;
    }
    // id
    if line.end_token(8).is_empty() {
        return false;
    }

    entry.owner_group.clear();
    entry.permissions.clear();
    true
}

fn parse_as_ibm_mvs_migrated(line: &TokenLine, entry: &mut Entry) -> bool {
    let first = line.token(0);
    if first.is_empty() || first.to_ascii_lowercase() != "migrated" {
        return false;
    }
    let name_token = line.token(1);
    if name_token.is_empty() {
        return false;
    }
    if !line.token(2).is_empty() {
        return false;
    }
    entry.name = name_token.to_string();
    entry.flags = 0;
    entry.size = -1;
    entry.owner_group.clear();
    entry.permissions.clear();
    true
}

fn parse_as_ibm_mvs_pds2(line: &TokenLine, entry: &mut Entry) -> bool {
    let name_token = line.token(0);
    if name_token.is_empty() {
        return false;
    }
    entry.name = name_token.to_string();
    entry.flags = 0;
    entry.owner_group.clear();
    entry.permissions.clear();
    entry.size = -1;

    let size_token = line.token(1);
    if size_token.is_empty() {
        return true;
    }

    let Some(size) = parse_hex_strict(size_token) else {
        return false;
    };
    entry.size = size;

    // Unused hexadecimal token
    let hex_token = line.token(2);
    if hex_token.is_empty() || !is_hex_numeric(hex_token) {
        return false;
    }

    // Unused numeric token
    let numeric_token = line.token(3);
    if numeric_token.is_empty() || !is_numeric(numeric_token) {
        return false;
    }

    let start = 4usize;
    let mut index = start;
    while !line.token(index).is_empty() {
        index += 1;
    }
    if index - start < 2 {
        return false;
    }
    index -= 1;

    let last = line.token(index);
    if last.is_empty() || (!is_numeric(last) && last != "ANY") {
        return false;
    }
    let second_last = line.token(index - 1);
    if second_last.is_empty() || (!is_numeric(second_last) && second_last != "ANY") {
        return false;
    }

    for i in start..index - 1 {
        let token = line.token(i);
        if token.is_empty() {
            return false;
        }
        if !token.chars().all(|c| c.is_ascii_uppercase()) {
            return false;
        }
    }

    true
}

fn parse_as_ibm_mvs_tape(line: &TokenLine, entry: &mut Entry) -> bool {
    if line.token(0).is_empty() {
        return false;
    }
    let unit = line.token(1);
    if unit.is_empty() || unit.to_ascii_lowercase() != "tape" {
        return false;
    }
    let name_token = line.token(2);
    if name_token.is_empty() {
        return false;
    }
    entry.name = name_token.to_string();
    entry.flags = 0;
    entry.owner_group.clear();
    entry.permissions.clear();
    entry.size = -1;

    if !line.token(3).is_empty() {
        return false;
    }
    true
}

fn parse_as_os9(line: &TokenLine, entry: &mut Entry) -> bool {
    let owner_token = line.token(0);
    if owner_token.is_empty() {
        return false;
    }
    let Some(pos) = owner_token.find('.') else {
        return false;
    };
    if pos == 0 || pos == owner_token.len() - 1 {
        return false;
    }
    if !owner_token[..pos].bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if !owner_token[pos + 1..].bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }

    entry.flags = 0;

    let date_token = line.token(1);
    if date_token.is_empty() || !parse_short_date(date_token, entry, true) {
        return false;
    }

    // Unused token
    if line.token(2).is_empty() {
        return false;
    }

    let perm_token = line.token(3);
    if perm_token.is_empty() {
        return false;
    }
    if perm_token.starts_with('d') {
        entry.flags |= FLAG_DIR;
    }

    // Unused token
    if line.token(4).is_empty() {
        return false;
    }

    let size_token = line.token(5);
    if size_token.is_empty() || !is_numeric(size_token) {
        return false;
    }
    entry.size = parse_leading_number(size_token);

    let name_token = line.end_token(6);
    if name_token.is_empty() {
        return false;
    }
    entry.name = name_token.to_string();
    entry.owner_group = owner_token.to_string();
    entry.permissions = perm_token.to_string();
    true
}

fn parse_as_zvm(line: &TokenLine, entry: &mut Entry) -> bool {
    let name_token = line.token(0);
    if name_token.is_empty() {
        return false;
    }
    entry.name = name_token.to_string();

    let ext = line.token(1);
    if ext.is_empty() {
        return false;
    }
    entry.name.push('.');
    entry.name.push_str(ext);

    // File format, unused
    let format = line.token(2);
    if format.is_empty() || (format != "V" && format != "F") {
        return false;
    }

    // Record length
    let record_len = line.token(3);
    if record_len.is_empty() || !is_numeric(record_len) {
        return false;
    }
    entry.size = parse_leading_number(record_len);

    // Number of records
    let records_token = line.token(4);
    if records_token.is_empty() || !is_numeric(records_token) {
        return false;
    }
    let records = parse_leading_number(records_token);
    if entry.size > 0 {
        if records < 0 {
            return false;
        }
        entry.size = match entry.size.checked_mul(records) {
            Some(v) => v,
            None => return false,
        };
    }

    // Unused (block size)
    let block_size = line.token(5);
    if block_size.is_empty() || !is_numeric(block_size) {
        return false;
    }

    entry.flags = 0;

    let date_token = line.token(6);
    if date_token.is_empty() || !parse_short_date(date_token, entry, true) {
        return false;
    }

    let time_token = line.token(7);
    if time_token.is_empty() || !parse_time(time_token, entry) {
        return false;
    }

    let owner_token = line.token(8);
    if owner_token.is_empty() {
        return false;
    }

    if !line.token(9).is_empty() {
        return false;
    }

    entry.owner_group = owner_token.to_string();
    entry.permissions.clear();
    true
}

fn parse_as_hp_nonstop(line: &TokenLine, entry: &mut Entry) -> bool {
    let name_token = line.token(0);
    if name_token.is_empty() {
        return false;
    }
    entry.name = name_token.to_string();

    // File code, numeric, unused
    let code = line.token(1);
    if code.is_empty() || !is_numeric(code) {
        return false;
    }

    let size_token = line.token(2);
    if size_token.is_empty() || !is_numeric(size_token) {
        return false;
    }
    entry.size = parse_leading_number(size_token);
    entry.flags = 0;

    let date_token = line.token(3);
    if date_token.is_empty() || !parse_short_date(date_token, entry, false) {
        return false;
    }

    let time_token = line.token(4);
    if time_token.is_empty() || !parse_time(time_token, entry) {
        return false;
    }

    let owner_token = line.token(5);
    if owner_token.is_empty() {
        return false;
    }
    let mut owner_group = owner_token.to_string();
    let mut index = 6usize;
    if owner_token.ends_with(',') {
        let part2 = line.token(index);
        if part2.is_empty() {
            return false;
        }
        owner_group.push(' ');
        owner_group.push_str(part2);
        index += 1;
    }

    let perm_token = line.token(index);
    if perm_token.is_empty() {
        return false;
    }
    index += 1;

    if !line.token(index).is_empty() {
        return false;
    }

    entry.permissions = perm_token.to_string();
    entry.owner_group = owner_group;
    true
}

fn parse_as_mlsd(line: &TokenLine, entry: &mut Entry) -> u8 {
    let facts = line.token(0);
    if facts.is_empty() {
        return 0;
    }

    entry.flags = 0;
    entry.size = -1;
    entry.time = None;

    let mut owner = "";
    let mut ownername = "";
    let mut group = "";
    let mut groupname = "";
    let mut user = "";
    let mut uid = "";
    let mut gid = "";
    let mut permissions = String::new();

    let mut start = 0usize;
    while start < facts.len() {
        let delim = match facts[start..].find(';') {
            Some(p) => start + p,
            None => facts.len(),
        };
        if delim < facts.len() && delim < start + 3 {
            return 0;
        }

        let pos = match facts[start..].find('=') {
            Some(p) => start + p,
            None => return 0,
        };
        if pos < start + 1 || pos > delim {
            return 0;
        }

        let factname = facts[start..pos].to_ascii_lowercase();
        let value = &facts[pos + 1..delim];

        match factname.as_str() {
            "type" => {
                let (prefix, target) = match value.find(':') {
                    Some(colon) => (
                        value[..colon].to_ascii_lowercase(),
                        Some(&value[colon + 1..]),
                    ),
                    None => (value.to_ascii_lowercase(), None),
                };
                if prefix == "dir" && target.is_none() {
                    entry.flags |= FLAG_DIR;
                } else if prefix == "os.unix=slink" || prefix == "os.unix=symlink" {
                    // We can't distinguish symlinks to links and dirs.
                    entry.flags |= FLAG_DIR | FLAG_LINK;
                    let _ = target;
                } else if (prefix == "cdir" || prefix == "pdir") && target.is_none() {
                    return 2;
                }
            }
            "size" => {
                let Some(size) = parse_strict_i64(value) else {
                    return 0;
                };
                entry.size = size;
            }
            "modify" => {
                let Some(time) = parse_mlsd_time(value) else {
                    return 0;
                };
                entry.time = Some(time);
            }
            "create" if entry.time.is_none() => {
                let Some(time) = parse_mlsd_time(value) else {
                    return 0;
                };
                entry.time = Some(time);
            }
            "perm" => {
                if !value.is_empty() {
                    if permissions.is_empty() {
                        permissions = value.to_string();
                    } else {
                        permissions = format!("{value} ({permissions})");
                    }
                }
            }
            "unix.mode" => {
                if permissions.is_empty() {
                    permissions = value.to_string();
                } else {
                    permissions = format!("{permissions} ({value})");
                }
            }
            "unix.owner" => owner = value,
            "unix.ownername" => ownername = value,
            "unix.group" => group = value,
            "unix.groupname" => groupname = value,
            "unix.user" => user = value,
            "unix.uid" => uid = value,
            "unix.gid" => gid = value,
            _ => {}
        }

        start = delim + 1;
    }

    let mut owner_group = String::new();
    for candidate in [ownername, owner, user, uid] {
        if !candidate.is_empty() {
            owner_group = candidate.to_string();
            break;
        }
    }
    for candidate in [groupname, group, gid] {
        if !candidate.is_empty() {
            owner_group.push(' ');
            owner_group.push_str(candidate);
            break;
        }
    }

    let name_token = line.end_token_with_ws(1);
    if name_token.is_empty() {
        return 0;
    }

    entry.name = name_token.to_string();
    entry.owner_group = owner_group;
    entry.permissions = permissions;
    1
}

/// `fz::datetime(value, utc)` for MLSD `modify`/`create` facts.
fn parse_mlsd_time(value: &str) -> Option<NaiveDateTime> {
    let value = value.trim();
    let base = value.split('.').next().unwrap_or(value);

    if base.len() >= 14 && base.bytes().take(14).all(|b| b.is_ascii_digit()) {
        let year: i32 = base[0..4].parse().ok()?;
        let month: u32 = base[4..6].parse().ok()?;
        let day: u32 = base[6..8].parse().ok()?;
        let hour: u32 = base[8..10].parse().ok()?;
        let minute: u32 = base[10..12].parse().ok()?;
        let second: u32 = base[12..14].parse().ok()?;
        return NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second);
    }

    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d"] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(base, format) {
            return Some(dt);
        }
        if let Ok(date) = NaiveDate::parse_from_str(base, format) {
            return date.and_hms_opt(0, 0, 0);
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Line dispatch
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum LineResult {
    Parsed(Entry),
    /// An MLSD `cdir`/`pdir` entry: valid, but intentionally not listed.
    Skipped,
    NotParsed,
}

fn parse_line(line: &str, server_type: ServerType, maybe_multiline_vms: bool) -> LineResult {
    let tokens = TokenLine::new(line);
    let mut entry = Entry::new();

    if server_type == ServerType::Zvm && parse_as_zvm(&tokens, &mut entry) {
        return LineResult::Parsed(entry);
    }
    if server_type == ServerType::HpNonStop && parse_as_hp_nonstop(&tokens, &mut entry) {
        return LineResult::Parsed(entry);
    }

    match parse_as_mlsd(&tokens, &mut entry) {
        1 => return LineResult::Parsed(entry),
        2 => return LineResult::Skipped,
        _ => {}
    }

    if parse_as_unix(&tokens, &mut entry, true) {
        return LineResult::Parsed(entry);
    }
    if parse_as_dos(&tokens, &mut entry) {
        return LineResult::Parsed(entry);
    }
    if parse_as_eplf(&tokens, &mut entry) {
        return LineResult::Parsed(entry);
    }
    if parse_as_vms(&tokens, &mut entry) {
        return LineResult::Parsed(entry);
    }
    if parse_other(&tokens, &mut entry, maybe_multiline_vms) {
        return LineResult::Parsed(entry);
    }
    if parse_as_ibm(&tokens, &mut entry) {
        return LineResult::Parsed(entry);
    }
    if parse_as_wfftp(&tokens, &mut entry) {
        return LineResult::Parsed(entry);
    }
    if parse_as_ibm_mvs(&tokens, &mut entry) {
        return LineResult::Parsed(entry);
    }
    if parse_as_ibm_mvs_pds(&tokens, &mut entry) {
        return LineResult::Parsed(entry);
    }
    if parse_as_os9(&tokens, &mut entry) {
        return LineResult::Parsed(entry);
    }

    if server_type == ServerType::Mvs {
        if parse_as_ibm_mvs_migrated(&tokens, &mut entry) {
            return LineResult::Parsed(entry);
        }
        if parse_as_ibm_mvs_pds2(&tokens, &mut entry) {
            return LineResult::Parsed(entry);
        }
        if parse_as_ibm_mvs_tape(&tokens, &mut entry) {
            return LineResult::Parsed(entry);
        }
    }

    // 'ls -l' without the date/time (some servers omit it entirely)
    if parse_as_unix(&tokens, &mut entry, false) {
        return LineResult::Parsed(entry);
    }

    LineResult::NotParsed
}

// ---------------------------------------------------------------------------
// Entry conversion
// ---------------------------------------------------------------------------

fn mode_from_permissions(permissions: &str) -> Option<u32> {
    let chars: Vec<char> = permissions.chars().collect();
    if chars.len() >= 10
        && matches!(chars[0], 'd' | '-' | 'l' | 'b' | 'c' | 'p' | 's')
        && chars[1..10]
            .iter()
            .all(|c| matches!(c, 'r' | 'w' | 'x' | 's' | 't' | '-'))
    {
        let mut mode = 0u32;
        let triples = [(1usize, 0o400u32), (4, 0o040), (7, 0o004)];
        for (offset, base) in triples {
            if chars[offset] == 'r' {
                mode |= base;
            }
            if chars[offset + 1] == 'w' {
                mode |= base >> 1;
            }
            if matches!(chars[offset + 2], 'x' | 's' | 't') {
                mode |= base >> 2;
            }
        }
        return Some(mode);
    }

    if !permissions.is_empty()
        && permissions.len() <= 7
        && permissions.bytes().all(|b| (b'0'..=b'7').contains(&b))
    {
        if let Ok(mode) = u32::from_str_radix(permissions, 8) {
            return Some(mode & 0o7777);
        }
    }

    None
}

fn into_remote_entry(entry: Entry, dir: &str) -> Option<RemoteEntry> {
    let is_dir = entry.is_dir();
    let is_symlink = entry.is_link();
    let name = entry.name;
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    let permissions = if entry.permissions.is_empty() {
        None
    } else {
        Some(entry.permissions.clone())
    };
    let mode = permissions.as_deref().and_then(mode_from_permissions);
    Some(RemoteEntry {
        path: join(dir, &name),
        name,
        is_dir,
        is_symlink,
        size: if entry.size < 0 { 0 } else { entry.size as u64 },
        modified: entry
            .time
            .map(|t| t.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
        permissions,
        mode,
        owner: if entry.owner_group.is_empty() {
            None
        } else {
            Some(entry.owner_group)
        },
        // Written listings combine owner and group into one field, exactly as
        // FileZilla's CDirentry::ownerGroup does.
        group: None,
    })
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse one MLSD fact line, e.g.
/// `type=file;size=123;modify=20240112083000;UNIX.mode=0644; README.txt`
pub fn parse_mlsd_line(line: &str, dir: &str) -> Option<RemoteEntry> {
    let dir = normalize(dir);
    let tokens = TokenLine::new(line);
    let mut entry = Entry::new();
    if parse_as_mlsd(&tokens, &mut entry) != 1 {
        return None;
    }
    into_remote_entry(entry, &dir)
}

/// Parse a batch of MLSD lines into entries.
pub fn parse_mlsd(lines: &[String], dir: &str) -> Vec<RemoteEntry> {
    let dir = normalize(dir);
    lines
        .iter()
        .filter_map(|line| parse_mlsd_line(line, &dir))
        .collect()
}

/// Parse LIST output, trying every known listing format per line.
pub fn parse_list(lines: &[String], dir: &str) -> Vec<RemoteEntry> {
    parse_list_for_server(lines, dir, "UNIX")
}

/// Parse LIST output for a specific server dialect.
///
/// `server_type` uses FileZilla's names (`UNIX`, `MVS`, `VMS`, `ZVM`,
/// `HPNONSTOP`); unknown values behave like `UNIX`.  MVS-only formats (bare
/// member names, a PDS member layout and tape labels) are deliberately gated
/// because they are ambiguous with other formats, matching FileZilla.
pub fn parse_list_for_server(lines: &[String], dir: &str, server_type: &str) -> Vec<RemoteEntry> {
    let dir = normalize(dir);
    let server_type = ServerType::from_name(server_type);

    let mut entries: Vec<RemoteEntry> = Vec::new();
    let mut prev_line: Option<String> = None;
    let mut maybe_multiline_vms = false;
    let mut file_list: Vec<String> = Vec::new();
    let mut file_list_only = true;

    for raw in lines {
        for line in split_raw_lines(raw) {
            // `ls -l` always starts with a "total" line which carries no entry.
            if line.starts_with("total ") {
                continue;
            }

            match parse_line(line, server_type, maybe_multiline_vms) {
                LineResult::Parsed(entry) => {
                    if let Some(entry) = into_remote_entry(entry, &dir) {
                        entries.push(entry);
                    }
                    prev_line = None;
                    maybe_multiline_vms = false;
                    file_list.clear();
                    file_list_only = false;
                }
                LineResult::Skipped => {
                    prev_line = None;
                    maybe_multiline_vms = false;
                    file_list.clear();
                    file_list_only = false;
                }
                LineResult::NotParsed => {
                    // Some servers send just a list of filenames.  Remember
                    // single-token lines and use them only if nothing else
                    // in the listing could be parsed.
                    let first = TokenLine::new(line).end_token(0).to_string();
                    if first.is_empty() || first.contains(' ') {
                        maybe_multiline_vms = false;
                        file_list.clear();
                        file_list_only = false;
                    } else {
                        maybe_multiline_vms = first.contains(';');
                        if file_list_only {
                            file_list.push(first);
                        }
                    }

                    // Multi-line listings: retry the previous line together
                    // with this one (e.g. VMS file names spanning two lines).
                    if let Some(prev) = prev_line.take() {
                        let concatenated = format!("{prev} {line}");
                        match parse_line(&concatenated, server_type, false) {
                            LineResult::Parsed(entry) => {
                                if let Some(entry) = into_remote_entry(entry, &dir) {
                                    entries.push(entry);
                                }
                                maybe_multiline_vms = false;
                                file_list.clear();
                                file_list_only = false;
                            }
                            _ => {
                                prev_line = Some(line.to_string());
                                maybe_multiline_vms = false;
                                file_list.clear();
                                file_list_only = false;
                            }
                        }
                    } else {
                        prev_line = Some(line.to_string());
                    }
                }
            }
        }
    }

    // Raw file listing fallback.
    if file_list_only && entries.is_empty() && !file_list.is_empty() {
        entries.extend(file_list.into_iter().filter_map(|name| {
            into_remote_entry(
                Entry {
                    name,
                    ..Entry::new()
                },
                &dir,
            )
        }));
    }

    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mlsd() {
        let lines = vec![
            "type=dir;size=0;modify=20240112083000;UNIX.mode=0755; sub dir".to_string(),
            "type=file;size=1234;modify=20240112083000.123;UNIX.mode=0644; readme.txt".to_string(),
            "type=cdir;modify=20240112083000; .".to_string(),
            "type=OS.unix=symlink;size=7;modify=20240112083000; link".to_string(),
        ];
        let entries = parse_mlsd(&lines, "/pub");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "sub dir");
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].path, "/pub/sub dir");
        assert_eq!(entries[1].size, 1234);
        assert_eq!(entries[1].permissions.as_deref(), Some("0644"));
        assert_eq!(entries[1].mode, Some(0o644));
        assert_eq!(entries[1].modified.as_deref(), Some("2024-01-12T08:30:00Z"));
        assert!(entries[2].is_symlink);
        assert!(entries[2].is_dir);
    }

    #[test]
    fn parses_unix_list() {
        let lines = vec![
            "drwxr-xr-x    2 ftp      ftp          4096 Jan 12 12:34 folder name".to_string(),
            "-rw-r--r--    1 ftp      ftp          1024 Dec 31  2023 file.txt".to_string(),
            "lrwxrwxrwx    1 ftp      ftp             7 Jan 12 12:34 latest -> v2".to_string(),
            "total 4".to_string(),
        ];
        let entries = parse_list(&lines, "/var/www");
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().any(|e| e.name == "folder name" && e.is_dir));
        assert!(entries
            .iter()
            .any(|e| e.name == "file.txt" && e.size == 1024));
        let link = entries.iter().find(|e| e.name == "latest").unwrap();
        assert!(link.is_symlink);
        assert_eq!(link.path, "/var/www/latest");
    }

    #[test]
    fn parses_dos_list() {
        let lines = vec![
            "01-12-24  12:34PM       <DIR>          My Folder".to_string(),
            "01-12-2024  12:34PM                12345 report final.pdf".to_string(),
        ];
        let entries = parse_list(&lines, "/");
        assert_eq!(entries.len(), 2);
        let dir = entries.iter().find(|e| e.name == "My Folder").unwrap();
        assert!(dir.is_dir);
        let file = entries
            .iter()
            .find(|e| e.name == "report final.pdf")
            .unwrap();
        assert_eq!(file.size, 12345);
    }

    #[test]
    fn parses_vms_list() {
        let lines = vec![
            "36-vms-dir.DIR;1  1 19-NOV-2001 21:41 [root,root] (RWE,RWE,RE,RE)".to_string(),
            "38-vms-notime-file;1    2/8    7-JAN-2000    [IV2_XXX]   (RWED,RWED,RE,)".to_string(),
        ];
        let entries = parse_list(&lines, "/");
        assert_eq!(entries.len(), 2);
        let dir = entries.iter().find(|e| e.name == "36-vms-dir").unwrap();
        assert!(dir.is_dir);
        assert_eq!(dir.size, 512);
        let file = entries
            .iter()
            .find(|e| e.name == "38-vms-notime-file;1")
            .unwrap();
        assert_eq!(file.size, 1024);
    }

    #[test]
    fn parses_vms_multiline_entry() {
        let lines = vec![
            "40-vms-multiline-file;1\r\n170774/170775     24-APR-2003 08:16:15  [FTP_CLIENT,SCOT]      (RWED,RWED,RE,)".to_string(),
        ];
        let entries = parse_list(&lines, "/");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "40-vms-multiline-file;1");
        assert_eq!(entries[0].size, 87436288);
    }

    #[test]
    fn parses_mvs_with_server_type() {
        let lines = vec![
            "WYOSPT 3420   2003/05/21  1  200  FB      80  8053  PS  60-MVS.FILE".to_string(),
            "WPTA01 3290   2004/03/04  1    3  FB      80  3125  PO  61-MVS.DATASET".to_string(),
        ];
        let entries = parse_list_for_server(&lines, "/", "MVS");
        assert_eq!(entries.len(), 2);
        let file = entries.iter().find(|e| e.name == "60-MVS.FILE").unwrap();
        assert_eq!(file.size, 100);
        assert!(!file.is_dir);
        let dir = entries.iter().find(|e| e.name == "61-MVS.DATASET").unwrap();
        assert!(dir.is_dir);

        let member = parse_list_for_server(&["65-MVS-PDS-MEMBER".to_string()], "/", "MVS");
        assert_eq!(member.len(), 1);
        assert_eq!(member[0].name, "65-MVS-PDS-MEMBER");
    }

    #[test]
    fn parses_eplf_and_mlsd_whitespace() {
        let eplf = parse_list_for_server(
            &["+i8388621.48594,m825718503,r,s280,up755\t14-eplf file".to_string()],
            "/",
            "UNIX",
        );
        assert_eq!(eplf.len(), 1);
        assert_eq!(eplf[0].size, 280);
        assert_eq!(eplf[0].permissions.as_deref(), Some("755"));

        let trailing = parse_mlsd_line("Type=file; 74-MLSD-whitespace trailing\t ", "/").unwrap();
        assert_eq!(trailing.name, "74-MLSD-whitespace trailing\t ");
        let leading = parse_mlsd_line("Type=file; \t 75-MLSD-whitespace leading", "/").unwrap();
        assert_eq!(leading.name, "\t 75-MLSD-whitespace leading");
    }
}
