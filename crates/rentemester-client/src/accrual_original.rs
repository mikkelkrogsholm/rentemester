//! Original accrual transport: exact source DTOs, server defaults, and never a write retry.
use crate::{
    ApiClient, Error, MAX_RESPONSE, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
const MAX_ID: i64 = 9_007_199_254_740_991;
const RULE: &str = "DK-BOOKKEEPING-ACCRUAL-001";
fn id() -> Value {
    json!({"type":"integer","minimum":1,"maximum":MAX_ID})
}
fn count(max: u32) -> Value {
    json!({"type":"integer","minimum":1,"maximum":max})
}
fn text(max: usize) -> Value {
    json!({"type":"string","minLength":1,"maxLength":max})
}
fn date_schema() -> Value {
    json!({"type":"string","minLength":10,"maxLength":10})
}
fn kind() -> Value {
    json!({"type":"string","enum":["prepaid_expense","accrued_expense","deferred_revenue"]})
}
fn money_schema() -> Value {
    json!({"type":"number","minimum":0,"maximum":90071992547409.91})
}
fn strings() -> Value {
    json!({"type":"array","maxItems":128,"items":text(512)})
}
/// Transport authority fields are deliberately not part of this schema.
pub fn original_accrual_schema(register: bool) -> Value {
    if register {
        object(
            json!({"accrualType":kind(),"description":text(500),"totalAmount":{"type":"number","exclusiveMinimum":0,"maximum":90071992547409.91},"recognitionPeriods":count(12000),"firstRecognitionDate":date_schema(),"resultAccountNo":text(128),"registrationDate":date_schema(),"periodStepMonths":count(1200),"balanceAccountNo":text(128),"settlementAccountNo":text(128),"documentId":id(),"note":{"type":"string","maxLength":4096}}),
            &[
                "accrualType",
                "description",
                "totalAmount",
                "recognitionPeriods",
                "firstRecognitionDate",
                "resultAccountNo",
            ],
        )
    } else {
        object(
            json!({"period":count(12000),"date":date_schema(),"settlementAccountNo":text(128)}),
            &["period"],
        )
    }
}
fn shape(v: &Value, s: &Value) -> bool {
    serde_json::to_vec(v).is_ok_and(|b| body(&b, s).is_ok())
}
fn date(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 10
            && s.bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            })
            && s[..4]
                .parse::<i32>()
                .ok()
                .filter(|y| *y > 0)
                .zip(s[5..7].parse::<u8>().ok())
                .zip(s[8..].parse::<u8>().ok())
                .is_some_and(|((y, m), d)| {
                    time::Month::try_from(m)
                        .ok()
                        .is_some_and(|m| time::Date::from_calendar_date(y, m, d).is_ok())
                })
    })
}
fn cents(v: &Value) -> Option<i64> {
    let n = v.as_f64()?;
    let c = (n * 100.0).round();
    (n.is_finite() && n >= 0.0 && c <= MAX_ID as f64 && (n * 100.0 - c).abs() < 0.000001)
        .then_some(c as i64)
}
pub fn validate_original_accrual(input: &Value, register: bool) -> Result<(), Error> {
    let b = serde_json::to_vec(input).map_err(|_| Error::InvalidInput("invalid accrual JSON"))?;
    if b.len() > 16384 {
        return Err(Error::InvalidInput("accrual input exceeds16KiB"));
    }
    body(&b, &original_accrual_schema(register))?;
    for k in ["firstRecognitionDate", "registrationDate", "date"] {
        if input.get(k).is_some_and(|v| !date(v)) {
            return Err(Error::InvalidInput("valid calendar date required"));
        }
    }
    for k in [
        "description",
        "resultAccountNo",
        "balanceAccountNo",
        "settlementAccountNo",
    ] {
        if input.get(k).is_some_and(|v| {
            v.as_str()
                .is_none_or(|s| s.trim().is_empty() || s.chars().any(char::is_control))
        }) {
            return Err(Error::InvalidInput("nonblank accrual text required"));
        }
    }
    if register && cents(&input["totalAmount"]).is_none_or(|n| n <= 0) {
        return Err(Error::InvalidInput(
            "positive exact two-decimal DKK amount required",
        ));
    }
    Ok(())
}
pub fn validate_original_accrual_actor(actor: Option<&str>) -> Result<(), Error> {
    if actor.is_some_and(|s| {
        s.trim().is_empty()
            || s.trim() != s
            || s.encode_utf16().count() > 160
            || s.chars().any(char::is_control)
    }) {
        return Err(Error::InvalidInput("invalid expected actor assertion"));
    }
    Ok(())
}
fn rules(v: &Value) -> bool {
    v.as_array().is_some_and(|a| {
        !a.is_empty()
            && a.len() <= 128
            && a.iter().all(|v| {
                v.as_str().is_some_and(|s| {
                    s.starts_with("DK-")
                        && s.len() <= 128
                        && s.bytes()
                            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
                })
            })
    })
}
fn receipt(v: &Value, register: bool, expected: &Value, accrual_id: Option<i64>) -> bool {
    let mut p = json!({"ok":{"const":true},"entryId":id(),"entryNo":text(128),"entryHash":text(64),"appliedRules":strings(),"errors":{"type":"array","maxItems":0,"items":text(512)},"accrualId":id(),"periodAmount":money_schema(),"totalPeriods":count(12000)});
    let mut required = vec![
        "ok",
        "entryId",
        "entryNo",
        "entryHash",
        "appliedRules",
        "errors",
        "accrualId",
        "periodAmount",
        "totalPeriods",
    ];
    if register {
        p["accrualType"] = kind();
        required.push("accrualType");
    } else {
        p["periodIndex"] = count(12000);
        p["recognizedPeriods"] = count(12000);
        p["fullyRecognized"] = json!({"type":"boolean"});
        required.extend(["periodIndex", "recognizedPeriods", "fullyRecognized"]);
    }
    shape(v, &object(p, &required))
        && cents(&v["periodAmount"]).is_some_and(|n| register || n > 0)
        && rules(&v["appliedRules"])
        && v["appliedRules"].as_array().unwrap().contains(&json!(RULE))
        && v["entryHash"]
            .as_str()
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        && if register {
            v["accrualType"] == expected["accrualType"]
                && v["totalPeriods"] == expected["recognitionPeriods"]
        } else {
            v["accrualId"].as_i64() == accrual_id
                && v["periodIndex"] == expected["period"]
                && v["periodIndex"].as_u64() <= v["totalPeriods"].as_u64()
                && v["recognizedPeriods"].as_u64() <= v["totalPeriods"].as_u64()
                && v["fullyRecognized"] == json!(v["recognizedPeriods"] == v["totalPeriods"])
        }
}
fn report(v: &Value) -> bool {
    let m = money_schema();
    let row = object(
        json!({"accrualId":id(),"accrualType":kind(),"description":text(500),"totalAmount":m,"recognitionPeriods":count(12000),"recognizedPeriods":{"type":"integer","minimum":0,"maximum":12000},"recognizedAmount":m,"remainingAmount":m,"fullyRecognized":{"type":"boolean"},"balanceAccountNo":text(128),"resultAccountNo":text(128),"firstRecognitionDate":date_schema(),"periodStepMonths":count(1200)}),
        &[
            "accrualId",
            "accrualType",
            "description",
            "totalAmount",
            "recognitionPeriods",
            "recognizedPeriods",
            "recognizedAmount",
            "remainingAmount",
            "fullyRecognized",
            "balanceAccountNo",
            "resultAccountNo",
            "firstRecognitionDate",
            "periodStepMonths",
        ],
    );
    let totals = object(
        json!({"totalAmount":m,"recognizedAmount":m,"remainingAmount":m}),
        &["totalAmount", "recognizedAmount", "remainingAmount"],
    );
    if !shape(
        v,
        &object(
            json!({"ok":{"const":true},"accruals":{"type":"array","items":row},"totals":totals,"errors":{"type":"array","maxItems":0,"items":text(512)}}),
            &["ok", "accruals", "totals", "errors"],
        ),
    ) {
        return false;
    }
    let rows = v["accruals"].as_array().unwrap();
    if !rows
        .windows(2)
        .all(|p| p[0]["accrualId"].as_i64() < p[1]["accrualId"].as_i64())
    {
        return false;
    }
    let mut sums = [0i64; 3];
    for r in rows {
        if !date(&r["firstRecognitionDate"])
            || r["recognizedPeriods"].as_u64() > r["recognitionPeriods"].as_u64()
            || r["fullyRecognized"] != json!(r["recognizedPeriods"] == r["recognitionPeriods"])
        {
            return false;
        }
        let mut amounts = [0i64; 3];
        for (i, k) in ["totalAmount", "recognizedAmount", "remainingAmount"]
            .iter()
            .enumerate()
        {
            let Some(c) = cents(&r[*k]) else {
                return false;
            };
            amounts[i] = c;
            let Some(sum) = sums[i].checked_add(c) else {
                return false;
            };
            sums[i] = sum;
        }
        if amounts[0] <= 0 || amounts[1].checked_add(amounts[2]) != Some(amounts[0]) {
            return false;
        }
    }
    ["totalAmount", "recognizedAmount", "remainingAmount"]
        .iter()
        .enumerate()
        .all(|(i, k)| cents(&v["totals"][*k]) == Some(sums[i]))
}
/// Domain errors are deliberately projected, never echoed as arbitrary server text.
fn refusal(v: &Value) -> Option<Value> {
    if !shape(
        v,
        &object(
            json!({"ok":{"const":false},"appliedRules":strings(),"errors":strings(),"code":{"type":"string","enum":["invalid_input","not_found","conflict","precondition_failed","BACKUP_LOCKED","ORIGINAL_UNAVAILABLE","ORIGINAL_HASH_MISMATCH"]}}),
            &["ok", "errors"],
        ),
    ) || v["errors"].as_array()?.is_empty()
        || v.get("appliedRules").is_some_and(|r| !rules(r))
    {
        return None;
    }
    let errors = v["errors"].as_array().unwrap().iter().map(|error| {
        let text = error.as_str().unwrap_or("").to_ascii_lowercase();
        if text.contains("already") || text.contains("duplicate") { "Accrual period has already been recognized" }
        else if text.contains("not found") || text.contains("does not exist") { "Accrual or required accounting source not found" }
        else if text.contains("closed") || text.contains("reported period") { "Posting date falls in a closed or reported period" }
        else if text.contains("account") { "Select distinct active directly postable accounts of the required type and normal balance" }
        else if text.contains("document") || text.contains("evidence") || text.contains("original") { "Required current original document evidence is unavailable or invalid" }
        else if text.contains("period") || text.contains("schedule") { "Selected accrual period or schedule is invalid" }
        else if text.contains("positive") || text.contains("amount") { "Journal posting requires a positive valid amount" }
        else { "Accrual refused by current bookkeeping or source-evidence controls" }
    }).collect::<Vec<_>>();
    let mut safe = json!({"ok":false,"errors":errors});
    if let Some(r) = v.get("appliedRules") {
        safe["appliedRules"] = r.clone();
    }
    if let Some(c) = v.get("code") {
        safe["code"] = c.clone();
    }
    Some(safe)
}
impl ApiClient {
    pub async fn original_accrual_register(
        &self,
        company: &str,
        input: &Value,
        confirmed: bool,
        expected_actor: Option<&str>,
    ) -> Result<Value, Error> {
        self.original_accrual_write(
            company,
            "register",
            input,
            true,
            None,
            confirmed,
            expected_actor,
        )
        .await
    }
    pub async fn original_accrual_recognize(
        &self,
        company: &str,
        accrual_id: i64,
        input: &Value,
        confirmed: bool,
        expected_actor: Option<&str>,
    ) -> Result<Value, Error> {
        if !(1..=MAX_ID).contains(&accrual_id) {
            return Err(Error::InvalidInput("positive safe accrual id required"));
        }
        self.original_accrual_write(
            company,
            &format!("{accrual_id}/recognize"),
            input,
            false,
            Some(accrual_id),
            confirmed,
            expected_actor,
        )
        .await
    }
    pub async fn original_accrual_register_report(&self, company: &str) -> Result<Value, Error> {
        self.original_accrual_request(company, "register-report", None, None, None)
            .await
    }
    #[allow(clippy::too_many_arguments)]
    async fn original_accrual_write(
        &self,
        company: &str,
        suffix: &str,
        input: &Value,
        register: bool,
        id: Option<i64>,
        confirmed: bool,
        actor: Option<&str>,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput("confirmation required"));
        }
        validate_original_accrual_actor(actor)?;
        validate_original_accrual(input, register)?;
        let mut payload = input.clone();
        payload["confirm"] = json!(true);
        if let Some(a) = actor {
            payload["expectedActor"] = json!(a);
        }
        self.original_accrual_request(company, suffix, Some(payload), Some((register, input)), id)
            .await
    }
    async fn original_accrual_request(
        &self,
        company: &str,
        suffix: &str,
        payload: Option<Value>,
        expected: Option<(bool, &Value)>,
        id: Option<i64>,
    ) -> Result<Value, Error> {
        let write = payload.is_some();
        let unknown = || {
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
        let bytes = payload
            .map(|p| serde_json::to_vec(&p).map_err(|_| Error::Protocol))
            .transpose()?;
        let mut response = self
            .send(
                if write { Method::POST } else { Method::GET },
                &company_path(company, &format!("accrual/original/{suffix}"))?,
                &[],
                bytes,
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
            return Err(transport());
        }
        if status != 200 && status != 422 {
            return Err(Error::Refused { status });
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| transport())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status == 422 {
                Error::Refused { status }
            } else {
                unknown()
            }
        })?;
        if let Some(safe) = refusal(&v) {
            return Ok(safe);
        }
        if status == 422 {
            return Err(Error::Refused { status });
        }
        if expected.map_or_else(|| report(&v), |(reg, input)| receipt(&v, reg, input, id)) {
            Ok(v)
        } else {
            Err(unknown())
        }
    }
}
