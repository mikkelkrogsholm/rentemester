//! Source-hashed ownership snapshots; server owns authority, review, dates and graph arithmetic.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_OWNERSHIP_PROPOSE_BODY: usize = 2 * 1024 * 1024;
pub const MAX_OWNERSHIP_BODY: usize = 32 * 1024;
#[derive(Clone, Copy)]
pub enum OwnershipMutation {
    Propose,
    Review,
    Apply,
}
pub fn ownership_body_schema(action: OwnershipMutation) -> Value {
    let text = json!({"type":"string","minLength":1});
    let id = json!({"type":"string","minLength":1,"maxLength":80});
    let bp = json!({"type":["integer","null"],"minimum":0,"maximum":10000});
    match action {
        OwnershipMutation::Propose => {
            let company = object(
                json!({"kind":{"const":"company"},"companySlug":{"type":"string","minLength":1,"maxLength":120}}),
                &["kind", "companySlug"],
            );
            let party = object(
                json!({"kind":{"const":"party"},"partyId":{"type":"string","minLength":1,"maxLength":64}}),
                &["kind", "partyId"],
            );
            let mut owner = object(
                json!({"kind":{"type":"string","enum":["company","party"]},"companySlug":{"type":"string","minLength":1,"maxLength":120},"partyId":{"type":"string","minLength":1,"maxLength":64}}),
                &["kind"],
            );
            owner["oneOf"] = json!([company, party]);
            let mut interval = object(
                json!({"min":{"type":"integer","minimum":0,"maximum":10000},"max":{"type":"integer","minimum":0,"maximum":10000}}),
                &["min", "max"],
            );
            interval["type"] = json!(["object", "null"]);
            let mut fact = object(
                json!({"owner":owner,"ownedCompanySlug":{"type":"string","minLength":1,"maxLength":120},"validFrom":text,"validToExclusive":{"type":["string","null"]},"economicBasisPoints":bp,"economicIntervalBasisPoints":interval,"votingBasisPoints":bp,"controlType":{"type":"string","enum":["equity","voting","board","agreement","other"]},"shareClass":{"type":["string","null"],"maxLength":80},"jurisdiction":{"type":"string","minLength":2,"maxLength":2},"evidenceRefs":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"string","minLength":1,"maxLength":256}}}),
                &[
                    "owner",
                    "ownedCompanySlug",
                    "validFrom",
                    "controlType",
                    "jurisdiction",
                    "evidenceRefs",
                ],
            );
            fact["description"] = json!(
                "Exactly one economic exact value or interval; source dates, endpoints, evidence and graph invariants remain server-owned. No client calculation."
            );
            fact["oneOf"] = json!([{"required":["economicBasisPoints"],"properties":{"economicBasisPoints":{"type":"integer"},"economicIntervalBasisPoints":{"type":"null"}}},{"required":["economicIntervalBasisPoints"],"properties":{"economicBasisPoints":{"type":"null"},"economicIntervalBasisPoints":{"type":"object"}}}]);
            object(
                json!({"snapshotId":{"type":["string","null"],"maxLength":80},"source":{"type":"string","minLength":1,"maxLength":160},"observedAt":text,"facts":{"type":"array","minItems":1,"maxItems":2048,"items":fact},"confirm":{"const":true}}),
                &["source", "observedAt", "facts", "confirm"],
            )
        }
        OwnershipMutation::Review => object(
            json!({"snapshotId":id,"decision":{"type":"string","enum":["approved","rejected"]},"confirm":{"const":true}}),
            &["snapshotId", "decision", "confirm"],
        ),
        OwnershipMutation::Apply => object(
            json!({"snapshotId":id,"snapshotHash":{"type":"string","minLength":64,"maxLength":64},"diffHash":{"type":"string","minLength":64,"maxLength":64},"confirm":{"const":true}}),
            &["snapshotId", "snapshotHash", "diffHash", "confirm"],
        ),
    }
}
fn as_of(s: &str) -> Result<(), Error> {
    if s.len() != 10
        || s.bytes().enumerate().any(|(i, b)| {
            if i == 4 || i == 7 {
                b != b'-'
            } else {
                !b.is_ascii_digit()
            }
        })
    {
        return Err(Error::InvalidInput(
            "explicit ownership asOf ISO date required",
        ));
    }
    Ok(())
}
fn utf8_bounds(v: &mut Value) {
    match v {
        Value::Object(m) => {
            if let Some(n) = m.get("maxLength").and_then(Value::as_u64) {
                m.insert("maxLength".into(), json!(n * 4));
            }
            for v in m.values_mut() {
                utf8_bounds(v);
            }
        }
        Value::Array(a) => {
            for v in a {
                utf8_bounds(v);
            }
        }
        _ => {}
    }
}
impl ApiClient {
    pub async fn ownership(&self, company: &str, as_of_date: &str) -> Result<Value, Error> {
        as_of(as_of_date)?;
        self.get(
            &company_path(company, "ownership")?,
            &[("asOf", as_of_date)],
        )
        .await
    }
    pub async fn ownership_history(
        &self,
        company: &str,
        snapshot_id: Option<&str>,
        as_of_date: Option<&str>,
    ) -> Result<Value, Error> {
        let mut query = vec![];
        if let Some(id) = snapshot_id {
            if id.trim().is_empty() || id.encode_utf16().count() > 80 {
                return Err(Error::InvalidInput(
                    "bounded explicit ownership snapshotId required",
                ));
            }
            query.push(("snapshotId", id));
        }
        if let Some(date) = as_of_date {
            as_of(date)?;
            query.push(("asOf", date));
        }
        self.get(&company_path(company, "ownership/history")?, &query)
            .await
    }
    /// Exact confirmed bytes once; hashes and independent review come from canonical server snapshots.
    /// No generated IDs/keys/hashes, omitted-fact revocation, ownership calculation or automatic retry.
    pub async fn mutate_ownership(
        &self,
        company: &str,
        action: OwnershipMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        let max = if matches!(action, OwnershipMutation::Propose) {
            MAX_OWNERSHIP_PROPOSE_BODY
        } else {
            MAX_OWNERSHIP_BODY
        };
        if !confirmed || bytes.len() > max {
            return Err(Error::InvalidInput(
                "explicit ownership confirmation and native body bound required",
            ));
        }
        let mut value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("native ownership JSON object required"))?;
        let mut schema = ownership_body_schema(action);
        if matches!(action, OwnershipMutation::Propose) {
            let facts = value
                .as_object_mut()
                .and_then(|o| o.remove("facts"))
                .ok_or(Error::InvalidInput("explicit ownership facts required"))?;
            let facts = facts
                .as_array()
                .filter(|a| !a.is_empty() && a.len() <= 2048)
                .ok_or(Error::InvalidInput("ownership facts require1..2048 rows"))?;
            let mut fact_schema = schema["properties"]["facts"]["items"].clone();
            utf8_bounds(&mut fact_schema);
            for fact in facts {
                body(
                    &serde_json::to_vec(fact)
                        .map_err(|_| Error::InvalidInput("invalid ownership fact"))?,
                    &fact_schema,
                )?;
            }
            schema["properties"]
                .as_object_mut()
                .unwrap()
                .remove("facts");
            schema["required"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v != "facts");
        }
        utf8_bounds(&mut schema);
        body(
            &serde_json::to_vec(&value)
                .map_err(|_| Error::InvalidInput("invalid ownership JSON"))?,
            &schema,
        )?;
        let suffix = match action {
            OwnershipMutation::Propose => "propose",
            OwnershipMutation::Review => "review",
            OwnershipMutation::Apply => "apply",
        };
        self.request(
            Method::POST,
            &company_path(company, &format!("ownership/{suffix}"))?,
            &[],
            Some(bytes),
        )
        .await
    }
}
