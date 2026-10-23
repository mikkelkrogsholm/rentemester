//! Exact mileage posting transport. No rates, accounting decisions or automatic retries.
use crate::{
    ApiClient, Error, MAX_RESPONSE, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
pub const MAX_MILEAGE_POSTING_BODY: usize = 32 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MileagePostingOperation {
    Configure,
    Preview,
    Post,
}
pub fn mileage_posting_body_schema(op: MileagePostingOperation) -> Value {
    let text = |max| json!({"type":"string","minLength":1,"maxLength":max});
    if op == MileagePostingOperation::Configure {
        return object(
            json!({"version":text(128),"validFrom":text(10),"validTo":text(10),"vehicle":text(4096),"ratePerKm":text(4096),"basis":text(4096),"source":text(4096),"expenseAccountNo":text(128),"liabilityAccountNo":text(128),"confirm":{"const":true}}),
            &[
                "version",
                "validFrom",
                "validTo",
                "vehicle",
                "ratePerKm",
                "basis",
                "source",
                "expenseAccountNo",
                "liabilityAccountNo",
                "confirm",
            ],
        );
    }
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    let mut props = json!({"mileageEntryId":id,"rateVersion":text(128),"documentId":id});
    let mut required = vec!["mileageEntryId", "rateVersion", "documentId"];
    if op == MileagePostingOperation::Post {
        props["expectedPreviewHash"] = text(64);
        props["operationKey"] = text(128);
        props["confirm"] = json!({"const":true});
        required.extend(["expectedPreviewHash", "operationKey", "confirm"]);
    }
    object(props, &required)
}
// Value normally collapses duplicate keys. Reject them before validating or sending exact bytes.
struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Strict;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("unique JSON object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Strict, M::Error> {
                let mut fields = serde_json::Map::new();
                while let Some((k, v)) = map.next_entry::<String, Value>()? {
                    if fields.insert(k, v).is_some() {
                        return Err(serde::de::Error::custom("duplicate field"));
                    }
                }
                Ok(Strict(Value::Object(fields)))
            }
        }
        d.deserialize_map(Visitor)
    }
}
pub fn validate_mileage_posting_body(
    bytes: &[u8],
    op: MileagePostingOperation,
) -> Result<(), Error> {
    let invalid = || Error::InvalidInput("invalid strict mileage posting body");
    if bytes.len() > MAX_MILEAGE_POSTING_BODY {
        return Err(invalid());
    }
    let Strict(v) = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    body(bytes, &mileage_posting_body_schema(op))?;
    for (k, v) in v.as_object().ok_or_else(invalid)? {
        if let Some(s) = v.as_str() {
            if s.trim().is_empty() || s.chars().any(char::is_control) {
                return Err(invalid());
            }
            if matches!(k.as_str(), "validFrom" | "validTo") {
                let fmt = time::format_description::well_known::Iso8601::DATE;
                if s.len() != 10 || time::Date::parse(s, &fmt).is_err() {
                    return Err(invalid());
                }
            }
        }
    }
    if op == MileagePostingOperation::Configure {
        let s = v["ratePerKm"].as_str().ok_or_else(invalid)?;
        let parts: Vec<_> = s.split('.').collect();
        if parts.len() > 2
            || parts
                .iter()
                .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
            || !s.bytes().any(|b| matches!(b, b'1'..=b'9'))
            || v["validFrom"].as_str() > v["validTo"].as_str()
        {
            return Err(invalid());
        }
    }
    if op == MileagePostingOperation::Post
        && !v["expectedPreviewHash"].as_str().is_some_and(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'))
        })
    {
        return Err(invalid());
    }
    Ok(())
}
impl ApiClient {
    pub async fn mileage_rate_versions(&self, company: &str) -> Result<Value, Error> {
        self.mileage_posting_request(company, "rate-versions", None, false)
            .await
    }
    pub async fn mileage_postings(&self, company: &str) -> Result<Value, Error> {
        self.mileage_posting_request(company, "postings", None, false)
            .await
    }
    pub async fn configure_mileage_rates(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput("confirmation required"));
        }
        validate_mileage_posting_body(&bytes, MileagePostingOperation::Configure)?;
        self.mileage_posting_request(company, "rate-versions", Some(bytes), true)
            .await
    }
    pub async fn preview_mileage_post(
        &self,
        company: &str,
        bytes: Vec<u8>,
    ) -> Result<Value, Error> {
        validate_mileage_posting_body(&bytes, MileagePostingOperation::Preview)?;
        self.mileage_posting_request(company, "post-preview", Some(bytes), false)
            .await
    }
    pub async fn post_mileage(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput("confirmation required"));
        }
        validate_mileage_posting_body(&bytes, MileagePostingOperation::Post)?;
        self.mileage_posting_request(company, "post", Some(bytes), true)
            .await
    }
    async fn mileage_posting_request(
        &self,
        company: &str,
        suffix: &str,
        bytes: Option<Vec<u8>>,
        write: bool,
    ) -> Result<Value, Error> {
        let fail = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        };
        let mut r = self
            .send(
                if bytes.is_some() {
                    Method::POST
                } else {
                    Method::GET
                },
                &company_path(company, &format!("mileage/{suffix}"))?,
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
        let status = r.status().as_u16();
        if status >= 500 || status == 408 {
            return Err(fail());
        }
        let mut bytes = vec![];
        while let Some(c) = r.chunk().await.map_err(|_| fail())? {
            if bytes.len().saturating_add(c.len()) > MAX_RESPONSE {
                return Err(fail());
            }
            bytes.extend_from_slice(&c);
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status == 200 {
                fail()
            } else {
                Error::Refused { status }
            }
        })?;
        if status == 200 && v["ok"] == true && v.get("mileagePosting").is_some()
            || (400..500).contains(&status) && v["ok"] == false && v["code"].is_string()
        {
            Ok(v)
        } else if status == 200 {
            Err(if write {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            })
        } else {
            Err(Error::Refused { status })
        }
    }
}
