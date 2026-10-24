//! Private owned integration base; replaced before public delivery.
use rentemester_client::ApiClient;
use serde_json::Value;
pub fn accepts(name:&str)->bool{["import_archive_list", "import_archive_year"].contains(&name)}
pub fn tools()->Vec<Value>{vec![]}
pub async fn call(_: &ApiClient,_:&str,_:Value)->Result<Value,&'static str>{Err("private read adapter awaiting owned implementation")}
