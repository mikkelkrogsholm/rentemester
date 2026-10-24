//! Stable private dispatch boundary; assigned worker replaces this before public acceptance.
use crate::dated_reports::Output;
use serde_json::{Value,json};
pub fn accepts(args:&[String])->bool {args.first().is_some_and(|s|s=="group")}
pub fn help()->Value {json!({"commands":[],"status":"integration_pending"})}
pub fn binding_refusal(_args:&[String])->Output {Output{stdout:String::new(),stderr:"Explicit verified binding required\n".into(),exit:2}}
pub fn preflight(args:&[String])->Result<bool,Output> {if args==["group"]||args==["group","--help"]{Ok(true)}else{Err(binding_refusal(args))}}
pub fn execute(args:&[String])->Output {binding_refusal(args)}
