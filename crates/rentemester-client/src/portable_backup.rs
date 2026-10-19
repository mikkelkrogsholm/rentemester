//! Explicit workspace download only; verification and fresh restore stay operator actions.
use crate::{ApiClient, Error};
use reqwest::Method;
use serde_json::{Value, json};
pub const WORKSPACE_BACKUP_CONTENT_TYPE: &str = "application/vnd.rentemester.native-backup+json";
/// Matches the native artifact's wire bound, not its decoded inventory bound.
pub const MAX_WORKSPACE_BACKUP_BYTES: usize = 96 * 1024 * 1024;
impl ApiClient {
    /// Sends exactly {confirm:true} once after explicit caller confirmation. No
    /// selectors, files, key custody, archive verification, restore or automatic retry.
    pub async fn workspace_backup(&self, confirmed: bool) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit workspace backup confirmation required",
            ));
        }
        let mut response = self
            .send(
                Method::POST,
                "/api/workspace/backup",
                &[],
                Some(br#"{"confirm":true}"#.to_vec()),
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        if response.status().is_server_error() {
            return Err(Error::OutcomeUnknown);
        }
        if !response.status().is_success() {
            return Err(Error::Refused {
                status: response.status().as_u16(),
            });
        }
        if response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            != Some(WORKSPACE_BACKUP_CONTENT_TYPE)
            || response
                .content_length()
                .is_some_and(|n| n > MAX_WORKSPACE_BACKUP_BYTES as u64)
        {
            return Err(Error::OutcomeUnknown);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::OutcomeUnknown)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_WORKSPACE_BACKUP_BYTES {
                return Err(Error::OutcomeUnknown);
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.is_empty() {
            return Err(Error::OutcomeUnknown);
        }
        Ok(json!({"ok":true,"backup":{"contentType":WORKSPACE_BACKUP_CONTENT_TYPE,"bytes":bytes}}))
    }
}
