//! Original flat bank query tools over server-owned filtering, amounts and relation proofs.
use crate::tools::tool;
use rentemester_client::{ApiClient, BankQuery, Error};
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut out = vec![];
    for name in ["bank_list", "reconcile_bank", "bank_account_list"] {
        let mut properties = json!({"company":{"type":"string","minLength":1,"description":"Connected-server company slug; filesystem/default selectors remain a separate compatibility boundary."}});
        let mut required = vec!["company"];
        if name == "bank_account_list" {
            properties["includeInactive"] = json!({"type":"boolean","default":true});
        } else {
            properties["status"] =
                json!({"type":"string","enum":["all","matched","unmatched"],"default":"all"});
            for key in ["from", "to", "textMatch", "account"] {
                properties[key] = json!({"type":"string"});
            }
            properties["account"]["description"] =
                json!("Positive account ID or exact slug; blank means all. No human-name lookup.");
            properties["amount"] = json!({"type":"number","description":"Signed original-currency decimal units; server rounds/compares safely, never a DKK-coalesced filter."});
            if name == "reconcile_bank" {
                required.extend(["from", "to"]);
                for key in ["from", "to"] {
                    properties[key]["minLength"] = json!(1);
                }
            } else {
                properties["limit"] =
                    json!({"type":"integer","minimum":1,"maximum":5000,"default":500});
                properties["offset"] =
                    json!({"type":"integer","minimum":0,"maximum":9007199254740991u64,"default":0});
            }
        }
        let mut t = tool(
            name,
            "Read canonical all-year bank evidence/account registry or reconciliation. Server owns inclusive date/text/status/original-unit amount/account filters, reversal/correction relations and totals. Lists explicitly request one server page (default500/offset0), no local filtering/aggregation. Reconciliation signed totals may mix currencies and are never labeled DKK. Exact original errors[] envelope; no data on business refusal, no writes or retries.",
            json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}),
            true,
        );
        t["annotations"]["openWorldHint"] = json!(false);
        t["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"errors":{"type":"array","items":{"type":"string"}},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});
        out.push(t);
    }
    out
}
fn result(value: Value) -> Value {
    json!({"isError":value["ok"]==false,"content":[{"type":"text","text":value.to_string()}],"structuredContent":value})
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
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let object = args
        .as_object()
        .ok_or("bank read requires object arguments")?;
    let accounts = name == "bank_account_list";
    let reconcile = name == "reconcile_bank";
    if !matches!(name, "bank_list" | "reconcile_bank" | "bank_account_list") {
        return Err("unknown bank read");
    }
    let allowed = if accounts {
        vec!["company", "includeInactive"]
    } else if reconcile {
        vec![
            "company",
            "from",
            "to",
            "status",
            "textMatch",
            "amount",
            "account",
        ]
    } else {
        vec![
            "company",
            "from",
            "to",
            "status",
            "textMatch",
            "amount",
            "account",
            "limit",
            "offset",
        ]
    };
    if object.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("unknown bank read argument");
    }
    let company = object
        .get("company")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("nonempty company required")?;
    let string = |key| {
        object
            .get(key)
            .map(|v| {
                v.as_str()
                    .ok_or("optional filters must be strings, never null")
            })
            .transpose()
    };
    let mut query = BankQuery::default();
    if !accounts {
        query.status = string("status")?.map(str::to_owned);
        if query
            .status
            .as_deref()
            .is_some_and(|s| !matches!(s, "all" | "matched" | "unmatched"))
        {
            return Err("status must be all|matched|unmatched");
        }
        query.from = string("from")?.map(str::to_owned);
        query.to = string("to")?.map(str::to_owned);
        query.text_match = string("textMatch")?.map(str::to_owned);
        query.account = string("account")?.map(str::to_owned);
        if reconcile
            && (query.from.as_deref().is_none_or(str::is_empty)
                || query.to.as_deref().is_none_or(str::is_empty))
        {
            return Err("reconciliation requires nonempty from/to");
        }
        query.amount = object
            .get("amount")
            .map(|v| {
                if v.is_number() {
                    Ok(v.to_string())
                } else {
                    Err("amount must be a finite number")
                }
            })
            .transpose()?;
        if !reconcile {
            for (key, slot, default, min, max) in [
                ("limit", &mut query.limit, 500, 1, 5000),
                ("offset", &mut query.offset, 0, 0, 9007199254740991u64),
            ] {
                let value = object
                    .get(key)
                    .map(|v| {
                        v.as_u64()
                            .filter(|n| (*n >= min) && (*n <= max))
                            .ok_or("invalid bank page selector")
                    })
                    .transpose()?
                    .unwrap_or(default);
                *slot = Some(value);
            }
        }
    }
    let include = object
        .get("includeInactive")
        .map(|v| {
            v.as_bool()
                .ok_or("includeInactive must be boolean, never null")
        })
        .transpose()?;
    let response = if accounts {
        api.bank_account_list(company, include).await
    } else if reconcile {
        api.bank_reconciliation(company, &query).await
    } else {
        api.bank_transactions(company, &query).await
    };
    Ok(match response {
        Ok(value) => result(value),
        Err(error) => failure(error),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_flat_schemas_null_type_and_unknown_fields_refuse_before_http() {
        let list = tools();
        for t in &list {
            assert_eq!(t["inputSchema"]["additionalProperties"], false);
            assert_eq!(t["annotations"]["readOnlyHint"], true);
            assert_eq!(t["annotations"]["idempotentHint"], true);
            assert_eq!(t["annotations"]["openWorldHint"], false);
            assert_eq!(
                crate::tools::list()["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|s| s["name"] == t["name"])
                    .count(),
                1
            );
        }
        assert_eq!(
            list[0]["inputSchema"]["properties"]["limit"]["default"],
            500
        );
        assert_eq!(
            list[1]["inputSchema"]["required"],
            json!(["company", "from", "to"])
        );
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        for (name, args) in [
            ("bank_list", json!({"company":"synthetic","from":null})),
            ("bank_list", json!({"company":"synthetic","amount":"12"})),
            ("bank_list", json!({"company":"synthetic","limit":0})),
            (
                "bank_list",
                json!({"company":"synthetic","offset":9007199254740992u64}),
            ),
            (
                "bank_list",
                json!({"company":"synthetic","status":"posted"}),
            ),
            (
                "reconcile_bank",
                json!({"company":"synthetic","from":"2026-01-01"}),
            ),
            (
                "bank_account_list",
                json!({"company":"synthetic","includeInactive":null}),
            ),
            ("bank_list", json!({"company":"synthetic","body":"{}"})),
            ("bank_list", json!({"company":"synthetic","actor":"fake"})),
        ] {
            assert!(rt.block_on(call(&api, name, args)).is_err(), "{name}");
        }
    }
    #[test]
    fn default_mcp_page_and_legacy_business_code_preserve_text_structured_agreement() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        let rt = tokio::runtime::Runtime::new().unwrap();
        for response in [
            json!({"ok":true,"data":{"rows":[],"count":0,"total":0,"limit":500,"offset":0,"hasMore":false},"errors":[]}),
            json!({"ok":false,"code":"NOT_FOUND","errors":["bank account 'unknown' does not exist"]}),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let expected = response.clone();
            let t = thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = vec![];
                while !request.ends_with(b"\r\n\r\n") {
                    let mut b = [0; 4096];
                    let n = s.read(&mut b).unwrap();
                    assert!(n > 0);
                    request.extend(&b[..n]);
                }
                let body = response.to_string();
                write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                String::from_utf8(request).unwrap()
            });
            let api = ApiClient::new(&origin, true, None).unwrap();
            let value = rt
                .block_on(crate::tools::call(
                    &api,
                    "bank_list",
                    json!({"company":"synthetic","account":"unknown"}),
                ))
                .unwrap();
            assert_eq!(value["structuredContent"], expected);
            assert_eq!(value["isError"], expected["ok"] == false);
            assert_eq!(
                serde_json::from_str::<Value>(value["content"][0]["text"].as_str().unwrap())
                    .unwrap(),
                expected
            );
            let request = t.join().unwrap();
            assert!(request.contains("account=unknown&limit=500&offset=0"));
        }
    }
}
