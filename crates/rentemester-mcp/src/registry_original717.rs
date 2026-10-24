//! R74_PRIVATE_STANDIN: never qualify or publish this implementation.
use serde_json::{Value,json};
use rentemester_client::ApiClient;
const NAMES: &[&str] = &["workspace_party_search", "workspace_party_inspect", "workspace_party_create", "workspace_party_link_role", "workspace_party_propose_merge", "workspace_party_approve_merge", "corporate_record_list", "corporate_record_inspect", "corporate_record_download", "corporate_record_ingest", "corporate_record_link", "corporate_record_enrich", "corporate_record_supersede"];
pub fn accepts(name:&str)->bool { NAMES.contains(&name) }
pub fn tools()->Vec<Value>{ NAMES.iter().map(|name|crate::tools::tool(name,"R74_PRIVATE_STANDIN",json!({"type":"object","properties":{},"additionalProperties":false}),true)).collect() }
pub async fn call(_api:&ApiClient,_name:&str,_args:Value)->Result<Value,&'static str>{ Err("R74_PRIVATE_STANDIN") }
