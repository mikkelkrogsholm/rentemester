use crate::{ApiClient, Error};
use reqwest::Method;
use serde_json::{Value, json};
fn id(v: &str) -> bool {
    (8..=64).contains(&v.len())
        && v.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
impl ApiClient {
    pub async fn backup_destinations(&self) -> Result<Value, Error> {
        self.get("/api/workspace/backup-destinations", &[]).await
    }
    pub async fn place_backup(
        &self,
        destination: &str,
        operation: &str,
        confirm: bool,
    ) -> Result<Value, Error> {
        if !confirm || !id(destination) || !id(operation) {
            return Err(Error::InvalidInput(
                "explicit destination/operation and confirmation required",
            ));
        }
        self.request(
            Method::POST,
            &format!("/api/workspace/backup-destinations/{destination}/place"),
            &[],
            Some(
                json!({"operationId":operation,"confirm":true})
                    .to_string()
                    .into_bytes(),
            ),
        )
        .await
    }
    pub async fn verify_backup_placement(
        &self,
        destination: &str,
        operation: &str,
    ) -> Result<Value, Error> {
        if !id(destination) || !id(operation) {
            return Err(Error::InvalidInput("invalid destination/operation"));
        }
        self.get(
            &format!("/api/workspace/backup-destinations/{destination}/placements/{operation}"),
            &[],
        )
        .await
    }
}
