use super::{ApiClient, Error};
use serde_json::Value;

impl ApiClient {
    pub async fn agent_about(&self) -> Result<Value, Error> {
        self.get("/api/agent-about", &[]).await
    }
    pub async fn agent_capabilities(
        &self,
        query: &str,
        cursor: usize,
        limit: usize,
    ) -> Result<Value, Error> {
        if query.chars().count() > 200 || cursor > 1_000_000 || !(1..=50).contains(&limit) {
            return Err(Error::InvalidInput("invalid discovery pagination/query"));
        }
        self.get(
            "/api/agent-capabilities",
            &[
                ("query", query),
                ("cursor", &cursor.to_string()),
                ("limit", &limit.to_string()),
            ],
        )
        .await
    }
    pub async fn agent_workflow(&self, id: &str) -> Result<Value, Error> {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(Error::InvalidInput("invalid workflow id"));
        }
        self.get(&format!("/api/agent-workflows/{id}"), &[]).await
    }
}
