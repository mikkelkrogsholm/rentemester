//! Explicit scheduled recurring consent and caller-driven dispatch; server owns every period.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_SCHEDULED_RECURRING_BODY: usize = 16 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduledRecurringMutation {
    Enable,
    Pause,
    Resume,
    Cancel,
    CatchUp,
    Skip,
    Dispatch,
}
impl ScheduledRecurringMutation {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Enable => "enable",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Cancel => "cancel",
            Self::CatchUp => "catch-up",
            Self::Skip => "skip",
            Self::Dispatch => "dispatch",
        }
    }
}
pub fn scheduled_recurring_body_schema(action: ScheduledRecurringMutation) -> Value {
    let integer = json!({"type":"integer","minimum":0,"maximum":9007199254740991u64});
    let hash = json!({"type":"string","pattern":"^[0-9a-f]{64}$","minLength":64,"maxLength":64});
    let mut properties = json!({"templateId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"confirm":{"const":true}});
    let mut required = vec!["templateId", "confirm"];
    let key = match action {
        ScheduledRecurringMutation::Enable => "templateHash",
        ScheduledRecurringMutation::Pause
        | ScheduledRecurringMutation::Resume
        | ScheduledRecurringMutation::Cancel => "expectedStateHash",
        _ => "planHash",
    };
    properties[key] = hash;
    required.push(key);
    if matches!(
        action,
        ScheduledRecurringMutation::CatchUp | ScheduledRecurringMutation::Skip
    ) {
        properties["periodIndexes"] =
            json!({"type":"array","minItems":1,"maxItems":100,"uniqueItems":true,"items":integer});
        required.push("periodIndexes");
    }
    if action == ScheduledRecurringMutation::Dispatch {
        properties["periodIndex"] = integer;
        required.push("periodIndex");
    }
    if matches!(
        action,
        ScheduledRecurringMutation::Pause
            | ScheduledRecurringMutation::Resume
            | ScheduledRecurringMutation::Cancel
            | ScheduledRecurringMutation::Skip
    ) {
        properties["reason"] = json!({"type":"string","minLength":1,"description":"Required nonblank after server trimming, at most1000 UTF16 code units; forwarded unchanged"});
        required.push("reason");
    }
    object(properties, &required)
}
fn exact_id(id: i64, positive: bool) -> Result<(), Error> {
    if ((if positive { 1 } else { 0 })..=9_007_199_254_740_991).contains(&id) {
        Ok(())
    } else {
        Err(Error::InvalidInput(
            "exact bounded template/period integer required",
        ))
    }
}
impl ApiClient {
    pub async fn scheduled_recurring(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "scheduled-recurring")?, &[])
            .await
    }
    pub async fn scheduled_recurring_plan(
        &self,
        company: &str,
        template_id: i64,
    ) -> Result<Value, Error> {
        exact_id(template_id, true)?;
        self.get(
            &company_path(company, &format!("scheduled-recurring/{template_id}/plan"))?,
            &[],
        )
        .await
    }
    /// Forward exact reviewed bytes once. No timer, key, default, principal or replay.
    /// Unknown outcomes require canonical status/plan reads before any further manual action.
    pub async fn mutate_scheduled_recurring(
        &self,
        company: &str,
        action: ScheduledRecurringMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > MAX_SCHEDULED_RECURRING_BODY {
            return Err(Error::InvalidInput(
                "explicit scheduled recurring confirmation and native16KiB body required",
            ));
        }
        body(&bytes, &scheduled_recurring_body_schema(action))?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("native scheduled recurring JSON required"))?;
        exact_id(
            value["templateId"]
                .as_i64()
                .ok_or(Error::InvalidInput("exact templateId required"))?,
            true,
        )?;
        for key in ["templateHash", "expectedStateHash", "planHash"] {
            if let Some(hash) = value.get(key).and_then(Value::as_str)
                && (hash.len() != 64
                    || !hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
            {
                return Err(Error::InvalidInput("native lowercase SHA256 hash required"));
            }
        }
        if let Some(period) = value.get("periodIndex") {
            exact_id(
                period
                    .as_i64()
                    .ok_or(Error::InvalidInput("exact periodIndex required"))?,
                false,
            )?;
        }
        if let Some(periods) = value.get("periodIndexes").and_then(Value::as_array) {
            let mut seen = std::collections::BTreeSet::new();
            for period in periods {
                let n = period
                    .as_i64()
                    .ok_or(Error::InvalidInput("exact periodIndexes required"))?;
                exact_id(n, false)?;
                if !seen.insert(n) {
                    return Err(Error::InvalidInput(
                        "distinct explicit periodIndexes required",
                    ));
                }
            }
        }
        self.request(
            Method::POST,
            &company_path(company, &format!("scheduled-recurring/{}", action.suffix()))?,
            &[],
            Some(bytes),
        )
        .await
    }
}
