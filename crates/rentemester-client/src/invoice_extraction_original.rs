//! Source-compatible extraction projections. Native methods and transport stay unchanged.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};

fn object(p: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":p,"required":required,"additionalProperties":false})
}
fn id_schema() -> Value {
    json!({"type":"integer","minimum":1,"maximum":9007199254740991u64})
}
fn strings() -> Value {
    json!({"type":"array","items":{"type":"string"}})
}
/// Exact retained storage::invoice_extraction::inspect projection, not provider JSON.
pub fn original_invoice_extraction_surface_schema() -> Value {
    let text = json!({"type":"string"});
    let scalar = json!({"type":["string","number","boolean","null"]});
    let field = object(
        json!({
            "key":{"enum":["invoiceNumber","supplierName","supplierCountry","supplierLegalId","supplierLegalIdKind","buyerName","buyerCountry","buyerLegalId","buyerLegalIdKind","invoiceDate","currency","netAmount","vatAmount","grossAmount","reverseChargeWording"]},
            "value":scalar,"confidence":{"type":"number","minimum":0,"maximum":1},"page":id_schema(),"sourceText":{"type":"string","minLength":1,"maxLength":4000},
            "box":object(json!({"x":{"type":"number"},"y":{"type":"number"},"width":{"type":"number","minimum":0},"height":{"type":"number","minimum":0}}), &["x","y","width","height"])
        }),
        &["key", "value", "confidence", "page", "sourceText"],
    );
    let terminal = object(
        json!({"fieldKey":{"const":"extraction_status"},"value":object(json!({"status":{"enum":["completed","needs_resolution"]},"errors":strings()}), &["status","errors"])}),
        &["fieldKey", "value"],
    );
    let failure = object(
        json!({"fieldKey":{"const":"provider_failure"},"value":object(json!({"code":{"enum":["EXTRACTION_PROVIDER_UNAVAILABLE","EXTRACTION_UNSUPPORTED"]}}), &["code"])}),
        &["fieldKey", "value"],
    );
    let surface = object(
        json!({"attemptId":id_schema(),"documentId":id_schema(),"status":{"enum":["started","completed","needs_resolution"]},"originalHash":{"type":"string","minLength":64,"maxLength":64},"fields":{"type":"array","maxItems":100,"items":field},"resolutions":{"type":"array","items":{"anyOf":[terminal,failure]}},"conflicts":strings(),"exceptionState":{"anyOf":[{"type":"null"},object(json!({"id":id_schema(),"status":text}), &["id","status"])]}}),
        &[
            "attemptId",
            "documentId",
            "status",
            "originalHash",
            "fields",
            "resolutions",
            "conflicts",
            "exceptionState",
        ],
    );
    json!({"anyOf":[{"type":"null"},surface]})
}
fn shape(v: &Value, s: &Value) -> bool {
    if let Some(a) = s["anyOf"].as_array() {
        return a.iter().any(|s| shape(v, s));
    }
    if let Some(c) = s.get("const") {
        return v == c;
    }
    if let Some(a) = s["enum"].as_array() {
        return a.contains(v);
    }
    if let Some(a) = s["type"].as_array() {
        return a.iter().any(|t| {
            let mut s = s.clone();
            s["type"] = t.clone();
            shape(v, &s)
        });
    }
    match s["type"].as_str() {
        Some("object") => v.as_object().is_some_and(|o| {
            s["required"]
                .as_array()
                .is_none_or(|a| a.iter().all(|k| o.contains_key(k.as_str().unwrap())))
                && o.iter()
                    .all(|(k, v)| s["properties"].get(k).is_some_and(|s| shape(v, s)))
        }),
        Some("array") => v.as_array().is_some_and(|a| {
            s["maxItems"].as_u64().is_none_or(|n| a.len() as u64 <= n)
                && a.iter().all(|v| shape(v, &s["items"]))
        }),
        Some("string") => v.as_str().is_some_and(|v| {
            let n = v.encode_utf16().count() as u64;
            s["minLength"].as_u64().is_none_or(|m| n >= m)
                && s["maxLength"].as_u64().is_none_or(|m| n <= m)
        }),
        Some("integer" | "number") => {
            (s["type"] != "integer" || v.as_i64().is_some())
                && v.as_f64().is_some_and(|n| {
                    n.is_finite()
                        && s["minimum"].as_f64().is_none_or(|m| n >= m)
                        && s["maximum"].as_f64().is_none_or(|m| n <= m)
                })
        }
        Some("boolean") => v.is_boolean(),
        Some("null") => v.is_null(),
        _ => false,
    }
}
const DOMAIN: &[&str] = &[
    "EXTRACTION_SNAPSHOT_HASH_MISMATCH",
    "EXTRACTION_SNAPSHOT_INVALID",
    "EXTRACTION_SNAPSHOT_UNAVAILABLE",
    "EXTRACTION_PROVIDER_UNAVAILABLE",
    "EXTRACTION_UNSUPPORTED",
    "EXTRACTION_EVIDENCE_TAMPERED",
    "EXTRACTION_FAILED",
];
fn failure(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.len() == 2)
        && v["ok"] == false
        && v["errors"].as_array().is_some_and(|a| {
            !a.is_empty()
                && a.iter()
                    .all(|e| e.as_str().is_some_and(|e| DOMAIN.contains(&e)))
        })
}
fn success(v: &Value, document_id: i64) -> bool {
    if !v.as_object().is_some_and(|o| o.len() == 2)
        || v["ok"] != true
        || !v
            .get("extraction")
            .is_some_and(|e| shape(e, &original_invoice_extraction_surface_schema()))
    {
        return false;
    }
    let e = &v["extraction"];
    if e.is_null() {
        return true;
    }
    e["documentId"] == document_id
        && e["originalHash"]
            .as_str()
            .is_some_and(|s| s.bytes().all(|b| b.is_ascii_hexdigit()))
        && e["fields"].as_array().unwrap().iter().all(|f| {
            let key = f["key"].as_str().unwrap();
            !f["sourceText"].as_str().unwrap().trim().is_empty()
                && match key {
                    "netAmount" | "vatAmount" | "grossAmount" => {
                        f["value"].as_f64().is_some_and(|n| n >= 0.0)
                    }
                    "invoiceDate" => f["value"].as_str().is_some_and(|s| {
                        s.len() == 10
                            && s.bytes().enumerate().all(|(i, b)| {
                                if i == 4 || i == 7 {
                                    b == b'-'
                                } else {
                                    b.is_ascii_digit()
                                }
                            })
                    }),
                    "currency" => f["value"].as_str().is_some_and(|s| {
                        !s.trim().is_empty() && s.trim().encode_utf16().count() == 3
                    }),
                    "supplierLegalId" | "buyerLegalId" => f["value"].as_str().is_some_and(|s| {
                        !s.trim().is_empty() && s.trim().encode_utf16().count() <= 64
                    }),
                    _ => {
                        f["value"].is_string()
                            || f["value"].is_boolean()
                            || f["value"].is_number()
                            || f["value"].is_null()
                    }
                }
        })
}
fn document(id: i64) -> Result<(), Error> {
    if (1..=9_007_199_254_740_991).contains(&id) {
        Ok(())
    } else {
        Err(Error::InvalidInput("positive exact document id required"))
    }
}
impl ApiClient {
    pub async fn original_invoice_extraction(
        &self,
        company: &str,
        document_id: i64,
    ) -> Result<Value, Error> {
        document(document_id)?;
        self.original_extraction_request(company, document_id, None)
            .await
    }
    pub async fn original_extract_invoice(
        &self,
        company: &str,
        document_id: i64,
        confirmed: bool,
        expected_actor: Option<&str>,
    ) -> Result<Value, Error> {
        document(document_id)?;
        if !confirmed {
            return Err(Error::InvalidInput("confirmation required"));
        }
        let mut body = json!({"confirm":true});
        if let Some(actor) = expected_actor {
            if actor.trim().is_empty()
                || actor != actor.trim()
                || actor.encode_utf16().count() > 160
                || actor.chars().any(char::is_control)
            {
                return Err(Error::InvalidInput("invalid expected actor assertion"));
            }
            body["expectedActor"] = json!(actor);
        }
        self.original_extraction_request(
            company,
            document_id,
            Some(serde_json::to_vec(&body).map_err(|_| Error::Protocol)?),
        )
        .await
    }
    async fn original_extraction_request(
        &self,
        company: &str,
        id: i64,
        body: Option<Vec<u8>>,
    ) -> Result<Value, Error> {
        let write = body.is_some();
        let uncertain = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let transport = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        };
        let suffix = if write {
            "extract-invoice"
        } else {
            "invoice-extraction"
        };
        let mut response = self
            .send(
                if write { Method::POST } else { Method::GET },
                &company_path(company, &format!("documents/original/{id}/{suffix}"))?,
                &[],
                body,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        let status = response.status().as_u16();
        // Authority status always wins over even a forged success/domain response.
        if ![200, 422, 503].contains(&status) {
            return Err(if status >= 500 || status == 408 {
                transport()
            } else {
                Error::Refused { status }
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| transport())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(if status == 503 {
                    transport()
                } else {
                    uncertain()
                });
            }
            bytes.extend_from_slice(&chunk);
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status == 503 {
                transport()
            } else if status == 422 {
                Error::Refused { status }
            } else {
                uncertain()
            }
        })?;
        if status == 503 {
            return if v == json!({"ok":false,"errors":["EXTRACTION_PROVIDER_UNAVAILABLE"]}) {
                Ok(v)
            } else {
                Err(transport())
            };
        }
        if status == 422 {
            return if failure(&v) {
                Ok(v)
            } else {
                Err(Error::Refused { status })
            };
        }
        if success(&v, id) || failure(&v) {
            Ok(v)
        } else {
            Err(uncertain())
        }
    }
}
