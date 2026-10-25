//! Original diagnostics over the admitted server; no local ledger reads or retries.
use crate::{ApiClient, Error, company_path};
use serde_json::Value;

fn strings(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|a| a.iter().all(Value::is_string))
}
fn nullable_string(value: &Value) -> bool {
    value.is_null() || value.is_string()
}
fn valid_core(core: &Value, retention: bool) -> bool {
    let Some(ok) = core["ok"].as_bool() else {
        return false;
    };
    if !strings(&core["errors"])
        || (ok && core["errors"].as_array().unwrap().len() != 0)
        || (!ok && core["errors"].as_array().unwrap().is_empty())
    {
        return false;
    }
    if retention {
        core["asOf"].is_string()
            && core["appliedRules"] == serde_json::json!(["DK-BOOKKEEPING-RETENTION-001"])
            && core["rows"].as_array().is_some_and(|rows| {
                (!ok && rows.is_empty() || ok && rows.len() == 3)
                && rows.iter().zip(["documents", "journal_entries", "bank_transactions"]).all(|(r, table)| {
                    r["table"] == table
                        && r["total"].as_u64().is_some()
                        && r["expired"].as_u64().is_some()
                        && r.get("nextExpiry").is_some_and(nullable_string)
                        && r.get("oldestExpired").is_some_and(nullable_string)
                })
            })
    } else {
        strings(&core["missing"])
            && core["schema_outdated"].is_boolean()
            && core["checks"].as_array().is_some_and(|checks| {
                checks.len() == 5
                    && checks
                        .iter()
                        .zip(["company_root", "data_dir", "ledger", "documents", "config"])
                        .all(|(c, name)| c["name"] == name && c["ok"].is_boolean() && (!ok || c["ok"] == true))
            })
            && core.get("schema").is_none_or(|s| {
                s.is_object()
                    && matches!(s["status"].as_str(), Some("current" | "pending" | "newer" | "corrupt" | "unavailable"))
                    && s["currentVersion"].as_i64().is_some()
                    && s["requiredVersion"].as_u64().is_some()
                    && s["pending"].is_array()
            })
            && (!ok || core["missing"].as_array().unwrap().is_empty()
                && core["schema_outdated"] == false && core["schema"]["status"] == "current")
    }
}
impl ApiClient {
    // Path selectors are assertions against a freshly verified explicit local root,
    // not filesystem discovery, target-ledger admission or caller authority.
    pub(crate) fn diagnostics_company_original118(&self, company: &str) -> Result<String, Error> {
        let selected = if std::path::Path::new(company).is_absolute() {
            let local = self.local.as_ref().ok_or(Error::InvalidInput(
                "absolute company requires explicit verified local profile",
            ))?;
            let root = local
                .workspace_root()
                .to_str()
                .ok_or(Error::InvalidInput("UTF-8 workspace root required"))?;
            self.assert_original_workspace89(root)?;
            let slug = company
                .strip_prefix(root)
                .and_then(|s| s.strip_prefix('/'))
                .ok_or(Error::InvalidInput(
                    "company differs from verified workspace root",
                ))?;
            // company_path below rejects separators, dot segments and empty slugs.
            slug
        } else {
            company
        };
        company_path(selected, "retention-status")?;
        Ok(selected.to_owned())
    }
    pub async fn retention_status_original118(
        &self,
        company: &str,
        as_of: Option<&str>,
    ) -> Result<Value, Error> {
        let company = self.diagnostics_company_original118(company)?;
        let query = as_of.map(|date| vec![("asOf", date)]).unwrap_or_default();
        self.diagnostic_core_original118(
            &company,
            "retention-status",
            "retentionStatus",
            &query,
            true,
        )
        .await
    }
    pub async fn system_healthcheck_original118(&self, company: &str) -> Result<Value, Error> {
        let company = self.diagnostics_company_original118(company)?;
        self.diagnostic_core_original118(&company, "system-healthcheck", "healthcheck", &[], false)
            .await
    }
    async fn diagnostic_core_original118(
        &self,
        company: &str,
        route: &str,
        field: &str,
        query: &[(&str, &str)],
        retention: bool,
    ) -> Result<Value, Error> {
        let value = self.get(&company_path(company, route)?, query).await?;
        if value["ok"] != true || !value[field].is_object() || !valid_core(&value[field], retention)
        {
            return Err(Error::Protocol);
        }
        Ok(value[field].clone())
    }
}
