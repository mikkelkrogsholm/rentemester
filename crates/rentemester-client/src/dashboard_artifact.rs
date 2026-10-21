//! Exact server-owned static HTML. No local files or financial calculations.
use crate::{ApiClient, Error, company_path};
use reqwest::Method;
use serde_json::{Value, json};

pub const MAX_DASHBOARD_BYTES: usize = 8 * 1024 * 1024;
pub const DASHBOARD_CONTENT_TYPE: &str = "text/html; charset=utf-8";

fn valid_date(s: &str) -> bool {
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
    let Some(date) = (|| {
        time::Date::from_calendar_date(
            s[..4].parse().ok()?,
            time::Month::try_from(s[5..7].parse::<u8>().ok()?).ok()?,
            s[8..].parse().ok()?,
        )
        .ok()
    })() else {
        return false;
    };
    date.year() >= 0
}
impl ApiClient {
    pub async fn dashboard_export(
        &self,
        company: &str,
        as_of: Option<&str>,
    ) -> Result<Value, Error> {
        if as_of.is_some_and(|s| !valid_date(s)) {
            return Err(Error::InvalidInput("asOf must be YYYY-MM-DD"));
        }
        let mut query = vec![("format", "html")];
        if let Some(date) = as_of {
            query.push(("asOf", date));
        }
        let mut response = self
            .send(
                Method::GET,
                &company_path(company, "dashboard/export")?,
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
        let headers = response.headers().clone();
        let disposition = response
            .headers()
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        if response
            .content_length()
            .is_some_and(|n| n > MAX_DASHBOARD_BYTES as u64)
        {
            return Err(Error::Protocol);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_DASHBOARD_BYTES {
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
        let filename = disposition
            .strip_prefix("attachment; filename=\"")
            .and_then(|s| s.strip_suffix('"'))
            .ok_or(Error::Protocol)?;
        if mime != DASHBOARD_CONTENT_TYPE
            || filename.len() > 255
            || filename.is_empty()
            || !filename.ends_with(".html")
            || filename.starts_with('.')
            || !filename
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            || std::str::from_utf8(&bytes).is_err()
        {
            return Err(Error::Protocol);
        }
        let header = |key| {
            headers
                .get(key)
                .and_then(|v| v.to_str().ok())
                .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
                .ok_or(Error::Protocol)
        };
        let as_of_date = header("x-rentemester-as-of")?;
        if !valid_date(as_of_date) || as_of.is_some_and(|s| s != as_of_date) {
            return Err(Error::Protocol);
        }
        let generated_at = header("x-rentemester-generated-at")?;
        if time::OffsetDateTime::parse(generated_at, &time::format_description::well_known::Rfc3339)
            .is_err()
        {
            return Err(Error::Protocol);
        }
        let render_ms: f64 = header("x-rentemester-render-ms")?
            .parse()
            .map_err(|_| Error::Protocol)?;
        if !render_ms.is_finite() || render_ms < 0.0 {
            return Err(Error::Protocol);
        }
        Ok(
            json!({"ok":true,"contentType":mime,"filename":filename,"bytes":bytes,"asOfDate":as_of_date,"generatedAt":generated_at,"commitSha":header("x-rentemester-commit-sha")?,"ruleBundleVersion":header("x-rentemester-rule-bundle-version")?,"renderMs":render_ms}),
        )
    }
}
