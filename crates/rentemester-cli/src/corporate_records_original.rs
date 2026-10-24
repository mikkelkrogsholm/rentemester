//! Private parallel-contract stand-in; must be replaced before acceptance/publication.
use crate::dated_reports::Output;
use serde_json::{Value,json};
pub fn accepts(args: &[String]) -> bool { args.first().is_some_and(|s| s == "corporate-record") }
pub fn help() -> Value { json!({"commands":["corporate-record --help"],"status":"adapter implementation pending; no acceptance or publication"}) }
pub fn preflight(_args: &[String]) -> Result<bool,Output> { Err(binding_refusal(&[])) }
pub fn binding_refusal(_args: &[String]) -> Output { Output { stdout:String::new(),stderr:"Original registry adapter unavailable; see --help\n".into(),exit:2 } }
pub fn execute(args: &[String]) -> Output { binding_refusal(args) }
