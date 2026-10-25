//! Presentation clock formatting only; never used to authorize or backdate an operation.
pub fn format_instant(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|d| {
            d.with_timezone(&chrono::Utc)
                .format("%Y-%m-%d %H:%M UTC")
                .to_string()
        })
        .unwrap_or_else(|| value.to_string())
}
#[cfg(test)]
mod tests {
    #[test]
    fn original716_metadata_instant_is_utc_or_literal() {
        assert_eq!(
            super::format_instant("2026-01-01T23:34:56+02:00"),
            "2026-01-01 21:34 UTC"
        );
        assert_eq!(super::format_instant("<literal>"), "<literal>");
    }
}
