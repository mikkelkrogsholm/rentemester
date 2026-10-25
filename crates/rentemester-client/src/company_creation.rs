//! Workspace-scoped fresh company creation. All defaults and authorization belong to the server.
use crate::{
    ApiClient, Error, MAX_RESPONSE,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_COMPANY_CREATION_BODY: usize = 16 * 1024;
pub fn company_creation_body_schema() -> Value {
    let text = json!({"type":["string","null"]});
    let mut payment = object(
        json!({"bankName":text,"registrationNo":text,"accountNo":text,"iban":text}),
        &[],
    );
    payment["type"] = json!(["object", "null"]);
    object(
        json!({
            "name":{"type":"string","minLength":1,"maxLength":1024},
            "slug":text,"cvr":text,
            "address":{"type":["string","null"],"maxLength":1024},
            "postalCode":{"type":["string","null"],"maxLength":1024},
            "city":{"type":["string","null"],"maxLength":1024},
            "paymentTermsDays":{"type":["integer","null"],"minimum":0,"maximum":365},
            "fiscalYearStartMonth":{"type":["string","null"],"description":"Native string month 1 through 12; omitted uses server default"},
            "fiscalYearLabelStrategy":{"type":["string","null"],"description":"Native end-year/start-year/span; server validates"},
            "vatPeriodType":{"type":["string","null"],"description":"Native month/quarter/half-year or explicit null for no VAT registration; omitted uses server default"},
            "payment":payment
        }),
        &["name"],
    )
}
impl ApiClient {
    /// Send exact reviewed bytes once; no company selector, copied chart, actor, key or retry.
    /// On an unknown outcome inspect companies/current profile before further explicit action.
    pub async fn create_company(&self, bytes: Vec<u8>, confirmed: bool) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit new-company confirmation required",
            ));
        }
        if bytes.len() > MAX_COMPANY_CREATION_BODY {
            return Err(Error::InvalidInput("company creation exceeds native16KiB"));
        }
        body(&bytes, &company_creation_body_schema())?;
        let mut response = self
            .send(
                Method::POST,
                "/api/companies",
                &[],
                Some(bytes),
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        let status = response.status();
        if status.is_server_error()
            || status.as_u16() == 408
            || status.is_success() && status.as_u16() != 201
        {
            return Err(Error::OutcomeUnknown);
        }
        if !status.is_success() {
            return Err(Error::Refused {
                status: status.as_u16(),
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::OutcomeUnknown)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(Error::OutcomeUnknown);
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::OutcomeUnknown)?;
        if value["ok"] != true
            || value["company"]["slug"].as_str().is_none_or(str::is_empty)
            || value["company"]["name"].as_str().is_none_or(str::is_empty)
        {
            return Err(Error::OutcomeUnknown);
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepts(value: Value) -> bool {
        body(
            &serde_json::to_vec(&value).unwrap(),
            &company_creation_body_schema(),
        )
        .is_ok()
    }

    #[test]
    fn creation_schema_extends_identity_without_changing_old_inputs() {
        for value in [
            json!({"name":"Synthetic"}),
            json!({"name":"Synthetic","address":null,"postalCode":null,"city":null,"paymentTermsDays":null}),
            json!({"name":"Synthetic","address":" Road 1 ","postalCode":"1234","city":"Town","paymentTermsDays":0}),
            json!({"name":"Synthetic","address":"","postalCode":" ","city":"Town","paymentTermsDays":365,"payment":{"bankName":"Synthetic bank"}}),
        ] {
            assert!(accepts(value));
        }
    }

    #[test]
    fn creation_schema_refuses_wrong_types_ranges_and_unknown_authority() {
        for (field, value) in [
            ("paymentTermsDays", json!(-1)),
            ("paymentTermsDays", json!(366)),
            ("paymentTermsDays", json!(1.5)),
            ("paymentTermsDays", json!("0")),
            ("paymentTermsDays", json!(false)),
            ("address", json!(7)),
            ("postalCode", json!([])),
            ("city", json!({})),
            ("city", json!("x".repeat(1025))),
            ("actor", json!("user:synthetic")),
            ("companyId", json!(1)),
        ] {
            let mut input = json!({"name":"Synthetic"});
            input[field] = value;
            assert!(!accepts(input), "accepted invalid {field}");
        }
    }
}

impl ApiClient {
    /// Original flat company_add transport. Workspace is only a verified binding
    /// assertion; current server credentials remain the sole authority.
    pub async fn create_company_original116(&self, args: Value) -> Result<Value, Error> {
        let mut o = args.as_object().cloned().ok_or(Error::InvalidInput("expected company_add object"))?;
        if o.keys().any(|k| !matches!(k.as_str(), "name"|"slug"|"cvr"|"workspace"|"confirm"|"fiscalYearStartMonth"|"fiscalYearLabelStrategy"))
            || !o.get("name").is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()))
            || ["workspace","slug","cvr"].iter().any(|k| o.get(*k).is_some_and(|v| !v.is_string()))
            || o.get("confirm").is_some_and(|v| !v.is_boolean())
            || o.get("fiscalYearStartMonth").is_some_and(|v| v.as_f64().is_none_or(|n| n.fract() != 0.0 || !(1.0..=12.0).contains(&n)))
            || o.get("fiscalYearLabelStrategy").is_some_and(|v| !matches!(v.as_str(),Some("end-year"|"start-year"|"span"))) {
            return Err(Error::InvalidInput("invalid original company_add grammar"));
        }
        if o.remove("confirm") != Some(json!(true)) {
            return Err(Error::InvalidInput("explicit new-company confirmation required"));
        }
        if let Some(workspace) = o.remove("workspace") {
            let requested = workspace.as_str().unwrap().trim();
            if !requested.is_empty() {
                if !std::path::Path::new(requested).is_absolute() {
                    return Err(Error::InvalidInput("company_add workspace must be an absolute verified local root"));
                }
                // A filesystem assertion must match the actual root, never a logical ID.
                self.assert_original_workspace89(requested)?;
                let local = self.local.as_ref().ok_or(Error::InvalidInput("company_add workspace requires an explicit verified local profile"))?;
                if local.workspace_root().to_str() != Some(requested) {
                    return Err(Error::InvalidInput("workspace differs from explicit verified local runtime root"));
                }
            }
        }
        if let Some(slug) = o.get_mut("slug") {
            *slug = json!(slug.as_str().unwrap().trim());
        }
        if let Some(month) = o.get_mut("fiscalYearStartMonth") {
            *month = json!((month.as_f64().unwrap() as u8).to_string());
        }
        let bytes = serde_json::to_vec(&o).map_err(|_| Error::InvalidInput("invalid company_add body"))?;
        if bytes.len() > MAX_COMPANY_CREATION_BODY {
            return Err(Error::InvalidInput("company creation exceeds native16KiB"));
        }
        body(&bytes, &company_creation_body_schema())?;
        let mut response = self.send(Method::POST, "/api/companies", &[], Some(bytes), self.cookie.as_ref().map(|c| c.as_str())).await?;
        let status = response.status();
        if status.is_server_error() || status.as_u16() == 408 || status.is_success() && status.as_u16() != 201 {
            return Err(Error::OutcomeUnknown);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| if status.is_success() { Error::OutcomeUnknown } else { Error::Protocol })? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(if status.is_success() { Error::OutcomeUnknown } else { Error::Protocol });
            }
            bytes.extend_from_slice(&chunk);
        }
        let value = serde_json::from_slice::<Value>(&bytes);
        if !status.is_success() {
            // Preserve original business provenance only from a valid envelope;
            // never turn a native error object into a fabricated success.
            if let Ok(v) = value {
                if v["ok"] == false
                    && v["errors"].as_array().is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
                    && v.as_object().is_some_and(|o| o.keys().all(|k| matches!(k.as_str(),"ok"|"errors"|"code"|"appliedRules")))
                    && v.get("code").is_none_or(Value::is_string)
                    && v.get("appliedRules").is_none_or(|v| v.as_array().is_some_and(|a| a.iter().all(Value::is_string))) {
                    return Ok(v);
                }
            }
            return Err(Error::Refused { status: status.as_u16() });
        }
        let v = value.map_err(|_| Error::OutcomeUnknown)?;
        if v["ok"] != true
            || v["company"]["slug"].as_str().is_none_or(str::is_empty)
            || v["company"]["name"].as_str().is_none_or(str::is_empty) {
            return Err(Error::OutcomeUnknown);
        }
        Ok(v)
    }
}
