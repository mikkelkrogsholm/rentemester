//! Latest-year Dinero transport. All parsing, review hashes and attestation stay server-side.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path, domain::object};
use base64::{Engine as _, engine::general_purpose::STANDARD};
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
        self.imported_receivables_optional(company, Some(as_of))
            .await
    }
    /// Omitted date is the server's UTC today, with no query parameter sent.
    pub async fn imported_receivables_optional(
        &self,
        company: &str,
        as_of: Option<&str>,
    ) -> Result<Value, Error> {
        if as_of.is_some_and(|date| !valid_date(date)) {
            return Err(Error::InvalidInput("valid as-of date required"));
        }
        let query = as_of.map(|date| vec![("asOf", date)]).unwrap_or_default();
        self.dinero_read(
            &company_path(company, "invoices/imported-receivables")?,
            &query,
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
            let mime = response
                .headers()
                .get("content-type")
                .and_then(|h| h.to_str().ok())
                .ok_or(Error::Protocol)?;
            if !matches!(mime, "application/json" | "application/zip") {
                return Err(Error::Protocol);
            }
            return Ok(json!({"ok":true,"original":{"contentType":mime,"bytes":bytes}}));
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

impl ApiClient {
    /// Build only the source transport: native parsing and accounting remain on
    /// the server. Explicit reviewed hash/key are preserved without planning.
    pub fn dinero_import_source_body(
        file_name: &str,
        source: &[u8],
        review: Option<(&str, &str)>,
    ) -> Result<Vec<u8>, Error> {
        if source.is_empty()
            || source.len() > MAX_DINERO_ORIGINAL
            || file_name.is_empty()
            || file_name.len() > 255
            || file_name.contains(['/', '\\'])
            || file_name.chars().any(char::is_control)
            || !(file_name.to_ascii_lowercase().ends_with(".zip")
                || file_name.to_ascii_lowercase().ends_with(".json"))
        {
            return Err(Error::InvalidInput(
                "safe JSON/ZIP filename and original at most25MiB required",
            ));
        }
        let mut body = json!({"fileName":file_name,"contentBase64":STANDARD.encode(source)});
        if let Some((plan_hash, key)) = review {
            body["planHash"] = json!(plan_hash);
            body["idempotencyKey"] = json!(key);
            body["confirm"] = json!(true);
        }
        let bytes = serde_json::to_vec(&body).map_err(|_| Error::Protocol)?;
        source_shape(&bytes, review.is_some())?;
        if bytes.len() > MAX_DINERO_IMPORT_BODY {
            return Err(Error::InvalidInput("native35MiB source body required"));
        }
        Ok(bytes)
    }
}
#[cfg(test)]
mod container_tests {
    use super::*;
    #[test]
    fn exact_source_wrapper_keeps_zip_bytes_and_explicit_review_authority() {
        let raw = b"PK\x03\x04exact synthetic raw ZIP";
        let body = ApiClient::dinero_import_source_body("source.zip", raw, None).unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            STANDARD
                .decode(value["contentBase64"].as_str().unwrap())
                .unwrap(),
            raw
        );
        assert!(value.get("confirm").is_none());
        let hash = "a".repeat(64);
        let body = ApiClient::dinero_import_source_body(
            "source.zip",
            raw,
            Some((&hash, "caller reviewed key")),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["planHash"], hash);
        assert_eq!(value["idempotencyKey"], "caller reviewed key");
        assert_eq!(value["confirm"], true);
        for name in [
            "/private/source.zip",
            "../source.json",
            "C:\\source.zip",
            "source.pdf",
        ] {
            assert!(ApiClient::dinero_import_source_body(name, raw, None).is_err());
        }
        assert!(
            ApiClient::dinero_import_source_body("source.zip", raw, Some((&hash, ""))).is_err()
        );
    }
    #[tokio::test]
    async fn original_transport_preserves_truthful_zip_or_json_and_refuses_other_mimes() {
        use std::{
            io::{Read, Write},

            thread,
        };
        for mime in [
            "application/json",
            "application/zip",
            "application/octet-stream",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let task = thread::spawn(move || {
                let (mut connection, _) = listener.accept().unwrap();
                connection
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut request = vec![];
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    assert_eq!(connection.read(&mut byte).unwrap(), 1);
                    request.push(byte[0]);
                }
                let bytes = b"PK\x03\x04exact-original";
                write!(connection,"HTTP/1.1 200 OK\r\nContent-Type:{mime}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n",bytes.len()).unwrap();
                connection.write_all(bytes).unwrap();
                request
            });
            let api = ApiClient::new(&origin, true, None).unwrap();
            let result = api.dinero_import_original("synthetic", 17).await;
            if mime == "application/octet-stream" {
                assert_eq!(result.unwrap_err(), Error::Protocol);
            } else {
                let value = result.unwrap();
                assert_eq!(value["original"]["contentType"], mime);
                assert_eq!(
                    value["original"]["bytes"],
                    json!(b"PK\x03\x04exact-original")
                );
            }
            assert!(
                task.join().unwrap().starts_with(
                    b"GET /api/companies/synthetic/imports/dinero/17/original HTTP/1.1"
                )
            );
        }
    }
}

#[cfg(test)]
use crate::test_transport::TcpListener;
