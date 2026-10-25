use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, BookkeepingBatchMutation as Op, Error, MAX_BOOKKEEPING_BATCH_BODY,
    OriginalBatchOperation, bookkeeping_batch_original_schema,
    bookkeeping_batch_body_schema, bookkeeping_batch_scope_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "bookkeeping_workbench"
            | "bookkeeping_batch_plan"
            | "bookkeeping_batch_status"
            | "bookkeeping_batch_persist"
            | "bookkeeping_batch_approve"
            | "bookkeeping_batch_apply"
            | "bookkeeping_batch_dry_run"
    )
}
pub fn tools() -> Vec<Value> {
    let mut out = vec![
        tool(
            "bookkeeping_batch_plan",
            "Read-only complete native source plan; explicit accounting/bank date ranges. No persistence or inferred readiness.",
            json!({"type":"object","properties":{"company":company_schema(),"scope":bookkeeping_batch_scope_schema()},"required":["company","scope"],"additionalProperties":false}),
            true,
        ),
        tool(
            "bookkeeping_batch_status",
            "Read immutable revisions/approval/receipts/checks by explicit runId OR exact runKey using current company read authority; no retry/apply.",
            json!({"type":"object","properties":{"company":company_schema(),"runId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"runKey":{"type":"string","minLength":1,"maxLength":1024}},"required":["company"],"oneOf":[{"required":["runId"]},{"required":["runKey"]}],"additionalProperties":false}),
            true,
        ),
    ];
    for (name, op) in [
        ("bookkeeping_batch_persist", Op::Persist),
        ("bookkeeping_batch_approve", Op::Approve),
        ("bookkeeping_batch_apply", Op::Apply),
    ] {
        let mut body = native_body(bookkeeping_batch_body_schema(op));
        body["maxLength"] = json!(MAX_BOOKKEEPING_BATCH_BODY);
        out.push(tool(name,"Exact native32KiB JSON with inner+outer confirm. Persist requires caller runKey and reviewed expectedPlanHash; approve/apply runId+planHash. Current principal/independent approval/atomic_v1 source checks/fences remain server-owned. Legacy partial history inspect-only. No generated keys/hashes/defaults/math, silent skip, local files or automatic retry. Unknown outcome requires canonical status before manual action; known refusal JSON preserved.",json!({"type":"object","properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"required":["company","body","confirm"],"additionalProperties":false}),false));
    }
    // Status has identical source/native input. Keep ONE canonical native envelope,
    // including data.ok; a second oneOf arm would make the schema impossible.
    for descriptor in &mut out {
        let name = descriptor["name"].as_str().unwrap();
        if name == "bookkeeping_batch_status" {
            descriptor["description"] = json!("Read canonical native {ok,state} by runId OR exact runKey. Identical source {company,runId} deliberately retains native MCP envelope including data.ok. Immutable revisions/approvals/attempts/receipts/finalChecks and legacy inspect-only history; current authority, no retry.");
        } else {
            let op = original_operation(name).unwrap();
            let native = descriptor["inputSchema"].take();
            descriptor["inputSchema"] = json!({"type":"object","oneOf":[native,original_schema(op)]});
            let old = descriptor["description"].as_str().unwrap().to_owned();
            descriptor["description"] = json!(format!("{old} Alternative original flat SDK fields plus company; never mix with native scope/body. Explicit reviewed hash required; current human authority and independent approval, atomic_v1, legacy inspect-only; no automatic retry. Source result lifts core ok/errors to outer envelope."));
        }
    }
    let mut dry_run = tool(
        "bookkeeping_batch_dry_run",
        "Durable dry-run WRITES an immutable reviewed revision, not a read-only preview. Flat four dates, runKey, explicit reviewed expectedPlanHash and confirm:true required. Same-key exact duplicate reuses persisted state. Current human authority, independent approval and atomic_v1 remain server-owned; legacy inspect-only, no generated hashes or automatic retry.",
        original_schema(OriginalBatchOperation::DryRun),
        false,
    );
    dry_run["annotations"]["destructiveHint"] = json!(false);
    dry_run["annotations"]["idempotentHint"] = json!(true);
    out.push(dry_run);
    let mut schema = ApiClient::bookkeeping_workbench_schema();
    schema["properties"]["company"] = json!({"type":"string","minLength":1});
    schema["required"].as_array_mut().unwrap().push(json!("company"));
    let mut workbench = tool("bookkeeping_workbench", "Read the deterministic unresolved bank-work population with canonical party-resolution, evidence and posting facets. Population blockers are independent of page filters; this never persists, approves or applies a batch. Company is a current server slug, or exactly one company directory under a freshly verified explicit local workspace. Absolute filesystem selectors require --local-profile; no remote path guessing or fallback.", schema, true);
    workbench["title"] = json!("Read bookkeeping workbench");
    workbench["outputSchema"] = json!({"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"ok":{"type":"boolean","description":"true ⇒ kaldet lykkedes og `data` er sat; false ⇒ se `errors`."},"data":{"description":"Kerne-resultatet ved ok=true. Den præcise feltliste er per-tool og dokumenteret i docs/mcp-tool-surface.md. Udeladt ved ok=false.","type":"object","properties":{},"additionalProperties":{}},"errors":{"type":"array","items":{"type":"string"},"description":"Menneskelæsbare fejl-/forudsætnings-strenge. Tom ved ok=true; ikke-tom ved ok=false."},"appliedRules":{"description":"Regel-id'er der fyrede for denne handling (sporbarhed). Sættes for bogførings-tools; udeladt ellers.","type":"array","items":{"type":"string"}},"code":{"description":"Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx \"BACKUP_LOCKED\"). Sættes kun ved ok=false og kun for fejl der lever uden for det enkelte tools forretningslogik — agenten kan branche på `code` uden at parse fri-tekst i `errors[]`. Udeladt ved per-tool forretningsfejl.","type":"string"}},"required":["ok","errors"],"additionalProperties":false});
    workbench["annotations"]["openWorldHint"] = json!(false);
    out.push(workbench);
    out
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    company: String,
    scope: Value,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Status {
    company: String,
    run_id: Option<i64>,
    run_key: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    company: String,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "bookkeeping_batch_plan" {
        let v: Plan =
            serde_json::from_value(args).map_err(|_| "batch plan accepts company/scope only")?;
        return Ok(api.bookkeeping_batch_plan(&v.company, &v.scope).await);
    }
    if name == "bookkeeping_batch_status" {
        let v: Status =
            serde_json::from_value(args).map_err(|_| "batch status accepts company/runId only")?;
        return match (v.run_id, v.run_key) {
            (Some(id), None) => Ok(api.bookkeeping_batch_status(&v.company, id).await),
            (None, Some(key)) => Ok(api.bookkeeping_batch_status_by_key(&v.company, &key).await),
            _ => Err("status requires exactly one runId or runKey"),
        };
    }
    let v: Write = serde_json::from_value(args)
        .map_err(|_| "batch mutation accepts company/body/confirm only")?;
    if v.body.len() > MAX_BOOKKEEPING_BATCH_BODY {
        return Err("native batch body exceeds32KiB");
    }
    let op = match name {
        "bookkeeping_batch_persist" => Op::Persist,
        "bookkeeping_batch_approve" => Op::Approve,
        "bookkeeping_batch_apply" => Op::Apply,
        _ => return Err("unknown batch operation"),
    };
    Ok(api
        .mutate_bookkeeping_batch(&v.company, op, v.body.into_bytes(), v.confirm)
        .await)
}


fn original_operation(name: &str) -> Option<OriginalBatchOperation> {
    match name {
        "bookkeeping_batch_plan" => Some(OriginalBatchOperation::Plan),
        "bookkeeping_batch_persist" => Some(OriginalBatchOperation::Persist),
        "bookkeeping_batch_dry_run" => Some(OriginalBatchOperation::DryRun),
        "bookkeeping_batch_approve" => Some(OriginalBatchOperation::Approve),
        "bookkeeping_batch_apply" => Some(OriginalBatchOperation::Apply),
        _ => None,
    }
}
fn original_schema(op: OriginalBatchOperation) -> Value {
    let mut schema = bookkeeping_batch_original_schema(op);
    schema["properties"]["company"] = company_schema();
    schema["required"].as_array_mut().unwrap().push(json!("company"));
    schema
}
/// A discriminator selects only disjoint forms. Mixed arguments still reach a
/// deny-unknown-fields parser, never silently discard the other dialect.
pub fn is_original(name: &str, args: &Value) -> bool {
    match name {
        "bookkeeping_workbench" => true,
        "bookkeeping_batch_plan" => args.get("scope").is_none(),
        "bookkeeping_batch_persist" | "bookkeeping_batch_approve" | "bookkeeping_batch_apply" => args.get("body").is_none(),
        "bookkeeping_batch_dry_run" => true,
        _ => false,
    }
}
fn result(value: Value) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":value["ok"]!=true})
}
fn failure(error: Error) -> Value {
    let code = match error {
        Error::InvalidInput(_) => "invalid_input",
        Error::Refused { .. } => "server_refused",
        Error::Transport => "transport",
        Error::Protocol => "protocol",
        Error::OutcomeUnknown => "outcome_unknown",
    };
    result(json!({"ok":false,"errors":[error.to_string()],"code":code}))
}
// Validate RPC grammar before transport. The SDK repeats semantic validation;
// its transport/protocol failures must remain tool failures, not parse errors.
fn original_input(name: &str, args: Value) -> Result<(String, Value), &'static str> {
    let op = original_operation(name).ok_or("unknown original batch operation")?;
    let schema = original_schema(op);
    let mut fields = args.as_object().ok_or("expected flat batch object")?.clone();
    for key in schema["required"].as_array().unwrap() {
        if !fields.contains_key(key.as_str().unwrap()) {
            return Err("required original batch field absent");
        }
    }
    for (key, value) in &fields {
        let property = schema["properties"].get(key).ok_or("unknown batch field; do not mix native/source forms or supply identity")?;
        match key.as_str() {
            "confirm" => { if !value.is_boolean() { return Err("confirm must be boolean"); } }
            "companyId" | "runId" => {
                if value.as_i64().is_none_or(|n| !(1..=9_007_199_254_740_991).contains(&n)) {
                    return Err("positive safe integer required");
                }
            }
            _ => {
                let text = value.as_str().ok_or("batch string required")?;
                if text.len() < property["minLength"].as_u64().unwrap_or(0) as usize
                    || text.len() > property["maxLength"].as_u64().unwrap_or(u64::MAX) as usize {
                    return Err("batch string outside bounds");
                }
                match key.as_str() {
                    "company" if text.is_empty() || !text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') => return Err("explicit company slug required"),
                    "runKey" if text.trim().is_empty() || text.len() > 1024 || text.chars().any(char::is_control) => return Err("bounded exact runKey required"),
                    "planHash" | "expectedPlanHash" if text.len()!=64 || !text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) => return Err("reviewed lowercase SHA256 required"),
                    "accountingFrom" | "accountingTo" | "bankFrom" | "bankTo" => {
                        rentemester_engine::fiscal_year::Date::parse(text).map_err(|_| "actual calendar dates required")?;
                    }
                    _ => {}
                }
            }
        }
    }
    for (from, to) in [("accountingFrom", "accountingTo"), ("bankFrom", "bankTo")] {
        if fields.get(from).and_then(Value::as_str) > fields.get(to).and_then(Value::as_str) {
            return Err("ordered date ranges required");
        }
    }
    let company = fields.remove("company").unwrap().as_str().unwrap().to_owned();
    let input = Value::Object(fields);
    if serde_json::to_vec(&input).map_err(|_| "invalid batch JSON")?.len() > MAX_BOOKKEEPING_BATCH_BODY {
        return Err("original batch input exceeds32KiB");
    }
    Ok((company, input))
}
pub async fn call_original(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    if name == "bookkeeping_workbench" {
        let invalid = || json!({"isError":true,"content":[{"type":"text","text":"MCP error -32602: Input validation error: Invalid arguments for tool bookkeeping_workbench"}]});
        let Some(mut fields) = args.as_object().cloned() else { return Ok(invalid()); };
        let Some(company) = fields.remove("company").and_then(|v| v.as_str().map(str::to_owned)).filter(|s| !s.is_empty()) else { return Ok(invalid()); };
        let input = Value::Object(fields);
        if ApiClient::validate_bookkeeping_workbench(&input).is_err() { return Ok(invalid()); }
        return Ok(match api.bookkeeping_workbench(&company, &input).await {
            Ok(mut core) => {
                let fields = core.as_object_mut().ok_or("invalid workbench response")?;
                let ok = fields.remove("ok").unwrap_or(json!(false));
                let errors = fields.remove("errors").unwrap_or(json!([]));
                let code = fields.remove("code");
                let rules = fields.remove("appliedRules");
                let mut envelope = json!({"ok":ok,"errors":errors});
                if !fields.is_empty() { envelope["data"] = core; }
                if let Some(code) = code { envelope["code"] = code; }
                if let Some(rules) = rules { envelope["appliedRules"] = rules; }
                result(envelope)
            }
            Err(error) => failure(error),
        });
    }
    let op = original_operation(name).ok_or("unknown original batch operation")?;
    let (company, input) = original_input(name, args)?;
    if input.get("confirm").is_some_and(|v| v == false) {
        return Ok(result(json!({"ok":false,"errors":["confirm:true required"],"code":"CONFIRM_REQUIRED"})));
    }
    Ok(match api.bookkeeping_batch_original(&company, op, input).await {
        Ok(mut data) => {
            let ok = data.as_object_mut().and_then(|o| o.remove("ok")).unwrap_or(json!(false));
            let errors = data.as_object_mut().and_then(|o| o.remove("errors")).unwrap_or(json!([]));
            result(json!({"ok":ok,"errors":errors,"data":data}))
        }
        Err(error) => failure(error),
    })
}

#[cfg(test)]
mod original_tests {
    use super::*;
    use std::{io::{Read, Write}, net::TcpListener, thread};
    fn server(status: u16, value: Value) -> (String, thread::JoinHandle<(String, Value)>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", l.local_addr().unwrap());
        let t = thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            let end = loop {
                let mut b = [0; 1024];
                let n = s.read(&mut b).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
                if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let header = String::from_utf8_lossy(&bytes[..end]).to_string();
            let length: usize = header
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap())
                })
                .unwrap_or(0);
            while bytes.len() < end + length {
                let mut b = [0; 1024];
                let n = s.read(&mut b).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
            }
            let body = if length == 0 {
                Value::Null
            } else {
                serde_json::from_slice(&bytes[end..end + length]).unwrap()
            };
            let response = value.to_string();
            write!(
                s,
                "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{response}",
                response.len()
            )
            .unwrap();
            drop(s);
            // Any automatic retry/extra request violates this transport contract.
            l.set_nonblocking(true).unwrap();
            thread::sleep(std::time::Duration::from_millis(60));
            assert!(l.accept().is_err(), "unexpected retry or extra transport call");
            (header, body)
        });
        (origin, t)
    }

    fn scope() -> Value {
        json!({"accountingFrom":"2026-01-01","accountingTo":"2026-01-31","bankFrom":"2026-01-01","bankTo":"2026-01-31"})
    }
    fn source(name: &str) -> Value {
        let mut input = if matches!(name,"bookkeeping_batch_approve"|"bookkeeping_batch_apply"|"bookkeeping_batch_status") {
            json!({"runId":7})
        } else { scope() };
        input["company"] = json!("synthetic");
        if matches!(name,"bookkeeping_batch_persist"|"bookkeeping_batch_dry_run") {
            input["runKey"] = json!(" reviewed key ");
            input["expectedPlanHash"] = json!("a".repeat(64));
            input["confirm"] = json!(true);
        }
        if matches!(name,"bookkeeping_batch_approve"|"bookkeeping_batch_apply") {
            input["planHash"] = json!("a".repeat(64));
            input["confirm"] = json!(true);
        }
        input
    }
    fn plan() -> Value {
        let mut p = scope();
        p["companyId"] = json!(1);
        p["executionPolicy"] = json!("atomic_v1");
        p["items"] = json!([]);
        p["sourceIdentities"] = json!({"candidateUniverse":{}});
        p["candidateSetHash"] = json!("b".repeat(64));
        p["planHash"] = json!("a".repeat(64));
        p
    }
    fn state() -> Value {
        json!({"run":{"runId":7,"runKey":" reviewed key ","planHash":"a".repeat(64),"plan":plan().to_string(),"createdAt":"2026-01-31T12:00:00Z"},
            "revisions":[{"revisionId":9,"planHash":"a".repeat(64),"candidateSetHash":"b".repeat(64),"plannerKind":"user","plannerSubjectId":"planner","plannerActor":"user:planner","approverKind":"user","approverSubjectId":"reviewer","approverActor":"user:reviewer"}],
            "attempts":[{"attemptId":11,"planHash":"a".repeat(64),"principalKind":"user","principalSubjectId":"planner","actor":"user:planner","eventType":"applied"}],
            "receipts":[{"actionKey":"bank:3","receipt":"{\"entryId\":13}"}],
            "finalChecks":[{"attemptId":11,"name":"trialBalance","ok":1,"detail":"{}"}],
            "legacyEvents":[{"actor":"agent:historical","event_type":"planned"}],"legacyFinalChecks":[]})
    }
    fn response(name: &str) -> Value {
        match name {
            "bookkeeping_batch_plan" => json!({"ok":true,"dryRun":true,"plan":plan()}),
            "bookkeeping_batch_persist" | "bookkeeping_batch_dry_run" => json!({"ok":true,"dryRun":true,"runId":7,"duplicate":true,"plan":plan(),"state":state()}),
            "bookkeeping_batch_apply" => json!({"ok":true,"runId":7,"planHash":"a".repeat(64),"duplicate":true,"applyAttemptId":11,"results":[{"actionKey":"bank:3","receipt":{"entryId":13}}],"checks":[{"name":"trialBalance","ok":true}],"state":state()}),
            _ => json!({"ok":true,"state":state()}),
        }
    }
    fn agreement(result: &Value) {
        assert_eq!(serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),result["structuredContent"]);
        assert_eq!(result["isError"],result["structuredContent"]["ok"]!=true);
    }
    #[test]
    fn registry_has_only_required_alias_and_disjoint_schemas() {
        let catalogue = crate::tools::list();
        let all = catalogue["tools"].as_array().unwrap();
        assert_eq!(all.len(),434);
        let names = all.iter().map(|t|t["name"].as_str().unwrap()).collect::<std::collections::HashSet<_>>();
        assert_eq!(names.len(),434);
        let batch = tools();
        assert_eq!(batch.len(),7);
        for t in &batch {
            let name = t["name"].as_str().unwrap();
            assert!(accepts(name));
            if name == "bookkeeping_workbench" {
                assert_eq!(t["annotations"]["openWorldHint"],false);
            } else if name == "bookkeeping_batch_dry_run" {
                assert_eq!(t["annotations"]["readOnlyHint"],false);
                assert_eq!(t["annotations"]["destructiveHint"],false);
                assert_eq!(t["annotations"]["idempotentHint"],true);
                assert!(t["inputSchema"]["required"].as_array().unwrap().contains(&json!("expectedPlanHash")));
            } else if name == "bookkeeping_batch_status" {
                assert_eq!(t["inputSchema"]["oneOf"],json!([{"required":["runId"]},{"required":["runKey"]}]));
                assert_eq!(t["annotations"]["readOnlyHint"],true);
                assert!(!is_original(name,&source(name)));
                assert!(t["description"].as_str().unwrap().contains("data.ok"));
            } else {
                assert_eq!(t["inputSchema"]["oneOf"].as_array().unwrap().len(),2);
                for arm in t["inputSchema"]["oneOf"].as_array().unwrap() {
                    assert_eq!(arm["additionalProperties"],false);
                }
            }
        }
    }
    #[test]
    fn malformed_mixed_and_forged_arguments_are_parse_errors_without_transport() {
        let api=ApiClient::new("http://127.0.0.1:9",true,None).unwrap();
        let rt=tokio::runtime::Runtime::new().unwrap();
        for name in ["bookkeeping_batch_plan","bookkeeping_batch_persist","bookkeeping_batch_dry_run","bookkeeping_batch_approve","bookkeeping_batch_apply"] {
            for (field,value) in [("actor",json!("user:forged")),("principal",json!({})),("body",json!("{}")),("scope",scope()),("company",Value::Null),("companyId",json!(9007199254740992u64))] {
                let mut args=source(name);args[field]=value;
                assert!(rt.block_on(crate::tools::call(&api,name,args)).is_err(),"{name}/{field}");
            }
            let mut args=source(name);args.as_object_mut().unwrap().remove("company");
            assert!(rt.block_on(crate::tools::call(&api,name,args)).is_err());
        }
        for (field,value) in [("accountingFrom",json!("2026-02-30")),("bankTo",json!("2025-12-31")),("expectedPlanHash",json!("A".repeat(64))),("runKey",json!("x".repeat(1025))),("confirm",Value::Null)] {
            let mut args=source("bookkeeping_batch_persist");args[field]=value;
            assert!(rt.block_on(crate::tools::call(&api,"bookkeeping_batch_persist",args)).is_err());
        }
        for name in ["bookkeeping_batch_persist","bookkeeping_batch_dry_run","bookkeeping_batch_approve","bookkeeping_batch_apply"] {
            let mut args=source(name);args["confirm"]=json!(false);
            let refusal=rt.block_on(crate::tools::call(&api,name,args)).unwrap();
            agreement(&refusal);assert_eq!(refusal["structuredContent"]["code"],"CONFIRM_REQUIRED");
            let mut args=source(name);args.as_object_mut().unwrap().remove("confirm");
            assert!(rt.block_on(crate::tools::call(&api,name,args)).is_err());
        }
        for id in [json!(0),json!(9007199254740992u64),json!("7")] {
            let mut args=source("bookkeeping_batch_apply");args["runId"]=id;
            assert!(rt.block_on(crate::tools::call(&api,"bookkeeping_batch_apply",args)).is_err());
        }
    }
    #[test]
    fn six_source_calls_use_real_native_transport_and_exact_projection() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for name in ["bookkeeping_batch_plan","bookkeeping_batch_persist","bookkeeping_batch_dry_run","bookkeeping_batch_approve","bookkeeping_batch_apply","bookkeeping_batch_status"] {
                let native=response(name);let(origin,t)=server(200,native.clone());
                let api=ApiClient::new(&origin,true,None).unwrap();
                let result=crate::tools::call(&api,name,source(name)).await.unwrap();agreement(&result);
                let mut expected=native.clone();
                if name=="bookkeeping_batch_status" {
                    assert_eq!(result["structuredContent"],json!({"ok":true,"data":native}));
                } else {
                    expected.as_object_mut().unwrap().remove("ok");
                    if name=="bookkeeping_batch_persist" {expected.as_object_mut().unwrap().remove("dryRun");}
                    assert_eq!(result["structuredContent"],json!({"ok":true,"errors":[],"data":expected}));
                }
                let(header,body)=t.join().unwrap();
                if name=="bookkeeping_batch_plan" {
                    assert!(header.starts_with("GET /api/companies/synthetic/bookkeeping-batch?"));
                    for key in ["accountingFrom","accountingTo","bankFrom","bankTo"] {assert!(header.contains(key));}
                } else if name=="bookkeeping_batch_status" {
                    assert!(header.starts_with("GET /api/companies/synthetic/bookkeeping-batch/runs/7 "));
                } else {
                    let suffix=if name=="bookkeeping_batch_dry_run" {"persist"} else {name.strip_prefix("bookkeeping_batch_").unwrap()};
                    assert!(header.starts_with(&format!("POST /api/companies/synthetic/bookkeeping-batch/{suffix} ")));
                    let mut expected=source(name);expected.as_object_mut().unwrap().remove("company");
                    assert_eq!(body,expected);
                }
            }
        });
    }
    #[test]
    fn all_five_native_forms_keep_exact_envelopes_and_bytes() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for name in ["bookkeeping_batch_plan","bookkeeping_batch_persist","bookkeeping_batch_approve","bookkeeping_batch_apply","bookkeeping_batch_status"] {
                let native=response(name);let(origin,t)=server(200,native.clone());
                let api=ApiClient::new(&origin,true,None).unwrap();
                let args=if name=="bookkeeping_batch_plan" {json!({"company":"synthetic","scope":scope()})}
                    else if name=="bookkeeping_batch_status" {json!({"company":"synthetic","runId":7})}
                    else {let mut input=source(name);input.as_object_mut().unwrap().remove("company");json!({"company":"synthetic","body":input.to_string(),"confirm":true})};
                let result=crate::tools::call(&api,name,args.clone()).await.unwrap();agreement(&result);
                assert_eq!(result["structuredContent"],json!({"ok":true,"data":native}));
                let(_,body)=t.join().unwrap();
                if args.get("body").is_some() {assert_eq!(body,serde_json::from_str::<Value>(args["body"].as_str().unwrap()).unwrap());}
            }
            let native=response("bookkeeping_batch_status");let(origin,t)=server(200,native.clone());
            let api=ApiClient::new(&origin,true,None).unwrap();
            let result=crate::tools::call(&api,"bookkeeping_batch_status",json!({"company":"synthetic","runKey":" reviewed key "})).await.unwrap();
            assert_eq!(result["structuredContent"],json!({"ok":true,"data":native}));
            assert!(t.join().unwrap().0.contains("runKey=+reviewed+key+"));
        });
    }
    #[test]
    fn native_confirmation_false_stays_native_tool_failure() {
        let api=ApiClient::new("http://127.0.0.1:9",true,None).unwrap();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for name in ["bookkeeping_batch_persist","bookkeeping_batch_approve","bookkeeping_batch_apply"] {
                let mut input=source(name);input.as_object_mut().unwrap().remove("company");
                let result=crate::tools::call(&api,name,json!({"company":"synthetic","body":input.to_string(),"confirm":false})).await.unwrap();
                agreement(&result);
                assert_eq!(result["structuredContent"]["error"]["code"],"invalid_input");
                assert!(result["structuredContent"].get("errors").is_none());
            }
        });
    }
    #[test]
    fn business_refusals_and_unknown_writes_are_tool_errors_not_parse_errors() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for status in [409,422] {
                let refusal=json!({"ok":false,"code":"bookkeeping_batch_refused","errors":["reviewed source changed"],"state":state()});
                let(origin,t)=server(status,refusal.clone());let api=ApiClient::new(&origin,true,None).unwrap();
                let result=crate::tools::call(&api,"bookkeeping_batch_apply",source("bookkeeping_batch_apply")).await.unwrap();agreement(&result);
                assert_eq!(result["structuredContent"]["ok"],false);
                assert_eq!(result["structuredContent"]["errors"],refusal["errors"]);
                assert_eq!(result["structuredContent"]["data"]["code"],refusal["code"]);
                assert!(result["structuredContent"]["data"].get("ok").is_none());
                t.join().unwrap();
            }
            let(origin,t)=server(503,json!({"ok":false,"code":"internal"}));let api=ApiClient::new(&origin,true,None).unwrap();
            let result=crate::tools::call(&api,"bookkeeping_batch_apply",source("bookkeeping_batch_apply")).await.unwrap();agreement(&result);
            assert_eq!(result["structuredContent"]["code"],"outcome_unknown");
            t.join().unwrap();
        });
    }
}
