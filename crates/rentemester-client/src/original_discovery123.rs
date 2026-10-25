//! Inert original discovery on the common server. No retries or native aliases.
use crate::{ApiClient, Error};
use serde_json::Value;

impl ApiClient {
    pub async fn original_agent_about123(&self) -> Result<Value, Error> {
        core(self.get("/api/original/agent-about", &[]).await?,"about",None)
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
        core(self.get("/api/original/agent-capabilities", &[
            ("query", query), ("cursor", &cursor.to_string()), ("limit", &limit.to_string()),
        ]).await?,"search",None)
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
        core(self.get(&format!("/api/original/agent-workflows/{segment}"), &[]).await?,"workflow",Some(id))
    }
}

fn strings(v:&Value)->bool{v.as_array().is_some_and(|a|a.iter().all(Value::is_string))}
fn safety(v:&Value)->bool{matches!(v.as_str(),Some("read"|"write"|"destructive"))}
fn retry(v:&Value)->bool{matches!(v.as_str(),Some("safe-read"|"key-idempotent"|"natural-idempotent"|"external-provider-reconciled"|"unsafe-read-back"))}
fn operation(v:&Value)->bool{
 v.is_object()&&v.get("resolved").is_some_and(|x|x.is_null()||x.is_boolean())&&match v["surface"].as_str(){Some("mcp")=>v["name"].is_string(),Some("cli")=>v["key"].is_string(),Some("http")=>v["method"].is_string()&&v["pattern"].is_string(),_=>false}
}
fn binding(v:&Value)->bool{v.is_object()&&v["id"].is_string()&&strings(&v["capabilityIds"])&&safety(&v["safety"])&&v.get("idempotent").is_some_and(|x|x.is_null()||x.is_boolean())&&v["requiresActor"].is_boolean()&&v["requiresConfirmation"].is_boolean()&&retry(&v["retryClass"])}
fn core(v:Value,kind:&str,id:Option<&str>)->Result<Value,Error>{
 let c=&v["catalogue"];
 let common=v.is_object()&&v["workspaceConnected"].is_boolean()&&c.is_object()&&c["schemaVersion"]=="rentemester-agent-discovery-v1"&&c["hash"].as_str().is_some_and(|s|s.len()==64&&s.bytes().all(|b|b.is_ascii_hexdigit()))&&c["capabilityCount"].as_u64().is_some()&&c["workflowCount"].as_u64().is_some()&&c["fullCoverage"].is_boolean();
 let valid=common&&match kind{
  "about"=>v["serverName"]=="rentemester-mcp"&&v["serverVersion"].is_string()&&v.get("toolCount").is_some_and(|x|x.is_null()||x.as_u64().is_some())&&strings(&v["contractDocs"]),
  "search"=>v["total"].as_u64().is_some()&&v["cursor"].as_u64().is_some()&&v["limit"].as_u64().is_some_and(|n|(1..=50).contains(&n))&&v["hasMore"].is_boolean()&&v.get("nextCursor").is_some_and(|x|x.is_null()||x.as_u64().is_some())&&v["items"].as_array().is_some_and(|items|v["count"].as_u64()==Some(items.len()as u64)&&items.iter().all(|item|item.is_object()&&item["id"].is_string()&&strings(&item["workflowIds"])&&item["operations"].as_array().is_some_and(|ops|ops.iter().all(binding)))),
  "workflow"=>v["capability"].is_object()&&v["workflow"].is_object()&&v["workflow"]["id"].as_str()==id&&v["workflow"]["live"].is_boolean()&&v["workflow"]["steps"].as_array().is_some_and(|steps|!steps.is_empty()&&steps.iter().all(|s|s.is_object()&&s["id"].is_string()&&safety(&s["expectedSafety"])&&s["expectedIdempotent"].is_boolean()&&s["requiresActor"].is_boolean()&&s["requiresConfirmation"].is_boolean()&&retry(&s["retryClass"])&&operation(&s["operation"])&&s.get("uncertainOutcomeReadBack").is_none_or(operation))),
  _=>false,
 };
 if valid{Ok(v)}else{Err(Error::Protocol)}
}
