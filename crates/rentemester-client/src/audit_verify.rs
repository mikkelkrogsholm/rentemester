//! Full audit findings are successful observations even when accounting fails.
use crate::{ApiClient, Error, company_path};
use serde_json::Value;
fn checked(value: Value, company: &str) -> Result<Value, Error> {
    let fail = || Error::Protocol;
    let audit = &value["audit"];
    if value["ok"] != true
        || audit["format"] != "rentemester-native-audit-verification/1"
        || audit["slug"] != company
        || audit["entries"]
            .as_u64()
            .is_none_or(|entries| entries > 9_007_199_254_740_991)
    {
        return Err(fail());
    }
    let components = audit["components"].as_array().ok_or_else(fail)?;
    let mut names = std::collections::BTreeSet::new();
    let mut available = true;
    let mut ok = true;
    let mut count = 0;
    for component in components {
        let name = component["name"].as_str().ok_or_else(fail)?;
        if !["ledger", "bank", "invoices", "originals", "physical_schema"].contains(&name)
            || !names.insert(name)
        {
            return Err(fail());
        }
        let current_available = component["available"].as_bool().ok_or_else(fail)?;
        let current_ok = component["ok"].as_bool().ok_or_else(fail)?;
        let errors = component["errors"].as_array().ok_or_else(fail)?;
        let counts = component["checked"].as_object().ok_or_else(fail)?;
        if counts.values().any(|count| count.as_u64().is_none())
            || current_ok != (current_available && errors.is_empty())
            || (!current_available && errors.is_empty())
        {
            return Err(fail());
        }
        available &= current_available;
        ok &= current_ok;
        count += errors.len();
    }
    let errors = audit["errors"].as_array().ok_or_else(fail)?;
    if names.len() != 5
        || audit["available"] != available
        || audit["ok"] != (available && ok)
        || errors.len() != count
        || errors.iter().any(|error| !error.is_string())
    {
        return Err(fail());
    }
    Ok(value)
}
impl ApiClient {
    pub async fn audit_verify_original717(&self, company: &str) -> Result<Value, Error> {
        let company = self.diagnostics_company_original118(company)?;
        let value = self.audit_verify(&company).await?;
        let audit = &value["audit"];
        Ok(
            serde_json::json!({"ok":audit["ok"],"errors":audit["errors"],"data":{"entries":audit["entries"]}}),
        )
    }
    pub async fn audit_verify(&self, company: &str) -> Result<Value, Error> {
        checked(
            self.get(&company_path(company, "audit-verify")?, &[])
                .await?,
            company,
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn report() -> Value {
        json!({"ok":true,"audit":{"format":"rentemester-native-audit-verification/1","slug":"synthetic","entries":0,"ok":true,"available":true,"errors":[],"components":(["ledger","bank","invoices","originals","physical_schema"].map(|name|json!({"name":name,"ok":true,"available":true,"checked":{},"errors":[]})))}})
    }
    #[test]
    fn accepts_failed_observation_but_refuses_incomplete_or_falsely_green_reports() {
        let clean = report();
        assert_eq!(checked(clean.clone(), "synthetic").unwrap(), clean);
        let mut failed = clean.clone();
        failed["audit"]["ok"] = json!(false);
        failed["audit"]["errors"] = json!(["originals: synthetic missing original"]);
        failed["audit"]["components"][3]["ok"] = json!(false);
        failed["audit"]["components"][3]["errors"] =
            json!([{"error":"synthetic missing original"}]);
        assert!(checked(failed.clone(), "synthetic").is_ok());
        failed["audit"]["ok"] = json!(true);
        assert_eq!(checked(failed, "synthetic"), Err(Error::Protocol));
        let mut missing = clean;
        missing["audit"]["components"].as_array_mut().unwrap().pop();
        assert_eq!(checked(missing, "synthetic"), Err(Error::Protocol));
    }
}
