//! Original flat payable tools; server owns principal, money, source and receipts.
use rentemester_client::{
    ApiClient, Error, PayableOriginalMutation, PayableOriginalQuery, payable_original_body_schema,
};
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    ["payable_register","payable_pay","payable_list"].into_iter().map(|name|{let list=name=="payable_list";let mut schema=if list{json!({"type":"object","properties":{"status":{"type":"string","enum":["all","open","paid","overdue"]},"asOf":{"type":"string"},"asOfDate":{"type":"string"},"supplier":{"type":"string"},"vendorId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"from":{"type":"string"},"to":{"type":"string"},"minDays":{"type":"integer","minimum":0,"maximum":9007199254740991u64}},"required":[],"additionalProperties":false})}else{let mut schema=payable_original_body_schema(if name=="payable_register"{PayableOriginalMutation::Register}else{PayableOriginalMutation::Pay});schema["properties"]["confirm"]=json!({"type":"boolean"});schema["properties"]["idempotencyKey"]["maxLength"]=json!(128);schema["required"].as_array_mut().unwrap().retain(|v|v!="confirm");schema};schema["properties"]["company"]=json!({"type":"string","minLength":1});schema["required"].as_array_mut().unwrap().push(json!("company"));let mut tool=crate::tools::tool(name,"Original payable fields and exact canonical errors[]/rules/receipt. Register expenseAccount; pay date/paymentAccount. List asOf wins over asOfDate; omitted date uses server today. No local balance/aging/filter math, identity, generated keys or retries. Mutation requires confirm:true before network; optional raw key128 preserved.",schema,list);tool["annotations"]=json!({"readOnlyHint":list,"destructiveHint":false,"idempotentHint":list,"openWorldHint":false});tool["outputSchema"]=json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"errors":{"type":"array","items":{"type":"string"}},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});tool}).collect()
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
    let list = name == "payable_list";
    let register = name == "payable_register";
    let mut object = args
        .as_object()
        .ok_or("payable requires flat object")?
        .clone();
    let allowed = if list {
        vec![
            "company", "status", "asOf", "asOfDate", "supplier", "vendorId", "from", "to",
            "minDays",
        ]
    } else if register {
        vec![
            "company",
            "documentId",
            "billDate",
            "dueDate",
            "expenseAccount",
            "vatTreatment",
            "vendorId",
            "note",
            "confirm",
            "idempotencyKey",
        ]
    } else {
        vec![
            "company",
            "payableId",
            "bankTransactionId",
            "amount",
            "date",
            "paymentAccount",
            "note",
            "confirm",
            "idempotencyKey",
        ]
    };
    for (key, value) in &object {
        if !allowed.contains(&key.as_str()) {
            return Err("unknown payable field; no native aliases/identity/body");
        }
        match key.as_str() {
            "confirm" => {
                if !value.is_boolean() {
                    return Err("confirm must be boolean");
                }
            }
            "documentId" | "payableId" | "bankTransactionId" | "vendorId" | "minDays" => {
                if value
                    .as_u64()
                    .is_none_or(|n| n > 9007199254740991 || n == 0 && key != "minDays")
                {
                    return Err("positive safe IDs/nonnegative minDays required");
                }
            }
            "amount" => {
                if value.as_f64().is_none_or(|n| !n.is_finite() || n <= 0.0) {
                    return Err("positive finite amount required");
                }
            }
            _ => {
                let text = value
                    .as_str()
                    .ok_or("optional strings reject null/nonstrings")?;
                if matches!(
                    key.as_str(),
                    "company" | "expenseAccount" | "idempotencyKey"
                ) && text.is_empty()
                {
                    return Err("nonempty company/expenseAccount/key required");
                }
                if key == "idempotencyKey" && text.encode_utf16().count() > 128 {
                    return Err("idempotencyKey exceeds128");
                }
                if key == "status" && !matches!(text, "all" | "open" | "paid" | "overdue") {
                    return Err("invalid status");
                }
                if key == "vatTreatment"
                    && !matches!(text, "standard" | "exempt" | "non_deductible")
                {
                    return Err("invalid VAT treatment");
                }
            }
        }
    }
    let required = if list {
        vec!["company"]
    } else if register {
        vec![
            "company",
            "documentId",
            "billDate",
            "dueDate",
            "expenseAccount",
        ]
    } else {
        vec!["company", "payableId", "bankTransactionId"]
    };
    for key in required {
        if !object.contains_key(key) {
            return Err("required payable field absent");
        }
    }
    if !list && object.get("confirm") != Some(&json!(true)) {
        return Ok(result(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {name}")]}),
        ));
    }
    let company = object
        .remove("company")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let outcome = if list {
        let mut query = PayableOriginalQuery::default();
        for (key, slot) in [
            ("status", &mut query.status),
            ("asOf", &mut query.as_of),
            ("asOfDate", &mut query.as_of_date),
            ("supplier", &mut query.supplier),
            ("from", &mut query.from),
            ("to", &mut query.to),
        ] {
            *slot = object.get(key).and_then(Value::as_str).map(str::to_owned);
        }
        query.vendor_id = object.get("vendorId").and_then(Value::as_u64);
        query.min_days = object.get("minDays").and_then(Value::as_u64);
        api.payable_original_list(&company, &query).await
    } else {
        api.payable_original_mutation(
            &company,
            if register {
                PayableOriginalMutation::Register
            } else {
                PayableOriginalMutation::Pay
            },
            serde_json::to_vec(&object).map_err(|_| "payable body unavailable")?,
        )
        .await
    };
    Ok(match outcome {
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
    fn strict_flat_schema_confirmation_no_identity_and_distinct_native_registry() {
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        for confirm in [None, Some(false)] {
            let mut args = json!({"company":"synthetic","payableId":7,"bankTransactionId":11});
            if let Some(confirm) = confirm {
                args["confirm"] = json!(confirm);
            }
            assert_eq!(
                rt.block_on(crate::tools::call(&api, "payable_pay", args))
                    .unwrap()["structuredContent"]["code"],
                "CONFIRM_REQUIRED"
            );
        }
        for (key, value) in [
            ("note", Value::Null),
            ("amount", Value::Null),
            ("paymentAccount", Value::Null),
            ("date", Value::Null),
            ("confirm", Value::Null),
            ("principal", json!("forged")),
            ("actor", json!("forged")),
            ("paymentDate", json!("2026-01-01")),
            ("idempotencyKey", json!("x".repeat(129))),
        ] {
            let mut args =
                json!({"company":"synthetic","payableId":7,"bankTransactionId":11,"confirm":true});
            args[key] = value;
            assert!(rt.block_on(call(&api, "payable_pay", args)).is_err());
        }
        for name in [
            "payable_register",
            "payable_list",
            "payable_pay",
            "register_payable",
            "payables",
            "pay_payable",
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
    }
    #[test]
    fn alias_filters_keyed_register_partial_pay_and_business_envelopes_are_exact() {
        tokio::runtime::Runtime::new().unwrap().block_on(async{for(name,args,value)in[("payable_list",json!({"company":"synthetic","status":"overdue","asOf":"2026-04-01","asOfDate":"1970-01-01","supplier":" SYN ","vendorId":3,"from":"2026-03-01","to":"2026-01-01","minDays":0}),json!({"ok":true,"data":{"count":0,"rows":[],"asOfDate":"2026-04-01"},"errors":[]})),("payable_register",json!({"company":"synthetic","documentId":4,"billDate":"2026-01-01","dueDate":"2026-01-31","expenseAccount":"expense","confirm":true,"idempotencyKey":" raw register "}),json!({"ok":true,"data":{"payableId":7,"entryId":8,"idempotency":{"replayed":true,"receiptId":"server-register"}},"errors":[],"appliedRules":["DK-PAYABLE-001"]})),("payable_pay",json!({"company":"synthetic","payableId":7,"bankTransactionId":11,"amount":50.25,"date":"2026-02-01","paymentAccount":"bank","confirm":true,"idempotencyKey":" raw pay "}),json!({"ok":true,"data":{"paymentId":12,"openBalance":74.75,"idempotency":{"replayed":false,"receiptId":"server-pay"}},"errors":[],"appliedRules":["DK-PAYABLE-PAYMENT-001"]})),("payable_list",json!({"company":"synthetic","asOfDate":"2026-02-30"}),json!({"ok":false,"code":"business","errors":["server-owned invalid date"]}))]{let(origin,t)=server(200,value.clone());let api=ApiClient::new(&origin,true,None).unwrap();let result=crate::tools::call(&api,name,args.clone()).await.unwrap();assert_eq!(result["structuredContent"],value);assert_eq!(serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),value);assert_eq!(result["isError"],value["ok"]!=true);let(header,body)=t.join().unwrap();if name=="payable_list"{assert!(body.is_null());if args.get("asOf").is_some(){assert!(header.contains("asOf=2026-04-01") && !header.contains("1970") && header.contains("minDays=0"));}}else{let mut expected=args;expected.as_object_mut().unwrap().remove("company");assert_eq!(body,expected);}}});
    }
}
