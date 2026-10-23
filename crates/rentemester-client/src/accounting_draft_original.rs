//! Original draft ABI: source projections, not a second journal engine.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use AccountingDraftOriginalOperation::*;
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_ACCOUNTING_DRAFT_BODY: usize = 270 * 1024;
const MAX_PAYLOAD: usize = 262144;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountingDraftOriginalOperation {
    Create,
    Revise,
    Submit,
    Reject,
    ApproveAndPost,
    Show,
    List,
}
impl AccountingDraftOriginalOperation {
    pub fn write(self) -> bool {
        !matches!(self, Show | List)
    }
    pub fn name(self) -> &'static str {
        match self {
            Create => "create",
            Revise => "revise",
            Submit => "submit",
            Reject => "reject",
            ApproveAndPost => "approve-and-post",
            Show => "show",
            List => "list",
        }
    }
}
fn id(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        let s = s.trim();
        !s.is_empty()
            && s.len() <= 64
            && s.as_bytes()[0].is_ascii_lowercase()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })
}
fn hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn positive(v: &Value) -> bool {
    v.as_u64()
        .is_some_and(|n| (1..=9007199254740991).contains(&n))
}
fn text(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.trim().is_empty())
}
fn typed(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        ["user:", "service_account:"].iter().any(|p| {
            s.strip_prefix(p)
                .is_some_and(|s| !s.is_empty() && s.trim() == s && !s.chars().any(char::is_control))
        })
    })
}
// Immutable historical attribution does not select a current credential.
fn historical_actor(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() <= 256
            && s.split_once(':').is_some_and(|(kind, subject)| {
                matches!(kind, "user" | "service_account" | "agent" | "system")
                    && !subject.is_empty()
                    && !subject.chars().any(|c| c.is_whitespace() || c.is_control())
            })
    })
}
fn exact(v: &Value, keys: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == keys.len() && keys.iter().all(|k| o.contains_key(*k)))
}
pub fn original_accounting_draft_schema(op: AccountingDraftOriginalOperation) -> Value {
    let mut props = json!({});
    let mut required = vec![];
    if op != List {
        props["draftId"] = json!({"type":"string","pattern":"^\\s*[a-z][a-z0-9-]{0,63}\\s*$"});
        required.push("draftId");
    }
    if op.write() {
        props["confirm"] = json!({"const":true});
        required.push("confirm");
    }
    if matches!(op, Revise | Submit | Reject | ApproveAndPost) {
        props["expectedEventHash"] = json!({"type":"string","pattern":"^[0-9a-f]{64}$"});
        required.push("expectedEventHash");
    }
    if matches!(op, Create | Revise) {
        props["payload"] = json!({"type":"object"});
        required.push("payload");
    }
    if op == Reject {
        props["reason"] = json!({"type":"string","minLength":1,"maxLength":1000});
        required.push("reason");
    }
    json!({"type":"object","properties":props,"required":required,"additionalProperties":false})
}
// Structural source ABI only. Accounting policy and evidence remain server-owned.
fn payload(v: &Value) -> bool {
    let Some(o) = v.as_object() else {
        return false;
    };
    if !o.keys().all(|k| {
        matches!(
            k.as_str(),
            "transactionDate"
                | "text"
                | "currency"
                | "documentId"
                | "sourceBankTransactionId"
                | "amountForeign"
                | "amountDkk"
                | "fxRateToDkk"
                | "lines"
        )
    }) {
        return false;
    }
    let date = v["transactionDate"].as_str().is_some_and(|s| {
        let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]").unwrap();
        s.len() == 10 && time::Date::parse(s, &format).is_ok()
    });
    date && text(&v["text"])
        && v.get("currency").is_none_or(|c| {
            c.as_str()
                .is_some_and(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase()))
        })
        && ["documentId", "sourceBankTransactionId"]
            .iter()
            .all(|k| v.get(*k).is_none_or(|v| v.is_null() || positive(v)))
        && ["amountForeign", "amountDkk", "fxRateToDkk"]
            .iter()
            .all(|k| v.get(*k).is_none_or(|v| v.is_null() || v.is_number()))
        && v["lines"].as_array().is_some_and(|a| {
            a.iter().all(|l| {
                l.as_object().is_some_and(|o| {
                    o.keys().all(|k| {
                        matches!(
                            k.as_str(),
                            "accountNo" | "debitAmount" | "creditAmount" | "vatCode" | "text"
                        )
                    })
                }) && text(&l["accountNo"])
                    && ["debitAmount", "creditAmount"]
                        .iter()
                        .all(|k| l.get(*k).is_none_or(Value::is_number))
                    && ["vatCode", "text"]
                        .iter()
                        .all(|k| l.get(*k).is_none_or(|v| v.is_null() || v.is_string()))
            })
        })
        && serde_json::to_vec(v).is_ok_and(|b| b.len() <= MAX_PAYLOAD)
}
pub fn validate_original_accounting_draft(
    op: AccountingDraftOriginalOperation,
    input: &Value,
) -> Result<(), Error> {
    let schema = original_accounting_draft_schema(op);
    let valid = input.as_object().is_some_and(|o| {
        schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .all(|k| o.contains_key(k.as_str().unwrap()))
            && o.keys().all(|k| schema["properties"].get(k).is_some())
    }) && (op == List || id(&input["draftId"]))
        && (!op.write() || input["confirm"] == true)
        && (!matches!(op, Revise | Submit | Reject | ApproveAndPost)
            || hash(&input["expectedEventHash"]))
        && (!matches!(op, Create | Revise) || payload(&input["payload"]))
        && (op != Reject
            || input["reason"]
                .as_str()
                .is_some_and(|s| !s.trim().is_empty() && s.trim().len() <= 1000))
        && serde_json::to_vec(input).is_ok_and(|b| b.len() <= MAX_ACCOUNTING_DRAFT_BODY);
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidInput(
            "invalid original accounting draft input",
        ))
    }
}
/// Bounded local JSON object parser; duplicate keys are rejected recursively.
pub fn parse_original_accounting_draft_json(bytes: &[u8]) -> Result<Value, Error> {
    if bytes.len() > MAX_PAYLOAD {
        return Err(Error::InvalidInput("draft payload exceeds byte limit"));
    }
    let v = serde_json::from_slice::<UniqueJson>(bytes)
        .map_err(|_| Error::InvalidInput("invalid or duplicate JSON fields"))?
        .0;
    if !payload(&v) {
        return Err(Error::InvalidInput(
            "draft payload object required; caller authority forbidden",
        ));
    }
    Ok(v)
}
pub fn original_accounting_draft_file(path: &std::path::Path) -> Result<Value, Error> {
    use std::io::Read;
    if !std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() <= MAX_PAYLOAD as u64) {
        return Err(Error::InvalidInput(
            "bounded regular local draft JSON file required",
        ));
    }
    let f = std::fs::File::open(path)
        .map_err(|_| Error::InvalidInput("cannot open local draft JSON file"))?;
    if !f
        .metadata()
        .is_ok_and(|m| m.is_file() && m.len() <= MAX_PAYLOAD as u64)
    {
        return Err(Error::InvalidInput("bounded regular JSON file required"));
    }
    let mut bytes = Vec::new();
    f.take(MAX_PAYLOAD as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::InvalidInput("cannot read local draft JSON file"))?;
    parse_original_accounting_draft_json(&bytes)
}
fn failure(v: &Value) -> bool {
    exact(v, &["ok", "errors"])
        && v["ok"] == false
        && v["errors"]
            .as_array()
            .is_some_and(|a| !a.is_empty() && a.iter().all(text))
}
fn state(v: &Value) -> bool {
    let mut keys = vec![
        "id",
        "version",
        "status",
        "payloadHash",
        "eventHash",
        "payload",
        "actorId",
    ];
    for k in ["reason", "journalEntryId", "journal"] {
        if v.get(k).is_some() {
            keys.push(k);
        }
    }
    exact(v, &keys)
        && id(&v["id"])
        && v["id"].as_str().is_some_and(|s| s.trim() == s)
        && positive(&v["version"])
        && hash(&v["payloadHash"])
        && hash(&v["eventHash"])
        && payload(&v["payload"])
        && historical_actor(&v["actorId"])
        && [
            "created",
            "revised",
            "submitted",
            "rejected",
            "approved_posted",
        ]
        .contains(&v["status"].as_str().unwrap_or(""))
        && v.get("reason").is_none_or(|s| {
            s.as_str()
                .is_some_and(|s| !s.trim().is_empty() && s.len() <= 1000)
        })
        && ((v["status"] == "rejected") == v.get("reason").is_some())
        && (v["status"] != "created" || v["version"] == 1)
        && (v["status"] != "revised" || v["version"].as_u64().is_some_and(|n| n >= 2))
        && if v["status"] == "approved_posted" {
            positive(&v["journalEntryId"])
        } else {
            v.get("journalEntryId").is_none() && v.get("journal").is_none()
        }
}
fn success(
    op: AccountingDraftOriginalOperation,
    v: &Value,
    input: &Value,
    actor: Option<&str>,
    principal: Option<&str>,
) -> bool {
    if v["ok"] != true {
        return false;
    }
    if op == List {
        return exact(v, &["ok", "accountingDrafts"])
            && v["accountingDrafts"].as_array().is_some_and(|a| {
                a.iter().all(|s| state(s) && s.get("journal").is_none())
                    && a.windows(2)
                        .all(|p| p[0]["id"].as_str() < p[1]["id"].as_str())
            });
    }
    if !exact(v, &["ok", "accountingDraft"]) {
        return false;
    }
    let s = &v["accountingDraft"];
    if op == Show && s.is_null() {
        return true;
    }
    if !state(s) || s["id"].as_str() != input["draftId"].as_str().map(str::trim) {
        return false;
    }
    if !op.write() {
        return s.get("journal").is_none();
    }
    if [actor, principal]
        .iter()
        .flatten()
        .any(|a| s["actorId"] != *a)
    {
        return false;
    }
    let status = match op {
        Create => "created",
        Revise => "revised",
        Submit => "submitted",
        Reject => "rejected",
        ApproveAndPost => "approved_posted",
        _ => return false,
    };
    if s["status"] != status
        || (op == Create && s["version"] != 1)
        || (matches!(op, Create | Revise) && s["payload"] != input["payload"])
        || (op == Reject && s["reason"].as_str() != input["reason"].as_str().map(str::trim))
    {
        return false;
    }
    if op != ApproveAndPost {
        return s.get("journal").is_none();
    }
    let j = &s["journal"];
    exact(
        j,
        &[
            "ok",
            "entryId",
            "entryNo",
            "entryHash",
            "appliedRules",
            "errors",
        ],
    ) && j["ok"] == true
        && j["entryId"] == s["journalEntryId"]
        && text(&j["entryNo"])
        && hash(&j["entryHash"])
        && j["errors"] == json!([])
        && j["appliedRules"]
            .as_array()
            .is_some_and(|a| a.iter().all(text))
}
struct UniqueJson(Value);
impl<'de> serde::Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<UniqueJson, E> {
                Ok(UniqueJson(json!(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(json!(v)))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(json!(v)))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<UniqueJson, E> {
                Ok(UniqueJson(json!(v)))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<UniqueJson, E> {
                Ok(UniqueJson(json!(v)))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<UniqueJson, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<UniqueJson, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<UniqueJson, A::Error> {
                let mut out = vec![];
                while let Some(v) = a.next_element::<UniqueJson>()? {
                    out.push(v.0);
                }
                Ok(UniqueJson(Value::Array(out)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<UniqueJson, A::Error> {
                let mut out = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if out.contains_key(&k) {
                        return Err(serde::de::Error::custom("duplicate key"));
                    }
                    let v = a.next_value::<UniqueJson>()?;
                    out.insert(k, v.0);
                }
                Ok(UniqueJson(Value::Object(out)))
            }
        }
        d.deserialize_any(Visitor)
    }
}

impl ApiClient {
    pub async fn original_accounting_draft(
        &self,
        company: &str,
        op: AccountingDraftOriginalOperation,
        input: &Value,
        actor: Option<&str>,
        principal: Option<&str>,
    ) -> Result<Value, Error> {
        validate_original_accounting_draft(op, input)?;
        for a in [actor, principal].into_iter().flatten() {
            if !op.write() || !typed(&json!(a)) || a.len() > 256 {
                return Err(Error::InvalidInput(
                    "identity is a current typed write equality assertion only",
                ));
            }
        }
        let write = op.write();
        let ambiguous = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let mut wire = input.clone();
        if let Some(a) = actor {
            wire["actor"] = json!(a);
        }
        if let Some(p) = principal {
            wire["principal"] = json!(p);
        }
        let body = if write {
            let b = serde_json::to_vec(&wire).map_err(|_| Error::InvalidInput("invalid JSON"))?;
            if b.len() > MAX_ACCOUNTING_DRAFT_BODY {
                return Err(Error::InvalidInput("draft body exceeds byte limit"));
            }
            Some(b)
        } else {
            None
        };
        let query = if op == Show {
            vec![("draftId", input["draftId"].as_str().unwrap().trim())]
        } else {
            vec![]
        };
        let mut response = self
            .send(
                if write { Method::POST } else { Method::GET },
                &company_path(company, &format!("original/accounting-draft/{}", op.name()))?,
                &query,
                body,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        let status = response.status().as_u16();
        if status >= 500 || status == 408 {
            return Err(if write {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            });
        }
        if status != 200 && status != 422 {
            return Err(Error::Refused { status });
        }
        let invalid = || {
            if status == 422 {
                Error::Refused { status }
            } else {
                ambiguous()
            }
        };
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| invalid())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(invalid());
            }
            bytes.extend_from_slice(&chunk);
        }
        let v = serde_json::from_slice::<UniqueJson>(&bytes)
            .map_err(|_| invalid())?
            .0;
        if status == 422 {
            return if failure(&v) { Ok(v) } else { Err(invalid()) };
        }
        if success(op, &v, input, actor, principal) {
            Ok(v)
        } else {
            Err(ambiguous())
        }
    }
}
