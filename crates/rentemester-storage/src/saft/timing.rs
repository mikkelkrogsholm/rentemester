//! Export metadata timestamps only; authorization continues to use the runtime clock.
use super::{Input, refused, saft};
use rentemester_engine::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
pub(super) struct Generation {
    pub timestamp: String,
    pub source: &'static str,
    pub explicit: bool,
}
impl Generation {
    pub(super) fn resolve(input: &Input, now_ms: i64) -> Result<Self, Error> {
        let (timestamp, source, explicit) = match (
            input.generation_mode.as_deref(),
            input.generated_at.as_deref(),
        ) {
            (None, None) => (saft::timestamp(now_ms)?, "trusted_runtime_clock", false),
            (Some("period_end"), None) => {
                rentemester_engine::fiscal_year::Date::parse(&input.period_end)?;
                (
                    format!("{}T23:59:59.000Z", input.period_end),
                    "period_end",
                    false,
                )
            }
            (Some("caller_supplied"), Some(raw)) => {
                if raw.is_empty() || raw.len() > 80 || raw.chars().any(char::is_control) {
                    return Err(refused(
                        "generatedAt must be RFC3339 with explicit timezone",
                    ));
                }
                let parsed = OffsetDateTime::parse(raw, &Rfc3339)
                    .map_err(|_| refused("generatedAt must be RFC3339 with explicit timezone"))?;
                if raw.get(17..19) == Some("60") {
                    return Err(refused("generatedAt leap seconds are unsupported"));
                }
                // Floor rather than truncate: -0.5ms belongs to 1969-12-31T23:59:59.999Z.
                let ms: i64 = parsed
                    .unix_timestamp_nanos()
                    .div_euclid(1_000_000)
                    .try_into()
                    .map_err(|_| refused("generatedAt exceeds supported timestamp range"))?;
                (saft::timestamp(ms)?, "caller_supplied", true)
            }
            (None, Some(_)) => {
                return Err(refused(
                    "generatedAt requires generationMode caller_supplied",
                ));
            }
            (Some("period_end"), Some(_)) => {
                return Err(refused(
                    "period_end generationMode does not accept generatedAt",
                ));
            }
            (Some("caller_supplied"), None) => {
                return Err(refused(
                    "caller_supplied generationMode requires generatedAt",
                ));
            }
            _ => {
                return Err(refused(
                    "generationMode must be period_end or caller_supplied when provided",
                ));
            }
        };
        Ok(Self {
            timestamp,
            source,
            explicit,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn input(mode: Option<&str>, raw: Option<&str>) -> Input {
        Input {
            period_end: "2027-06-30".into(),
            generation_mode: mode.map(str::to_owned),
            generated_at: raw.map(str::to_owned),
            ..Input::default()
        }
    }
    #[test]
    fn modes_normalize_utc_milliseconds_including_pre_epoch_fraction_floor() {
        let native = Generation::resolve(&input(None, None), 1767225600000).unwrap();
        assert_eq!(native.timestamp, "2026-01-01T00:00:00.000Z");
        assert_eq!(native.source, "trusted_runtime_clock");
        assert!(!native.explicit);
        let period = Generation::resolve(&input(Some("period_end"), None), 1767225600000).unwrap();
        assert_eq!(period.timestamp, "2027-06-30T23:59:59.000Z");
        assert_eq!(period.source, "period_end");
        assert!(!period.explicit);
        for (raw, expected) in [
            (
                "2026-01-01T01:02:03.987654+02:00",
                "2025-12-31T23:02:03.987Z",
            ),
            ("1969-12-31T23:59:59.9995Z", "1969-12-31T23:59:59.999Z"),
            ("2024-02-29T23:30:00-02:00", "2024-03-01T01:30:00.000Z"),
        ] {
            let caller =
                Generation::resolve(&input(Some("caller_supplied"), Some(raw)), 0).unwrap();
            assert_eq!(caller.timestamp, expected);
            assert_eq!(caller.source, "caller_supplied");
            assert!(caller.explicit);
        }
    }
    #[test]
    fn ambiguous_combinations_invalid_calendar_local_and_overflow_refuse() {
        for (mode, raw) in [
            (None, Some("2026-01-01T00:00:00Z")),
            (Some("period_end"), Some("2026-01-01T00:00:00Z")),
            (Some("caller_supplied"), None),
            (Some("unknown"), None),
            (Some("caller_supplied"), Some("2026-02-30T00:00:00Z")),
            (Some("caller_supplied"), Some("2026-01-01T00:00:00")),
            (Some("caller_supplied"), Some("2026-01-01")),
            (Some("caller_supplied"), Some("2026-01-01T00:00:60Z")),
            (Some("caller_supplied"), Some("9999-12-31T23:59:59-01:00")),
            (Some("caller_supplied"), Some("")),
        ] {
            assert!(
                Generation::resolve(&input(mode, raw), 0).is_err(),
                "{mode:?}/{raw:?}"
            );
        }
    }
}
