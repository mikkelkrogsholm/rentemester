//! Explicit opening adoption transport. Server owns accounting, source proof, plans and receipts.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path, domain::object};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};
pub const MAX_OPENING_BALANCE_BODY: usize = 65_536;
pub fn opening_balance_body_schema(apply: bool) -> Value {
    let money = json!({"type":"string","pattern":"^-?(0|[1-9][0-9]*)\\.[0-9]{2}$","description":"Exact canonical native Money string; server validates signs/balance/precision, no client arithmetic"});
    let line = object(
        json!({"accountNo":{"type":"string","minLength":1},"debitAmount":money,"creditAmount":money,"text":{"type":["string","null"]}}),
        &["accountNo"],
    );
    let input = object(
        json!({"cutOverDate":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$","description":"Explicit human cutover, server validates calendar/horizon/periods; no inferred archive boundary"},"lines":{"type":"array","minItems":2,"items":line},"note":{"type":["string","null"],"description":"Optional note, server limit4096 UTF8 bytes"},"sourceArchiveImportIds":{"type":"array","maxItems":32,"uniqueItems":true,"items":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"description":"Optional explicit proof citations only; never copy or infer amounts"}}),
        &["cutOverDate", "lines"],
    );
    let source = object(
        json!({"sourceSystem":{"const":"synthetic-csv"},"content":{"type":"string","minLength":1,"description":"Exact original UTF-8 synthetic CSV, max48KiB; integer source amounts are øre. Server parses and reviews existing unclassified chart without guessing types."}}),
        &["sourceSystem", "content"],
    );
    let input = json!({"oneOf":[input,source]});
    if apply {
        object(
            json!({"input":input,"planHash":{"type":"string","minLength":64,"maxLength":64,"pattern":"^[0-9a-f]{64}$","description":"Exact reviewed server planHash"},"idempotencyKey":{"type":"string","minLength":1,"description":"Caller original key, preserve exact input/hash/key after an unknown outcome; no automatic retry"},"confirm":{"const":true}}),
            &["input", "planHash", "idempotencyKey", "confirm"],
        )
    } else {
        input
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Line {
    account_no: String,
    #[serde(default, deserialize_with = "optional_amount")]
    debit_amount: Option<String>,
    #[serde(default, deserialize_with = "optional_amount")]
    credit_amount: Option<String>,
    text: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    cut_over_date: String,
    lines: Vec<Line>,
    note: Option<String>,
    #[serde(default)]
    source_archive_import_ids: Vec<i64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Apply {
    input: PlanInput,
    plan_hash: String,
    idempotency_key: String,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceInput {
    source_system: String,
    content: String,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum PlanInput {
    Opening(Input),
    Synthetic(SourceInput),
}
fn plan_shape(input: &PlanInput) -> bool {
    match input {
        PlanInput::Opening(i) => input_shape(i),
        PlanInput::Synthetic(i) => {
            i.source_system == "synthetic-csv"
                && !i.content.is_empty()
                && i.content.len() <= 48 * 1024
        }
    }
}
fn optional_amount<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}
fn input_shape(i: &Input) -> bool {
    !i.cut_over_date.is_empty()
        && i.lines.len() >= 2
        && i.note.as_ref().is_none_or(|n| n.len() <= 4096)
        && i.source_archive_import_ids.len() <= 32
        && i.source_archive_import_ids
            .iter()
            .all(|n| (1..=9_007_199_254_740_991).contains(n))
        && i.lines.iter().all(|l| {
            !l.account_no.is_empty()
                && l.debit_amount.as_ref().is_none_or(|s| !s.is_empty())
                && l.credit_amount.as_ref().is_none_or(|s| !s.is_empty())
                && l.text
                    .as_ref()
                    .is_none_or(|s| s.len() <= MAX_OPENING_BALANCE_BODY)
        })
}
fn shape(bytes: &[u8], apply: bool) -> Result<(), Error> {
    let invalid = || {
        Error::InvalidInput(
            "exact documented native opening JSON required; no actor/currency/date inference",
        )
    };
    if bytes.len() > MAX_OPENING_BALANCE_BODY {
        return Err(invalid());
    }
    let valid = if apply {
        let i: Apply = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        plan_shape(&i.input)
            && i.confirm
            && !i.idempotency_key.is_empty()
            && i.plan_hash.len() == 64
            && i.plan_hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    } else {
        let i: PlanInput = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        plan_shape(&i)
    };
    if valid { Ok(()) } else { Err(invalid()) }
}
impl ApiClient {
    pub async fn opening_balance(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "opening-balance")?, &[])
            .await
    }
    pub async fn opening_balance_plan(
        &self,
        company: &str,
        bytes: Vec<u8>,
    ) -> Result<Value, Error> {
        shape(&bytes, false)?;
        self.opening_json(company, "plan", bytes, false).await
    }
    pub async fn opening_balance_apply(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit opening apply confirmation required",
            ));
        }
        shape(&bytes, true)?;
        self.opening_json(company, "apply", bytes, true).await
    }
    async fn opening_json(
        &self,
        company: &str,
        suffix: &str,
        bytes: Vec<u8>,
        write: bool,
    ) -> Result<Value, Error> {
        let failure = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        };
        let mut response = self
            .send(
                Method::POST,
                &company_path(company, &format!("opening-balance/{suffix}"))?,
                &[],
                Some(bytes),
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await
            .map_err(|e| {
                if !write && e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let status = response.status();
        if status.is_server_error() || status.as_u16() == 408 {
            return Err(failure());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| failure())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(failure());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status.is_success() {
                failure()
            } else {
                Error::Refused {
                    status: status.as_u16(),
                }
            }
        })?;
        if value["ok"] == false && value["code"].as_str().is_some_and(|s| !s.is_empty()) {
            if write
                && matches!(
                    value["code"].as_str(),
                    Some("internal" | "outcome_unknown" | "outcome_unconfirmed")
                )
            {
                return Err(Error::OutcomeUnknown);
            }
            return Ok(value);
        }
        if status.is_success() && value["ok"] == true {
            Ok(value)
        } else if status.is_success() {
            Err(failure())
        } else {
            Err(Error::Refused {
                status: status.as_u16(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_source_envelopes_preserve_exact_content_and_reject_authority_fields() {
        let source = json!({"sourceSystem":"synthetic-csv","content":"\u{feff}# cutOverDate: 2024-07-01\r\nsection,accountNo,name,debit,credit\r\n"});
        assert!(shape(&serde_json::to_vec(&source).unwrap(), false).is_ok());
        let mut invalid = source.clone();
        invalid["actor"] = json!("user:caller");
        assert!(shape(&serde_json::to_vec(&invalid).unwrap(), false).is_err());
        let mut apply = json!({"input":source,"planHash":"a".repeat(64),"idempotencyKey":"original-source-key","confirm":true});
        assert!(shape(&serde_json::to_vec(&apply).unwrap(), true).is_ok());
        apply["confirm"] = json!(false);
        assert!(shape(&serde_json::to_vec(&apply).unwrap(), true).is_err());
        apply["confirm"] = json!(true);
        apply["input"]["sourceSystem"] = json!("unsupported-system");
        assert!(shape(&serde_json::to_vec(&apply).unwrap(), true).is_err());
    }
}
