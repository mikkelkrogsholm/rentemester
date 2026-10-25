use super::*;

fn envelope(row: &Value) -> &Value {
    let r = &row["result"];
    let e = &r["structuredContent"];
    assert_eq!(serde_json::from_str::<Value>(r["content"][0]["text"].as_str().unwrap()).unwrap(), *e);
    assert_eq!(r["isError"], e["ok"] != true);
    e
}

#[test]
fn original_first_use116_company_confirmation_precedes_workspace_and_network() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    for confirm in [None, Some(false)] {
        let mut args = json!({"name":"Synthetic","workspace":"/unverified/workspace"});
        if let Some(c) = confirm { args["confirm"] = json!(c); }
        let rows = parsed(&run(&origin, &[init("2025-11-25"),ready(),call(2,"company_add",args)]));
        assert_eq!(envelope(&rows[1]), &json!({"ok":false,"errors":["confirm: true required for write tool company_add"],"code":"CONFIRM_REQUIRED"}));
    }
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err());
}

#[test]
fn original_first_use116_company_grammar_and_unverified_selector_refuse_without_network() {
    for args in [json!({"name":null}),json!({"name":""}),json!({"name":"Synthetic","confirm":null}),json!({"name":"Synthetic","fiscalYearStartMonth":"7"}),json!({"name":"Synthetic","fiscalYearStartMonth":0}),json!({"name":"Synthetic","fiscalYearStartMonth":13}),json!({"name":"Synthetic","fiscalYearStartMonth":1.5}),json!({"name":"Synthetic","fiscalYearLabelStrategy":"unknown"}),json!({"name":"Synthetic","actor":"user:forged"}),json!({"name":"Synthetic","body":"{}"})] {
        let rows = parsed(&run("http://127.0.0.1:9", &[init("2025-11-25"),ready(),call(2,"company_add",args)]));
        assert_eq!(rows[1]["result"]["isError"], true);
        assert!(rows[1]["result"].get("structuredContent").is_none());
    }
    for workspace in ["/unverified/workspace","relative","synthetic-workspace"] {
        let rows = parsed(&run("http://127.0.0.1:9", &[init("2025-11-25"),ready(),call(2,"company_add",json!({"name":"Synthetic","confirm":true,"workspace":workspace}))]));
        assert_eq!(envelope(&rows[1])["code"], "invalid_input");
    }
}

#[test]
fn original_first_use116_company_all_fields_success_current_refusal_and_unknown() {
    for (status, reply, expected) in [
        ("201 Created",r#"{"ok":true,"company":{"slug":"synthetic-new","name":"Synthetic new","extra":7}}"#, "success"),
        ("403 Forbidden",r#"{"ok":false,"errors":["current permission revoked"],"code":"MCP_UNAUTHORIZED","appliedRules":["current-scope"]}"#, "MCP_UNAUTHORIZED"),
        ("409 Conflict",r#"{"error":"duplicate"}"#, "server_refused"),
        ("503 Unavailable","", "outcome_unknown"),
        ("201 Created","{", "outcome_unknown"),
    ] {
        let response=format!("HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",reply.len());
        let (origin, request, task)=http(Box::leak(response.into_boxed_str()));
        let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"company_add",json!({"name":" Synthetic new ","slug":" synthetic-new ","cvr":"DK12345678","fiscalYearStartMonth":7.0,"fiscalYearLabelStrategy":"span","confirm":true}))]));
        let e=envelope(&rows[1]);
        if expected=="success" { assert_eq!(e,&json!({"ok":true,"data":{"slug":"synthetic-new","name":"Synthetic new"},"errors":[]})); }
        else { assert_eq!(e["code"],expected); }
        if expected=="MCP_UNAUTHORIZED" { assert_eq!(e, &serde_json::from_str::<Value>(reply).unwrap()); }
        let wire=String::from_utf8(request.recv().unwrap()).unwrap();
        assert!(wire.starts_with("POST /api/companies HTTP/1.1"));
        assert!(wire.to_ascii_lowercase().contains("cookie: session=never-print-this"));
        let b=wire.split_once("\r\n\r\n").unwrap().1;
        assert_eq!(serde_json::from_str::<Value>(b).unwrap(),json!({"name":" Synthetic new ","slug":"synthetic-new","cvr":"DK12345678","fiscalYearStartMonth":"7","fiscalYearLabelStrategy":"span"}));
        task.join().unwrap();
    }
}

