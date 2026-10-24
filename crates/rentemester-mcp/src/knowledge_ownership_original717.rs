//! R74_PRIVATE_STANDIN: never qualify or publish this implementation.
use serde_json::{Value,json};
use rentemester_client::ApiClient;
const NAMES: &[&str] = &["company_knowledge_context", "company_knowledge_propose", "company_knowledge_review", "company_knowledge_supersede", "ownership_graph_query", "ownership_snapshot_history", "ownership_snapshot_propose", "ownership_snapshot_review", "ownership_snapshot_apply"];
pub fn accepts(name:&str)->bool { NAMES.contains(&name) }
pub fn tools()->Vec<Value>{ NAMES.iter().map(|name|crate::tools::tool(name,"R74_PRIVATE_STANDIN",json!({"type":"object","properties":{},"additionalProperties":false}),true)).collect() }
pub async fn call(_api:&ApiClient,_name:&str,_args:Value)->Result<Value,&'static str>{ Err("R74_PRIVATE_STANDIN") }
