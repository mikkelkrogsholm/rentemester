//! Frozen capitalized derecognition wire. No storage dependency, money derivation or retry.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
fn id() -> Value {
    json!({"type":"integer","minimum":1,"maximum":9007199254740991u64})
}
fn text(max: usize) -> Value {
    json!({"type":"string","minLength":1,"maxLength":max})
}
fn hash() -> Value {
    json!({"type":"string","pattern":"^[0-9a-f]{64}$"})
}
fn date() -> Value {
    json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"})
}
fn valid_hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn valid_date(v: &Value) -> bool {
    let Some(s) = v.as_str() else {
        return false;
    };
    if s.len() != 10
        || !s.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
    {
        return false;
    }
    s[..4]
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
}
fn money() -> Value {
    json!({"type":"number","minimum":0,"maximum":90071992547409.9})
}
pub fn asset_disposal_schema(apply: bool) -> Value {
    let mut p = json!({"transactionDate":date(),"documentId":id(),"reason":text(4096),"lossAccountNo":text(128)});
    let mut required = vec!["transactionDate", "documentId", "reason"];
    if apply {
        p["expectedPlanHash"] = hash();
        p["idempotencyKey"] = json!({"type":"string","minLength":8,"maxLength":160});
        p["confirm"] = json!({"const":true});
        required.extend(["expectedPlanHash", "idempotencyKey", "confirm"]);
    }
    object(p, &required)
}
pub fn validate_asset_disposal(input: &Value, apply: bool) -> Result<(), Error> {
    let bytes =
        serde_json::to_vec(input).map_err(|_| Error::InvalidInput("invalid disposal body"))?;
    if bytes.len() > 16384 {
        return Err(Error::InvalidInput("disposal exceeds16KiB"));
    }
    body(&bytes, &asset_disposal_schema(apply))?;
    for k in ["reason", "lossAccountNo", "idempotencyKey"] {
        if input
            .get(k)
            .is_some_and(|v| v.as_str().is_none_or(|s| s.trim().is_empty()))
        {
            return Err(Error::InvalidInput("blank disposal field"));
        }
    }
    if !valid_date(&input["transactionDate"]) || (apply && !valid_hash(&input["expectedPlanHash"]))
    {
        return Err(Error::InvalidInput("canonical date/hash required"));
    }
    Ok(())
}
fn output_schema(preview: bool) -> Value {
    let mut p = json!({"assetId":id(),"transactionDate":date(),"documentId":id(),"reason":text(4096),"cost":money(),"accumulatedDepreciation":money()});
    let mut required = vec![
        "assetId",
        "transactionDate",
        "documentId",
        "reason",
        "cost",
        "accumulatedDepreciation",
    ];
    if preview {
        p["lossAccountNo"] = json!({"type":["string","null"],"minLength":1,"maxLength":128});
        p["carryingValue"] = money();
        p["planHash"] = hash();
        p["lines"] = json!({"type":"array","minItems":2,"maxItems":3,"items":object(json!({"accountNo":text(128),"debit":money(),"credit":money()}), &["accountNo","debit","credit"])});
        required.extend(["lossAccountNo", "carryingValue", "planHash", "lines"]);
    } else {
        p["disposalId"] = id();
        p["carryingValueWrittenOff"] = money();
        p["remainingCarryingValue"] = json!({"const":0});
        p["journalEntryId"] = id();
        p["journalEntryNo"] = text(128);
        p["journalEntryHash"] = hash();
        p["replayed"] = json!({"type":"boolean"});
        required.extend([
            "disposalId",
            "carryingValueWrittenOff",
            "remainingCarryingValue",
            "journalEntryId",
            "journalEntryNo",
            "journalEntryHash",
            "replayed",
        ]);
    }
    let key = if preview {
        "disposalPreview"
    } else {
        "disposal"
    };
    let mut fields = json!({"ok":{"const":true}});
    fields[key] = object(p, &required);
    object(fields, &["ok", key])
}
fn validate_output(
    v: Value,
    preview: bool,
    asset_id: i64,
    input: Option<&Value>,
    write: bool,
) -> Result<Value, Error> {
    let bad = || {
        if write {
            Error::OutcomeUnknown
        } else {
            Error::Protocol
        }
    };
    let bytes = serde_json::to_vec(&v).map_err(|_| bad())?;
    body(&bytes, &output_schema(preview)).map_err(|_| bad())?;
    let p = &v[if preview {
        "disposalPreview"
    } else {
        "disposal"
    }];
    if !valid_date(&p["transactionDate"])
        || !valid_hash(
            &p[if preview {
                "planHash"
            } else {
                "journalEntryHash"
            }],
        )
        || p["reason"].as_str().is_none_or(|s| s.trim().is_empty())
    {
        return Err(bad());
    }
    if p["assetId"] != asset_id {
        return Err(bad());
    }
    if let Some(i) = input {
        for k in ["transactionDate", "documentId", "reason"] {
            if p[k] != i[k] {
                return Err(bad());
            }
        }
        if preview && p["lossAccountNo"] != i.get("lossAccountNo").cloned().unwrap_or(Value::Null) {
            return Err(bad());
        }
    }
    Ok(v)
}
impl ApiClient {
    pub async fn asset_disposal_preview(
        &self,
        company: &str,
        id: i64,
        input: &Value,
    ) -> Result<Value, Error> {
        self.disposal_request(company, id, Some(input), false, None)
            .await
    }
    pub async fn asset_dispose(
        &self,
        company: &str,
        id: i64,
        input: &Value,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit disposal confirmation required",
            ));
        }
        self.disposal_request(company, id, Some(input), true, None)
            .await
    }
    pub async fn asset_disposal_receipt(
        &self,
        company: &str,
        id: i64,
        key: &str,
    ) -> Result<Value, Error> {
        if !(8..=160).contains(&key.len()) || key.trim().is_empty() {
            return Err(Error::InvalidInput("bounded receipt key required"));
        }
        self.disposal_request(company, id, None, false, Some(key))
            .await
    }
    async fn disposal_request(
        &self,
        company: &str,
        id: i64,
        input: Option<&Value>,
        apply: bool,
        key: Option<&str>,
    ) -> Result<Value, Error> {
        if !(1..=9007199254740991).contains(&id) {
            return Err(Error::InvalidInput("positive safe assetId required"));
        }
        if let Some(v) = input {
            validate_asset_disposal(v, apply)?;
        }
        let suffix = if key.is_some() {
            "disposal-receipt"
        } else if apply {
            "dispose"
        } else {
            "disposal/preview"
        };
        let path = company_path(company, &format!("assets/{id}/{suffix}"))?;
        let v = if let Some(key) = key {
            self.get(&path, &[("key", key)]).await?
        } else {
            // Preview is POST but inert; transport uncertainty is never a write outcome.
            self.request(
                Method::POST,
                &path,
                &[],
                Some(serde_json::to_vec(input.unwrap()).unwrap()),
            )
            .await
            .map_err(|e| {
                if !apply && matches!(e, Error::OutcomeUnknown) {
                    Error::Transport
                } else {
                    e
                }
            })?
        };
        validate_output(v, key.is_none() && !apply, id, input, apply)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asset_disposal_strict_body() {
        let v = json!({"transactionDate":"2026-02-28","documentId":3,"reason":"Retired"});
        assert!(validate_asset_disposal(&v, false).is_ok());
        let mut bad = v.clone();
        bad["amount"] = json!(2);
        assert!(validate_asset_disposal(&bad, false).is_err());
        bad = v;
        bad["transactionDate"] = json!("2026-02-30");
        assert!(validate_asset_disposal(&bad, false).is_err());
    }
    #[test]
    fn asset_disposal_malformed_success_unknown() {
        assert!(matches!(
            validate_output(json!({"ok":true}), false, 1, None, true),
            Err(Error::OutcomeUnknown)
        ));
    }
}
