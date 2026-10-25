//! Fixed public-invoice handoff over immutable issued snapshots; never local provider or accounting logic.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_PUBLIC_INVOICE_BODY: usize = 16 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicInvoiceMutation {
    Register,
    RegisterPair,
    Onboard,
    Send,
    RefreshStatus,
}
pub fn public_invoice_body_schema(action: PublicInvoiceMutation) -> Value {
    let mut fields = json!({"confirm":{"const":true}});
    let mut required = vec!["confirm"];
    if matches!(
        action,
        PublicInvoiceMutation::Send | PublicInvoiceMutation::RefreshStatus
    ) {
        fields["invoiceDocumentId"] =
            json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
        required.push("invoiceDocumentId");
    }
    if matches!(
        action,
        PublicInvoiceMutation::RegisterPair | PublicInvoiceMutation::Onboard
    ) {
        fields["network"] = json!({"type":"string","enum":["nemhandel","peppol"]});
    }
    object(fields, &required)
}
impl ApiClient {
    pub async fn public_invoice_config(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "invoices/public-config")?, &[])
            .await
    }
    pub async fn public_invoice_submissions(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "invoices/public-submissions")?, &[])
            .await
    }
    /// Sends exact reviewed body once. HTTP success is not a delivery assertion:
    /// retain canonical state/transmissionId/redeliveryBlocked and read before any further action.
    pub async fn mutate_public_invoice(
        &self,
        company: &str,
        action: PublicInvoiceMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > MAX_PUBLIC_INVOICE_BODY {
            return Err(Error::InvalidInput(
                "explicit public-invoice confirmation and16KiB body bound required",
            ));
        }
        body(&bytes, &public_invoice_body_schema(action))?;
        if matches!(
            action,
            PublicInvoiceMutation::Send | PublicInvoiceMutation::RefreshStatus
        ) {
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|_| Error::InvalidInput("native public-invoice JSON required"))?;
            if value["invoiceDocumentId"]
                .as_i64()
                .is_none_or(|id| !(1..=9_007_199_254_740_991).contains(&id))
            {
                return Err(Error::InvalidInput(
                    "positive safe invoiceDocumentId required",
                ));
            }
        }
        let path = match action {
            PublicInvoiceMutation::RegisterPair => "invoices/public-register-pair",
            PublicInvoiceMutation::Onboard => "invoices/public-onboard",
            PublicInvoiceMutation::Register => "invoices/public-register",
            PublicInvoiceMutation::Send => "invoices/send-public",
            PublicInvoiceMutation::RefreshStatus => "invoices/send-public/status",
        };
        self.request(
            Method::POST,
            &company_path(company, path)?,
            &[],
            Some(bytes),
        )
        .await
        .map_err(|e| match e {
            Error::Refused { status: 408 } => Error::OutcomeUnknown,
            other => other,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pair_and_onboard_strict_native_body_schema() {
        for action in [
            PublicInvoiceMutation::RegisterPair,
            PublicInvoiceMutation::Onboard,
        ] {
            let schema = public_invoice_body_schema(action);
            for value in [
                json!({"confirm":true}),
                json!({"confirm":true,"network":"peppol"}),
                json!({"confirm":true,"network":"nemhandel"}),
            ] {
                body(&serde_json::to_vec(&value).unwrap(), &schema).unwrap();
            }
            for value in [
                json!({"confirm":false}),
                json!({"confirm":true,"network":"other"}),
                json!({"confirm":true,"companyKey":"override"}),
                json!({"confirm":true,"apiLicenseKey":"secret"}),
                json!({"confirm":true,"participantId":"87654321"}),
                json!({"confirm":true,"invoiceDocumentId":1}),
            ] {
                assert!(body(&serde_json::to_vec(&value).unwrap(), &schema).is_err());
            }
        }
        assert!(
            body(
                br#"{"confirm":true,"network":"peppol"}"#,
                &public_invoice_body_schema(PublicInvoiceMutation::Register)
            )
            .is_err()
        );
    }
}

impl ApiClient {
    /// Exact source flat fields. One admitted request, no provider retries.
    pub async fn original717_digisense(&self, name: &str, args: &Value) -> Result<Value, Error> {
        let workspace = name == "efaktura_modtag_workspace";
        let read = name == "efaktura_onboarding_status";
        let path = if workspace {
            self.assert_original_workspace89(
                args["workspace"]
                    .as_str()
                    .ok_or(Error::InvalidInput("workspace required"))?,
            )?;
            "/api/workspace/original-efaktura-modtag".to_owned()
        } else {
            let company = self.diagnostics_company_original118(
                args["company"]
                    .as_str()
                    .ok_or(Error::InvalidInput("company required"))?,
            )?;
            company_path(
                &company,
                match name {
                    "efaktura_onboarding_status" => "original-efaktura-onboarding-status",
                    "efaktura_onboard" => "original-efaktura-onboard",
                    "efaktura_registrer" => "original-efaktura-registrer",
                    "efaktura_modtag" => "original-efaktura-modtag",
                    _ => return Err(Error::InvalidInput("unknown DigiSense source operation")),
                },
            )?
        };
        let mut input = args.clone();
        let object = input
            .as_object_mut()
            .ok_or(Error::InvalidInput("object required"))?;
        object.remove("company");
        object.remove("workspace");
        let bytes = if read {
            None
        } else {
            if object.get("confirm").is_some_and(|v| !v.is_boolean()) {
                return Err(Error::InvalidInput("boolean confirm required"));
            }
            if object.get("confirm") != Some(&json!(true)) {
                return Ok(
                    json!({"ok":false,"errors":["confirm: true is required"],"code":"CONFIRM_REQUIRED"}),
                );
            }
            let bytes =
                serde_json::to_vec(&input).map_err(|_| Error::InvalidInput("JSON required"))?;
            if bytes.len() > 16384 {
                return Err(Error::InvalidInput("DigiSense source body exceeds16KiB"));
            }
            Some(bytes)
        };
        let response = self
            .request(
                if read { Method::GET } else { Method::POST },
                &path,
                &[],
                bytes,
            )
            .await?;
        if response["ok"] != true {
            return Err(Error::Protocol);
        }
        let core = response
            .get("result")
            .filter(|v| v.is_object())
            .ok_or(Error::Protocol)?
            .clone();
        if core["ok"] == false {
            if !core["errors"]
                .as_array()
                .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
            {
                return Err(Error::Protocol);
            }
            return Ok(core);
        }
        let valid = match name {
            "efaktura_onboarding_status" => source_status(&core),
            "efaktura_onboard" => core["companyKey"].is_string() && source_status(&core["status"]),
            "efaktura_registrer" => {
                core["companyKey"].is_string()
                    && core["directionsRegistered"].is_array()
                    && core["participantType"] == "DK:CVR"
                    && core["participantId"].is_string()
            }
            "efaktura_modtag" => {
                [
                    "pagesFetched",
                    "documentsListed",
                    "documentsIngested",
                    "documentsSkipped",
                    "documentsQuarantined",
                ]
                .iter()
                .all(|k| core[*k].as_u64().is_some())
                    && core["documents"].is_array()
                    && core["errors"].is_array()
            }
            "efaktura_modtag_workspace" => {
                core["ok"] == true
                    && core["companies"].as_array().is_some_and(|a| {
                        a.iter().all(|v| {
                            v["slug"].is_string()
                                && matches!(
                                    v["status"].as_str(),
                                    Some("polled" | "skipped" | "failed")
                                )
                        })
                    })
            }
            _ => false,
        };
        if !valid {
            return Err(Error::Protocol);
        }
        Ok(core)
    }
}
fn source_status(v: &Value) -> bool {
    [
        "configured",
        "companyKeyConfigured",
        "inboundReady",
        "outboundReady",
        "ready",
    ]
    .iter()
    .all(|k| v[*k].is_boolean())
        && (v["environment"].is_null()
            || matches!(v["environment"].as_str(), Some("test" | "production")))
        && (v["identity"].is_null()
            || v["identity"]["cvr"].is_string() && v["identity"]["companyName"].is_string())
        && v["blockers"]
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
}

#[cfg(test)]
mod original717_provider_tests {
    use super::*;
    #[test]
    fn original717_digisense_status_schema_is_source_not_native_lookalike() {
        let source = json!({"configured":true,"environment":"test","identity":{"cvr":"12345678","companyName":"Synthetic"},"companyKeyConfigured":true,"inboundReady":true,"outboundReady":true,"ready":true,"blockers":[]});
        assert!(source_status(&source));
        assert!(!source_status(
            &json!({"ok":true,"publicEInvoice":{"configured":true,"technicalReady":true,"networks":{}}})
        ));
        let mut bad = source;
        bad["blockers"] = json!([42]);
        assert!(!source_status(&bad));
    }
}

impl ApiClient {
    /// Two original CLI-only operations; not additions to the frozen MCP catalogue.
    pub async fn original716_test_registration(
        &self,
        name: &str,
        args: &Value,
    ) -> Result<Value, Error> {
        if !matches!(name, "registrer-test-gln" | "registrer-test-afsender")
            || !args.as_object().is_some_and(|o| {
                o.keys().all(|k| {
                    matches!(k.as_str(), "company" | "confirm")
                        || name == "registrer-test-gln" && k == "network"
                })
            })
            || args["confirm"] != true
            || args
                .get("network")
                .is_some_and(|v| v != "nemhandel" && v != "peppol")
        {
            return Err(Error::InvalidInput("invalid original716 test registration"));
        }
        let company = self.diagnostics_company_original118(
            args["company"]
                .as_str()
                .ok_or(Error::InvalidInput("company required"))?,
        )?;
        let mut body = args.clone();
        body.as_object_mut()
            .ok_or(Error::Protocol)?
            .remove("company");
        let value = self
            .request(
                Method::POST,
                &company_path(&company, &format!("original-efaktura-{name}"))?,
                &[],
                Some(serde_json::to_vec(&body).map_err(|_| Error::Protocol)?),
            )
            .await?;
        let r = &value["result"];
        if value["ok"] != true {
            return Err(Error::Protocol);
        }
        if r["ok"] == true && r["registered"] == true {
            // Deliberately project only the public source receipt.
            Ok(json!({"ok":true,"registered":true}))
        } else if r["ok"] == false
            && r["errors"]
                .as_array()
                .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
        {
            Ok(json!({"ok":false,"errors":r["errors"]}))
        } else {
            Err(Error::Protocol)
        }
    }
}
