//! Native reviewed supplier planning. Exact bodies; no accounting or FX logic here.
use crate::{
    ApiClient, Error, MAX_RESPONSE, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug)]
pub enum CommitmentMutation {
    Apply,
    Change,
    Match,
}
fn date_schema() -> Value {
    json!({"type":"string","minLength":10,"maxLength":10,"pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"})
}
fn hash_schema() -> Value {
    json!({"type":"string","minLength":64,"maxLength":64,"pattern":"^[0-9a-f]{64}$","description":"Exact native domain proof from plan/current state; never invent or replace"})
}
fn id_schema() -> Value {
    json!({"type":"string","minLength":3,"maxLength":96})
}
fn input_schema() -> Value {
    let mut fields = json!({"commitmentId":id_schema(),"vendorPartyId":{"type":"string","minLength":8,"maxLength":96,"pattern":"^vendor:[1-9][0-9]*$","description":"Explicit existing ledger-local vendor id from contacts; server resolves immutable snapshot. No shared-party equivalence inferred"},"type":{"type":"string","minLength":1,"maxLength":256},"description":{"type":"string","minLength":1,"maxLength":2000},"businessPurpose":{"type":"string","minLength":1,"maxLength":2000},"amount":{"type":"number","exclusiveMinimum":0,"description":"Amount per occurrence; native server quantizes and verifies exact cents"},"currency":{"type":"string","minLength":3,"maxLength":3,"pattern":"^[A-Z]{3}$","description":"Explicit currency, unsupported FX is excluded from forecast, never converted"},"frequency":{"type":"string","enum":["weekly","monthly","quarterly","yearly"]},"nextDate":date_schema(),"evidenceRefs":{"type":"array","minItems":1,"maxItems":20,"items":{"type":"string","minLength":10,"maxLength":160,"pattern":"^document:[1-9][0-9]*$"},"description":"Original source document:<id> references; server verifies original bytes/hash"},"vatProposal":{"type":["string","null"],"maxLength":2000}});
    for key in ["startDate", "endDate", "renewalDate", "noticeDate"] {
        let mut date = date_schema();
        date["type"] = json!(["string", "null"]);
        fields[key] = date;
    }
    object(
        fields,
        &[
            "commitmentId",
            "vendorPartyId",
            "type",
            "description",
            "businessPurpose",
            "amount",
            "currency",
            "frequency",
            "nextDate",
            "evidenceRefs",
        ],
    )
}
pub fn commitment_plan_schema() -> Value {
    object(json!({"commitment":input_schema()}), &["commitment"])
}
pub fn commitment_body_schema(operation: CommitmentMutation) -> Value {
    match operation {
        CommitmentMutation::Apply => object(
            json!({"commitment":input_schema(),"payloadHash":hash_schema(),"confirm":{"const":true},"idempotencyKey":{"type":["string","null"],"minLength":1,"maxLength":160,"description":"Optional existing native approval replay key supplied by caller. Client never generates a key or retries"}}),
            &["commitment", "payloadHash", "confirm"],
        ),
        CommitmentMutation::Change => object(
            json!({"commitmentId":id_schema(),"action":{"type":"string","enum":["paused","ended","superseded"]},"reason":{"type":"string","minLength":1,"maxLength":2000},"expectedPayloadHash":hash_schema(),"confirm":{"const":true}}),
            &[
                "commitmentId",
                "action",
                "reason",
                "expectedPayloadHash",
                "confirm",
            ],
        ),
        CommitmentMutation::Match => object(
            json!({"commitmentId":id_schema(),"occurrenceDate":date_schema(),"evidence":object(json!({"kind":{"type":"string","enum":["canonical_document","payable","bank_transaction"]},"id":{"type":"string","minLength":1,"maxLength":20,"pattern":"^[1-9][0-9]*$","description":"Actual canonical evidence id from supplier_commitments.evidenceOptions; all facts resolved server-side"}}),&["kind","id"]),"confirm":{"const":true}}),
            &["commitmentId", "occurrenceDate", "evidence", "confirm"],
        ),
    }
}
/// Contextual selector validation only; domain dates remain server-authoritative.
pub fn valid_commitment_start(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let year = value[..4].parse::<u32>().unwrap();
    let month = value[5..7].parse::<u32>().unwrap();
    let day = value[8..].parse::<u32>().unwrap();
    let max = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    year >= 1 && day >= 1 && day <= max
}
impl ApiClient {
    pub async fn supplier_commitments(
        &self,
        company: &str,
        start_date: Option<&str>,
    ) -> Result<Value, Error> {
        if start_date.is_some_and(|s| !valid_commitment_start(s)) {
            return Err(Error::InvalidInput(
                "supplier commitments startDate requires real YYYY-MM-DD calendar date",
            ));
        }
        let query = start_date
            .map(|date| vec![("startDate", date)])
            .unwrap_or_default();
        self.get(&company_path(company, "supplier-commitments")?, &query)
            .await
    }
    pub async fn supplier_commitment_matches(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "supplier-commitments/matches")?, &[])
            .await
    }
    /// Nonmutating POST. No confirmation, reservation, immutable event or write uncertainty.
    pub async fn plan_supplier_commitment(
        &self,
        company: &str,
        bytes: Vec<u8>,
    ) -> Result<Value, Error> {
        body(&bytes, &commitment_plan_schema())?;
        let mut response = self
            .send(
                Method::POST,
                &company_path(company, "supplier-commitments/plan")?,
                &[],
                Some(bytes),
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await
            .map_err(|e| {
                if e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        if !response.status().is_success() {
            return Err(Error::Refused {
                status: response.status().as_u16(),
            });
        }
        let mut result = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if result.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(Error::Protocol);
            }
            result.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&result).map_err(|_| Error::Protocol)
    }
    /// One confirmed attempt; on outcome_unknown reconcile approval hash/key, match and history.
    pub async fn mutate_supplier_commitment(
        &self,
        company: &str,
        operation: CommitmentMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit supplier commitment confirmation required",
            ));
        }
        body(&bytes, &commitment_body_schema(operation))?;
        let path = match operation {
            CommitmentMutation::Apply => "supplier-commitments",
            CommitmentMutation::Change => "supplier-commitments/change",
            CommitmentMutation::Match => "supplier-commitments/match",
        };
        self.request(
            Method::POST,
            &company_path(company, path)?,
            &[],
            Some(bytes),
        )
        .await
    }
}
