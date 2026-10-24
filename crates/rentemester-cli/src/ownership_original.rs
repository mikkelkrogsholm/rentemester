//! Source CLI envelopes over the existing authenticated ownership transport.
use crate::dated_reports::Output;
use rentemester_client::{ApiClient, Error, OwnershipMutation as Mutation, ownership_body_schema, MAX_OWNERSHIP_PROPOSE_BODY};
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};

pub fn accepts(args: &[String]) -> bool { args.first().is_some_and(|s| s == "ownership") }
pub fn help() -> Value {
    json!({"commands":["ownership query --as-of YYYY-MM-DD","ownership propose --input JSONFILE --confirm yes","ownership review --snapshot-id ID --decision approved|rejected --confirm yes","ownership apply --snapshot-id ID --snapshot-hash SHA256 --diff-hash SHA256 --confirm yes","ownership history [--snapshot-id ID]","ownership projection --as-of YYYY-MM-DD"],"connection":"Explicit --company SLUG|VERIFIED_LOCAL_PATH, --server ORIGIN or --local-profile ABSOLUTE_PRIVATE_PROFILE, --cookie-file PRIVATEFILE; --allow-loopback-http; --json or --format human|json. --workspace PATH is an optional local-profile equality assertion only.","authority":"Human cookie only; --service-key-file unsupported. Optional write --actor user:ID and --principal-id ID assert current api.me identity, never choose authority. Input actor/principal assertions must match current user; input confirm/authorized/authority/bytesBase64 and unknown metadata are refused. No stdin, generated IDs/hashes, automatic review/apply or retry. Unknown write outcomes require canonical query/history before further explicit action.","compatibility":"Projection requires an actually present complete native projection; partial native graphs cannot establish workspace completeness.","bodySchema":ownership_body_schema(Mutation::Propose)})
}
fn usage(message: &str) -> Output { Output { stdout:String::new(), stderr:format!("{message}; see ownership --help\n"), exit:2 } }
pub fn binding_refusal(_: &[String]) -> Output { usage("Ownership explicit local binding refused") }
fn write(command: &str) -> bool { matches!(command,"propose"|"review"|"apply") }
fn parse(args: &[String]) -> Result<BTreeMap<&str,&str>,Output> {
    if args == ["ownership"] || args == ["ownership","--help"] { return Ok(BTreeMap::from([("--help","true")])); }
    let command=args.get(1).map(String::as_str).unwrap_or("");
    if !matches!(command,"query"|"propose"|"review"|"apply"|"history"|"projection") { return Err(usage("Unknown ownership command")); }
    let mut flags=BTreeMap::new(); let mut i=2;
    while i<args.len() {
        let (key,attached)=crate::original_args::option(&args[i]);
        let allowed=matches!(key,"--company"|"--server"|"--local-profile"|"--workspace"|"--cookie-file"|"--allow-loopback-http"|"--help"|"--json"|"--format")
            || write(command) && matches!(key,"--actor"|"--principal-id"|"--confirm")
            || matches!(command,"query"|"projection") && key=="--as-of"
            || command=="propose" && key=="--input"
            || matches!(command,"review"|"apply"|"history") && key=="--snapshot-id"
            || command=="review" && key=="--decision"
            || command=="apply" && matches!(key,"--snapshot-hash"|"--diff-hash");
        if !allowed { return Err(usage("Unknown or unsupported ownership flag/argument (human cookie only; service-key-file unsupported)")); }
        let (v,next)=if matches!(key,"--help"|"--json"|"--allow-loopback-http") {
            if attached.is_some() { return Err(usage("Boolean flag takes no value")); } ("true",i+1)
        } else { crate::original_args::value(args,i).filter(|(v,_)| !v.is_empty() && v.len()<=16384 && !v.contains('\0')).ok_or_else(||usage("Missing or invalid flag value"))? };
        if flags.insert(key,v).is_some() { return Err(usage("Duplicate ownership flag")); } i=next;
    }
    if flags.get("--format").is_some_and(|v| !matches!(*v,"human"|"json")) { return Err(usage("Invalid format")); }
    if flags.contains_key("--local-profile") && (flags.contains_key("--server") || flags.contains_key("--allow-loopback-http")) { return Err(usage("Choose one explicit transport")); }
    if flags.contains_key("--workspace") && !flags.contains_key("--local-profile") { return Err(usage("Workspace assertion requires local-profile")); }
    if let Some(path)=flags.get("--local-profile") { if !std::path::Path::new(path).is_absolute() { return Err(usage("Local profile must be absolute")); } }
    if let Some(server)=flags.get("--server") { ApiClient::new(server,flags.contains_key("--allow-loopback-http"),None).map_err(|_|usage("Invalid server origin"))?; }
    if let Some(company)=flags.get("--company") {
        if !flags.contains_key("--local-profile") && (company.len()>128 || !company.bytes().all(|b| b.is_ascii_alphanumeric() || b==b'-' || b==b'_')) { return Err(usage("Invalid company selector")); }
    }
    if flags.get("--confirm").is_some_and(|v| *v!="yes") { return Err(usage("--confirm must be exactly yes")); }
    if let Some(id)=flags.get("--snapshot-id") { if id.trim().is_empty() || id.encode_utf16().count()>80 { return Err(usage("Invalid snapshot ID")); } }
    if let Some(decision)=flags.get("--decision") { if !matches!(*decision,"approved"|"rejected") { return Err(usage("Decision must be approved or rejected")); } }
    for key in ["--snapshot-hash","--diff-hash"] { if let Some(hash)=flags.get(key) { if hash.len()!=64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) { return Err(usage("Exact SHA256 hash required")); } } }
    if let Some(date)=flags.get("--as-of") { rentemester_engine::ownership::date(date).map_err(|_|usage("Invalid --as-of date"))?; }
    if flags.contains_key("--help") { return Ok(flags); }
    for key in ["--company","--cookie-file"] { if !flags.contains_key(key) { return Err(usage("Explicit company and private cookie required")); } }
    if !flags.contains_key("--server") && !flags.contains_key("--local-profile") { return Err(usage("Explicit transport required")); }
    let required: &[&str]=match command { "query"|"projection"=>&["--as-of"],"propose"=>&["--input","--confirm"],"review"=>&["--snapshot-id","--decision","--confirm"],"apply"=>&["--snapshot-id","--snapshot-hash","--diff-hash","--confirm"],_=>&[] };
    if required.iter().any(|key| !flags.contains_key(key)) { return Err(usage("Missing required ownership selector or --confirm yes")); }
    Ok(flags)
}
pub fn preflight(args: &[String]) -> Result<bool,Output> { parse(args).map(|f|f.contains_key("--help")) }
fn label(command:&str)-> &'static str { match command {
    "query"=>"Viser kun synlige, godkendte ownership/control-facts pr. dato; aldrig en konsolidering.",
    "propose"=>"Gemmer en kilde-hashet ownership-observation og deterministisk diff uden at ændre juridiske facts.",
    "review"=>"Review af én eksakt ownership-snapshot; approval anvender ikke facts.",
    "apply"=>"Anvender kun den eksakt reviewede diff append-only; v1 group manifest berøres ikke.",
    "history"=>"Viser append-only snapshot-, review- og apply-historik.",
    _=>"Viser om facts eksplicit kan projiceres; den ændrer aldrig group-manifest v1." } }
