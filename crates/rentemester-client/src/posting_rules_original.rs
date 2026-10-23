//! Frozen source posting-rule ABI. All calls are single-shot; dry runs grant no posting authority.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PostingRulesOriginalOperation {
    Propose,
    Approve,
    Disable,
    Supersede,
    Explain,
    Test,
}
use PostingRulesOriginalOperation::*;
impl PostingRulesOriginalOperation {
    pub fn write(self) -> bool {
        !matches!(self, Explain | Test)
    }
    pub fn name(self) -> &'static str {
        match self {
            Propose => "propose",
            Approve => "approve",
            Disable => "disable",
            Supersede => "supersede",
            Explain => "explain",
            Test => "test",
        }
    }
}
fn object(p: Value, r: &[&str]) -> Value {
    crate::domain::object(p, r)
}
fn text(max: usize) -> Value {
    json!({"type":"string","minLength":1,"maxLength":max})
}
fn money() -> Value {
    json!({"type":"number","minimum":-90071992547409.9f64,"maximum":90071992547409.9f64})
}
fn facts(context: bool) -> Value {
    let mut p = json!({});
    for k in [
        "supplierIdentity",
        "documentType",
        "supplierCountry",
        "supplierVat",
        "currency",
    ] {
        p[k] = text(256);
    }
    p["reverseChargeWording"] = json!({"type":"boolean"});
    p["attributes"] = json!({"type":"object","maxProperties":12,"propertyNames":{"pattern":"^[A-Za-z][A-Za-z0-9_.-]{0,63}$"},"additionalProperties":text(256)});
    if context {
        p["amount"] = money();
        p["vatAmount"] = money();
        p["documentId"] = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
        for k in [
            "changedBuyer",
            "changedSupplier",
            "changedVat",
            "changedCurrency",
            "unusualAmount",
            "contradictoryEvidence",
        ] {
            p[k] = json!({"type":"boolean"});
        }
    } else {
        p["vat"] = json!({"type":"string","enum":["zero","positive"]});
        p["amount"] = object(json!({"min":money(),"max":money()}), &[]);
    }
    object(p, &[])
}
fn outcome() -> Value {
    object(
        json!({"account":text(128),"vatTreatment":text(256),"textTemplate":text(2000),"dimensions":{"type":"object","maxProperties":12,"additionalProperties":text(256)}}),
        &[],
    )
}
pub fn original_posting_rules_schema(op: PostingRulesOriginalOperation) -> Value {
    match op {
        Propose => object(
            json!({"proposal":object(json!({"ruleId":text(128),"effectiveFrom":text(24),"effectiveTo":text(24),"conditions":facts(false),"outcome":outcome(),"provenance":text(256),"rationale":text(2000)}),&["ruleId","effectiveFrom","conditions","outcome","provenance","rationale"]),"confirm":{"const":true}}),
            &["proposal", "confirm"],
        ),
        Approve | Disable | Supersede => object(
            json!({"ruleId":text(128),"version":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"expectedPayloadHash":{"type":"string","minLength":64,"maxLength":64,"pattern":"^[0-9a-fA-F]{64}$"},"rationale":text(2000),"provenance":text(256),"effectiveAt":text(24),"confirm":{"const":true}}),
            &[
                "ruleId",
                "version",
                "expectedPayloadHash",
                "rationale",
                "provenance",
                "confirm",
            ],
        ),
        Explain | Test => object(json!({"context":facts(true),"at":text(24)}), &["context"]),
    }
}
fn date(v: &Value) -> bool {
    let Some(s) = v.as_str() else { return false };
    if s.len() == 10 && s.is_ascii() && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' {
        return s[..4]
            .parse::<i32>()
            .ok()
            .filter(|y| (1..=9999).contains(y))
            .and_then(|y| {
                Some(
                    time::Date::from_calendar_date(
                        y,
                        time::Month::try_from(s[5..7].parse::<u8>().ok()?).ok()?,
                        s[8..].parse().ok()?,
                    )
                    .ok()?,
                )
            })
            .is_some();
    }
    matches!(s.len(), 20 | 24)
        && s.is_ascii()
        && s.ends_with('Z')
        && time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).is_ok()
}
fn hash(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
}
fn cents(v: &Value) -> bool {
    let s = v.to_string();
    // Exact decimal cents: never accept a value by rounding a floating point product.
    let (mantissa, exponent) = s
        .split_once(['e', 'E'])
        .map_or((s.as_str(), Some(0i32)), |(a, b)| {
            (a, b.parse::<i32>().ok())
        });
    let Some(exponent) = exponent else {
        return false;
    };
    let digits = mantissa.trim_start_matches('-');
    let (whole, frac) = digits.split_once('.').unwrap_or((digits, ""));
    let all = format!("{whole}{frac}");
    let scale = frac.len() as i32 - exponent - 2;
    if scale > 0
        && (scale as usize > all.len()
            || !all[all.len() - scale as usize..].bytes().all(|b| b == b'0'))
    {
        return false;
    }
    v.as_f64()
        .is_some_and(|n| n.is_finite() && n.abs() <= 90071992547409.9)
}
fn key(s: &str) -> bool {
    let mut b = s.bytes();
    s.len() <= 64
        && b.next().is_some_and(|b| b.is_ascii_alphabetic())
        && b.all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}
