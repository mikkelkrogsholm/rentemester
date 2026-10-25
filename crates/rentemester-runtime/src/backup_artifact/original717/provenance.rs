//! Source isReleaseProvenance rules, not a claim that Rust is a TypeScript release.
use serde_json::Value;
fn hex(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn digest(s: &str) -> bool {
    s.strip_prefix("sha256:").is_some_and(|s| hex(s, 64))
}
fn identifiers(s: &str, numeric_zero_rule: bool) -> bool {
    s.split('.').all(|p| {
        !p.is_empty()
            && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && (!numeric_zero_rule
                || !p.bytes().all(|b| b.is_ascii_digit())
                || p.len() == 1
                || !p.starts_with('0'))
    })
}
fn semver(s: &str) -> bool {
    let (s, build) = s.split_once('+').map_or((s, None), |(a, b)| (a, Some(b)));
    if build.is_some_and(|b| !identifiers(b, false)) {
        return false;
    }
    let (core, pre) = s.split_once('-').map_or((s, None), |(a, b)| (a, Some(b)));
    let parts: Vec<_> = core.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.bytes().all(|b| b.is_ascii_digit())
                && (p.len() == 1 || !p.starts_with('0'))
        })
        && pre.is_none_or(|p| identifiers(p, true))
}
fn nullable(v: Option<&Value>, check: impl FnOnce(&str) -> bool) -> bool {
    v.is_some_and(|v| v.is_null() || v.as_str().is_some_and(check))
}
pub(super) fn valid(v: &Value) -> bool {
    let p = &v["product"];
    let s = &v["schema"];
    let r = &v["rules"];
    p["version"].as_str().is_some_and(semver)
        && nullable(p.get("gitCommit"), |s| {
            (7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
        })
        && nullable(p.get("builtAt"), |s| {
            chrono::DateTime::parse_from_rfc3339(s).is_ok()
                || chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
        })
        && nullable(p.get("bunVersion"), semver)
        && nullable(p.get("baseImageDigest"), digest)
        && s["version"].as_u64().is_some_and(|n| n > 0)
        && s["baselineChecksum"].as_str().is_some_and(|s| hex(s, 64))
        && r["digest"].as_str().is_some_and(digest)
}
