//! Original flat invoice read tools; all financial/history/filter semantics stay on server.
use rentemester_client::{
    ApiClient, Error, InvoiceOriginalKind, InvoiceOriginalQuery, invoice_original_schema,
};
use serde_json::{Value, json};
fn kind(name: &str) -> InvoiceOriginalKind {
    match name {
        "invoice_list" => InvoiceOriginalKind::List,
        "invoice_find" => InvoiceOriginalKind::Find,
        "invoice_status" => InvoiceOriginalKind::Status,
        _ => InvoiceOriginalKind::Overdue,
    }
}
pub fn tools() -> Vec<Value> {
    ["invoice_list","invoice_find","invoice_status","invoice_overdue"].into_iter().map(|name|{let mut schema=invoice_original_schema(kind(name));schema["properties"]["company"]=json!({"type":"string","minLength":1});schema["required"]=json!(["company"]);let mut tool=crate::tools::tool(name,"Original all-year native invoice read with canonical errors[]/data. Find amount wins over min/max; invoiceNumber exact case-sensitive, free query/customer literal substrings. Status documentId wins. asOf changes aging only, current histories retained; foreign principal and DKK claims stay separate. No provider calls, local financial/filter math, identity, confirmation, keys or retries.",schema,true);tool["annotations"]=json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false});tool["outputSchema"]=json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"errors":{"type":"array","items":{"type":"string"}},"code":{"type":"string"},"appliedRules":{"type":"array","items":{"type":"string"}}},"additionalProperties":false});tool}).collect()
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
pub async fn call(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let kind = kind(name);
    let schema = invoice_original_schema(kind);
    let object = args
        .as_object()
        .ok_or("invoice read requires flat object")?;
    let company = object
        .get("company")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("nonempty company required")?;
    for (key, value) in object {
        if key == "company" {
            continue;
        }
        let Some(property) = schema["properties"].get(key) else {
            return Err("unknown invoice read field; no identity/body/confirmation");
        };
        match property["type"].as_str() {
            Some("string") => {
                let text = value
                    .as_str()
                    .ok_or("optional strings reject null/nonstrings")?;
                if key == "status"
                    && !matches!(
                        text,
                        "all"
                            | "open"
                            | "paid"
                            | "credited"
                            | "refunded"
                            | "overpaid"
                            | "written_off"
                            | "overdue"
                    )
                {
                    return Err("invalid invoice status");
                }
            }
            Some("number") => {
                if !value.is_number() {
                    return Err("amount bounds must be numbers, never null");
                }
            }
            Some("integer") => {
                if value
                    .as_u64()
                    .is_none_or(|n| n > 9007199254740991 || n == 0 && key != "minDays")
                {
                    return Err("positive safe documentId/nonnegative minDays required");
                }
            }
            _ => return Err("invalid invoice schema"),
        }
    }
    let mut query = InvoiceOriginalQuery::default();
    for (key, slot) in [
        ("status", &mut query.status),
        ("from", &mut query.from),
        ("to", &mut query.to),
        ("customerCvr", &mut query.customer_cvr),
        ("customer", &mut query.customer),
        ("invoiceNumber", &mut query.invoice_number),
        ("query", &mut query.query),
        ("asOf", &mut query.as_of),
    ] {
        *slot = object.get(key).and_then(Value::as_str).map(str::to_owned);
    }
    for (key, slot) in [
        ("minAmount", &mut query.min_amount),
        ("maxAmount", &mut query.max_amount),
        ("amount", &mut query.amount),
    ] {
        *slot = object.get(key).and_then(|v| {
            if let Value::Number(n) = v {
                Some(n.clone())
            } else {
                None
            }
        });
    }
    query.document_id = object.get("documentId").and_then(Value::as_i64);
    query.min_days = object.get("minDays").and_then(Value::as_u64);
    Ok(match api.invoice_original(company, kind, &query).await {
        Ok(value) => result(value),
        Err(error) => failure(error),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
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
            (header, body)
        });
        (origin, t)
    }

    #[test]
    fn original_read_schema_null_identity_and_irrelevant_fields_refuse_before_http() {
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        for (name, key, value) in [
            ("invoice_list", "customer", Value::Null),
            ("invoice_list", "minAmount", json!("-1")),
            ("invoice_list", "status", json!("wrong")),
            ("invoice_find", "amount", Value::Null),
            ("invoice_find", "from", json!("2026-01-01")),
            ("invoice_status", "documentId", json!("7")),
            ("invoice_status", "confirm", json!(true)),
            ("invoice_overdue", "minDays", json!(-1)),
            ("invoice_overdue", "actor", json!("forged")),
            ("invoice_list", "body", json!("{}")),
        ] {
            let mut args = json!({"company":"synthetic"});
            args[key] = value;
            assert!(rt.block_on(call(&api, name, args)).is_err());
        }
        for name in [
            "invoice_list",
            "invoice_find",
            "invoice_status",
            "invoice_overdue",
            "invoices",
        ] {
            assert_eq!(
                crate::tools::list()["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|t| t["name"] == name)
                    .count(),
                1
            );
        }
        for tool in tools() {
            assert_eq!(tool["annotations"]["readOnlyHint"], true);
        }
    }
    #[test]
    fn canonical_four_reads_preserve_negative_bounds_amount_priority_and_status_histories() {
        tokio::runtime::Runtime::new().unwrap().block_on(async{for(name,args,value,contains)in[("invoice_list",json!({"company":"synthetic","customer":" ","customerCvr":" dk123 ","minAmount":-12.34}),json!({"ok":true,"data":{"count":0,"rows":[]},"errors":[]}),vec!["customer=+","customerCvr=+dk123+","minAmount=-12.34"]),("invoice_find",json!({"company":"synthetic","query":"literal % words","amount":-12.34,"minAmount":-999,"maxAmount":999,"asOf":"2020-01-01"}),json!({"ok":true,"data":{"count":1,"rows":[{"invoiceNumber":"server exact amount result"}]},"errors":[]}),vec!["amount=-12.34","minAmount=-999","maxAmount=999","query=literal+%25+words"]),("invoice_status",json!({"company":"synthetic","documentId":7,"invoiceNumber":"ignored"}),json!({"ok":true,"data":{"invoiceDocumentId":7,"claimOpenBalance":null,"currency":"EUR","claimCurrency":"DKK","registeredClaimBalanceDkk":100,"payments":[{"amount":12}]},"errors":[]}),vec!["documentId=7","invoiceNumber=ignored"]),("invoice_overdue",json!({"company":"synthetic","minDays":0}),json!({"ok":false,"errors":["server-owned business refusal"]}),vec!["minDays=0"])]{let(origin,t)=server(200,value.clone());let api=ApiClient::new(&origin,true,None).unwrap();let result=crate::tools::call(&api,name,args).await.unwrap();assert_eq!(result["structuredContent"],value);assert_eq!(serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),value);assert_eq!(result["isError"],value["ok"]!=true);let(header,body)=t.join().unwrap();assert!(contains.into_iter().all(|s|header.contains(s)));assert!(body.is_null());}});
    }
    #[test]
    fn exact_422_receipts_are_mcp_errors_without_losing_code_or_rules() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for name in ["invoice_list", "invoice_find", "invoice_status", "invoice_overdue"] {
                let receipt = json!({"ok":false,"code":"ORIGINAL_SOURCE_MISSING","errors":["server-owned source refusal"],"appliedRules":["original.source"]});
                let (origin, t) = server(422, receipt.clone());
                let api = ApiClient::new(&origin, true, None).unwrap();
                let result = crate::tools::call(&api, name, json!({"company":"synthetic"})).await.unwrap();
                assert_eq!(result["isError"], true);
                assert_eq!(result["structuredContent"], receipt);
                assert_eq!(serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(), receipt);
                let (header, body) = t.join().unwrap();
                assert!(header.starts_with("GET "));
                assert!(body.is_null());
            }
        });
    }
}
