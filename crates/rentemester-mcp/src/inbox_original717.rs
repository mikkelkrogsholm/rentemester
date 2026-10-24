//! R74_PRIVATE_STANDIN: never qualify or publish this implementation.
use serde_json::{Value,json};
use rentemester_client::ApiClient;
const NAMES: &[&str] = &["workspace_inbox_list", "workspace_inbox_inspect", "workspace_inbox_status", "workspace_inbox_ingest", "workspace_inbox_assign", "workspace_inbox_complete"];
pub fn accepts(name:&str)->bool { NAMES.contains(&name) }
pub fn tools()->Vec<Value>{ NAMES.iter().map(|name|crate::tools::tool(name,"R74_PRIVATE_STANDIN",json!({"type":"object","properties":{},"additionalProperties":false}),true)).collect() }
pub async fn call(_api:&ApiClient,_name:&str,_args:Value)->Result<Value,&'static str>{ Err("R74_PRIVATE_STANDIN") }
