//! Closed-history archive transport. All parsing, review hashes and attestation stay server-side.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path, domain::object};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_ARCHIVE_IMPORT_BODY: usize = 35 * 1024 * 1024;
pub const MAX_ARCHIVE_ORIGINAL: usize = 25 * 1024 * 1024;
pub fn archive_import_body_schema(apply: bool) -> Value {
    let mut schema = object(
        json!({"fileName":{"type":"string","minLength":1},"contentBase64":{"type":"string","minLength":1}}),
        &["fileName", "contentBase64"],
    );
    if apply {
        for (k, v) in [
            (
                "planHash",
                json!({"type":"string","minLength":64,"maxLength":64,"pattern":"^[0-9a-f]{64}$"}),
            ),
            (
                "idempotencyKey",
                json!({"type":"string","minLength":1,"description":"Exact caller key; preserve reviewed source bytes/key after an unknown outcome. No automatic retry."}),
            ),
            ("confirm", json!({"const":true})),
        ] {
            schema["properties"][k] = v;
            schema["required"].as_array_mut().unwrap().push(json!(k));
        }
    }
    schema
}
impl ApiClient {
    pub async fn archive_import(
        &self,
        company: &str,
        bytes: Vec<u8>,
        apply: bool,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if confirmed != apply || bytes.len() > MAX_ARCHIVE_IMPORT_BODY {
            return Err(Error::InvalidInput(
                "plan is read-only; apply requires explicit confirmation and exact native35MiB JSON",
            ));
        }
        source_shape(&bytes, apply)?;
        let path = company_path(
            company,
            if apply {
                "archive-import/apply"
            } else {
                "archive-import/plan"
            },
        )?;
        let unknown = || {
            if apply {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        };
        let mut response = self
            .send(
                Method::POST,
                &path,
                &[],
                Some(bytes),
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await
            .map_err(|e| {
                if !apply && e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let status = response.status();
        if apply && (status.is_server_error() || status.as_u16() == 408) {
            return Err(Error::OutcomeUnknown);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unknown())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status.is_success() {
                unknown()
            } else {
                Error::Refused {
                    status: status.as_u16(),
                }
            }
        })?;
        if value["ok"] == false && value["code"].as_str().is_some_and(|s| !s.is_empty()) {
            if apply
                && matches!(
                    value["code"].as_str(),
                    Some("internal" | "outcome_unknown" | "outcome_unconfirmed")
                )
            {
                return Err(Error::OutcomeUnknown);
            }
            return Ok(value);
        }
        if status.is_success() && value["ok"] == true {
            Ok(value)
        } else if status.is_success() {
            Err(unknown())
        } else {
            Err(Error::Refused {
                status: status.as_u16(),
            })
        }
    }
}

impl ApiClient {
    pub async fn archives(&self, company: &str, fiscal_year: Option<u16>) -> Result<Value, Error> {
        if fiscal_year.is_some_and(|y| !(1000..=9999).contains(&y)) {
            return Err(Error::InvalidInput("four-digit fiscal year required"));
        }
        let suffix = fiscal_year
            .map(|y| format!("archives/{y}"))
            .unwrap_or_else(|| "archives".into());
        self.archive_read(&company_path(company, &suffix)?, false)
            .await
    }
    pub async fn archive_import_original(
        &self,
        company: &str,
        import_id: i64,
    ) -> Result<Value, Error> {
        if !(1..=9_007_199_254_740_991).contains(&import_id) {
            return Err(Error::InvalidInput("positive safe import ID required"));
        }
        self.archive_read(
            &company_path(company, &format!("archive-import/{import_id}/original"))?,
            true,
        )
        .await
    }
    async fn archive_read(&self, path: &str, original: bool) -> Result<Value, Error> {
        let mut response = self
            .send(
                Method::GET,
                path,
                &[],
                None,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        let status = response.status();
        let mime = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let bound = if original {
            MAX_ARCHIVE_ORIGINAL
        } else {
            96 * 1024 * 1024
        };
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if bytes.len().saturating_add(chunk.len()) > bound {
                return Err(Error::Protocol);
            }
            bytes.extend_from_slice(&chunk);
        }
        if status.is_success() && original {
            if mime != "application/json" {
                return Err(Error::Protocol);
            }
            return Ok(json!({"ok":true,"contentType":mime,"bytes":bytes}));
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status.is_success() {
                Error::Protocol
            } else {
                Error::Refused {
                    status: status.as_u16(),
                }
            }
        })?;
        if status.is_success() && value["ok"] == true
            || value["ok"] == false && value["code"].as_str().is_some_and(|s| !s.is_empty())
        {
            Ok(value)
        } else if status.is_success() {
            Err(Error::Protocol)
        } else {
            Err(Error::Refused {
                status: status.as_u16(),
            })
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlanSource {
    file_name: String,
    content_base64: String,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApplySource {
    file_name: String,
    content_base64: String,
    plan_hash: String,
    idempotency_key: String,
    confirm: bool,
}
fn source_shape(bytes: &[u8], apply: bool) -> Result<(), Error> {
    let invalid = || Error::InvalidInput("exact documented native archive source body required");
    let valid = if apply {
        let v: ApplySource = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        !v.file_name.is_empty()
            && !v.content_base64.is_empty()
            && v.confirm
            && !v.idempotency_key.is_empty()
            && v.plan_hash.len() == 64
            && v.plan_hash
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    } else {
        let v: PlanSource = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        !v.file_name.is_empty() && !v.content_base64.is_empty()
    };
    if valid { Ok(()) } else { Err(invalid()) }
}