fn emit(v:Value,json_mode:bool,command:&str)->Output {
    let exit=if v["ok"]==true {0}else{1};
    if json_mode { return Output {stdout:format!("{}\n",serde_json::to_string_pretty(&v).unwrap()),stderr:String::new(),exit}; }
    if exit==0 {
        let label=label(command); let clause=label.split(". ").next().unwrap();
        let heading=if clause.chars().count()>60 {format!("{}…",clause.chars().take(57).collect::<String>().trim_end())}else{clause.into()};
        // None of ownership's source payload fields are preferred scalar/drill keys.
        Output {stdout:format!("✔ {heading}\n"),stderr:String::new(),exit}
    } else {
        let errors=v["errors"].as_array().map(|a|a.iter().filter_map(Value::as_str).collect::<Vec<_>>()).unwrap_or_default();
        let errors=if errors.is_empty() {vec!["Kommandoen fejlede uden en specifik fejlbesked."]}else{errors};
        Output {stdout:String::new(),stderr:format!("✘ {} failed\n{}",label(command),errors.iter().map(|s|format!("  → {s}\n")).collect::<String>()),exit}
    }
}
// Syntax validation against the SDK schema before opening credentials. Domain facts remain server-owned.
fn schema_valid(v:&Value,s:&Value)->bool {
    if let Some(c)=s.get("const") { if v!=c {return false;} }
    if let Some(e)=s["enum"].as_array() { if !e.contains(v) {return false;} }
    if let Some(t)=s.get("type") {
        let test=|t:&str| match t {"object"=>v.is_object(),"array"=>v.is_array(),"string"=>v.is_string(),"integer"=>v.is_i64()||v.is_u64(),"boolean"=>v.is_boolean(),"null"=>v.is_null(),_=>false};
        if !t.as_str().map(test).unwrap_or_else(||t.as_array().is_some_and(|a|a.iter().any(|t|t.as_str().is_some_and(test)))) {return false;}
    }
    if let Some(choices)=s["oneOf"].as_array() {if choices.iter().filter(|s|schema_valid(v,s)).count()!=1 {return false;}}
    if let Some(o)=v.as_object() {
        if s["required"].as_array().is_some_and(|a|a.iter().any(|k|!o.contains_key(k.as_str().unwrap()))) {return false;}
        for (k,v) in o {if let Some(p)=s["properties"].get(k) {if !schema_valid(v,p) {return false;}}else if s["additionalProperties"]==false {return false;}}
    }
    if let Some(a)=v.as_array() {if a.len() < s["minItems"].as_u64().unwrap_or(0) as usize || a.len()>s["maxItems"].as_u64().unwrap_or(u64::MAX) as usize {return false;} if let Some(items)=s.get("items") {if !a.iter().all(|v|schema_valid(v,items)) {return false;}}}
    if let Some(t)=v.as_str() {let n=t.encode_utf16().count() as u64;if n<s["minLength"].as_u64().unwrap_or(0) || n>s["maxLength"].as_u64().unwrap_or(u64::MAX) {return false;}}
    if let Some(n)=v.as_f64() {if s["minimum"].as_f64().is_some_and(|b|n<b)||s["maximum"].as_f64().is_some_and(|b|n>b) {return false;}}
    true
}
fn snapshot(v:&Value)->bool {
    v.is_object() && ["snapshotId","source","observedAt","snapshotHash","diffHash","actor"].iter().all(|k|v[*k].as_str().is_some_and(|s|!s.is_empty()))
        && matches!(v["state"].as_str(),Some("proposed"|"approved"|"rejected"|"applied")) && v["diff"].is_object() && v["principal"].is_object() && v["history"].is_array()
}
fn project(mut v:Value,command:&str,date:Option<&&str>)->Option<Value> {
    if !v.is_object() {return None;}
    if v["ok"]==false { return (v["errors"].as_array().is_some_and(|a|!a.is_empty() && a.iter().all(Value::is_string))).then_some(v); }
    if v.get("ok").is_some_and(|ok|ok!=true) {return None;}
    let valid=match command {
        "query"=>v["asOf"].as_str()==date.copied() && v["facts"].is_array() && v["partial"].is_boolean() && v["consolidation"]["eligible"].is_boolean() && v["consolidation"]["reason"].is_string(),
        "propose"|"review"=>snapshot(&v["snapshot"]),
        "apply"=>snapshot(&v["snapshot"]) && matches!(v["status"].as_str(),Some("applied"|"unchanged")),
        "history"=>v["history"].as_array().is_some_and(|a|a.iter().all(snapshot)),
        "projection"=>v["projection"]["eligible"].is_boolean() && v["projection"]["edges"].is_array() && (v["projection"]["eligible"]==true || v["projection"]["reason"].is_string()),_=>false };
    if !valid {return None;}
    if command=="projection" {return Some(json!({"ok":true,"projection":v["projection"]}));}
    v["ok"]=json!(true); Some(v)
}
pub fn execute(args:&[String])->Output {
    let flags=match parse(args) {Ok(f)=>f,Err(o)=>return o};
    if flags.contains_key("--help") {return Output{stdout:format!("{}\n",help()),stderr:String::new(),exit:0};}
    let command=args[1].as_str();let json_mode=flags.contains_key("--json") || flags.get("--format")==Some(&"json");
    let fail=|message:&str|emit(json!({"ok":false,"errors":[message]}),json_mode,command);
    let mut input_actor=None; let mut input_principal=None;
    let action=match command {"propose"=>Mutation::Propose,"review"=>Mutation::Review,_=>Mutation::Apply};
    let mut body=if command=="propose" {
        let path=std::path::Path::new(flags["--input"]);
        let meta=match std::fs::symlink_metadata(path) {Ok(m) if m.is_file() && m.len()<=MAX_OWNERSHIP_PROPOSE_BODY as u64=>m,_=>return usage("Input must be a bounded regular JSON file")};
        let _=meta;
        let mut bytes=vec![];
        if std::fs::File::open(path).and_then(|f|f.take(MAX_OWNERSHIP_PROPOSE_BODY as u64+1).read_to_end(&mut bytes)).is_err() || bytes.len()>MAX_OWNERSHIP_PROPOSE_BODY {return usage("Input file unavailable or exceeds bound");}
        let mut v:Value=match serde_json::from_slice(&bytes) {Ok(v)=>v,Err(_)=>return usage("Input must contain valid JSON")};
        let Some(o)=v.as_object_mut() else{return usage("Input must be a JSON object")};
        if o.contains_key("confirm") {return usage("Input confirmation is unsupported; use exact --confirm yes");}
        input_actor=o.remove("actor");input_principal=o.remove("principal");
        o.insert("confirm".into(),json!(true));v
    } else if write(command) {
        let mut v=json!({"snapshotId":flags["--snapshot-id"],"confirm":true});
        if command=="review" {v["decision"]=json!(flags["--decision"]);}else {v["snapshotHash"]=json!(flags["--snapshot-hash"]);v["diffHash"]=json!(flags["--diff-hash"]);}v
    }else{Value::Null};
    if write(command) && !schema_valid(&body,&ownership_body_schema(action)) {return usage("Input does not match existing ownership body schema; unsupported metadata refused");}
    if input_actor.as_ref().is_some_and(|v|!v.is_string()) || input_principal.as_ref().is_some_and(|v|v.as_object().is_none_or(|o|o.len()!=2 || v["kind"]!="user" || !v["id"].is_string())) {return usage("Input identity must be a current user equality assertion");}
    let api=match crate::remote::credential_client(flags["--server"],flags.contains_key("--allow-loopback-http"),&flags,false) {Ok(a)=>a,Err(_)=>return fail("Ownership private credential or transport refused")};
    let rt=match tokio::runtime::Runtime::new() {Ok(r)=>r,Err(_)=>return fail("Ownership runtime unavailable")};
    let result=rt.block_on(async {
        if flags.contains_key("--actor") || flags.contains_key("--principal-id") || input_actor.is_some() || input_principal.is_some() {
            let me=api.me().await?;let id=me["user"]["id"].as_str().filter(|s|!s.is_empty()).ok_or(Error::Protocol)?;let actor=format!("user:{id}");
            if flags.get("--actor").is_some_and(|v|*v!=actor) || flags.get("--principal-id").is_some_and(|v|*v!=id) || input_actor.as_ref().is_some_and(|v|v.as_str()!=Some(actor.as_str())) || input_principal.as_ref().is_some_and(|v|v["id"].as_str()!=Some(id)) {return Err(Error::InvalidInput("identity assertion differs from current cookie user"));}
        }
        match command {"query"=>api.ownership(flags["--company"],flags["--as-of"]).await,"history"=>api.ownership_history(flags["--company"],flags.get("--snapshot-id").copied(),None).await,"projection"=>api.ownership_history(flags["--company"],None,Some(flags["--as-of"])).await,_=>api.mutate_ownership(flags["--company"],action,serde_json::to_vec(&body.take()).unwrap(),true).await}
    });
    match result {
        Ok(v)=>match project(v,command,flags.get("--as-of")) {Some(v)=>emit(v,json_mode,command),None if write(command)=>fail("Ownership write outcome unknown: incomplete native result; reconcile canonical snapshot/history before further explicit action; no retry performed"),None if command=="projection"=>fail("Ownership projection unavailable: native full projection absent or incomplete; workspace completeness cannot be inferred"),None=>fail("Ownership observation unavailable: incomplete native result")},
        Err(Error::OutcomeUnknown)=>fail("Ownership write outcome unknown; reconcile canonical snapshot/history before further explicit action; no retry performed"),
        Err(Error::InvalidInput(_))=>usage("Ownership input or current user identity assertion refused"),
        Err(_)=>fail("Ownership transport, authorization or protocol refused"),
    }
}
