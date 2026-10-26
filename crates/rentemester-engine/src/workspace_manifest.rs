//! Strict workspace registration metadata. Parsing never selects or rewrites a workspace.
use crate::Error;
use serde::{Deserialize, Deserializer, Serialize};

pub const MAX_BYTES: usize = 1_048_576;
pub const MAX_COMPANIES: usize = 1000;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Purpose {
    #[default]
    Live,
    Test,
    DryRun,
    Restore,
    Backup,
    Retest,
    Baseline,
}
// Absent means live; explicit null is not absence. Preserve that distinction
// until version validation so even `purpose: "live"` is refused in v1.
fn purpose<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Purpose>, D::Error> {
    Purpose::deserialize(d).map(Some)
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Company {
    pub slug: String,
    pub name: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    pub archived: bool,
    #[serde(
        default,
        deserialize_with = "purpose",
        skip_serializing_if = "Option::is_none"
    )]
    pub purpose: Option<Purpose>,
}
impl Company {
    pub fn purpose(&self) -> Purpose {
        self.purpose.unwrap_or_default()
    }
    pub fn active(&self) -> bool {
        !self.archived && self.purpose() == Purpose::Live
    }
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub companies: Vec<Company>,
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
pub fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 1024
        && (s.as_bytes()[0].is_ascii_lowercase() || s.as_bytes()[0].is_ascii_digit())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
pub fn parse(bytes: &[u8]) -> Result<Manifest, Error> {
    let refused = || Error::new("invalid_workspace_manifest", "workspace manifest refused");
    if bytes.len() > MAX_BYTES {
        return Err(refused());
    }
    let m: Manifest = serde_json::from_slice(bytes).map_err(|_| refused())?;
    if !matches!(m.version, 1 | 2) || m.companies.len() > MAX_COMPANIES {
        return Err(refused());
    }
    let mut seen = std::collections::HashSet::new();
    for c in &m.companies {
        if !valid_slug(&c.slug)
            || !text(&c.name, 1024)
            || !text(&c.created_at, 128)
            || !seen.insert(&c.slug)
            || (m.version == 1 && c.purpose.is_some())
        {
            return Err(refused());
        }
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn entry() -> serde_json::Value {
        json!({"slug":"synthetic-a","name":"Synthetic A","createdAt":"2026-01-01","archived":false})
    }
    #[test]
    fn versions_and_all_purposes_preserve_strict_absence_and_live_selection() {
        for version in [1, 2] {
            let raw =
                serde_json::to_vec(&json!({"version":version,"companies":[entry()]})).unwrap();
            let m = parse(&raw).unwrap();
            assert_eq!(m.companies[0].purpose(), Purpose::Live);
            assert!(m.companies[0].active());
            assert_eq!(
                serde_json::to_value(&m).unwrap(),
                serde_json::from_slice::<serde_json::Value>(&raw).unwrap()
            );
        }
        for p in [
            "live", "test", "dry-run", "restore", "backup", "retest", "baseline",
        ] {
            let mut c = entry();
            c["purpose"] = json!(p);
            assert!(
                parse(&serde_json::to_vec(&json!({"version":1,"companies":[c.clone()]})).unwrap())
                    .is_err()
            );
            let raw = serde_json::to_vec(&json!({"version":2,"companies":[c.clone()]})).unwrap();
            let m = parse(&raw).unwrap();
            assert_eq!(m.companies[0].active(), p == "live");
            assert_eq!(json!(m.companies[0].purpose()), p);
            c["archived"] = json!(true);
            assert!(
                !parse(&serde_json::to_vec(&json!({"version":2,"companies":[c]})).unwrap())
                    .unwrap()
                    .companies[0]
                    .active()
            );
        }
    }
    #[test]
    fn ambiguous_coerced_unknown_and_oversized_metadata_is_refused() {
        for p in [
            json!(null),
            json!(false),
            json!(1),
            json!("LIVE"),
            json!("unknown"),
            json!([]),
        ] {
            let mut c = entry();
            c["purpose"] = p;
            assert!(
                parse(&serde_json::to_vec(&json!({"version":2,"companies":[c]})).unwrap()).is_err()
            );
        }
        let valid = serde_json::to_string(&json!({"version":2,"companies":[entry()]})).unwrap();
        for bad in [
            valid.replace("\"version\":2", "\"version\":3"),
            valid.replace("\"version\":2", "\"version\":2,\"version\":2"),
            valid.replace("\"archived\":false", "\"archived\":\"false\""),
            valid.replace(
                "\"archived\":false",
                "\"purpose\":\"live\",\"purpose\":\"test\",\"archived\":false",
            ),
            valid.replace("\"version\":2", "\"extra\":1,\"version\":2"),
        ] {
            assert!(parse(bad.as_bytes()).is_err(), "{bad}");
        }
        assert!(
            parse(
                &serde_json::to_vec(&json!({"version":2,"companies":[entry(),entry()]})).unwrap()
            )
            .is_err()
        );
        assert!(parse(&vec![b' '; MAX_BYTES + 1]).is_err());
        let mut c = entry();
        c["slug"] = json!("../test");
        assert!(
            parse(&serde_json::to_vec(&json!({"version":2,"companies":[c]})).unwrap()).is_err()
        );
    }
}
