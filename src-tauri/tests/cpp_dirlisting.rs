//! Directory-listing fixtures extracted from FileZilla's C++ test suite.
//!
//! The fixture is generated from `tests/dirparsertest.cpp`:
//!   node scripts/extract-cpp-listing-tests.mjs ../tests/dirparsertest.cpp \
//!        src-tauri/tests/fixtures/dirlisting_cpp.json
//!
//! Every case is asserted on name, size, directory/link flags, permissions and
//! owner/group.  `modified` is checked too, but the C++ fixture stores UTC
//! while LIST timestamps are server-local: for dates that the parser derives
//! relative to "now" (no year in the listing) only the month/day/time
//! components are compared, since the fixture froze the year it was generated
//! in.  Symlink targets cannot be asserted because `RemoteEntry` has no target
//! field; `is_link` is asserted for those cases.

use chrono::{Datelike, NaiveDateTime, Timelike};
use filezilla_rs_lib::list_parser::{parse_list_for_server, parse_mlsd_line};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct Case {
    line: String,
    name: Option<String>,
    size: Option<i64>,
    permissions: Option<String>,
    owner: Option<String>,
    #[allow(dead_code)]
    link: Option<String>,
    modified: Option<String>,
    is_dir: bool,
    is_link: bool,
    server_type: String,
}

/// Cases that are genuinely not supported, with the reason why.  They are
/// still counted in the per-format totals but skipped by the assertions so a
/// real regression elsewhere stays visible.  Keep this list empty when
/// possible; an entry must name an existing fixture.
const UNSUPPORTED: &[(&str, &str)] = &[];

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/dirlisting_cpp.json")).expect("valid fixture JSON")
}

fn first_token(line: &str) -> &str {
    line.split([' ', '\t']).next().unwrap_or("")
}

/// MLSD listings are recognised by their leading fact.  Everything else is a
/// LIST line and goes through the format autodetection (with the server type
/// from the fixture, as some formats are gated on it).
fn is_mlsd(case: &Case) -> bool {
    let token = first_token(&case.line).to_ascii_lowercase();
    let fact = token.split('=').next().unwrap_or("");
    matches!(fact, "type" | "size" | "modify" | "create") && token.contains('=')
}

fn parse_case(case: &Case) -> Option<filezilla_rs_lib::models::RemoteEntry> {
    if is_mlsd(case) {
        return parse_mlsd_line(&case.line, "/");
    }
    parse_list_for_server(&[case.line.clone()], "/", &case.server_type)
        .into_iter()
        .next()
}

fn parse_timestamp(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%SZ").ok()
}

/// Compare timestamps, ignoring the year for dates the parser derived from
/// "now" (the C++ fixture froze the extraction year in `modified`).
fn timestamps_match(want: &NaiveDateTime, got: &NaiveDateTime) -> bool {
    if want.year() >= 2025 {
        want.month() == got.month()
            && want.day() == got.day()
            && want.hour() == got.hour()
            && want.minute() == got.minute()
            && want.second() == got.second()
    } else {
        want == got
    }
}

#[test]
fn cpp_dirlisting_fixtures() {
    let fixture = fixture();
    assert_eq!(fixture.cases.len(), 88, "fixture case count changed");

    let mut checked = 0usize;
    let mut failures = Vec::new();
    let mut per_type: std::collections::BTreeMap<String, (usize, usize)> =
        std::collections::BTreeMap::new();
    let mut missing_unsupported = Vec::new();

    for unsupported in UNSUPPORTED {
        if !fixture
            .cases
            .iter()
            .any(|case| case.name.as_deref() == Some(unsupported.0))
        {
            missing_unsupported.push(unsupported.0.to_string());
        }
    }
    assert!(
        missing_unsupported.is_empty(),
        "UNSUPPORTED names unknown fixtures: {missing_unsupported:?}"
    );

    for case in &fixture.cases {
        let entry = per_type.entry(case.server_type.clone()).or_insert((0, 0));
        entry.1 += 1;

        if UNSUPPORTED
            .iter()
            .any(|(name, _)| case.name.as_deref() == Some(name))
        {
            continue;
        }

        let Some(parsed) = parse_case(case) else {
            failures.push(format!("no parse [{}]: {:?}", case.server_type, case.line));
            continue;
        };
        checked += 1;
        entry.0 += 1;

        let want_name = case.name.as_deref().unwrap_or_default();
        if parsed.name != want_name {
            failures.push(format!(
                "name: {:?} => {:?} (want {:?})",
                case.line, parsed.name, want_name
            ));
        }
        if parsed.is_dir != case.is_dir {
            failures.push(format!(
                "is_dir: {:?} => {} (want {})",
                case.line, parsed.is_dir, case.is_dir
            ));
        }
        if parsed.is_symlink != case.is_link {
            failures.push(format!(
                "is_link: {:?} => {} (want {})",
                case.line, parsed.is_symlink, case.is_link
            ));
        }
        match case.size {
            Some(want) if want >= 0 => {
                if parsed.size as i64 != want {
                    failures.push(format!(
                        "size: {:?} => {} (want {})",
                        case.line, parsed.size, want
                    ));
                }
            }
            Some(_) => {
                // The C++ parser uses -1 for "unknown size"; RemoteEntry uses
                // 0.  Assert that we did not invent a size.
                if parsed.size != 0 {
                    failures.push(format!(
                        "size: {:?} => {} (want unknown/-1)",
                        case.line, parsed.size
                    ));
                }
            }
            None => {}
        }
        if parsed.permissions.as_deref() != case.permissions.as_deref() {
            failures.push(format!(
                "permissions: {:?} => {:?} (want {:?})",
                case.line, parsed.permissions, case.permissions
            ));
        }
        if parsed.owner.as_deref() != case.owner.as_deref() {
            failures.push(format!(
                "owner: {:?} => {:?} (want {:?})",
                case.line, parsed.owner, case.owner
            ));
        }
        match &case.modified {
            Some(want) => match (parse_timestamp(want), parsed.modified.as_deref()) {
                (Some(want), Some(got)) => match parse_timestamp(got) {
                    Some(got) if timestamps_match(&want, &got) => {}
                    Some(got) => failures.push(format!(
                        "modified: {:?} => {:?} (want {:?})",
                        case.line, got, want
                    )),
                    None => failures.push(format!(
                        "modified: {:?} => unparsable {:?} (want {:?})",
                        case.line, parsed.modified, want
                    )),
                },
                (Some(want), None) => failures.push(format!(
                    "modified: {:?} => none (want {:?})",
                    case.line, want
                )),
                (None, _) => failures.push(format!(
                    "fixture modified is not RFC3339: {want:?} ({:?})",
                    case.line
                )),
            },
            None => {
                if parsed.modified.is_some() {
                    failures.push(format!(
                        "modified: {:?} => {:?} (want none)",
                        case.line, parsed.modified
                    ));
                }
            }
        }
    }

    for (server_type, (passed, total)) in &per_type {
        eprintln!("{server_type}: {passed}/{total} passed");
    }
    if !UNSUPPORTED.is_empty() {
        for (name, reason) in UNSUPPORTED {
            eprintln!("UNSUPPORTED {name}: {reason}");
        }
    }

    assert!(
        failures.is_empty(),
        "{} mismatch(es) across {} checked cases:\n{}",
        failures.len(),
        checked,
        failures.join("\n")
    );
}
