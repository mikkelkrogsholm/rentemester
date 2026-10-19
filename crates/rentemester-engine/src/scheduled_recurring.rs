//! Generation-only intent. No posting, delivery, stored credentials or unattended authority.
use crate::{Error, fiscal_year::Date};
use serde::{Deserialize, Serialize};
pub const MAX_SELECTION: usize = 100;
pub fn refused() -> Error {
    Error::new(
        "scheduled_recurring_refused",
        "scheduled recurring intent, authority or current proof refused",
    )
}
pub fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn reason(s: &str) -> Result<&str, Error> {
    let s = crate::company_knowledge::trim(s);
    if s.is_empty() || s.encode_utf16().count() > 1000 {
        Err(refused())
    } else {
        Ok(s)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Occurrence {
    pub period_index: u32,
    pub issue_date: String,
}
impl Occurrence {
    pub fn validate(&self) -> Result<(), Error> {
        Date::parse(&self.issue_date)?;
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Enable {
    pub template_id: i64,
    pub template_hash: String,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Change {
    pub template_id: i64,
    pub expected_state_hash: String,
    pub reason: String,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub template_id: i64,
    pub plan_hash: String,
    pub period_indexes: Vec<u32>,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Skip {
    pub template_id: i64,
    pub plan_hash: String,
    pub period_indexes: Vec<u32>,
    pub reason: String,
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Dispatch {
    pub template_id: i64,
    pub plan_hash: String,
    pub period_index: u32,
    pub confirm: bool,
}
pub fn selection(indices: &[u32]) -> Result<(), Error> {
    let mut v = indices.to_vec();
    v.sort_unstable();
    if v.is_empty() || v.len() > MAX_SELECTION || v.windows(2).any(|w| w[0] == w[1]) {
        Err(refused())
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_explicit_selection_and_reason() {
        assert!(selection(&[]).is_err());
        assert!(selection(&[1, 1]).is_err());
        assert!(selection(&(0..101).collect::<Vec<_>>()).is_err());
        assert!(selection(&[0, 2]).is_ok());
        assert!(reason("  ").is_err());
        assert_eq!(reason(" explicit pause ").unwrap(), "explicit pause");
        assert!(reason(&"🧾".repeat(501)).is_err());
    }
}
