//! Explicit CVR provider/cache effects. Credentials and company mapping stay server-owned.
use crate::{ApiClient, Error, company_path};
use reqwest::Method;
use serde_json::Value;
fn inner(value: Value, key: &str, field: &str, uncertain: bool) -> Result<Value, Error> {
    if value["ok"] != true || !value[key].is_object() || !value[key][field].is_boolean() {
        return Err(if uncertain {
            Error::OutcomeUnknown
        } else {
            Error::Protocol
        });
    }
    Ok(value[key].clone())
}
fn lookup_error(error: Error) -> Error {
    match error {
        Error::Transport | Error::Protocol => Error::OutcomeUnknown,
        Error::Refused { status } if status >= 500 => Error::OutcomeUnknown,
        other => other,
    }
}
impl ApiClient {
    /// Credential presence only; no provider request or cache changes.
    pub async fn cvr_status(&self) -> Result<Value, Error> {
        inner(
            self.get("/api/system/cvr-status", &[]).await?,
            "cvrStatus",
            "configured",
            false,
        )
    }
    /// One explicitly confirmed provider/cache operation; no local CVR normalization.
    /// Return the exact inner result, including business failure and retained fetchedAt.
    pub async fn cvr_lookup(
        &self,
        company: &str,
        cvr: &str,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit CVR lookup confirmation required",
            ));
        }
        if cvr.len() > 128 {
            return Err(Error::InvalidInput("CVR lookup text exceeds128 bytes"));
        }
        let value = self
            .get(&company_path(company, "cvr-lookup")?, &[("cvr", cvr)])
            .await
            .map_err(lookup_error)?;
        inner(value, "cvr", "ok", true)
    }
    /// Sync the company's current own CVR once; an uncertain outcome requires
    /// canonical profile/cache readback before another explicit action.
    pub async fn sync_company_cvr(&self, company: &str, confirmed: bool) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit company CVR sync confirmation required",
            ));
        }
        let value = self
            .request(
                Method::POST,
                &company_path(company, "sync-cvr")?,
                &[],
                Some(b"{}".to_vec()),
            )
            .await?;
        inner(value, "sync", "ok", true)
    }
}
