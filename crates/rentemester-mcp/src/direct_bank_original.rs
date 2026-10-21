//! Original flat direct-bank correction tools; no caller identity or client accounting.
use rentemester_client::{ApiClient, Error, direct_bank_original_schema};
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    [false,true].into_iter().map(|apply|{let name=if apply{"direct_bank_purchase_payable_correction_apply"}else{"direct_bank_purchase_payable_correction_plan"};let mut schema=direct_bank_original_schema(apply);schema["properties"]["company"]=json!({"type":"string","minLength":1});schema["required"].as_array_mut().unwrap().push(json!("company"));if apply{schema["properties"].as_object_mut().unwrap().remove("expectedPrincipal");schema["properties"]["confirm"]=json!({"type":"boolean"});schema["required"].as_array_mut().unwrap().retain(|v|v!="confirm");schema["properties"]["reason"]["maxLength"]=json!(1000);schema["properties"]["idempotencyKey"]["maxLength"]=json!(128);}
 let mut tool=crate::tools::tool(name,"Original direct-bank purchase correction flat arguments and canonical errors[] envelope. Plan preserves full native source proof/hash; confirmed apply returns server correction and durable idempotency receipt unchanged. No actor/principal fields, local math/hash or automatic retry. Missing/false confirmation refuses before network.",schema,!apply);tool["annotations"]=json!({"readOnlyHint":!apply,"destructiveHint":false,"idempotentHint":!apply,"openWorldHint":false});tool["outputSchema"]=json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"errors":{"type":"array","items":{"type":"string"}},"code":{"type":"string"},"appliedRules":{"type":"array","items":{"type":"string"}}},"additionalProperties":false});tool}).collect()
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
    let apply = name == "direct_bank_purchase_payable_correction_apply";
    let mut object = args
        .as_object()
        .ok_or("direct correction requires flat object")?
        .clone();
    for (key, value) in &object {
        match key.as_str() {
            "company" | "billDate" | "dueDate" | "expenseAccountNo" | "vatTreatment" | "note" => {
                let text = value
                    .as_str()
                    .ok_or("strings reject explicit null/nonstrings")?;
                if matches!(key.as_str(), "company" | "expenseAccountNo") && text.is_empty() {
                    return Err("company/expenseAccountNo must be nonempty");
                }
                if key == "vatTreatment"
                    && !matches!(text, "standard" | "exempt" | "non_deductible")
                {
                    return Err("unknown VAT treatment");
                }
            }
            "documentId" | "bankTransactionId" | "vendorId" => {
                if value
                    .as_i64()
                    .is_none_or(|n| !(1..=9007199254740991).contains(&n))
                {
                    return Err("positive safe integer IDs required");
                }
            }
            "confirm" if apply => {
                if !value.is_boolean() {
                    return Err("confirm must be boolean");
                }
            }
            "planHash" | "reason" | "idempotencyKey" if apply => {
                let text = value
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("nonempty apply strings required")?;
                if key == "planHash"
                    && (text.len() != 64
                        || !text
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
                    || key == "reason" && text.encode_utf16().count() > 1000
                    || key == "idempotencyKey" && text.encode_utf16().count() > 128
                {
                    return Err("original apply field bounds differ");
                }
            }
            _ => return Err("unknown direct correction field; no caller identity/body accepted"),
        }
    }
    for key in [
        "company",
        "documentId",
        "bankTransactionId",
        "billDate",
        "dueDate",
        "expenseAccountNo",
    ] {
        if !object.contains_key(key) {
            return Err("required direct correction field absent");
        }
    }
    if apply {
        for key in ["planHash", "reason", "idempotencyKey"] {
            if !object.contains_key(key) {
                return Err("required apply field absent");
            }
        }
        if object.get("confirm") != Some(&json!(true)) {
            return Ok(result(
                json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for write tool direct_bank_purchase_payable_correction_apply"]}),
            ));
        }
    }
    let company = object
        .remove("company")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let body = serde_json::to_vec(&object).map_err(|_| "direct correction JSON unavailable")?;
    Ok(
        match api.direct_bank_original(&company, body, apply).await {
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

    fn plan() -> Value {
        json!({"company":"synthetic","documentId":3,"bankTransactionId":7,"billDate":"2026-01-01","dueDate":"2026-01-31","expenseAccountNo":"expense"})
    }
    fn apply() -> Value {
        let mut args = plan();
        args["planHash"] = json!("a".repeat(64));
        args["reason"] = json!(" raw reason ");
        args["idempotencyKey"] = json!(" raw key ");
        args
    }
    #[test]
    fn original_flat_schema_confirmation_and_distinct_native_names() {
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        for confirm in [None, Some(false)] {
            let mut args = apply();
            if let Some(confirm) = confirm {
                args["confirm"] = json!(confirm);
            }
            assert_eq!(
                rt.block_on(crate::tools::call(
                    &api,
                    "direct_bank_purchase_payable_correction_apply",
                    args
                ))
                .unwrap()["structuredContent"]["code"],
                "CONFIRM_REQUIRED"
            );
        }
        for (key, value) in [
            ("note", Value::Null),
            ("vatTreatment", Value::Null),
            ("vendorId", Value::Null),
            ("confirm", Value::Null),
            (
                "expectedPrincipal",
                json!({"kind":"user","subjectId":"forged"}),
            ),
            ("principal", json!("user:forged")),
            ("actor", json!("forged")),
            ("body", json!("{}")),
            ("documentId", json!("3")),
        ] {
            let mut args = apply();
            args[key] = value;
            assert!(
                rt.block_on(call(
                    &api,
                    "direct_bank_purchase_payable_correction_apply",
                    args
                ))
                .is_err()
            );
        }
        for name in [
            "direct_bank_purchase_payable_correction_plan",
            "direct_bank_purchase_payable_correction_apply",
            "direct_bank_payable_correction_plan",
            "direct_bank_payable_correction",
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
    fn exact_server_plan_apply_replay_and_errors_have_matching_text_structured_result() {
        tokio::runtime::Runtime::new().unwrap().block_on(async{for(write,value)in[(false,json!({"ok":true,"data":{"plan":{"planHash":"native-proof","sourceProof":{"kept":true}}},"errors":[]})),(true,json!({"ok":true,"data":{"id":4,"idempotent":false,"sourcePrincipal":{"kind":"user","subjectId":"trusted"},"idempotency":{"replayed":true,"receiptId":"server"}},"errors":[]})),(true,json!({"ok":false,"code":"business","errors":["server-owned failure"]}))]{let(origin,t)=server(200,value.clone());let api=ApiClient::new(&origin,true,None).unwrap();let mut args=if write{apply()}else{plan()};if write{args["confirm"]=json!(true);}let result=crate::tools::call(&api,if write{"direct_bank_purchase_payable_correction_apply"}else{"direct_bank_purchase_payable_correction_plan"},args).await.unwrap();assert_eq!(result["structuredContent"],value);assert_eq!(serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),value);assert_eq!(result["isError"],value["ok"]!=true);let(header,body)=t.join().unwrap();assert!(header.contains("/bank/original/direct-payable-"));assert!(body.get("expectedPrincipal").is_none());assert!(body.get("company").is_none());if write{assert_eq!(body["idempotencyKey"]," raw key ");assert_eq!(body["reason"]," raw reason ");}}});
    }
}
