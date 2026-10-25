use crate::original_discovery123::{self as discovery, valid};
use rentemester_client::ApiClient;
use rentemester_engine::agent_discovery::original123;
use serde_json::{Value, json};

#[test]
fn original_discovery123_exact_tools_and_structural_bounds() {
    let registry = crate::tools::list();
    for tool in discovery::tools() {
        let name = tool["name"].as_str().unwrap();
        assert_eq!(registry["tools"].as_array().unwrap().iter().filter(|t| t["name"] == name).count(), 1);
        assert_eq!(tool["annotations"], json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}));
        assert_eq!(tool["outputSchema"], serde_json::from_str::<Value>(crate::diagnostics_original118::OUTPUT_SCHEMA).unwrap());
    }
    assert!(valid("meta_about", &json!({})));
    assert!(!valid("meta_about", &json!({"company":"synthetic"})));
    for value in [json!(null), json!([]), json!({"query":null}), json!({"cursor":null}), json!({"cursor":-1}), json!({"cursor":1.5}), json!({"limit":0}), json!({"limit":51}), json!({"actor":"agent:test"}), json!({"query":"😀".repeat(101)})] {
        assert!(!valid("agent_capability_search", &value), "{value}");
    }
    assert!(valid("agent_capability_search", &json!({"cursor":u64::MAX,"query":"😀".repeat(100),"limit":50})));
    for id in ["../unknown?x#y", "unknown/flow", "😀", "A B"] {
        assert!(valid("agent_workflow_describe", &json!({"id":id})));
    }
    assert!(!valid("agent_workflow_describe", &json!({"id":""})));
    assert!(!valid("agent_workflow_describe", &json!({"id":"😀".repeat(51)})));
}

#[test]
fn original_discovery123_invalid_calls_are_source_text_only_without_network() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        for (name,args) in [("meta_about",json!(null)),("agent_capability_search",json!({"cursor":null})),("agent_workflow_describe",json!({"id":42}))] {
            let result = crate::tools::call(&api,name,args).await.unwrap();
            assert_eq!(result["isError"],true);
            assert!(result.get("structuredContent").is_none());
            assert!(result["content"][0]["text"].as_str().unwrap().starts_with("MCP error -32602: Input validation error:"));
        }
    });
}

#[test]
fn original_discovery123_full_page_and_envelopes_are_not_truncated() {
    let registry = crate::tools::list();
    let tools = registry["tools"].as_array().unwrap();
    let core = original123::search("", 0, 50, &[], true).unwrap();
    let result = discovery::success(core, "agent_capability_search", tools);
    assert_eq!(result["isError"], false);
    let envelope = &result["structuredContent"];
    assert_eq!(envelope["errors"], json!([]));
    assert_eq!(serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(), *envelope);
    assert_eq!(envelope["data"]["count"], 26);
    assert_eq!(envelope["data"]["items"].as_array().unwrap().len(),26);
    assert!(result.to_string().len() < 16 * 1024 * 1024, "full page must fit SDK response bound");
    let about = discovery::success(original123::about(), "meta_about", tools);
    assert_eq!(about["structuredContent"]["data"]["toolCount"],tools.len());
    let exhausted = original123::search("",u64::MAX,50,&[],false).unwrap();
    assert_eq!(exhausted["count"],0);
    assert_eq!(exhausted["nextCursor"],Value::Null);
}

#[test]
fn original_discovery123_all_source_workflows_decorate_once_with_actual_tools() {
    let registry = crate::tools::list();
    let tools = registry["tools"].as_array().unwrap();
    let page = original123::search("",0,50,&[],true).unwrap();
    let mut ids = std::collections::BTreeSet::new();
    for item in page["items"].as_array().unwrap() {
        for id in item["workflowIds"].as_array().unwrap() {
            ids.insert(id.as_str().unwrap().to_owned());
        }
    }
    assert_eq!(ids.len(),31);
    for id in ids {
        let core = original123::describe(&id,&[],true).unwrap();
        let once = original123::decorate_mcp(core,tools);
        assert_eq!(original123::decorate_mcp(once.clone(),tools),once,"decoration must not duplicate bindings: {id}");
        for step in once["workflow"]["steps"].as_array().unwrap() {
            for key in ["operation","uncertainOutcomeReadBack"] {
                let reference = &step[key];
                if reference["surface"] == "mcp" && reference["resolved"] == true {
                    assert!(tools.iter().any(|tool| tool["name"] == reference["name"] && tool["annotations"]["readOnlyHint"].is_boolean()),"unsupported operation advertised: {reference}");
                }
            }
        }
    }
    assert!(original123::describe("unknown/../?id",&[],false).is_none());
}

#[test]
fn original_discovery123_unknown_ids_are_original_error_without_url_or_network(){
 let api=ApiClient::new("http://127.0.0.1:9",true,None).unwrap();
 tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async{
  for id in [".","..","unknown/../?id","😀","UNKNOWN"]{
   let result=discovery::call(&api,"agent_workflow_describe",json!({"id":id})).await.unwrap();
   assert_eq!(result["structuredContent"],json!({"ok":false,"errors":["UNKNOWN_WORKFLOW"]}));
   assert_eq!(result["isError"],true);assert_eq!(serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),result["structuredContent"]);
  }
 });
}
