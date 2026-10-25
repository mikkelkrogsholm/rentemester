//! Inert original discovery on the common server. No retries or native aliases.
use crate::{ApiClient, Error};
use serde_json::Value;

impl ApiClient {
    pub async fn original_agent_about123(&self) -> Result<Value, Error> {
        self.get("/api/original/agent-about", &[]).await
    }

    pub async fn original_agent_capabilities123(
        &self,
        query: &str,
        cursor: u64,
        limit: usize,
    ) -> Result<Value, Error> {
        if query.encode_utf16().count() > 200 || !(1..=50).contains(&limit) {
            return Err(Error::InvalidInput("invalid discovery pagination/query"));
        }
        self.get("/api/original/agent-capabilities", &[
            ("query", query), ("cursor", &cursor.to_string()), ("limit", &limit.to_string()),
        ]).await
    }

    pub async fn original_agent_workflow123(&self, id: &str) -> Result<Value, Error> {
        if id.is_empty() || id.encode_utf16().count() > 100 {
            return Err(Error::InvalidInput("invalid workflow id"));
        }
        // Encode the complete UTF-8 selector, not a filesystem path or native slug.
        let mut segment = String::new();
        for byte in id.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'~') {
                segment.push(char::from(byte));
            } else {
                use std::fmt::Write;
                write!(segment, "%{byte:02X}").expect("String write");
            }
        }
        self.get(&format!("/api/original/agent-workflows/{segment}"), &[]).await
    }
}
