//! Original structured invoice lifecycle tools; no body strings or caller identity.
use rentemester_client::{
    ApiClient, Error, InvoiceLifecycleKind, invoice_lifecycle_schema, validate_invoice_lifecycle,
};
use serde_json::{Value, json};
fn kind(name: &str) -> InvoiceLifecycleKind {
    match name {
        "invoice_issue" => InvoiceLifecycleKind::Issue,
        "invoice_render" => InvoiceLifecycleKind::Render,
        "invoice_post" => InvoiceLifecycleKind::Post,
        "invoice_settle_bank" => InvoiceLifecycleKind::SettleBank,
        "invoice_credit_note" => InvoiceLifecycleKind::CreditNote,
        _ => InvoiceLifecycleKind::RefundBank,
    }
}
pub fn tools() -> Vec<Value> {
    ["invoice_issue","invoice_render","invoice_post","invoice_settle_bank","invoice_credit_note","invoice_refund_bank"].into_iter().map(|name|{let mut schema=invoice_lifecycle_schema(kind(name));schema["properties"]["company"]=json!({"type":"string","minLength":1});schema["properties"]["confirm"]=json!({"type":"boolean"});schema["required"].as_array_mut().unwrap().retain(|v|v!="confirm");schema["required"].as_array_mut().unwrap().push(json!("company"));let mut tool=crate::tools::tool(name,"Original structured payload/selectors; server validates supplied totals, master-data, immutable evidence, current authority and all ledger effects. confirm:true before network. Post requires explicit revenueAccountNo; no hardcoded chart account. Render verifies existing PDF and never regenerates. No caller identity, keys, local financial math or automatic retries.",schema,false);tool["annotations"]=json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":name=="invoice_render","openWorldHint":false});tool["outputSchema"]=json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"errors":{"type":"array","items":{"type":"string"}},"code":{"type":"string"},"appliedRules":{"type":"array","items":{"type":"string"}}},"additionalProperties":false});tool}).collect()
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
    let mut object = args
        .as_object()
        .ok_or("invoice lifecycle requires flat object and structured payload")?
        .clone();
    let company = object
        .remove("company")
        .and_then(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned))
        .ok_or("nonempty company required")?;
    if object.get("confirm").is_some_and(|v| !v.is_boolean()) {
        return Err("confirm must be boolean");
    }
    let confirmed = object.get("confirm") == Some(&json!(true));
    object.insert("confirm".into(), json!(true));
    let bytes = serde_json::to_vec(&object).map_err(|_| "invoice payload unavailable")?;
    validate_invoice_lifecycle(kind(name), &bytes).map_err(
        |_| "original structured lifecycle schema differs; no null/unknown/identity fields",
    )?;
    if !confirmed {
        return Ok(result(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {name}")]}),
        ));
    }
    Ok(
        match api.invoice_lifecycle(&company, kind(name), bytes).await {
            Ok(value) => result(value),
            Err(error) => failure(error),
        },
    )
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
    fn structured_source_schema_confirmation_identity_and_native_names() {
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        for confirm in [None, Some(false)] {
            let mut args = json!({"company":"synthetic","payload":{"invoiceType":"full"}});
            if let Some(confirm) = confirm {
                args["confirm"] = json!(confirm);
            }
            assert_eq!(
                rt.block_on(crate::tools::call(&api, "invoice_issue", args))
                    .unwrap()["structuredContent"]["code"],
                "CONFIRM_REQUIRED"
            );
        }
        for args in [
            json!({"company":"synthetic","payload":"{}","confirm":true}),
            json!({"company":"synthetic","payload":{"invoiceType":"full","totals":{"grossAmount":null}},"confirm":true}),
            json!({"company":"synthetic","payload":{"invoiceType":"full","createdBy":"forged"},"confirm":true}),
            json!({"company":"synthetic","payload":{"invoiceType":"full"},"confirm":null}),
            json!({"company":"synthetic","payload":{"invoiceType":"full"},"actor":"forged","confirm":true}),
            json!({"company":"synthetic","payload":{"invoiceType":"full"},"idempotencyKey":"key","confirm":true}),
        ] {
            assert!(rt.block_on(call(&api, "invoice_issue", args)).is_err());
        }
        for name in [
            "invoice_issue",
            "invoice_render",
            "invoice_post",
            "invoice_settle_bank",
            "invoice_credit_note",
            "invoice_refund_bank",
            "issue_invoice",
            "post_invoice",
            "settle_invoice",
            "credit_invoice",
            "refund_invoice",
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
        assert!(
            !crate::tools::list()["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["name"] == "invoice_create")
        );
        assert_eq!(
            tools()
                .iter()
                .find(|t| t["name"] == "invoice_render")
                .unwrap()["annotations"]["idempotentHint"],
            true
        );
    }
    #[test]
    fn all_original_structured_tools_preserve_payload_rules_and_exact_server_envelopes() {
        tokio::runtime::Runtime::new().unwrap().block_on(async{for(name,args)in[("invoice_issue",json!({"company":"synthetic","payload":{"invoiceType":"full","totals":{"grossAmount":125,"vatRate":0.25},"lines":[{"description":"Synthetic","quantity":2,"unitPriceExVat":50,"lineTotalExVat":100}]},"confirm":true})),("invoice_render",json!({"company":"synthetic","documentId":7,"confirm":true})),("invoice_post",json!({"company":"synthetic","invoiceNumber":"INV-7","revenueAccountNo":"synthetic-income","confirm":true})),("invoice_settle_bank",json!({"company":"synthetic","payload":{"invoiceNumber":"INV-7","bankTransactionReference":"incoming-ref","bankAccountNo":"bank","receivableAccountNo":"debtor"},"confirm":true})),("invoice_credit_note",json!({"company":"synthetic","payload":{"originalInvoiceDocumentId":7,"issueDate":"2026-01-02","reason":" raw reason ","grossAmount":25},"confirm":true})),("invoice_refund_bank",json!({"company":"synthetic","payload":{"invoiceDocumentId":7,"bankTransactionId":11,"refundDate":"2026-01-03","amount":25},"confirm":true}))]{let value=if name=="invoice_post"{json!({"ok":false,"code":"business","errors":["server refusal"]})}else{json!({"ok":true,"data":{"documentId":7,"renderDocumentId":7,"sha256":"server","pdfStoredPath":"/api/source.pdf"},"errors":[],"appliedRules":["server-rule"]})};let(origin,t)=server(200,value.clone());let api=ApiClient::new(&origin,true,None).unwrap();let result=crate::tools::call(&api,name,args.clone()).await.unwrap();assert_eq!(result["structuredContent"],value);assert_eq!(serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),value);assert_eq!(result["isError"],value["ok"]!=true);let(_,body)=t.join().unwrap();let mut expected=args;expected.as_object_mut().unwrap().remove("company");assert_eq!(body,expected);}});
    }
}
