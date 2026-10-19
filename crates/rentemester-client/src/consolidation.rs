//! Confirmed reporting-profile transport. Source scope, review, accounting and decimal strings stay server-owned.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_CONSOLIDATION_PROPOSE_BODY: usize = 1024 * 1024;
pub const MAX_CONSOLIDATION_BODY: usize = 32 * 1024;
#[derive(Clone, Copy)]
pub enum ConsolidationMutation {
    Propose,
    Approve,
    Revoke,
}
impl ConsolidationMutation {
    pub fn max_body(self) -> usize {
        match self {
            Self::Propose => MAX_CONSOLIDATION_PROPOSE_BODY,
            _ => MAX_CONSOLIDATION_BODY,
        }
    }
    fn suffix(self) -> &'static str {
        match self {
            Self::Propose => "propose",
            Self::Approve => "approve",
            Self::Revoke => "revoke",
        }
    }
}
pub fn consolidation_body_schema(action: ConsolidationMutation) -> Value {
    let id = json!({"type":"string","pattern":"^[a-z][a-z0-9-]{0,63}$"});
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    let end = json!({"type":["string","null"]});
    match action {
        ConsolidationMutation::Propose => {
            let line = object(
                json!({"id":id,"label":{"type":"string","minLength":1,"maxLength":640},"section":{"type":"string","enum":["asset","liability","equity","income","expense"]},"displayOrder":{"type":"integer","minimum":0,"maximum":100000},"role":{"type":["string","null"],"enum":["current-result",null]}}),
                &["id", "label", "section", "displayOrder"],
            );
            let mapping = object(
                json!({"id":id,"companySlug":{"type":"string","minLength":1},"accountNo":{"type":"string","pattern":"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$"},"reportingLineId":id,"validFrom":date,"validToExclusive":end}),
                &[
                    "id",
                    "companySlug",
                    "accountNo",
                    "reportingLineId",
                    "validFrom",
                ],
            );
            let profile = object(
                json!({"version":{"const":1},"id":id,"groupId":id,"currency":{"type":"string","minLength":3,"maxLength":3},"validFrom":date,"validToExclusive":end,"reportingLines":{"type":"array","minItems":5,"maxItems":512,"items":line},"accountMappings":{"type":"array","minItems":1,"maxItems":4096,"items":mapping},"evidenceRefs":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"string","minLength":1,"maxLength":1024}}}),
                &[
                    "version",
                    "id",
                    "groupId",
                    "currency",
                    "validFrom",
                    "reportingLines",
                    "accountMappings",
                    "evidenceRefs",
                ],
            );
            object(
                json!({"profile":profile,"confirm":{"const":true}}),
                &["profile", "confirm"],
            )
        }
        ConsolidationMutation::Approve => object(
            json!({"profileId":id,"profileHash":{"type":"string","minLength":64,"maxLength":64},"confirm":{"const":true}}),
            &["profileId", "profileHash", "confirm"],
        ),
        ConsolidationMutation::Revoke => object(
            json!({"profileId":id,"confirm":{"const":true}}),
            &["profileId", "confirm"],
        ),
    }
}
fn date(value: &str) -> Result<(), Error> {
    if value.len() != 10
        || value.bytes().enumerate().any(|(i, b)| {
            if i == 4 || i == 7 {
                b != b'-'
            } else {
                !b.is_ascii_digit()
            }
        })
    {
        return Err(Error::InvalidInput(
            "explicit consolidation YYYY-MM-DD date required",
        ));
    }
    Ok(())
}
impl ApiClient {
    pub async fn consolidation(&self, company: &str, as_of: &str) -> Result<Value, Error> {
        date(as_of)?;
        self.get(&company_path(company, "consolidation")?, &[("asOf", as_of)])
            .await
    }
    pub async fn group_report_profiles(&self, as_of: &str) -> Result<Value, Error> {
        date(as_of)?;
        self.get("/api/group-report-profiles", &[("asOf", as_of)])
            .await
    }
    pub async fn group_consolidated_report(
        &self,
        profile_id: &str,
        from: &str,
        as_of: &str,
    ) -> Result<Value, Error> {
        date(from)?;
        date(as_of)?;
        if profile_id.is_empty() {
            return Err(Error::InvalidInput(
                "explicit consolidation profileId required",
            ));
        }
        self.get(
            "/api/group-consolidated-report",
            &[("profileId", profile_id), ("from", from), ("asOf", as_of)],
        )
        .await
    }
    /// Exact reviewed bytes once; never retries, fills dates, changes actor, generates keys or calculates reports.
    pub async fn mutate_consolidation(
        &self,
        company: &str,
        action: ConsolidationMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > action.max_body() {
            return Err(Error::InvalidInput(
                "explicit confirmation and bounded native consolidation body required",
            ));
        }
        let mut value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("native consolidation JSON object required"))?;
        let mut schema = consolidation_body_schema(action);
        if matches!(action, ConsolidationMutation::Propose) {
            let mut profile = value
                .as_object_mut()
                .and_then(|o| o.remove("profile"))
                .ok_or(Error::InvalidInput(
                    "explicit consolidation profile required",
                ))?;
            let mut profile_schema = schema["properties"]["profile"].clone();
            // Validate large arrays row by row within the ordinary shape validator's bound.
            // Only transport framing is handled here; source-domain coverage and canonical bounds remain server-owned.
            for (field, min, max) in [("reportingLines", 5, 512), ("accountMappings", 1, 4096)] {
                let rows = profile
                    .as_object_mut()
                    .and_then(|o| o.remove(field))
                    .ok_or(Error::InvalidInput(
                        "explicit profile lines/mappings required",
                    ))?;
                let rows = rows
                    .as_array()
                    .filter(|a| a.len() >= min && a.len() <= max)
                    .ok_or(Error::InvalidInput(
                        "profile lines/mappings exceed native row bounds",
                    ))?;
                for row in rows {
                    body(
                        &serde_json::to_vec(row)
                            .map_err(|_| Error::InvalidInput("invalid profile row"))?,
                        &profile_schema["properties"][field]["items"],
                    )?;
                }
                profile_schema["properties"]
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                profile_schema["required"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|v| v != field);
            }
            body(
                &serde_json::to_vec(&profile)
                    .map_err(|_| Error::InvalidInput("invalid profile JSON"))?,
                &profile_schema,
            )?;
            schema["properties"]
                .as_object_mut()
                .unwrap()
                .remove("profile");
            schema["required"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v != "profile");
        }
        body(
            &serde_json::to_vec(&value)
                .map_err(|_| Error::InvalidInput("invalid consolidation JSON"))?,
            &schema,
        )?;
        self.request(
            Method::POST,
            &company_path(company, &format!("consolidation/{}", action.suffix()))?,
            &[],
            Some(bytes),
        )
        .await
    }
}
