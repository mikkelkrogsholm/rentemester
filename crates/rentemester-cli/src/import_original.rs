//! Private integration base; replaced by owned adapter before public delivery.
use crate::dated_reports::Output;
use serde_json::{json,Value};
pub fn accepts(a:&[String])->bool{a.first().is_some_and(|s|s=="import")}
pub fn needs_company(a:&[String])->bool{a.get(1).is_some_and(|s|s=="contacts")}
pub fn help()->Value{json!({"commands":[],"available":false,"reason":"private candidate awaiting owned implementation"})}
pub fn preflight(a:&[String])->Result<bool,Output>{if a==["import"]||a==["import","--help"]{Ok(true)}else{Ok(false)}}
pub fn binding_refusal(_: &[String])->Output{Output{stdout:String::new(),stderr:"Explicit current runtime and company required\n".into(),exit:2}}
pub fn execute(a:&[String])->Output{binding_refusal(a)}
