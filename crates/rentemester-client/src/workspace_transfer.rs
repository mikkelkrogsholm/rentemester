//! Explicit managed handoff and per-receipt reconciliation. No operator trust, activation or retry inference.
use crate::{
    ApiClient, Error, MAX_RESPONSE, MAX_WORKSPACE_BACKUP_BYTES, WORKSPACE_BACKUP_CONTENT_TYPE,
    company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_WORKSPACE_TRANSFER_BODY: usize = 16 * 1024;
pub const MAX_WORKSPACE_TRANSFER_PROOF_BODY: usize = 32 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceTransferMutation {
    Freeze,
    Cancel,
    Export,
    Release,
    Activate,
}
impl WorkspaceTransferMutation {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Freeze => "freeze",
            Self::Cancel => "cancel",
            Self::Export => "export",
            Self::Release => "release",
            Self::Activate => "activate",
        }
    }
    pub fn max_body_bytes(self) -> usize {
        if matches!(self, Self::Release | Self::Activate) {
            MAX_WORKSPACE_TRANSFER_PROOF_BODY
        } else {
            MAX_WORKSPACE_TRANSFER_BODY
        }
    }
}
fn sha_schema() -> Value {
    json!({"type":"string","minLength":64,"maxLength":64,"pattern":"^[0-9a-f]{64}$"})
}
fn id_schema() -> Value {
    json!({"type":"string","minLength":32,"maxLength":32,"pattern":"^[0-9a-f]{32}$"})
}
pub fn workspace_transfer_body_schema(action: WorkspaceTransferMutation) -> Value {
    let mut fields = json!({"expectedStateHash":sha_schema(),"confirm":{"const":true}});
    let mut required = vec!["expectedStateHash", "confirm"];
    if action != WorkspaceTransferMutation::Export {
        fields["reason"] = json!({"type":"string","minLength":1,"description":"Trimmed10..2000 UTF8 bytes, no controls. Exact input forwarded."});
        required.push("reason");
    }
    if action == WorkspaceTransferMutation::Freeze {
        fields["destination"] = json!({"type":"string","minLength":1,"description":"Explicit destination, trimmed10..300 UTF8 bytes, no controls"});
        required.push("destination");
    }
    if action == WorkspaceTransferMutation::Release {
        fields["challenge"] = object(
            json!({"version":{"const":1},"transferId":id_schema(),"sourceInstanceId":id_schema(),"workspaceId":{"type":"string","minLength":1,"maxLength":1024,"description":"Exact source workspace identity, at most1024 UTF8 bytes; server verifies"},"archiveSha256":sha_schema(),"destinationInstanceId":id_schema(),"destination":{"type":"string","minLength":1},"companies":{"type":"array","minItems":1,"maxItems":64,"description":"Exact sorted unique source company catalog; server validates registered slugs/UUIDs, no client inference", "items":object(json!({"slug":{"type":"string","minLength":1,"maxLength":100,"pattern":"^[a-z0-9][a-z0-9-]*$"},"ledgerUuid":id_schema()}),&["slug","ledgerUuid"])}}),
            &[
                "version",
                "transferId",
                "sourceInstanceId",
                "workspaceId",
                "archiveSha256",
                "destinationInstanceId",
                "destination",
                "companies",
            ],
        );
        required.push("challenge");
    }
    if action == WorkspaceTransferMutation::Activate {
        fields["release"] = json!({"type":"string","minLength":1,"maxLength":16384,"description":"Exact signed release JSON string (16KiB maximum), independently trusted SOURCE key is operator configuration, never request input"});
        required.push("release");
    }
    object(fields, &required)
}
pub fn restored_receipt_body_schema() -> Value {
    object(
        json!({"receiptId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"expectedStateHash":sha_schema(),"reason":{"type":"string","minLength":1,"description":"Explicit reconciliation reason, trimmed10..2000 UTF8 bytes, no controls"},"confirm":{"const":true}}),
        &["receiptId", "expectedStateHash", "reason", "confirm"],
    )
}
fn hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn text(s: &str, min: usize, max: usize) -> bool {
    let s = s.trim();
    (min..=max).contains(&s.len()) && !s.chars().any(char::is_control)
}
fn validate(
    bytes: &[u8],
    confirmed: bool,
    action: Option<WorkspaceTransferMutation>,
) -> Result<(), Error> {
    let max = action.map_or(
        MAX_WORKSPACE_TRANSFER_BODY,
        WorkspaceTransferMutation::max_body_bytes,
    );
    if !confirmed || bytes.len() > max {
        return Err(Error::InvalidInput(
            "explicit confirmation and bounded native transfer body required",
        ));
    }
    body(
        bytes,
        &action.map_or_else(restored_receipt_body_schema, workspace_transfer_body_schema),
    )?;
    let v: Value = serde_json::from_slice(bytes)
        .map_err(|_| Error::InvalidInput("exact native transfer JSON required"))?;
    if !hex(v["expectedStateHash"].as_str().unwrap_or(""), 64) {
        return Err(Error::InvalidInput(
            "exact current lowercase state SHA256 required",
        ));
    }
    if action != Some(WorkspaceTransferMutation::Export)
        && !text(v["reason"].as_str().unwrap_or(""), 10, 2000)
    {
        return Err(Error::InvalidInput(
            "explicit bounded reconciliation/transfer reason required",
        ));
    }
    if action == Some(WorkspaceTransferMutation::Freeze)
        && !text(v["destination"].as_str().unwrap_or(""), 10, 300)
    {
        return Err(Error::InvalidInput(
            "explicit reviewed bounded destination required",
        ));
    }
    if action == Some(WorkspaceTransferMutation::Activate)
        && v["release"]
            .as_str()
            .is_none_or(|s| s.is_empty() || s.len() > 16384)
    {
        return Err(Error::InvalidInput(
            "exact bounded signed release string required",
        ));
    }
    if action.is_none()
        && !v["receiptId"]
            .as_i64()
            .is_some_and(|id| (1..=9_007_199_254_740_991).contains(&id))
    {
        return Err(Error::InvalidInput(
            "positive exact imported receipt ID required",
        ));
    }
    // Signature/catalog/authority verification belongs to the native runtime/core.
    Ok(())
}
impl ApiClient {
    pub async fn workspace_transfer_status(&self) -> Result<Value, Error> {
        self.transfer_json(Method::GET, "/api/workspace/transfer", None, false)
            .await
    }
    pub async fn workspace_transfer_freeze(
        &self,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.transfer_write(WorkspaceTransferMutation::Freeze, bytes, confirmed)
            .await
    }
    pub async fn workspace_transfer_cancel(
        &self,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.transfer_write(WorkspaceTransferMutation::Cancel, bytes, confirmed)
            .await
    }
    pub async fn workspace_transfer_release(
        &self,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.transfer_write(WorkspaceTransferMutation::Release, bytes, confirmed)
            .await
    }
    pub async fn workspace_transfer_activate(
        &self,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.transfer_write(WorkspaceTransferMutation::Activate, bytes, confirmed)
            .await
    }
    pub async fn workspace_transfer_export(
        &self,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.transfer_write(WorkspaceTransferMutation::Export, bytes, confirmed)
            .await
    }
    pub async fn restored_receipts(&self, company: &str) -> Result<Value, Error> {
        self.transfer_json(
            Method::GET,
            &company_path(company, "restore/receipts")?,
            None,
            false,
        )
        .await
    }
    pub async fn reconcile_restored_receipt(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        validate(&bytes, confirmed, None)?;
        self.transfer_json(
            Method::POST,
            &company_path(company, "restore/receipts/reconcile")?,
            Some(bytes),
            false,
        )
        .await
    }
    async fn transfer_write(
        &self,
        action: WorkspaceTransferMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        validate(&bytes, confirmed, Some(action))?;
        self.transfer_json(
            Method::POST,
            &format!("/api/workspace/transfer/{}", action.suffix()),
            Some(bytes),
            action == WorkspaceTransferMutation::Export,
        )
        .await
    }
    async fn transfer_json(
        &self,
        method: Method,
        path: &str,
        body: Option<Vec<u8>>,
        export: bool,
    ) -> Result<Value, Error> {
        let mutation = body.is_some();
        let mut response = self
            .send(
                method,
                path,
                &[],
                body,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        let unknown = || {
            if mutation {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let status = response.status();
        if mutation && (status.is_server_error() || status.as_u16() == 408) {
            return Err(Error::OutcomeUnknown);
        }
        let binary = export && status.is_success();
        if binary
            && response
                .headers()
                .get("content-type")
                .and_then(|h| h.to_str().ok())
                != Some(WORKSPACE_BACKUP_CONTENT_TYPE)
        {
            return Err(Error::OutcomeUnknown);
        }
        let limit = if binary {
            MAX_WORKSPACE_BACKUP_BYTES
        } else {
            MAX_RESPONSE
        };
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err(unknown());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unknown())? {
            if bytes.len().saturating_add(chunk.len()) > limit {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        if binary {
            return if bytes.is_empty() {
                Err(Error::OutcomeUnknown)
            } else {
                Ok(
                    json!({"ok":true,"backup":{"contentType":WORKSPACE_BACKUP_CONTENT_TYPE,"bytes":bytes}}),
                )
            };
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
        if value["ok"] == false && value["code"].as_str().is_some_and(|code| !code.is_empty()) {
            if mutation
                && matches!(
                    value["code"].as_str(),
                    Some("internal" | "outcome_unknown" | "outcome_unconfirmed")
                )
            {
                return Err(Error::OutcomeUnknown);
            }
            return Ok(value);
        }
        if status.is_success() && value["ok"] == true && value.is_object() {
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
