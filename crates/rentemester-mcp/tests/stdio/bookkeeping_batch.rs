use super::*;
#[test]
fn five_batch_tools_have_explicit_read_scope_and_confirmed_exact_bodies() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init("2025-11-25"),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), EXPECTED_NATIVE_TOOLS);
    for (name, read) in [
        ("bookkeeping_batch_plan", true),
        ("bookkeeping_batch_status", true),
        ("bookkeeping_batch_persist", false),
        ("bookkeeping_batch_approve", false),
        ("bookkeeping_batch_apply", false),
    ] {
        let t = tools.iter().find(|t| t["name"] == name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"], read);
        let input_schema = &t["inputSchema"];
        let schema = if name == "bookkeeping_batch_status" {
            input_schema
        } else {
            input_schema
                .get("oneOf")
                .and_then(|variants| variants.get(0))
                .unwrap_or(input_schema)
        };
        assert_eq!(schema["additionalProperties"], false);
        if !read {
            assert_eq!(schema["properties"]["body"]["maxLength"], 32768);
            assert_eq!(schema["properties"]["confirm"]["const"], true);
        }
    }
    for (name, args) in [
        (
            "bookkeeping_batch_plan",
            json!({"company":"synthetic","scope":{},"confirm":true}),
        ),
        (
            "bookkeeping_batch_status",
            json!({"company":"synthetic","runId":1,"runKey":"both"}),
        ),
        ("bookkeeping_batch_status", json!({"company":"synthetic"})),
        (
            "bookkeeping_batch_apply",
            json!({"company":"synthetic","body":"{}","confirm":true,"actor":"forged"}),
        ),
    ] {
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[init("2025-11-25"), ready(), call(2, name, args)],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602);
    }
}
#[test]
fn batch_confirmed_apply_unknown_is_sent_once_without_rewriting() {
    let body = format!(
        r#" {{ "runId":7,"planHash":"{}","confirm":true }} "#,
        "a".repeat(64)
    );
    let (origin, request, task) =
        http("HTTP/1.1 503 Service Unavailable\r\nContent-Length:0\r\nConnection:close\r\n\r\n");
    let rows = parsed(&run(
        &origin,
        &[
            init("2025-11-25"),
            ready(),
            call(
                2,
                "bookkeeping_batch_apply",
                json!({"company":"synthetic","body":body,"confirm":true}),
            ),
        ],
    ));
    assert_eq!(
        rows[1]["result"]["structuredContent"]["error"]["code"],
        "outcome_unknown"
    );
    let wire = request.recv().unwrap();
    assert!(wire.starts_with(b"POST /api/companies/synthetic/bookkeeping-batch/apply HTTP/1.1"));
    assert!(wire.ends_with(body.as_bytes()));
    task.join().unwrap();
}

