//! Offline static commands share the embedded pure engine corpus.
use rentemester_engine::{Error, regulatory};
use serde_json::{Value,json};
use std::collections::BTreeMap;
fn invalid(s:&str)->(u8,Error){(2,Error::new("invalid_input",s))}
pub fn run(args:&[String])->Result<Value,(u8,Error)>{
    let command=args.get(1).map(String::as_str).ok_or_else(||invalid("reg coverage|citations [--out PATH] [--json]"))?;
    if !matches!(command,"coverage"|"citations"){return Err(invalid("reg coverage|citations"));}
    let mut out=None; let mut i=2;
    while i<args.len(){match args[i].as_str(){"--json"=>i+=1,"--out" if out.is_none()=>{out=Some(args.get(i+1).ok_or_else(||invalid("--out requires path"))?);i+=2;},_=>return Err(invalid("static reg accepts only --out and --json; no company or login"))}}
    let coverage=regulatory::bundled_coverage().map_err(|e|(1,e))?;
    let report=if command=="coverage"{regulatory::render_coverage(&coverage)}else{regulatory::render_citations(&coverage)}.map_err(|e|(1,e))?;
    if let Some(path)=out {std::fs::write(path,&report).map_err(|_|invalid("could not write requested regulatory report"))?;}
    Ok(json!({"ok":coverage.get("ok").and_then(Value::as_bool).unwrap_or(false),"report":report,"coverage":coverage}))
}
pub fn compliance(flags:&BTreeMap<&str,&str>,allow:bool,confirm:bool,preview:bool)->Result<Value,(u8,Error)>{
    if confirm||preview||flags.keys().any(|k|!["--company","--server","--cookie-file","--as-of","--out"].contains(k)){return Err(invalid("compliance-report requires company/current cookie; optional --as-of/--out only"));}
    let company=flags.get("--company").ok_or_else(||invalid("--company required"))?;
    let server=flags.get("--server").ok_or_else(||invalid("--server required"))?;
    let cookie=flags.get("--cookie-file").ok_or_else(||invalid("--cookie-file required"))?;
    let cookie=rentemester_client::read_cookie_file(std::path::Path::new(cookie)).map_err(crate::remote::client_error)?;
    let api=rentemester_client::ApiClient::new(server,allow,Some(cookie)).map_err(crate::remote::client_error)?;
    let rt=tokio::runtime::Runtime::new().map_err(|_|invalid("HTTP runtime unavailable"))?;
    let result=rt.block_on(api.compliance_report(company,flags.get("--as-of").copied())).map_err(crate::remote::client_error)?;
    if let Some(path)=flags.get("--out") {let html=result.pointer("/complianceReport/html").and_then(Value::as_str).ok_or_else(||invalid("missing server HTML"))?;std::fs::write(path,html).map_err(|_|invalid("could not write report"))?;}
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(false),"data":result}))
}
