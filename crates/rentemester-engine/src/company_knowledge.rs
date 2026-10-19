//! Advisory assertion policy. This module never selects accounting behavior.
use crate::Error;
pub const PREDICATES: &[&str] = &[
    "business_description",
    "operating_status",
    "products_services",
    "revenue_models",
    "markets",
    "customer_types",
    "industry_tags",
    "operational_systems",
    "document_sources",
    "reporting_dimensions",
    "responsible_party",
    "canonical_reference",
];
pub const SOURCES: &[&str] = &[
    "user",
    "registry",
    "party",
    "corporate_record",
    "external_snapshot",
    "canonical_setting",
];
pub fn singleton(predicate: &str) -> bool {
    matches!(
        predicate,
        "business_description" | "operating_status" | "canonical_reference"
    )
}
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("knowledge_refused", message)
}
pub fn trim(s: &str) -> &str {
    s.trim_matches(|c: char| c != '\u{0085}' && (c.is_whitespace() || c == '\u{feff}'))
}
pub fn text(s: &str, max: usize) -> Result<String, Error> {
    let s = trim(s);
    if s.is_empty() || s.encode_utf16().count() > max {
        Err(refused("bounded nonblank text required"))
    } else {
        Ok(s.into())
    }
}
/// ISO date or ISO datetime with explicit offset; canonical UTC millisecond precision.
/// Native HTTP intentionally rejects locale-dependent JavaScript date spellings.
pub fn iso(input: &str) -> Result<String, Error> {
    let s = text(input, 64)?;
    let bad = || refused("ISO date or datetime with explicit timezone required");
    if !s.is_ascii() || s.len() < 10 || !crate::calendar::valid_iso_date(&s[..10]) {
        return Err(bad());
    }
    let n = |a: usize, b: usize| {
        s.get(a..b)
            .ok_or_else(bad)?
            .parse::<u16>()
            .map_err(|_| bad())
    };
    let date = time::Date::from_calendar_date(
        i32::from(n(0, 4)?),
        time::Month::try_from(n(5, 7)? as u8).map_err(|_| bad())?,
        n(8, 10)? as u8,
    )
    .map_err(|_| bad())?;
    let (h, m, sec, ms, offset) = if s.len() == 10 {
        (0, 0, 0, 0, 0)
    } else {
        if s.len() < 20 || &s[10..11] != "T" || &s[13..14] != ":" || &s[16..17] != ":" {
            return Err(bad());
        }
        let mut pos = 19;
        let mut ms = 0;
        if s.as_bytes().get(pos) == Some(&b'.') {
            pos += 1;
            let start = pos;
            while s.as_bytes().get(pos).is_some_and(u8::is_ascii_digit) {
                pos += 1;
            }
            if start == pos {
                return Err(bad());
            }
            let frac = &s[start..pos];
            ms = format!("{frac:0<3}")[..3]
                .parse::<u16>()
                .map_err(|_| bad())?;
        }
        let tz = &s[pos..];
        let offset = if tz == "Z" {
            0
        } else if tz.len() == 6 && matches!(tz.as_bytes()[0], b'+' | b'-') && &tz[3..4] == ":" {
            let oh = tz[1..3].parse::<i32>().map_err(|_| bad())?;
            let om = tz[4..6].parse::<i32>().map_err(|_| bad())?;
            if oh > 23 || om > 59 {
                return Err(bad());
            }
            (oh * 60 + om) * if tz.starts_with('-') { -1 } else { 1 }
        } else {
            return Err(bad());
        };
        (
            n(11, 13)? as u8,
            n(14, 16)? as u8,
            n(17, 19)? as u8,
            ms,
            offset,
        )
    };
    let t = time::PrimitiveDateTime::new(
        date,
        time::Time::from_hms_milli(h, m, sec, ms).map_err(|_| bad())?,
    )
    .assume_utc()
    .checked_sub(time::Duration::minutes(i64::from(offset)))
    .ok_or_else(bad)?;
    if !(1..=9999).contains(&t.year()) {
        return Err(bad());
    }
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
        t.millisecond()
    ))
}
pub fn timestamp(now: i64) -> Result<String, Error> {
    let t = time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(now) * 1_000_000)
        .map_err(|_| refused("invalid clock"))?;
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second(),
        t.millisecond()
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates() {
        assert_eq!(iso("2026-02-01").unwrap(), "2026-02-01T00:00:00.000Z");
        assert_eq!(
            iso("2026-02-01T01:30:00.12+01:30").unwrap(),
            "2026-02-01T00:00:00.120Z"
        );
        for s in [
            "2026-02-30",
            "2026-01-01T00:00:00",
            "2026-01-01T00:00:00.Z",
            "2026-01-01T24:00:00Z",
        ] {
            assert!(iso(s).is_err(), "{s}");
        }
    }
}