fn original_first_use116_workbench_args() -> Value {
    json!({"company":"synthetic","from":"2026-01-01","to":"2026-01-31","status":"ready",
        "bankAccountId":7.0,"partyId":"party:synthetic","documentQuality":"matched","account":"1234",
        "vatTreatment":"purchase & VAT","dimension":"project:one","cursor":2.0,"limit":3.0,"search":"synthetic / æ & ?"})
}
fn original_first_use116_agreement(value: &Value) {
    assert_eq!(serde_json::from_str::<Value>(value["content"][0]["text"].as_str().unwrap()).unwrap(),value["structuredContent"]);
    assert_eq!(value["isError"],value["structuredContent"]["ok"]!=true);
}
#[test]
fn original_first_use116_workbench_discovery_exact_grammar_and_hints() {
    let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})]));
    let tool=rows[1]["result"]["tools"].as_array().unwrap().iter().find(|t|t["name"]=="bookkeeping_workbench").unwrap();
    assert_eq!(tool["annotations"],json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}));
    let schema=&tool["inputSchema"];
    assert_eq!(schema["additionalProperties"],false);
    assert_eq!(schema["required"],json!(["from","to","company"]));
    assert_eq!(schema["properties"].as_object().unwrap().len(),13);
    for key in original_first_use116_workbench_args().as_object().unwrap().keys() { assert!(schema["properties"].get(key).is_some(),"{key}"); }
    assert_eq!(tool["outputSchema"]["required"],json!(["ok","errors"]));
    assert_eq!(tool["outputSchema"]["additionalProperties"],false);
    assert_eq!(schema["properties"]["limit"]["maximum"],100);
    assert_eq!(schema["properties"]["search"]["maxLength"],200);
    assert_eq!(schema["properties"]["status"]["enum"].as_array().unwrap().len(),9);
}
#[test]
fn original_first_use116_workbench_all_filters_preserve_population_and_original_envelope() {
    let core=json!({"ok":true,"workbench":{"rows":[],"population":{"total":9,"ready":2,"blockers":7},"selection":{"total":0,"ready":0,"blockers":0},"page":{"cursor":2.0,"limit":3.0,"total":0,"nextCursor":null},"sourceHash":"canonical-source","staleSources":[{"changedSource":"evidence_changed"}],"plan":{"planHash":"reviewed"},"periodClose":{"status":"unavailable","blockers":null},"future":{"amount":"90071992547409.99"}}});
    let body=core.to_string();
    let response=Box::leak(format!("HTTP/1.1 200 OK\r\nContent-Type:application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",body.len()).into_boxed_str());
    let (origin,request,task)=http(response);
    let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"bookkeeping_workbench",original_first_use116_workbench_args())]));
    let result=&rows[1]["result"];original_first_use116_agreement(result);
    assert_eq!(result["structuredContent"],json!({"ok":true,"errors":[],"data":{"workbench":core["workbench"]}}));
    let wire=String::from_utf8(request.recv().unwrap()).unwrap();
    assert!(wire.starts_with("GET /api/companies/synthetic/bookkeeping-workbench?"));
    for query in ["from=2026-01-01","to=2026-01-31","status=ready","bankAccountId=7","partyId=party%3Asynthetic","documentQuality=matched","account=1234","vatTreatment=purchase+%26+VAT","dimension=project%3Aone","cursor=2","limit=3","search=synthetic+%2F+%C3%A6+%26+%3F"] {assert!(wire.contains(query),"{query}: {wire}");}
    assert!(wire.ends_with("\r\n\r\n"));task.join().unwrap();
}
#[test]
fn original_first_use116_workbench_malformed_and_forged_fields_are_parse_errors() {
    let mut calls=vec![init("2025-11-25"),ready()];
    for (i,(key,value)) in [("company",json!("")),("from",json!("2026-2-30")),("to",json!("2025-01")),("status",json!("unknown")),("bankAccountId",json!(0)),("partyId",json!("ab")),("documentQuality",json!("guessed")),("account",json!("x".repeat(33))),("vatTreatment",json!("x".repeat(81))),("dimension",json!("ab")),("cursor",json!(-1)),("limit",json!(101)),("search",json!("x".repeat(201))),("actor",json!("user:forged")),("workspace",json!("/invented")),("scope",json!({})),("body",json!("{}")),("confirm",json!(true)),("companyId",json!(1)),("status",Value::Null)].into_iter().enumerate() {
        let mut args=original_first_use116_workbench_args();args[key]=value;
        calls.push(call((i+2) as i32,"bookkeeping_workbench",args));
    }
    for key in ["company","from","to"] {
        let mut args=original_first_use116_workbench_args();args.as_object_mut().unwrap().remove(key);
        calls.push(call(calls.len() as i32,"bookkeeping_workbench",args));
    }
    let rows=parsed(&run("http://127.0.0.1:9",&calls));
    for row in rows.iter().skip(1) {assert_eq!(row["result"]["isError"],true,"{row}");assert!(row["result"].get("structuredContent").is_none(),"{row}");}
}
#[test]
fn original_first_use116_workbench_current_refusals_and_response_loss_are_tool_errors() {
    for (status,core) in [
        ("403 Forbidden",json!({"ok":false,"errors":["current CompanyRead required"],"code":"permission_denied","appliedRules":["current_authority"]})),
        ("404 Not Found",json!({"ok":false,"errors":["wrong selector"],"code":"company_not_found"})),
        ("401 Unauthorized",json!({"ok":false,"errors":["expired"],"code":"session_expired"})),
        ("200 OK",Value::Null),
        ("200 OK",json!({"ok":true,"workbench":null})),
        ("200 OK",json!({"ok":true,"other":[]})),
    ] {
        let body=if core.is_null(){String::new()}else{core.to_string()};
        let response=Box::leak(format!("HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",body.len()).into_boxed_str());
        let(origin,request,task)=http(response);
        let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"bookkeeping_workbench",json!({"company":"synthetic","from":"2026-01-01","to":"2026-01-31"}))]));
        let result=&rows[1]["result"];original_first_use116_agreement(result);
        assert_eq!(result["isError"],true);
        if core.is_null() || core["ok"]==true {assert_eq!(result["structuredContent"]["code"],"protocol");}
        else{assert_eq!(result["structuredContent"],core);}
        assert!(request.recv().unwrap().starts_with(b"GET "));task.join().unwrap();
    }
}

#[test]
fn original_first_use116_workbench_business_ranges_and_binding_are_not_rpc_parse_errors() {
    let core=json!({"ok":false,"errors":["ordered valid ISO range required"],"code":"invalid_input"});
    for (from,to) in [("2026-02-30","2026-03-01"),("2026-01-31","2026-01-01")] {
        let body=core.to_string();let response=Box::leak(format!("HTTP/1.1 400 Bad Request\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",body.len()).into_boxed_str());let(origin,request,task)=http(response);
        let rows=parsed(&run(&origin,&[init("2025-11-25"),ready(),call(2,"bookkeeping_workbench",json!({"company":"synthetic","from":from,"to":to}))]));
        original_first_use116_agreement(&rows[1]["result"]);assert_eq!(rows[1]["result"]["structuredContent"],core);assert!(request.recv().unwrap().starts_with(b"GET "));task.join().unwrap();
    }
    let rows=parsed(&run("http://127.0.0.1:9",&[init("2025-11-25"),ready(),call(2,"bookkeeping_workbench",json!({"company":"../other","from":"2026-01-01","to":"2026-01-31"}))]));
    original_first_use116_agreement(&rows[1]["result"]);assert_eq!(rows[1]["result"]["structuredContent"]["code"],"invalid_input");
}