#[test]
fn original_first_use116_company_catalogue_retains_source_schema_and_hints() {
    let rows=parsed(&run("http://127.0.0.1:9", &[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})]));
    let t=rows[1]["result"]["tools"].as_array().unwrap().iter().find(|t|t["name"]=="company_add").unwrap();
    assert_eq!(t["annotations"],json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}));
    assert_eq!(t["inputSchema"]["required"],json!(["name"]));
    assert_eq!(t["inputSchema"]["properties"]["fiscalYearStartMonth"]["type"],"integer");
    assert_eq!(t["inputSchema"]["additionalProperties"],false);
}

#[cfg(unix)]
#[test]
fn original_first_use116_company_verified_local_profile_success_and_mismatch() {
    use std::{fs,os::unix::{fs::{DirBuilderExt,PermissionsExt},net::UnixListener}};
    let root=fs::canonicalize(std::env::var_os("HOME").unwrap()).unwrap().join(format!(".rm116-mcp-company-{}",std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    fs::DirBuilder::new().mode(0o700).create(root.join("workspace")).unwrap();
    let listener=UnixListener::bind(root.join("r.sock")).unwrap();
    fs::set_permissions(root.join("r.sock"),fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(root.join("r.json"),json!({"format":"rentemester-local-runtime/1","origin":"http://127.0.0.1:9","socket":"r.sock","workspaceRoot":root.join("workspace"),"workspaceId":root.join("other")}).to_string()).unwrap();
    fs::set_permissions(root.join("r.json"),fs::Permissions::from_mode(0o600)).unwrap();
    let task=thread::spawn(move || {
        let(mut s,_)=listener.accept().unwrap();let mut b=vec![];
        loop {let mut buf=[0;4096];let n=s.read(&mut buf).unwrap();assert!(n>0);b.extend_from_slice(&buf[..n]);if b.ends_with(br#"{"name":"Synthetic"}"#) {break;}}
        assert!(String::from_utf8(b).unwrap().starts_with("POST /api/companies HTTP/1.1"));
        let reply=r#"{"ok":true,"company":{"slug":"synthetic","name":"Synthetic"}}"#;
        write!(s,"HTTP/1.1 201 Created\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",reply.len()).unwrap();drop(s);
        listener.set_nonblocking(true).unwrap();thread::sleep(std::time::Duration::from_millis(50));assert!(listener.accept().is_err());
    });
    let cookie=Cookie::new();
    let mut child=Command::new(BIN).args(["--local-profile",root.to_str().unwrap(),"--cookie-file",cookie.0.to_str().unwrap()]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let messages=[init("2025-11-25"),ready(),call(2,"company_add",json!({"name":"Synthetic","confirm":true,"workspace":root.join("other")})),call(3,"company_add",json!({"name":"Synthetic","confirm":true,"workspace":root.join("workspace")}))];
    let lines:String=messages.iter().map(|v|format!("{v}\n")).collect();
    child.stdin.take().unwrap().write_all(lines.as_bytes()).unwrap();
    let out=child.wait_with_output().unwrap();let rows=parsed(&out);
    assert_eq!(envelope(&rows[1])["code"],"invalid_input");
    assert_eq!(envelope(&rows[2]),&json!({"ok":true,"data":{"slug":"synthetic","name":"Synthetic"},"errors":[]}));
    task.join().unwrap();fs::remove_dir_all(root).unwrap();
}
