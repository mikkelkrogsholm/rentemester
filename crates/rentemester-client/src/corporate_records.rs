//! Exact corporate source transport. Scope, sensitivity, hash proof and history are server-owned.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_CORPORATE_INGEST_BODY: usize = 24 * 1024 * 1024;
pub const MAX_CORPORATE_BODY: usize = 32 * 1024;
pub const MAX_CORPORATE_ORIGINAL_JSON: usize = 24 * 1024 * 1024;
#[derive(Clone)]
pub enum CorporateRecordMutation {
    Ingest,
    Link(String),
    Enrich(String),
    Supersede(String),
}
pub fn corporate_record_body_schema(action: &CorporateRecordMutation) -> Value {
    let text = json!({"type":"string","minLength":1});
    let record_id = json!({"type":["string","null"],"maxLength":64});
    let link = object(
        json!({"type":{"type":"string","enum":["company","party","ownership","disposition","legacy_evidence_ref"]},"id":{"type":"string","minLength":1,"maxLength":256}}),
        &["type", "id"],
    );
    match action {
        CorporateRecordMutation::Ingest => object(
            json!({"recordId":record_id,"type":{"type":"string","enum":["incorporation","articles","registration","shareholder_register","shareholder_agreement","board_resolution","general_meeting_minutes","capital_or_dividend_resolution","annual_report","auditor_deliverable","intercompany_agreement","ownership_register","share_transfer","authority_letter","bank_mandate","tax_registration","other"]},"bytesBase64":{"type":"string","minLength":1,"maxLength":22369624},"filename":{"type":"string","minLength":1,"maxLength":320},"source":{"type":"string","minLength":1,"maxLength":320},"receivedAt":text,"recordDate":{"type":["string","null"]},"retentionUntil":{"type":["string","null"]},"uploader":{"type":"string","minLength":1,"maxLength":160},"sensitivity":{"type":["string","null"],"enum":["normal","confidential","restricted",null]},"links":{"type":["array","null"],"maxItems":4096,"items":link},"confirm":{"const":true}}),
            &[
                "type",
                "bytesBase64",
                "filename",
                "source",
                "receivedAt",
                "uploader",
                "confirm",
            ],
        ),
        CorporateRecordMutation::Link(_) => object(
            json!({"recordId":record_id,"type":link["properties"]["type"],"id":link["properties"]["id"],"confirm":{"const":true}}),
            &["type", "id", "confirm"],
        ),
        CorporateRecordMutation::Enrich(_) => object(
            json!({"recordId":record_id,"assertion":{"type":"string","minLength":1,"maxLength":2000},"confirm":{"const":true}}),
            &["assertion", "confirm"],
        ),
        CorporateRecordMutation::Supersede(_) => object(
            json!({"recordId":record_id,"replacementRecordId":{"type":"string","minLength":1,"maxLength":64},"reason":{"type":"string","minLength":1,"maxLength":1000},"confirm":{"const":true}}),
            &["replacementRecordId", "reason", "confirm"],
        ),
    }
}
fn id_segment(id: &str) -> Result<String, Error> {
    if id.trim().is_empty()
        || id.encode_utf16().count() > 64
        || matches!(id, "." | "..")
        || id.chars().any(char::is_control)
    {
        return Err(Error::InvalidInput(
            "bounded nonblank corporate record id required",
        ));
    }
    Ok(id
        .as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(b) {
                char::from(*b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect())
}
// Source text lengths are UTF-16/server-owned; the shared shape validator counts UTF-8.
fn utf8_bounds(v: &mut Value) {
    match v {
        Value::Object(m) => {
            if let Some(n) = m.get("maxLength").and_then(Value::as_u64) {
                m.insert("maxLength".into(), json!(n * 4));
            }
            for v in m.values_mut() {
                utf8_bounds(v);
            }
        }
        Value::Array(a) => {
            for v in a {
                utf8_bounds(v);
            }
        }
        _ => {}
    }
}
impl ApiClient {
    pub async fn corporate_records(
        &self,
        company: &str,
        cursor: usize,
        limit: usize,
    ) -> Result<Value, Error> {
        if !(1..=100).contains(&limit) {
            return Err(Error::InvalidInput(
                "corporate records limit1..100 required",
            ));
        }
        self.get(
            &company_path(company, "corporate-records")?,
            &[
                ("cursor", &cursor.to_string()),
                ("limit", &limit.to_string()),
            ],
        )
        .await
    }
    pub async fn corporate_record(&self, company: &str, id: &str) -> Result<Value, Error> {
        self.get(
            &company_path(company, &format!("corporate-records/{}", id_segment(id)?))?,
            &[],
        )
        .await
    }
    /// Exact native JSON {ok,bytesBase64,sha256}; no invented MIME, filename, file or verification claim.
    pub async fn corporate_record_original(&self, company: &str, id: &str) -> Result<Value, Error> {
        let mut response = self
            .send(
                Method::GET,
                &company_path(
                    company,
                    &format!("corporate-records/{}/download", id_segment(id)?),
                )?,
                &[],
                None,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        if !response.status().is_success() {
            return Err(Error::Refused {
                status: response.status().as_u16(),
            });
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_CORPORATE_ORIGINAL_JSON as u64)
        {
            return Err(Error::Protocol);
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_CORPORATE_ORIGINAL_JSON {
                return Err(Error::Protocol);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)
    }
    /// Exact confirmed body once. Canonical record/history and original reads reconcile unknown outcomes.
    pub async fn mutate_corporate_record(
        &self,
        company: &str,
        action: CorporateRecordMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        let ingest = matches!(action, CorporateRecordMutation::Ingest);
        let max = if ingest {
            MAX_CORPORATE_INGEST_BODY
        } else {
            MAX_CORPORATE_BODY
        };
        if !confirmed || bytes.len() > max {
            return Err(Error::InvalidInput(
                "explicit corporate confirmation and native body bound required",
            ));
        }
        let mut value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("native corporate JSON object required"))?;
        let mut schema = corporate_record_body_schema(&action);
        if ingest {
            let file = value
                .as_object_mut()
                .and_then(|o| o.remove("bytesBase64"))
                .filter(|v| {
                    v.as_str()
                        .is_some_and(|s| !s.is_empty() && s.len() <= 22_369_624)
                })
                .ok_or(Error::InvalidInput("bounded original base64 required"))?;
            drop(file);
            if let Some(links) = value.as_object_mut().and_then(|o| o.remove("links"))
                && !links.is_null()
            {
                let links =
                    links
                        .as_array()
                        .filter(|a| a.len() <= 4096)
                        .ok_or(Error::InvalidInput(
                            "native corporate links array bound required",
                        ))?;
                let mut link_schema = schema["properties"]["links"]["items"].clone();
                utf8_bounds(&mut link_schema);
                for link in links {
                    body(
                        &serde_json::to_vec(link)
                            .map_err(|_| Error::InvalidInput("invalid corporate link"))?,
                        &link_schema,
                    )?;
                }
            }
            schema["properties"]
                .as_object_mut()
                .unwrap()
                .remove("links");
            schema["properties"]
                .as_object_mut()
                .unwrap()
                .remove("bytesBase64");
            schema["required"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v != "bytesBase64");
        }
        utf8_bounds(&mut schema);
        body(
            &serde_json::to_vec(&value)
                .map_err(|_| Error::InvalidInput("invalid corporate JSON"))?,
            &schema,
        )?;
        let suffix = match action {
            CorporateRecordMutation::Ingest => "corporate-records".into(),
            CorporateRecordMutation::Link(id) => {
                format!("corporate-records/{}/link", id_segment(&id)?)
            }
            CorporateRecordMutation::Enrich(id) => {
                format!("corporate-records/{}/enrich", id_segment(&id)?)
            }
            CorporateRecordMutation::Supersede(id) => {
                format!("corporate-records/{}/supersede", id_segment(&id)?)
            }
        };
        self.request(
            Method::POST,
            &company_path(company, &suffix)?,
            &[],
            Some(bytes),
        )
        .await
    }
}
