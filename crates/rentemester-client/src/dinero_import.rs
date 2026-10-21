//! Latest-year Dinero transport. All parsing, review hashes and attestation stay server-side.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path, domain::object};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_DINERO_IMPORT_BODY: usize = 35 * 1024 * 1024;
pub const MAX_DINERO_ORIGINAL: usize = 25 * 1024 * 1024;
pub fn dinero_import_body_schema(apply: bool) -> Value {
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
    pub async fn dinero_import(
        &self,
        company: &str,
        bytes: Vec<u8>,
        apply: bool,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if confirmed != apply || bytes.len() > MAX_DINERO_IMPORT_BODY {
            return Err(Error::InvalidInput(
                "plan is read-only; apply requires explicit confirmation and exact native35MiB JSON",
            ));
        }
        source_shape(&bytes, apply)?;
        let path = company_path(
            company,
            if apply {
                "imports/dinero/apply"
            } else {
                "imports/dinero/plan"
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
    pub async fn dinero_import_plan(&self, company: &str, bytes: Vec<u8>) -> Result<Value, Error> {
        self.dinero_import(company, bytes, false, false).await
    }
    pub async fn dinero_import_apply(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.dinero_import(company, bytes, true, confirmed).await
    }
    pub async fn dinero_imports(&self, company: &str) -> Result<Value, Error> {
        self.dinero_read(&company_path(company, "imports/dinero")?, &[], false)
            .await
    }
    pub async fn imported_receivables(&self, company: &str, as_of: &str) -> Result<Value, Error> {
        if !valid_date(as_of) {
            return Err(Error::InvalidInput("explicit valid as-of date required"));
        }
        self.dinero_read(
            &company_path(company, "invoices/imported-receivables")?,
            &[("asOf", as_of)],
            false,
        )
        .await
    }
    pub async fn dinero_import_original(
        &self,
        company: &str,
        attempt_id: i64,
    ) -> Result<Value, Error> {
        if !(1..=9_007_199_254_740_991).contains(&attempt_id) {
            return Err(Error::InvalidInput("positive safe attempt ID required"));
        }
        self.dinero_read(
            &company_path(company, &format!("imports/dinero/{attempt_id}/original"))?,
            &[],
            true,
        )
        .await
    }
    async fn dinero_read(
        &self,
        path: &str,
        query: &[(&str, &str)],
        original: bool,
    ) -> Result<Value, Error> {
        let mut response = self
            .send(
                Method::GET,
                path,
                query,
                None,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        let status = response.status();
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            let bound = if original {
                MAX_DINERO_ORIGINAL
            } else {
                MAX_RESPONSE
            };
            if bytes.len().saturating_add(chunk.len()) > bound {
                return Err(Error::Protocol);
            }
            bytes.extend_from_slice(&chunk);
        }
        if status.is_success() && original {
            if response
                .headers()
                .get("content-type")
                .and_then(|h| h.to_str().ok())
                != Some("application/json")
            {
                return Err(Error::Protocol);
            }
            return Ok(
                json!({"ok":true,"original":{"contentType":"application/json","bytes":bytes}}),
            );
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        if value["ok"] == false && value["code"].as_str().is_some_and(|s| !s.is_empty()) {
            return Ok(value);
        }
        if !status.is_success() {
            return Err(Error::Refused {
                status: status.as_u16(),
            });
        }
        if value["ok"] != true {
            return Err(Error::Protocol);
        }
        Ok(value)
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
    let invalid = || Error::InvalidInput("exact documented native dinero source body required");
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

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let parsed = (|| {
        let y = value[..4].parse::<i32>().ok()?;
        if y < 1 {
            return None;
        }
        let m = time::Month::try_from(value[5..7].parse::<u8>().ok()?).ok()?;
        time::Date::from_calendar_date(y, m, value[8..].parse::<u8>().ok()?).ok()
    })();
    parsed.is_some()
}
