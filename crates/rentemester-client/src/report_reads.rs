//! Source selectors are forwarded; all source admission and financial figures stay server-owned.
use crate::{ApiClient, Error, Report, company_path};
use reqwest::Method;
use serde_json::{Value, json};

fn query<'a>(
    year: Option<&'a str>,
    account: Option<&'a str>,
    source: Option<&'a str>,
) -> Result<Vec<(&'static str, &'a str)>, Error> {
    if year.is_some_and(|s| s.len() > 128)
        || account.is_some_and(|s| s.len() > 128)
        || source.is_some_and(|s| !matches!(s, "live" | "archive"))
    {
        return Err(Error::InvalidInput(
            "bounded year/account and source live|archive required",
        ));
    }
    Ok([("year", year), ("account", account), ("source", source)]
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect())
}

impl ApiClient {
    pub async fn report_source(
        &self,
        company: &str,
        report: Report,
        year: Option<&str>,
        account: Option<&str>,
        source: Option<&str>,
    ) -> Result<Value, Error> {
        let mut response = self
            .send(
                Method::GET,
                &company_path(company, report.path())?,
                &query(year, account, source)?,
                None,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        if !response.status().is_success() {
            return Err(Error::Refused {
                status: response.status().as_u16(),
            });
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if bytes.len().saturating_add(chunk.len()) > 96 * 1024 * 1024 {
                return Err(Error::Protocol);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)
    }

    /// Exact downloaded bytes, bounded to 96 MiB. This read performs no local export or calculation.
    pub async fn report_export(
        &self,
        company: &str,
        report: Report,
        year: Option<&str>,
        account: Option<&str>,
        source: Option<&str>,
        format: &str,
    ) -> Result<Value, Error> {
        if matches!(report, Report::Overview) || !matches!(format, "csv" | "pdf") {
            return Err(Error::InvalidInput(
                "financial/journal report and csv|pdf required",
            ));
        }
        let mut query = query(year, account, source)?;
        query.push(("format", format));
        let mut response = self
            .send(
                Method::GET,
                &company_path(company, &format!("{}/export", report.path()))?,
                &query,
                None,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        let status = response.status();
        let mime = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if bytes.len().saturating_add(chunk.len()) > 96 * 1024 * 1024 {
                return Err(Error::Protocol);
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes)
                && value["ok"] == false
                && value["code"].as_str().is_some_and(|s| !s.is_empty())
            {
                return Ok(value);
            }
            return Err(Error::Refused {
                status: status.as_u16(),
            });
        }
        if mime
            != if format == "pdf" {
                "application/pdf"
            } else {
                "text/csv; charset=utf-8"
            }
        {
            return Err(Error::Protocol);
        }
        Ok(json!({"ok":true,"contentType":mime,"bytes":bytes}))
    }
}
