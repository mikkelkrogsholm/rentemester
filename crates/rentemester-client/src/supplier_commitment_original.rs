//! Source-shaped adapter over the native source-bound commitment engine.
//! Native hashes bind vendor/provenance, immutable source bytes, requested input and
//! previousPayloadHash; schedules end at nextDate + 90 days inclusive. These are
//! NOT offline TypeScript pure-proposal hash/schedule aliases. No retries.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SupplierCommitmentOriginalOperation {
    Plan,
    Apply,
    List,
    Change,
    Match,
    Matches,
    Alerts,
}
use SupplierCommitmentOriginalOperation::*;
impl SupplierCommitmentOriginalOperation {
    fn write(self) -> bool {
        matches!(self, Apply | Change | Match)
    }
    fn suffix(self) -> &'static str {
        match self {
            Plan => "/plan",
            Apply => "/apply",
            List => "",
            Change => "/change",
            Match => "/match",
            Matches => "/matches",
            Alerts => "/alerts",
        }
    }
}
fn text() -> Value {
    json!({"type":"string","minLength":1,"maxLength":2000})
}
fn identifier() -> Value {
    json!({"type":"string","minLength":3,"maxLength":96})
}
fn hash_schema() -> Value {
    json!({"type":"string","pattern":"^[a-f0-9]{64}$"})
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
/// Actor and company deliberately absent: actor is only a separate write equality assertion.
pub fn original_supplier_commitment_schema(op: SupplierCommitmentOriginalOperation) -> Value {
    let commitment = object(
        json!({"commitmentId":identifier(),"vendorPartyId":text(),"vendorSnapshot":text(),"type":text(),"description":text(),"businessPurpose":text(),"amount":{"type":"number","exclusiveMinimum":0},"currency":{"type":"string","pattern":"^[A-Z]{3}$"},"frequency":{"enum":["weekly","monthly","quarterly","yearly"]},"nextDate":text(),"evidenceRefs":{"type":"array","minItems":1,"maxItems":100,"items":text()},"status":{"enum":["active","paused","ended","unresolved"]},"startDate":text(),"renewalDate":text(),"noticeDate":text(),"endDate":text(),"vatProposal":text()}),
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
    );
    match op {
        Plan => object(json!({"commitment":commitment}), &["commitment"]),
        Apply => object(
            json!({"commitment":commitment,"payloadHash":hash_schema(),"idempotencyKey":text(),"confirm":{"const":true}}),
            &["commitment", "payloadHash", "confirm"],
        ),
        List => object(json!({}), &[]),
        Change => object(
            json!({"commitmentId":identifier(),"action":{"enum":["paused","ended","superseded"]},"reason":text(),"confirm":{"const":true}}),
            &["commitmentId", "action", "reason", "confirm"],
        ),
        Match => object(
            json!({"commitmentId":identifier(),"occurrenceDate":text(),"evidence":object(json!({"kind":{"enum":["canonical_document","payable","bank_transaction"]},"id":text()}), &["kind","id"]),"confirm":{"const":true}}),
            &["commitmentId", "occurrenceDate", "evidence", "confirm"],
        ),
        Matches => object(json!({"commitmentId":identifier()}), &[]),
        Alerts => object(json!({"asOf":text()}), &["asOf"]),
    }
}
fn schema_valid(s: &Value, v: &Value) -> bool {
    if let Some(c) = s.get("const") {
        return c == v;
    }
    if let Some(a) = s["enum"].as_array() {
        return a.contains(v);
    }
    match s["type"].as_str() {
        Some("object") => v.as_object().is_some_and(|o| {
            o.keys().all(|k| s["properties"].get(k).is_some())
                && s["required"]
                    .as_array()
                    .is_some_and(|a| a.iter().all(|k| o.contains_key(k.as_str().unwrap_or(""))))
                && o.iter().all(|(k, v)| schema_valid(&s["properties"][k], v))
        }),
        Some("array") => v.as_array().is_some_and(|a| {
            a.len() >= s["minItems"].as_u64().unwrap_or(0) as usize
                && a.len() <= s["maxItems"].as_u64().unwrap_or(10000) as usize
                && a.iter().all(|v| schema_valid(&s["items"], v))
        }),
        Some("number") => v.as_f64().is_some_and(|n| n.is_finite() && n > 0.0),
        Some("string") => v.as_str().is_some_and(|v| {
            let n = v.chars().count();
            !v.chars().any(char::is_control)
                && (s.get("minLength").is_none() || !v.trim().is_empty())
                && n >= s["minLength"].as_u64().unwrap_or(0) as usize
                && n <= s["maxLength"].as_u64().unwrap_or(2000) as usize
                && match s["pattern"].as_str() {
                    Some("^[a-f0-9]{64}$") => hash(&json!(v)),
                    Some("^[A-Z]{3}$") => v.len() == 3 && v.bytes().all(|b| b.is_ascii_uppercase()),
                    _ => true,
                }
        }),
        _ => false,
    }
}
pub fn validate_original_supplier_commitment(
    op: SupplierCommitmentOriginalOperation,
    input: &Value,
) -> Result<(), Error> {
    if serde_json::to_vec(input)
        .map_err(|_| Error::InvalidInput("invalid JSON"))?
        .len()
        > 65536
        || !schema_valid(&original_supplier_commitment_schema(op), input)
    {
        return Err(Error::InvalidInput("invalid supplier commitment input"));
    }
    // Initial non-active statuses are deliberately sent for a definite native business
    // refusal, never silently coerced into active. Invalid asOf is source-defined empty rows.
    Ok(())
}
fn nonblank(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| !s.trim().is_empty() && s.len() <= 65536)
}
fn hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn exact(v: &Value, keys: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == keys.len() && keys.iter().all(|k| o.contains_key(*k)))
}
fn date(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        time::format_description::parse("[year]-[month]-[day]")
            .is_ok_and(|f| s.len() == 10 && time::Date::parse(s, &f).is_ok())
    })
}
fn money(v: &Value) -> bool {
    v.as_f64().is_some_and(f64::is_finite)
        || v.as_str()
            .is_some_and(|s| s.len() <= 64 && s.parse::<f64>().is_ok_and(f64::is_finite))
}
fn errors(v: &Value) -> bool {
    v.as_array()
        .is_some_and(|a| !a.is_empty() && a.len() <= 1000 && a.iter().all(nonblank))
}
fn failure(v: &Value) -> bool {
    exact(v, &["ok", "errors"]) && v["ok"] == false && errors(&v["errors"])
}
fn variance(v: &Value) -> bool {
    exact(
        v,
        &[
            "amount",
            "dateDays",
            "late",
            "currency",
            "party",
            "documentation",
            "comparability",
        ],
    ) && (v["amount"].is_null() || money(&v["amount"]))
        && v["dateDays"].is_i64()
        && v["late"].is_boolean()
        && exact(&v["currency"], &["expected", "actual", "status"])
        && nonblank(&v["currency"]["expected"])
        && nonblank(&v["currency"]["actual"])
        && ["same", "unsupported_fx"].contains(&v["currency"]["status"].as_str().unwrap_or(""))
        && exact(&v["party"], &["expected", "actual", "status"])
        && ["same", "missing", "mismatch"].contains(&v["party"]["status"].as_str().unwrap_or(""))
        && ["present", "missing"].contains(&v["documentation"].as_str().unwrap_or(""))
        && ["native_currency", "unsupported_fx"]
            .contains(&v["comparability"].as_str().unwrap_or(""))
}
fn proposal(p: &Value) -> bool {
    exact(
        p,
        &[
            "commitmentId",
            "vendorPartyId",
            "vendorSnapshot",
            "vendorProvenance",
            "type",
            "description",
            "businessPurpose",
            "amount",
            "currency",
            "frequency",
            "nextDate",
            "startDate",
            "renewalDate",
            "noticeDate",
            "endDate",
            "evidenceRefs",
            "sourceEvidence",
            "vatProposal",
            "status",
            "requestedInput",
            "previousPayloadHash",
            "payloadHash",
            "schedule",
            "warnings",
        ],
    ) && [
        "commitmentId",
        "vendorPartyId",
        "vendorSnapshot",
        "type",
        "description",
        "businessPurpose",
        "currency",
    ]
    .iter()
    .all(|k| nonblank(&p[*k]))
        && p["status"] == "active"
        && hash(&p["payloadHash"])
        && (p["previousPayloadHash"].is_null() || hash(&p["previousPayloadHash"]))
        && money(&p["amount"])
        && date(&p["nextDate"])
        && ["weekly", "monthly", "quarterly", "yearly"]
            .contains(&p["frequency"].as_str().unwrap_or(""))
        && p["vendorProvenance"]["kind"] == "ledger_vendor"
        && p["vendorProvenance"]["vendor"].is_object()
        && p["requestedInput"].is_object()
        && p["evidenceRefs"]
            .as_array()
            .is_some_and(|a| !a.is_empty() && a.len() <= 100 && a.iter().all(nonblank))
        && p["sourceEvidence"].as_array().is_some_and(|a| {
            !a.is_empty()
                && a.len() <= 100
                && a.iter().all(|r| {
                    exact(r, &["documentId", "sha256", "filename"])
                        && r["documentId"].as_i64().is_some_and(|n| n > 0)
                        && hash(&r["sha256"])
                        && nonblank(&r["filename"])
                })
        })
        && p["warnings"]
            .as_array()
            .is_some_and(|a| a.len() <= 100 && a.iter().all(nonblank))
        && p["schedule"].as_array().is_some_and(|a| {
            !a.is_empty()
                && a.len() <= 14
                && a.iter().all(|r| {
                    exact(r, &["date", "amount", "currency"])
                        && date(&r["date"])
                        && money(&r["amount"])
                        && r["currency"] == p["currency"]
                })
                && a.windows(2)
                    .all(|w| w[0]["date"].as_str() < w[1]["date"].as_str())
        })
}
fn valid(op: SupplierCommitmentOriginalOperation, v: &Value, input: &Value) -> bool {
    if v["ok"] == false {
        return failure(v);
    }
    if v["ok"] != true {
        return false;
    }
    match op {
        Plan => {
            exact(v, &["ok", "proposal"])
                && proposal(&v["proposal"])
                && v["proposal"]["commitmentId"] == input["commitment"]["commitmentId"]
                && input["commitment"].as_object().is_some_and(|fields| {
                    fields
                        .iter()
                        .filter(|(k, _)| !matches!(k.as_str(), "vendorSnapshot" | "status"))
                        .all(|(k, expected)| v["proposal"]["requestedInput"][k] == *expected)
                })
        }
        Apply => {
            exact(v, &["ok", "idempotent", "commitmentId", "payloadHash"])
                && v["idempotent"].is_boolean()
                && hash(&v["payloadHash"])
                && v["payloadHash"] == input["payloadHash"]
                && v["commitmentId"] == input["commitment"]["commitmentId"]
        }
        Change => {
            exact(v, &["ok", "idempotent", "payloadHash"])
                && v["idempotent"].is_boolean()
                && hash(&v["payloadHash"])
        }
        Match => {
            exact(v, &["ok", "idempotent", "payloadHash", "variance"])
                && v["idempotent"].is_boolean()
                && hash(&v["payloadHash"])
                && variance(&v["variance"])
        }
        List | Matches | Alerts => {
            exact(v, &["ok", "rows", "errors"])
                && v["errors"] == json!([])
                && v["rows"].as_array().is_some_and(|rows| {
                    rows.len() <= 100000
                        && (op != Alerts
                            || rows.windows(2).all(|w| {
                                (w[0]["date"].as_str(), w[0]["commitmentId"].as_str())
                                    <= (w[1]["date"].as_str(), w[1]["commitmentId"].as_str())
                            }))
                        && rows.iter().all(|r| match op {
                            List => {
                                exact(
                                    r,
                                    &[
                                        "commitment_id",
                                        "payload_json",
                                        "payload_hash",
                                        "created_at",
                                    ],
                                ) && nonblank(&r["commitment_id"])
                                    && hash(&r["payload_hash"])
                                    && nonblank(&r["created_at"])
                                    && r["payload_json"]
                                        .as_str()
                                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                                        .is_some_and(|p| proposal(&p))
                            }
                            Matches => {
                                exact(
                                    r,
                                    &[
                                        "commitmentId",
                                        "occurrenceDate",
                                        "expected",
                                        "evidence",
                                        "variance",
                                        "payloadHash",
                                        "actor",
                                        "principal",
                                        "createdAt",
                                    ],
                                ) && nonblank(&r["commitmentId"])
                                    && date(&r["occurrenceDate"])
                                    && hash(&r["payloadHash"])
                                    && ["actor", "principal", "createdAt"]
                                        .iter()
                                        .all(|k| nonblank(&r[*k]))
                                    && variance(&r["variance"])
                                    && exact(
                                        &r["expected"],
                                        &["date", "amount", "currency", "payloadHash"],
                                    )
                                    && date(&r["expected"]["date"])
                                    && money(&r["expected"]["amount"])
                                    && hash(&r["expected"]["payloadHash"])
                                    && exact(
                                        &r["evidence"],
                                        &[
                                            "kind",
                                            "id",
                                            "date",
                                            "amount",
                                            "currency",
                                            "partyId",
                                            "documented",
                                            "sourceHash",
                                            "documentId",
                                        ],
                                    )
                                    && ["canonical_document", "payable", "bank_transaction"]
                                        .contains(&r["evidence"]["kind"].as_str().unwrap_or(""))
                                    && nonblank(&r["evidence"]["id"])
                                    && date(&r["evidence"]["date"])
                                    && money(&r["evidence"]["amount"])
                                    && hash(&r["evidence"]["sourceHash"])
                                    && r["evidence"]["documented"].is_boolean()
                                    && (input.get("commitmentId").is_none()
                                        || input["commitmentId"] == r["commitmentId"])
                            }
                            Alerts => {
                                exact(r, &["commitmentId", "kind", "date", "severity"])
                                    && nonblank(&r["commitmentId"])
                                    && date(&r["date"])
                                    && ["notice", "renewal"]
                                        .contains(&r["kind"].as_str().unwrap_or(""))
                                    && ["due", "upcoming"]
                                        .contains(&r["severity"].as_str().unwrap_or(""))
                                    && time::format_description::parse("[year]-[month]-[day]")
                                        .is_ok_and(|f| {
                                            let start = input["asOf"]
                                                .as_str()
                                                .and_then(|s| time::Date::parse(s, &f).ok());
                                            let day = r["date"]
                                                .as_str()
                                                .and_then(|s| time::Date::parse(s, &f).ok());
                                            match (start, day) {
                                                (Some(start), Some(day)) => {
                                                    day >= start
                                                        && (day - start).whole_days() <= 30
                                                        && r["severity"]
                                                            == if day == start {
                                                                "due"
                                                            } else {
                                                                "upcoming"
                                                            }
                                                }
                                                _ => false,
                                            }
                                        })
                            }
                            _ => false,
                        })
                })
        }
    }
}
impl ApiClient {
    pub async fn original_supplier_commitment(
        &self,
        company: &str,
        operation: SupplierCommitmentOriginalOperation,
        input: &Value,
        actor: Option<&str>,
    ) -> Result<Value, Error> {
        validate_original_supplier_commitment(operation, input)?;
        if actor.is_some_and(|a| {
            a.trim().is_empty() || a.len() > 256 || a.chars().any(char::is_control)
        }) || (actor.is_some() && !operation.write())
        {
            return Err(Error::InvalidInput(
                "actor is a write equality assertion only",
            ));
        }
        let write = operation.write();
        let ambiguous = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let post = matches!(operation, Plan | Apply | Change | Match);
        let mut wire = input.clone();
        if let Some(actor) = actor {
            wire["actor"] = json!(actor);
        }
        let query: Vec<(&str, &str)> = if post {
            vec![]
        } else {
            input
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str().unwrap()))
                .collect()
        };
        let mut response = self
            .send(
                if post { Method::POST } else { Method::GET },
                &company_path(
                    company,
                    &format!("original/supplier-commitments{}", operation.suffix()),
                )?,
                &query,
                if post {
                    Some(
                        serde_json::to_vec(&wire)
                            .map_err(|_| Error::InvalidInput("invalid JSON"))?,
                    )
                } else {
                    None
                },
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await
            .map_err(|e| {
                if !write && e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
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
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| ambiguous())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(ambiguous());
            }
            bytes.extend_from_slice(&chunk);
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status == 422 {
                Error::Refused { status }
            } else {
                ambiguous()
            }
        })?;
        if status == 422 {
            return if failure(&v) {
                Ok(v)
            } else {
                Err(Error::Refused { status })
            };
        }
        if valid(operation, &v, input) {
            Ok(v)
        } else {
            Err(ambiguous())
        }
    }
}
