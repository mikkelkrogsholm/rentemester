//! Recipient and request metadata only; no wall clock or source/accounting decisions.
use super::{Input, refused};
use rentemester_engine::Error;
use time::{Duration, OffsetDateTime, UtcOffset, format_description::well_known::Rfc3339};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Profile {
    Accountant,
    Authority,
}
impl Profile {
    pub(super) fn package_type(self) -> &'static str {
        match self {
            Self::Accountant => "accountant_handoff_export",
            Self::Authority => "authority_export",
        }
    }
    pub(super) fn prefix(self) -> &'static str {
        match self {
            Self::Accountant => "revisor-eksport",
            Self::Authority => "myndigheds-eksport",
        }
    }
}
pub(super) struct Context {
    pub profile: Profile,
    pub requested_at: Option<String>,
    pub generated_at: String,
    pub deadline_at: Option<String>,
    pub requester: Option<String>,
}
fn timestamp(value: OffsetDateTime) -> Result<String, Error> {
    let value = value.to_offset(UtcOffset::UTC);
    if !(0..=9999).contains(&value.year()) {
        return Err(refused(
            "requestedAt or deadlineAt exceeds supported ISO year range",
        ));
    }
    Ok(format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        value.year(),
        u8::from(value.month()),
        value.day(),
        value.hour(),
        value.minute(),
        value.second(),
        value.millisecond()
    ))
}
impl Context {
    pub(super) fn resolve(input: &Input) -> Result<Self, Error> {
        let profile = match input.package_profile.as_deref() {
            None | Some("accountant_handoff") => Profile::Accountant,
            Some("authority") => Profile::Authority,
            _ => {
                return Err(refused(
                    "packageProfile must be authority or accountant_handoff",
                ));
            }
        };
        if input
            .requester
            .as_ref()
            .is_some_and(|s| s.chars().count() > 256 || s.chars().any(char::is_control))
        {
            return Err(refused(
                "requester must contain at most 256 characters without control characters",
            ));
        }
        let (requested_at, deadline_at) = match input.requested_at.as_deref() {
            None => (None, None),
            Some(raw) => {
                if raw.len() > 80 || raw.is_empty() || raw.chars().any(char::is_control) {
                    return Err(refused(
                        "requestedAt must be a valid RFC3339 datetime with explicit timezone",
                    ));
                }
                let value = OffsetDateTime::parse(raw, &Rfc3339).map_err(|_| {
                    refused("requestedAt must be a valid RFC3339 datetime with explicit timezone")
                })?;
                // RFC3339's parser permits leap-second notation; JS Date.parse did not.
                if raw.get(17..19) == Some("60") {
                    return Err(refused("requestedAt leap seconds are unsupported"));
                }
                let deadline = value
                    .checked_add(Duration::days(28))
                    .ok_or_else(|| refused("requestedAt deadline overflow"))?;
                (Some(timestamp(value)?), Some(timestamp(deadline)?))
            }
        };
        Ok(Self {
            profile,
            generated_at: requested_at
                .clone()
                .unwrap_or_else(|| format!("{}T23:59:59.000Z", input.period_end)),
            requested_at,
            deadline_at,
            requester: input.requester.clone(),
        })
    }
    pub(super) fn root(&self, start: &str, end: &str) -> String {
        let stamp = self.generated_at.replace(['-', ':'], "");
        let stamp = stamp.strip_suffix('Z').unwrap().split('.').next().unwrap();
        format!("{}-{start}_{end}_{stamp}Z", self.profile.prefix())
    }
    pub(super) fn readme(&self, start: &str, end: &str, legacy: String) -> String {
        if self.profile == Profile::Accountant
            && self.requested_at.is_none()
            && self.requester.is_none()
        {
            return legacy;
        }
        let generated = if self.requested_at.is_some() {
            format!("Genereret: {}", self.generated_at)
        } else {
            format!(
                "Genereret (deterministisk, udledt af periodeslut — ikke et reelt ur-tidspunkt): {}",
                self.generated_at
            )
        };
        let title = match self.profile {
            Profile::Accountant => "revisor-eksportpakke",
            Profile::Authority => "myndighedseksportpakke",
        };
        let scope = match self.profile {
            Profile::Accountant => {
                "Lokal overdragelsespakke uden adgang til virksomhedens kørende data. Pakken er ikke en myndighedseksport eller en signeret backup."
            }
            Profile::Authority => {
                "Lokal myndighedsoverdragelsespakke til gennemsyn. Pakken er ikke en digital indberetning eller en signeret backup."
            }
        };
        format!(
            "Rentemester — {title}\n\nPeriode: {start}..{end}\nRekvirent: {}\nAnmodet: {}\n{generated}\nFrist: {}\n\n{scope}\nJSON og CSV findes i machine-readable/. Originaler findes i documents-readable/. Manifestet angiver SHA-256 og byteantal for hver fil.\nEksporten ændrer ingen posteringer. Audit-loggen indeholder revisionssporet før denne eksport; eksporthandlingen registreres efter pakningen.\n\nBegrænsning: Rust-buildets fulde release-/regelkorpus-proveniens er endnu ikke tilgængelig; provenance er null. Skemaets migrationsidentiteter følger med. requestedAt kræver RFC3339 med eksplicit tidszone; dato alene og lokale tidsstempler understøttes ikke.\n",
            self.requester.as_deref().unwrap_or("ikke angivet"),
            self.requested_at.as_deref().unwrap_or("ikke angivet"),
            self.deadline_at.as_deref().unwrap_or("ikke relevant")
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn context(raw: &str) -> Result<Context, Error> {
        Context::resolve(&Input {
            period_end: "2026-01-31".into(),
            requested_at: Some(raw.into()),
            ..Input::default()
        })
    }
    #[test]
    fn utc_offsets_fractions_leap_and_year_rollover_have_exact_28_day_deadline() {
        for (raw, normalized, deadline) in [
            (
                "2024-02-01T01:02:03.987654+01:00",
                "2024-02-01T00:02:03.987Z",
                "2024-02-29T00:02:03.987Z",
            ),
            (
                "2025-12-20T23:30:00-02:00",
                "2025-12-21T01:30:00.000Z",
                "2026-01-18T01:30:00.000Z",
            ),
            (
                "2026-03-01T00:00:00Z",
                "2026-03-01T00:00:00.000Z",
                "2026-03-29T00:00:00.000Z",
            ),
        ] {
            let c = context(raw).unwrap();
            assert_eq!(c.requested_at.as_deref(), Some(normalized));
            assert_eq!(c.deadline_at.as_deref(), Some(deadline));
            assert_eq!(c.generated_at, normalized);
        }
    }
    #[test]
    fn malformed_impossible_local_and_overflow_timestamps_are_refused() {
        for raw in [
            "2026-02-30T10:00:00Z",
            "2026-01-01",
            "2026-01-01T10:00:00",
            "",
            "2026-01-01T10:00:60Z",
            "9999-12-31T23:59:59Z",
        ] {
            assert!(context(raw).is_err(), "{raw}");
        }
        let c = Context::resolve(&Input {
            period_end: "2026-07-31".into(),
            ..Input::default()
        })
        .unwrap();
        assert!(c.requested_at.is_none());
        assert!(c.deadline_at.is_none());
        assert_eq!(c.generated_at, "2026-07-31T23:59:59.000Z");
    }
}
