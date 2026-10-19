//! Explicit workspace ingress and handoff; the server owns scope, evidence and document policy.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_WORKSPACE_INBOX_BODY: usize = 35 * 1024 * 1024;
#[derive(Clone)]
pub enum WorkspaceInboxMutation {
    Ingest,
    Assign(String),
    Complete(String),
}
pub fn workspace_inbox_body_schema(action: &WorkspaceInboxMutation) -> Value {
    let text = json!({"type":"string","minLength":1});
    match action {
        WorkspaceInboxMutation::Ingest => object(
            json!({
                "confirm":{"const":true},"sourceId":{"type":["string","null"],"maxLength":100},
                "idempotencyKey":text,"bytesBase64":{"type":"string","minLength":1,"maxLength":34952536},
                "filename":{"type":"string","minLength":1,"maxLength":255},"mimeType":text,
                "transport":{"const":"upload"},"transportIdentity":{"type":["string","null"]},
                "receivedAt":text,"metadata":{"type":"object","description":"Exact caller facts; native64KiB/depth32. Empty object may be ingested, but completion requires native document Metadata. No inferred invoice/receipt facts."},
                "candidates":{"type":"array","maxItems":100,"items":object(json!({"companySlug":text,"evidenceKind":text,"evidence":text}),&["companySlug","evidenceKind","evidence"])}}
            ),
            &[
                "confirm",
                "idempotencyKey",
                "bytesBase64",
                "filename",
                "mimeType",
                "transport",
                "receivedAt",
                "metadata",
            ],
        ),
        _ => object(
            json!({"companySlug":text,"confirm":{"const":true}}),
            &["companySlug", "confirm"],
        ),
    }
}
fn source_segment(id: &str) -> Result<String, Error> {
    if id.trim().is_empty()
        || id.encode_utf16().count() > 100
        || matches!(id, "." | "..")
        || id.chars().any(char::is_control)
    {
        return Err(Error::InvalidInput("bounded nonblank sourceId required"));
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
impl ApiClient {
    pub async fn workspace_inbox(
        &self,
        company: &str,
        cursor: usize,
        limit: usize,
    ) -> Result<Value, Error> {
        if !(1..=100).contains(&limit) {
            return Err(Error::InvalidInput("inbox limit1..100 required"));
        }
        self.get(
            &company_path(company, "workspace-inbox")?,
            &[
                ("cursor", &cursor.to_string()),
                ("limit", &limit.to_string()),
            ],
        )
        .await
    }
    pub async fn workspace_inbox_source(
        &self,
        company: &str,
        source_id: &str,
    ) -> Result<Value, Error> {
        self.get(
            &company_path(
                company,
                &format!("workspace-inbox/{}", source_segment(source_id)?),
            )?,
            &[],
        )
        .await
    }
    /// Exact confirmed native body once. Reconcile source/assignments and target originals after uncertainty.
    /// The anchor is access scope, never an implicit assignment destination; no generated keys or retries.
    pub async fn mutate_workspace_inbox(
        &self,
        company: &str,
        action: WorkspaceInboxMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > MAX_WORKSPACE_INBOX_BODY {
            return Err(Error::InvalidInput(
                "explicit inbox confirmation and native35MiB body bound required",
            ));
        }
        let mut value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("native inbox JSON object required"))?;
        let mut schema = workspace_inbox_body_schema(&action);
        if matches!(action, WorkspaceInboxMutation::Ingest) {
            let metadata = value
                .as_object_mut()
                .and_then(|o| o.remove("metadata"))
                .filter(Value::is_object)
                .ok_or(Error::InvalidInput(
                    "explicit inbox metadata object required",
                ))?;
            // Arbitrary factual metadata remains untouched in the original body. Native metadata
            // bounds, eligibility and completion policy are checked by the server.
            drop(metadata);
            schema["properties"]
                .as_object_mut()
                .unwrap()
                .remove("metadata");
            let base64 = value
                .as_object_mut()
                .and_then(|o| o.remove("bytesBase64"))
                .filter(|v| {
                    v.as_str()
                        .is_some_and(|s| !s.is_empty() && s.len() <= 34_952_536)
                })
                .ok_or(Error::InvalidInput("bounded original base64 required"))?;
            drop(base64);
            schema["properties"]
                .as_object_mut()
                .unwrap()
                .remove("bytesBase64");
            schema["required"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v != "bytesBase64");
            schema["required"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v != "metadata");
            // Shared shape validator counts UTF-8; source text limit is UTF-16/server-owned.
            schema["properties"]["sourceId"]["maxLength"] = json!(400);
        }
        body(
            &serde_json::to_vec(&value).map_err(|_| Error::InvalidInput("invalid inbox JSON"))?,
            &schema,
        )?;
        let suffix = match action {
            WorkspaceInboxMutation::Ingest => "workspace-inbox".to_owned(),
            WorkspaceInboxMutation::Assign(id) => {
                format!("workspace-inbox/{}/assign", source_segment(&id)?)
            }
            WorkspaceInboxMutation::Complete(id) => {
                format!("workspace-inbox/{}/complete", source_segment(&id)?)
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
