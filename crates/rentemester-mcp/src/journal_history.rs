use crate::tools::{company_schema, tool};
use rentemester_client::{ApiClient, Error, JournalHistoryQuery, journal_history_query_schema};
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut schema = journal_history_query_schema();
    schema["properties"]["company"] = company_schema();
    schema["required"] = json!(["company"]);
    let mut tool = tool(
        "journal_list",
        "List original native append-only journal headers across fiscal years, newest ID first. Inclusive from/to dates; reversed intervals return empty. Stored status is literal: an original remains posted after reversal. Optional status all(default)|posted|reversed; limit1..5000(default500), offset(default0). data has entries,total,count,limit,offset,hasMore and nextOffset only when hasMore. Each page is a new observation; no archive merge, line totals, math, body, confirm or local files.",
        schema,
        true,
    );
    tool["annotations"]["openWorldHint"] = json!(false);
    vec![tool]
}
pub async fn call(api: &ApiClient, mut args: Value) -> Result<Result<Value, Error>, &'static str> {
    let company = args
        .as_object_mut()
        .and_then(|o| o.remove("company"))
        .and_then(|s| s.as_str().map(str::to_owned))
        .ok_or("journal_list requires company and only typed history selectors")?;
    let query: JournalHistoryQuery = serde_json::from_value(args).map_err(
        |_| "journal_list allows only company/from/to/status/limit/offset; no null selectors",
    )?;
    query
        .validate()
        .map_err(|_| "invalid journal_list dates/status/page selectors")?;
    Ok(api.journal_history(&company, &query, false).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_catalogue_is_closed_readonly_and_page_defaults_are_explicit() {
        let tool = tools().remove(0);
        assert_eq!(tool["name"], "journal_list");
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert_eq!(
            tool["inputSchema"]["properties"].as_object().unwrap().len(),
            6
        );
        assert_eq!(tool["inputSchema"]["properties"]["limit"]["default"], 500);
        assert_eq!(tool["inputSchema"]["properties"]["limit"]["maximum"], 5000);
        for key in ["readOnlyHint", "idempotentHint"] {
            assert_eq!(tool["annotations"][key], true);
        }
        assert_eq!(tool["annotations"]["openWorldHint"], false);
        assert_eq!(
            crate::tools::list()["tools"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|t| t["name"] == "journal_list")
                .count(),
            1
        );
    }
    #[test]
    fn original_tool_rejects_extra_null_and_invalid_selectors_before_network() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        for args in [
            json!({"company":"synthetic","all":true}),
            json!({"company":"synthetic","limit":null}),
            json!({"company":"synthetic","offset":-1}),
            json!({"company":"synthetic","confirm":true}),
            json!({"company":"synthetic","from":"2026-02-30"}),
            json!({"company":"synthetic","status":"effective"}),
        ] {
            assert!(runtime.block_on(call(&api, args)).is_err());
        }
    }
    #[test]
    fn canonical_http_page_maps_directly_to_original_envelope_data() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = vec![];
            while !request.ends_with(b"\r\n\r\n") {
                let mut b = [0; 4096];
                let n = socket.read(&mut b).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&b[..n]);
            }
            let reply=json!({"ok":true,"journalHistory":{"entries":[{"id":7,"entryNo":"SYN-7","transactionDate":"2026-10-20","text":"Synthetic reversal header","currency":"DKK","amountForeign":null,"amountDkk":null,"fxRateToDkk":null,"documentId":null,"sourceBankTransactionId":9,"status":"reversed","reversalOfEntryId":6,"bookedDebitTotal":1234.5}],"total":1,"count":1,"limit":500,"offset":0,"hasMore":false}}).to_string();
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
            String::from_utf8(request).unwrap()
        });
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let envelope = runtime
            .block_on(crate::tools::call(
                &api,
                "journal_list",
                json!({"company":"synthetic","status":"reversed"}),
            ))
            .unwrap();
        assert_eq!(envelope["isError"], false);
        let data = &envelope["structuredContent"]["data"];
        assert_eq!(data["total"], 1);
        assert_eq!(data["entries"][0].as_object().unwrap().len(), 12);
        assert_eq!(data["entries"][0]["amountDkk"], Value::Null);
        assert_eq!(data["entries"][0]["reversalOfEntryId"], 6);
        assert!(data.get("journalHistory").is_none());
        assert!(data.get("nextOffset").is_none());
        assert!(task.join().unwrap().starts_with("GET /api/companies/synthetic/journal/entries?status=reversed&limit=500&offset=0 HTTP/1.1"));
    }
}
