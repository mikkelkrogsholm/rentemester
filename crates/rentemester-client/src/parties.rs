//! Exact workspace party HTTP contracts. The server owns visibility, identity and provenance policy.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_PARTY_BODY: usize = 256 * 1024;
#[derive(Clone)]
pub enum PartyMutation {
    Create,
    CreateIdentity,
    ProposeMerge(String),
    ApproveMerge(String),
    Role(String),
    Assertion(String),
    Alias(String),
}
pub fn party_body_schema(action: &PartyMutation) -> Value {
    let text = json!({"type":"string","minLength":1});
    let role = json!({"type":"string","enum":["issuer","supplier","customer","recipient","payer","payee","processor","acquirer","related_company","establishment","location","payment_descriptor","vendor","owner","adviser","employee","authority","bank"]});
    let defaults = object(
        json!({"account":{},"vat":{},"currency":{},"paymentTermsDays":{}}),
        &[],
    );
    let review = json!({"type":"string","enum":["proposed","approved","rejected"]});
    match action {
        PartyMutation::ProposeMerge(_) => object(
            json!({"intoPartyId":{"type":"string","minLength":1,"maxLength":64},"reviewAssertion":{"type":"string","minLength":1,"maxLength":500},"confirm":{"const":true}}),
            &["intoPartyId", "reviewAssertion", "confirm"],
        ),
        PartyMutation::ApproveMerge(_) => object(
            json!({"proposalHash":{"type":"string","minLength":64,"maxLength":64,"pattern":"^[0-9a-f]{64}$"},"confirm":{"const":true}}),
            &["proposalHash", "confirm"],
        ),
        PartyMutation::CreateIdentity => {
            // Same identity grammar as native Create, without any company role/defaults.
            let mut schema = party_body_schema(&PartyMutation::Create);
            let properties = schema["properties"].as_object_mut().unwrap();
            properties.remove("role");
            properties.remove("defaults");
            schema["required"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v != "role");
            schema
        }
        PartyMutation::Create => object(
            json!({"partyId":{"type":["string","null"],"maxLength":64},"kind":{"type":"string","enum":["person","organization","public_authority","financial_institution"]},"name":{"type":"string","minLength":1,"maxLength":320},"aliases":{"type":"array","maxItems":16,"items":{"type":"string","minLength":1,"maxLength":160}},"identifiers":{"type":"array","items":object(json!({"country":text,"identifier":{"type":["string","null"]},"identifierKind":{"type":"string","enum":["dk_cvr","eu_vat","non_eu"]}}),&["country","identifierKind"])},"source":text,"observedAt":text,"reviewAssertion":text,"role":role,"defaults":defaults,"confirm":{"const":true}}),
            &[
                "kind",
                "name",
                "source",
                "observedAt",
                "reviewAssertion",
                "role",
                "confirm",
            ],
        ),
        PartyMutation::Role(_) => object(
            json!({"role":role,"defaults":defaults,"confirm":{"const":true}}),
            &["role", "confirm"],
        ),
        PartyMutation::Assertion(_) => object(
            json!({"field":text,"value":text,"source":text,"observedAt":text,"reviewState":review,"confirm":{"const":true}}),
            &[
                "field",
                "value",
                "source",
                "observedAt",
                "reviewState",
                "confirm",
            ],
        ),
        PartyMutation::Alias(_) => object(
            json!({"alias":text,"source":text,"observedAt":text,"reviewState":review,"confirm":{"const":true}}),
            &["alias", "source", "observedAt", "reviewState", "confirm"],
        ),
    }
}
fn id_segment(id: &str) -> Result<String, Error> {
    if id.is_empty()
        || id.encode_utf16().count() > 64
        || matches!(id, "." | "..")
        || id.chars().any(char::is_control)
    {
        return Err(Error::InvalidInput("bounded partyId required"));
    }
    Ok(id
        .as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(b) {
                char::from(*b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect())
}
impl ApiClient {
    pub async fn workspace_parties(
        &self,
        company: &str,
        query: Option<&str>,
        cursor: usize,
        limit: usize,
    ) -> Result<Value, Error> {
        if query.is_some_and(|s| s.encode_utf16().count() > 512) || !(1..=100).contains(&limit) {
            return Err(Error::InvalidInput("party query max512, limit1..100"));
        }
        let c = cursor.to_string();
        let l = limit.to_string();
        let mut q = vec![("cursor", c.as_str()), ("limit", l.as_str())];
        if let Some(query) = query {
            q.push(("query", query));
        }
        self.get(&company_path(company, "workspace-parties")?, &q)
            .await
    }
    pub async fn workspace_party(&self, company: &str, id: &str) -> Result<Value, Error> {
        self.get(
            &company_path(company, &format!("workspace-parties/{}", id_segment(id)?))?,
            &[],
        )
        .await
    }
    /// Exact confirmed bytes once, no generated keys or retries. Read visible party/history after uncertainty.
    pub async fn mutate_workspace_party(
        &self,
        company: &str,
        action: PartyMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > MAX_PARTY_BODY {
            return Err(Error::InvalidInput(
                "explicit party confirmation and native256KiB body required",
            ));
        }
        let mut value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("native party JSON object required"))?;
        if let PartyMutation::ProposeMerge(id) = &action {
            let bounded = |key: &str, max| {
                let text = value[key]
                    .as_str()
                    .ok_or(Error::InvalidInput("merge text required"))?
                    .trim_matches(|c: char| {
                        c != '\u{0085}' && (c.is_whitespace() || c == '\u{feff}')
                    });
                if text.is_empty() || text.encode_utf16().count() > max {
                    Err(Error::InvalidInput("bounded merge text required"))
                } else {
                    Ok(text.to_owned())
                }
            };
            let target = bounded("intoPartyId", 64)?;
            let review = bounded("reviewAssertion", 500)?;
            id_segment(&target)?;
            if target == *id {
                return Err(Error::InvalidInput("distinct merge target required"));
            }
            // Validate the same canonical strings as the server, while sending the original bytes.
            value["intoPartyId"] = json!(target);
            value["reviewAssertion"] = json!(review);
        }
        if matches!(action, PartyMutation::ApproveMerge(_)) && !merge_hash(&value["proposalHash"]) {
            return Err(Error::InvalidInput(
                "exact lowercase proposal hash required",
            ));
        }
        let merge = matches!(
            action,
            PartyMutation::ProposeMerge(_) | PartyMutation::ApproveMerge(_)
        );
        let source = match &action {
            PartyMutation::ProposeMerge(id) | PartyMutation::ApproveMerge(id) => id.clone(),
            _ => String::new(),
        };
        let mut schema = party_body_schema(&action);
        // Defaults are arbitrary source JSON under a fixed whitelist; never interpret accounting defaults.
        if let Some(defaults) = value
            .as_object_mut()
            .filter(|_| matches!(action, PartyMutation::Create | PartyMutation::Role(_)))
            .and_then(|v| v.remove("defaults"))
            && !defaults.is_null()
            && (!defaults.is_object()
                || defaults.as_object().unwrap().keys().any(|k| {
                    !["account", "vat", "currency", "paymentTermsDays"].contains(&k.as_str())
                }))
        {
            return Err(Error::InvalidInput(
                "unsupported company-scoped party defaults",
            ));
        }
        schema["properties"]
            .as_object_mut()
            .unwrap()
            .remove("defaults");
        relax_utf8(&mut schema);
        body(
            &serde_json::to_vec(&value).map_err(|_| Error::InvalidInput("invalid party JSON"))?,
            &schema,
        )?;
        let suffix = match action {
            PartyMutation::ProposeMerge(id) => {
                format!("workspace-parties/{}/propose-merge", id_segment(&id)?)
            }
            PartyMutation::ApproveMerge(id) => {
                format!("workspace-parties/{}/approve-merge", id_segment(&id)?)
            }
            PartyMutation::Create => "workspace-parties".into(),
            PartyMutation::CreateIdentity => "workspace-parties/create-identity".into(),
            PartyMutation::Role(id) => format!("workspace-parties/{}/role", id_segment(&id)?),
            PartyMutation::Assertion(id) => {
                format!("workspace-parties/{}/assertion", id_segment(&id)?)
            }
            PartyMutation::Alias(id) => format!("workspace-parties/{}/alias", id_segment(&id)?),
        };
        let result = self
            .request(
                Method::POST,
                &company_path(company, &suffix)?,
                &[],
                Some(bytes),
            )
            .await?;
        if merge && result["ok"] != false {
            let valid = result["ok"] == true
                && if suffix.ends_with("/propose-merge") {
                    use sha2::{Digest, Sha256};
                    let payload = json!({"intoPartyId":value["intoPartyId"],"reviewAssertion":value["reviewAssertion"]});
                    let expected = format!(
                        "{:x}",
                        Sha256::digest(serde_json::to_vec(&payload).unwrap())
                    );
                    result["proposalHash"].as_str() == Some(expected.as_str())
                } else {
                    merge_receipt(&result["party"], &source, &value["proposalHash"])
                };
            if !valid {
                return Err(Error::OutcomeUnknown);
            }
        }
        Ok(result)
    }
}
fn merge_hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn merge_receipt(p: &Value, source: &str, hash: &Value) -> bool {
    p["partyId"] == source
        && matches!(
            p["kind"].as_str(),
            Some("person" | "organization" | "public_authority" | "financial_institution")
        )
        && ["name", "source", "observedAt", "reviewAssertion"]
            .iter()
            .all(|k| p[k].as_str().is_some_and(|s| !s.is_empty()))
        && ["identifiers", "aliases", "assertions", "roles"]
            .iter()
            .all(|k| p[k].is_array())
        && p["roles"].as_array().is_some_and(|rows| {
            rows.iter().all(|r| {
                r["companySlug"].is_string() && r["role"].is_string() && r["defaults"].is_object()
            })
        })
        && p["identifiers"].as_array().is_some_and(|rows| {
            rows.iter().all(|r| {
                ["country", "identifier", "identifierKind"]
                    .iter()
                    .all(|k| r[k].is_string())
            })
        })
        && p["aliases"].as_array().is_some_and(|rows| {
            rows.iter().all(|r| {
                ["alias", "source", "review_state"]
                    .iter()
                    .all(|k| r[k].is_string())
                    && r.get("observed_at").is_some()
                    && r.get("payload_hash").is_some()
            })
        })
        && p["assertions"].as_array().is_some_and(|rows| {
            rows.iter().all(|r| {
                ["field", "value", "source", "observed_at", "review_state"]
                    .iter()
                    .all(|k| r[k].is_string())
                    && merge_hash(&r["payload_hash"])
            })
        })
        && p["history"].as_array().is_some_and(|events| {
            events.iter().all(|e| {
                merge_hash(&e["payload_hash"])
                    && ["event_type", "actor", "created_at"]
                        .iter()
                        .all(|k| e[k].as_str().is_some_and(|s| !s.is_empty()))
            }) && ["proposed_merge", "approved_merge", "superseded"]
                .iter()
                .all(|kind| {
                    events
                        .iter()
                        .any(|e| e["event_type"] == *kind && e["payload_hash"] == *hash)
                })
                && events.iter().any(|e| e["event_type"] == "created")
        })
}
// Shared shape validator counts UTF-8; native text bounds are UTF-16 and remain server-owned.
fn relax_utf8(v: &mut Value) {
    match v {
        Value::Object(m) => {
            if let Some(n) = m.get("maxLength").and_then(Value::as_u64) {
                m.insert("maxLength".into(), json!(n * 4));
            }
            for v in m.values_mut() {
                relax_utf8(v);
            }
        }
        Value::Array(a) => {
            for v in a {
                relax_utf8(v);
            }
        }
        _ => {}
    }
}
