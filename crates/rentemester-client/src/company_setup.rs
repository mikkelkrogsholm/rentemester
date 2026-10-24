//! Current company profile/chart configuration only; server owns all compatibility and cadence rules.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_COMPANY_SETUP_BODY: usize = 16 * 1024;
#[derive(Clone, Copy, Debug)]
pub enum CompanySetupMutation {
    UpdateProfile,
    CreateAccount,
    ConfirmAccountRole,
}
pub fn company_setup_body_schema(op: CompanySetupMutation) -> Value {
    let text = json!({"type":"string"});
    match op {
        CompanySetupMutation::UpdateProfile => object(
            json!({"name":{"type":"string","minLength":1,"maxLength":1024,"description":"Optional canonical profile label, trimmed by server; nonblank, at most 1024 UTF-8 bytes, no controls. Does not rename company slug/path."},"cvr":text,"address":text,"postalCode":text,"city":text,"vatPeriodType":{"type":["string","null"],"description":"Explicit native cadence month/quarter/half-year or null; existing VAT obligations can refuse changes"},"paymentTermsDays":{"type":"integer","minimum":0,"maximum":365},"payment":object(json!({"bankName":text,"registrationNo":text,"accountNo":text,"iban":text}),&[])}),
            &[],
        ),
        CompanySetupMutation::CreateAccount => object(
            json!({"accountNo":text,"name":text,"type":{"type":"string","enum":["asset","liability","equity","income","expense","vat"]},"normalBalance":{"type":"string","enum":["debit","credit"]},"defaultVatCode":{"type":["string","null"],"description":"Explicit caller choice; native server validates known VAT codes, no inferred default"}}),
            &["accountNo", "name", "type", "normalBalance"],
        ),
        CompanySetupMutation::ConfirmAccountRole => object(
            json!({"role":{"type":"string","enum":["bank","debtors","creditors","output_vat","input_vat","reverse_charge_vat","vat_settlement","operational_default"]},"accountNo":text,"confirm":{"const":true}}),
            &["role", "accountNo", "confirm"],
        ),
    }
}
impl ApiClient {
    /// Exact bytes once. No caller keys, inferred account mappings or hidden journal writes.
    /// On unknown outcomes read company_profile/accounts before any further explicit action.
    pub async fn company_setup_mutation(
        &self,
        company: &str,
        op: CompanySetupMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit current-company setup confirmation required",
            ));
        }
        if bytes.len() > MAX_COMPANY_SETUP_BODY {
            return Err(Error::InvalidInput(
                "company setup body exceeds native16KiB",
            ));
        }
        body(&bytes, &company_setup_body_schema(op))?;
        let (method, path) = match op {
            CompanySetupMutation::UpdateProfile => (Method::PATCH, "company"),
            CompanySetupMutation::CreateAccount => (Method::POST, "accounts"),
            CompanySetupMutation::ConfirmAccountRole => (Method::POST, "account-roles"),
        };
        self.request(method, &company_path(company, path)?, &[], Some(bytes))
            .await
    }
}

#[cfg(test)]
mod profile_name_schema_tests {
    use super::*;

    fn accepts(value: Value) -> bool {
        body(
            &serde_json::to_vec(&value).unwrap(),
            &company_setup_body_schema(CompanySetupMutation::UpdateProfile),
        )
        .is_ok()
    }

    #[test]
    fn optional_name_extends_profile_without_changing_old_inputs() {
        for input in [
            json!({}),
            json!({"address":"Synthetic Street"}),
            json!({"vatPeriodType":null,"paymentTermsDays":0}),
            json!({"payment":{"bankName":"Synthetic bank"}}),
            json!({"name":"  Synthetic Æ Company  ","city":"Synthetic City"}),
            json!({"name":"ø".repeat(512)}),
        ] {
            assert!(accepts(input));
        }
        assert_eq!(MAX_COMPANY_SETUP_BODY, 16 * 1024);
        let schema = company_setup_body_schema(CompanySetupMutation::UpdateProfile);
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["required"], json!([]));
        // Shape validation only: server remains authority for blank/control refusal and trimming.
        assert!(
            schema["properties"]["name"]["description"]
                .as_str()
                .unwrap()
                .contains("trimmed by server")
        );
    }

    #[test]
    fn name_type_bounds_and_unknown_authority_remain_closed() {
        for input in [
            json!({"name":null}),
            json!({"name":4}),
            json!({"name":""}),
            json!({"name":"x".repeat(1025)}),
            json!({"name":"ø".repeat(513)}),
            json!({"name":"Synthetic","actor":"user:invented"}),
            json!({"name":"Synthetic","companyId":2}),
            json!({"name":"Synthetic","slug":"renamed"}),
            json!({"payment":{"unknown":"value"}}),
            json!({"paymentTermsDays":366}),
        ] {
            assert!(!accepts(input));
        }
    }
}