fn shape(v: &Value, s: &Value) -> bool {
    if let Some(c) = s.get("const") {
        return v == c;
    }
    if s["enum"].as_array().is_some_and(|a| !a.contains(v)) {
        return false;
    }
    match s["type"].as_str() {
        Some("object") => v.as_object().is_some_and(|o| {
            s["required"]
                .as_array()
                .is_none_or(|a| a.iter().all(|k| o.contains_key(k.as_str().unwrap())))
                && o.len() <= s["maxProperties"].as_u64().unwrap_or(u64::MAX) as usize
                && o.iter().all(|(k, v)| {
                    if let Some(p) = s["properties"].get(k) {
                        shape(v, p)
                    } else {
                        s["additionalProperties"].is_object()
                            && (s.get("propertyNames").is_none() || key(k))
                            && shape(v, &s["additionalProperties"])
                    }
                })
        }),
        Some("string") => v.as_str().is_some_and(|t| {
            !t.trim().is_empty()
                && t == t.trim()
                && !t.chars().any(char::is_control)
                && t.chars().count() >= s["minLength"].as_u64().unwrap_or(0) as usize
                && t.chars().count() <= s["maxLength"].as_u64().unwrap_or(u64::MAX) as usize
        }),
        Some("boolean") => v.is_boolean(),
        Some("integer") => v.as_u64().is_some_and(|n| {
            n >= s["minimum"].as_u64().unwrap_or(0)
                && n <= s["maximum"].as_u64().unwrap_or(9007199254740991)
        }),
        Some("number") => cents(v),
        _ => false,
    }
}
pub fn validate_original_posting_rules(
    op: PostingRulesOriginalOperation,
    input: &Value,
) -> Result<(), Error> {
    let bad = || Error::InvalidInput("invalid original posting-rule input");
    if serde_json::to_vec(input).map_err(|_| bad())?.len() > 65_536
        || !shape(input, &original_posting_rules_schema(op))
    {
        return Err(bad());
    }
    let p = if op == Propose {
        &input["proposal"]
    } else {
        input
    };
    for k in ["effectiveFrom", "effectiveTo", "effectiveAt", "at"] {
        if p.get(k).is_some_and(|v| !date(v)) {
            return Err(bad());
        }
    }
    if let (Some(a), Some(b)) = (p["effectiveFrom"].as_str(), p["effectiveTo"].as_str()) {
        let normalize = |s: &str| match s.len() {
            10 => format!("{s}T00:00:00.000Z"),
            20 => format!("{}.000Z", &s[..19]),
            _ => s.to_owned(),
        };
        if normalize(a) > normalize(b) {
            return Err(bad());
        }
    }
    if op.write() && op != Propose && !hash(&p["expectedPayloadHash"]) {
        return Err(bad());
    }
    let f = if op == Propose {
        &p["conditions"]
    } else {
        &p["context"]
    };
    if f.get("currency").is_some_and(|v| {
        v.as_str()
            .is_none_or(|s| s.len() != 3 || !s.bytes().all(|b| b.is_ascii_uppercase()))
    }) {
        return Err(bad());
    }
    let a = &f["amount"];
    if a.is_object()
        && (a["min"]
            .as_f64()
            .zip(a["max"].as_f64())
            .is_some_and(|(a, b)| a > b))
    {
        return Err(bad());
    }
    Ok(())
}
fn exact(v: &Value, keys: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == keys.len() && keys.iter().all(|k| o.contains_key(*k)))
}
fn strings(v: &Value) -> bool {
    v.as_array().is_some_and(|a| {
        a.iter()
            .all(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()))
    })
}
fn failure(v: &Value) -> bool {
    exact(v, &["ok", "errors"])
        && v["ok"] == false
        && strings(&v["errors"])
        && !v["errors"].as_array().unwrap().is_empty()
}
fn positive(v: &Value) -> bool {
    v.as_u64()
        .is_some_and(|n| (1..=9007199254740991).contains(&n))
}
fn valid(op: PostingRulesOriginalOperation, v: &Value) -> bool {
    if op == Propose {
        return exact(v, &["ok", "duplicate", "version", "payloadHash", "errors"])
            && v["ok"] == true
            && v["duplicate"].is_boolean()
            && positive(&v["version"])
            && hash(&v["payloadHash"])
            && v["errors"] == json!([]);
    }
    if op.write() {
        return exact(v, &["ok", "errors"]) && v["ok"] == true && v["errors"] == json!([]);
    }
    let proposed = v["decision"] == "proposed";
    if !(if proposed {
        exact(
            v,
            &[
                "decision",
                "outcome",
                "matched",
                "explanations",
                "reasons",
                "ruleVersionId",
                "payloadHash",
                "dryRun",
            ],
        )
    } else {
        v["decision"] == "human_decision"
            && exact(
                v,
                &["decision", "matched", "explanations", "reasons", "dryRun"],
            )
    }) || v["dryRun"] != true
        || !strings(&v["reasons"])
    {
        return false;
    }
    let Some(explanations) = v["explanations"].as_array() else {
        return false;
    };
    if !explanations.iter().all(|e| {
        exact(
            e,
            &[
                "ruleId",
                "version",
                "payloadHash",
                "state",
                "matched",
                "reasons",
            ],
        ) && e["ruleId"].as_str().is_some_and(|s| !s.is_empty())
            && positive(&e["version"])
            && hash(&e["payloadHash"])
            && (e["state"].is_null()
                || ["proposed", "approved", "rejected", "disabled", "superseded"]
                    .contains(&e["state"].as_str().unwrap_or("")))
            && e["matched"].is_boolean()
            && (if e["matched"] == true {
                e["state"] == "approved" && e["reasons"] == json!([])
            } else {
                e["reasons"].as_array().is_some_and(|r| !r.is_empty())
            })
            && strings(&e["reasons"])
    }) {
        return false;
    }
    if !explanations.windows(2).all(|w| {
        (w[0]["ruleId"].as_str(), w[0]["version"].as_u64())
            < (w[1]["ruleId"].as_str(), w[1]["version"].as_u64())
    }) {
        return false;
    }
    let matched: Vec<Value> = explanations
        .iter()
        .filter(|e| e["matched"] == true)
        .cloned()
        .collect();
    if v["matched"] != json!(matched) {
        return false;
    }
    if proposed {
        matched.len() == 1
            && matched[0]["state"] == "approved"
            && v["payloadHash"] == matched[0]["payloadHash"]
            && positive(&v["ruleVersionId"])
            && shape(&v["outcome"], &outcome())
            && v["reasons"] == json!([])
    } else {
        !v["reasons"].as_array().unwrap().is_empty()
    }
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
    pub async fn original_posting_rules(
        &self,
        company: &str,
        op: PostingRulesOriginalOperation,
        input: &Value,
        actor: Option<&str>,
        principal: Option<&str>,
    ) -> Result<Value, Error> {
        validate_original_posting_rules(op, input)?;
        for a in [actor, principal] {
            if a.is_some_and(|a| {
                !op.write()
                    || a.trim().is_empty()
                    || a.len() > 256
                    || a.chars().any(char::is_control)
            }) {
                return Err(Error::InvalidInput(
                    "identity is a write credential equality assertion only",
                ));
            }
        }
        let mut wire = input.clone();
        if let Some(a) = actor {
            wire["actor"] = json!(a);
        }
        if let Some(a) = principal {
            wire["principal"] = json!(a);
        }
        let body = serde_json::to_vec(&wire).map_err(|_| Error::InvalidInput("invalid JSON"))?;
        if body.len() > 65_536 {
            return Err(Error::InvalidInput(
                "posting-rule final body exceeds 65 KiB",
            ));
        }
        let ambiguous = || {
            if op.write() {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let mut response = self
            .send(
                Method::POST,
                &company_path(company, &format!("original/posting-rules/{}", op.name()))?,
                &[],
                Some(body),
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await
            .map_err(|e| {
                if !op.write() && e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let status = response.status().as_u16();
        if status >= 500 || status == 408 {
            return Err(if op.write() {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            });
        }
        if status != 200 && status != 422 {
            return Err(Error::Refused { status });
        }
        let mut bytes = Vec::new();
        while let Some(c) = response.chunk().await.map_err(|_| ambiguous())? {
            if bytes.len().saturating_add(c.len()) > MAX_RESPONSE {
                return Err(ambiguous());
            }
            bytes.extend_from_slice(&c);
        }
        let v = serde_json::from_slice::<UniqueJson>(&bytes)
            .map_err(|_| {
                if status == 422 {
                    Error::Refused { status }
                } else {
                    ambiguous()
                }
            })?
            .0;
        if status == 422 {
            return if failure(&v) {
                Ok(v)
            } else {
                Err(Error::Refused { status })
            };
        }
        if !valid(op, &v) {
            return Err(ambiguous());
        }
        if !op.write()
            && v["decision"] == "proposed"
            && [
                "changedBuyer",
                "changedSupplier",
                "changedVat",
                "changedCurrency",
                "unusualAmount",
                "contradictoryEvidence",
            ]
            .iter()
            .any(|k| input["context"][*k] == true)
        {
            return Err(ambiguous());
        }
        Ok(v)
    }
}

/// Strict inline JSON shared with source CLI, before credentials or network access.
pub fn parse_original_posting_rules_json(raw: &str) -> Result<Value, Error> {
    if raw.len() > 65_536 {
        return Err(Error::InvalidInput("original rule JSON exceeds body bound"));
    }
    serde_json::from_str::<UniqueJson>(raw)
        .map(|v| v.0)
        .map_err(|_| Error::InvalidInput("original rule JSON contains invalid or duplicate fields"))
}
