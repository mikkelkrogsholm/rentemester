// r71_private_standin: replaced by the isolated adapter owner before acceptance/publication.
use crate::dated_reports::Output;
use serde_json::{Value,json};
pub fn accepts(args:&[String])->bool { args.first().is_some_and(|s|s=="workspace-inbox") }
pub fn help()->Value { json!({"status":"r71_private_standin"}) }
fn unavailable()->Output { Output{stdout:String::new(),stderr:"r71_private_standin\n".into(),exit:2} }
pub fn preflight(_: &[String])->Result<bool,Output> {Err(unavailable())}
pub fn execute(_: &[String])->Output {unavailable()}
pub fn binding_refusal(_: &[String])->Output {unavailable()}
