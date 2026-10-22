//! Original flat claim tools coexist with exact existing native body-string alternatives.
use rentemester_client::{
    ApiClient, Error, OriginalInvoiceClaimKind as Kind, invoice_claims_original_schema,
    validate_invoice_claims_original,
};
use serde_json::{Value, json};
fn name(kind: Kind) -> &'static str {
    match kind {
        Kind::Remind => "invoice_remind",
        Kind::PostReminder => "invoice_post_reminder",
        Kind::Compensation => "invoice_compensation_calc",
        Kind::ClaimCompensation => "invoice_claim_compensation",
        Kind::PostCompensation => "invoice_post_compensation",
        Kind::Interest => "invoice_interest_calc",
        Kind::ClaimInterest => "invoice_claim_interest",
        Kind::PostInterest => "invoice_post_interest",
        Kind::InterestCorrection => "invoice_interest_correction_calc",
        Kind::PostInterestCorrection => "invoice_post_interest_correction",
        Kind::SettleClaimBank => "invoice_settle_claim_bank",
    }
}
fn kind(tool: &str) -> Option<Kind> {
    Kind::ALL.into_iter().find(|k| name(*k) == tool)
}
fn native(tool: &str) -> bool {
    crate::invoice_compensation::accepts(tool) || crate::invoice_interest::accepts(tool)
}
pub fn accepts(tool: &str, args: &Value) -> bool {
    kind(tool).is_some() && (!native(tool) || args.get("body").is_none())
}
fn schema(kind: Kind) -> Value {
    let mut schema = invoice_claims_original_schema(kind);
    schema["properties"]["company"] = crate::tools::company_schema();
    schema["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("company"));
    if !kind.is_read_only() {
        schema["properties"]["confirm"] = json!({"type":"boolean"});
        schema["required"]
            .as_array_mut()
            .unwrap()
            .retain(|v| v != "confirm");
    }
    if matches!(kind, Kind::Interest | Kind::ClaimInterest) {
        schema["required"]
            .as_array_mut()
            .unwrap()
            .push(json!("referenceRate"));
    }
    for key in ["date", "asOf"] {
        if schema["required"].as_array().unwrap().contains(&json!(key)) {
            schema["properties"][key]["minLength"] = json!(1);
        }
    }
    schema
}
pub fn register(tools: &mut Vec<Value>) {
    for kind in Kind::ALL {
        let tool_name = name(kind);
        let flat = schema(kind);
        let description = "Original flat alternative: server owns invoice selectors, lawful eligibility/rates/amounts, causal correction evidence, original account allocation and all source/authority/period gates. Read calculations have no confirm/effects. Writes require confirm:true before network. Registrations do not post. Explicit compatible receivable/income accounts for posting, explicit reason for correction. Flat interest requires referenceRate; CLI omission retains statutory table. Omitted fee is server100DKK; omitted reminderId is server earliest including posted, never auto-next. No identities, keys, inferred accounts, math, email or retries.";
        if let Some(existing) = tools.iter_mut().find(|t| t["name"] == tool_name) {
            let old = existing["inputSchema"].take();
            existing["inputSchema"] = json!({"type":"object","oneOf":[old,flat]});
            let old = existing["description"].as_str().unwrap_or("");
            existing["description"] = json!(format!(
                "{old} {description} Native body and original flat fields are mutually exclusive; native output stays unchanged."
            ));
        } else {
            let mut tool = crate::tools::tool(tool_name, description, flat, kind.is_read_only());
            tool["annotations"] = json!({"readOnlyHint":kind.is_read_only(),"idempotentHint":kind.is_read_only(),"destructiveHint":false,"openWorldHint":false});
            tool["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"errors":{"type":"array","items":{"type":"string"}},"code":{"type":"string"},"appliedRules":{"type":"array","items":{"type":"string"}}},"additionalProperties":false});
            tools.push(tool);
        }
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
pub async fn call(api: &ApiClient, tool: &str, args: Value) -> Result<Value, &'static str> {
    let kind = kind(tool).ok_or("unknown original claim tool")?;
    let mut object = args.as_object().ok_or("expected flat object")?.clone();
    let company = object
        .remove("company")
        .and_then(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned))
        .ok_or("nonempty company required")?;
    if object.get("confirm").is_some_and(|v| !v.is_boolean()) {
        return Err("confirm must be boolean");
    }
    if matches!(kind, Kind::Interest | Kind::ClaimInterest) && !object.contains_key("referenceRate")
    {
        return Err("original flat interest requires referenceRate");
    }
    for key in ["date", "asOf"] {
        if schema(kind)["required"]
            .as_array()
            .unwrap()
            .contains(&json!(key))
            && object.get(key).and_then(Value::as_str) == Some("")
        {
            return Err("required date/asOf must not be empty");
        }
    }
    let confirmed = object.get("confirm") == Some(&json!(true));
    if !kind.is_read_only() {
        object.insert("confirm".into(), json!(true));
    }
    let bytes = serde_json::to_vec(&object).map_err(|_| "claim payload unavailable")?;
    validate_invoice_claims_original(kind, &bytes).map_err(
        |_| "original flat claim schema differs: null/unknown/type/identity fields refused",
    )?;
    if !kind.is_read_only() && !confirmed {
        return Ok(result(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {tool}")]}),
        ));
    }
    Ok(
        match api.invoice_claims_original(&company, kind, bytes).await {
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

    fn body(kind: Kind) -> Value {
        use Kind::*;
        let mut value = match kind {
            Remind => {
                json!({"documentId":7,"invoiceNumber":" fallback ","date":"2026-02-03","note":" raw note "})
            }
            PostReminder => {
                json!({"invoiceNumber":"INV","receivableAccountNo":"debtor","reminderIncomeAccountNo":"reminder-income"})
            }
            Compensation => json!({"documentId":7,"asOf":"2026-02-03","amountDkk":250}),
            ClaimCompensation => json!({"documentId":7,"asOf":"2026-02-03","note":" raw note "}),
            PostCompensation => {
                json!({"documentId":7,"date":"2026-02-03","receivableAccountNo":"debtor","compensationIncomeAccountNo":"comp-income"})
            }
            Interest => json!({"documentId":7,"asOf":"2026-02-03"}),
            ClaimInterest => {
                json!({"documentId":7,"asOf":"2026-02-03","referenceRate":2.65,"note":" raw note "})
            }
            PostInterest => {
                json!({"documentId":7,"claimId":9,"receivableAccountNo":"debtor","interestIncomeAccountNo":"interest-income"})
            }
            InterestCorrection => json!({"documentId":7}),
            PostInterestCorrection => json!({"documentId":7,"reason":" reviewed reason "}),
            SettleClaimBank => {
                json!({"payload":{"invoiceDocumentId":7,"invoiceNumber":" fallback ","bankTransactionReference":" raw bank ref ","paymentDate":"2026-02-03","amount":12.34,"bankAccountNo":"bank","receivableAccountNo":"historical-debtor"}})
            }
        };
        if !kind.is_read_only() {
            value["confirm"] = json!(true);
        }
        value
    }

    #[test]
    fn all_eleven_original_flat_tools_preserve_envelopes_confirmation_rate_and_annotations() {
        tokio::runtime::Runtime::new().unwrap().block_on(async{
 let api=ApiClient::new("http://127.0.0.1:9",true,None).unwrap();let listed=crate::tools::list();
 for kind in Kind::ALL {
 let tool=name(kind);let mut args=body(kind);args["company"]=json!("synthetic");if kind==Kind::Interest{args["referenceRate"]=json!(2.65);}
 let descriptor=listed["tools"].as_array().unwrap().iter().find(|t|t["name"]==tool).unwrap();assert_eq!(listed["tools"].as_array().unwrap().iter().filter(|t|t["name"]==tool).count(),1);assert_eq!(descriptor["annotations"]["readOnlyHint"],kind.is_read_only());
 if !kind.is_read_only(){for confirm in [None,Some(false)]{let mut no=args.clone();if let Some(c)=confirm{no["confirm"]=json!(c);}else{no.as_object_mut().unwrap().remove("confirm");}let out=crate::tools::call(&api,tool,no).await.unwrap();assert_eq!(out["structuredContent"]["code"],"CONFIRM_REQUIRED");assert_eq!(out["isError"],true);}}
 let value=if kind==Kind::Interest{json!({"ok":false,"code":"unsupported_foreign_interest","errors":["unsupported foreign interest"]})}else{json!({"ok":true,"data":{"sourceOperation":kind.suffix(),"claimOpenBalance":null,"claimBalanceDkk":17,"principalCurrency":"EUR","canPost":false,"requiresRefund":true},"errors":[],"appliedRules":["source-rule"]})};let(origin,t)=server(200,value.clone());let api=ApiClient::new(&origin,true,None).unwrap();let out=crate::tools::call(&api,tool,args.clone()).await.unwrap();assert_eq!(out["structuredContent"],value);assert_eq!(serde_json::from_str::<Value>(out["content"][0]["text"].as_str().unwrap()).unwrap(),value);assert_eq!(out["isError"],value["ok"]!=true);let(header,actual)=t.join().unwrap();assert!(header.starts_with(&format!("POST /api/companies/synthetic/invoices/original/{} ",kind.suffix())));args.as_object_mut().unwrap().remove("company");assert_eq!(actual,args);
 }
 for args in [json!({"company":"synthetic","documentId":7,"asOf":"2026-01-01"}),json!({"company":"synthetic","documentId":7,"asOf":"2026-01-01","referenceRate":null}),json!({"company":"synthetic","documentId":7,"asOf":"2026-01-01","referenceRate":2,"confirm":true}),json!({"company":"synthetic","documentId":7,"asOf":"2026-01-01","referenceRate":2,"actor":"forged"})]{assert!(crate::tools::call(&api,"invoice_interest_calc",args).await.is_err());}
 let(origin,t)=server(503,json!({"ok":false}));let api=ApiClient::new(&origin,true,None).unwrap();let out=crate::tools::call(&api,"invoice_remind",json!({"company":"synthetic","documentId":7,"date":"2026-01-01","confirm":true})).await.unwrap();assert_eq!(out["structuredContent"]["code"],"outcome_unknown");assert!(out["structuredContent"].get("data").is_none());t.join().unwrap();
 });
    }
    #[test]
    fn eight_native_body_alternatives_remain_exact_and_mixtures_never_fall_back() {
        tokio::runtime::Runtime::new().unwrap().block_on(async{
 for(kind,route,native_body)in[
 (Kind::Compensation,"compensation/calculate",json!({"invoiceDocumentId":7,"asOfDate":"2026-01-01"})),
 (Kind::ClaimCompensation,"compensation/register",json!({"invoiceDocumentId":7,"asOfDate":"2026-01-01","confirm":true})),
 (Kind::PostCompensation,"compensation/post",json!({"invoiceDocumentId":7,"receivableAccountNo":"debtor","compensationIncomeAccountNo":"income","confirm":true})),
 (Kind::Interest,"interest/calculate",json!({"invoiceDocumentId":7,"asOfDate":"2026-01-01"})),
 (Kind::ClaimInterest,"interest/register",json!({"invoiceDocumentId":7,"asOfDate":"2026-01-01","confirm":true})),
 (Kind::PostInterest,"interest/post",json!({"invoiceDocumentId":7,"receivableAccountNo":"debtor","interestIncomeAccountNo":"income","confirm":true})),
 (Kind::InterestCorrection,"interest/correction",json!({"invoiceDocumentId":7})),
 (Kind::PostInterestCorrection,"interest/post-correction",json!({"invoiceDocumentId":7,"reason":"reviewed","confirm":true}))] {
 let tool=name(kind);let mut args=json!({"company":"synthetic","body":native_body.to_string()});if !kind.is_read_only(){args["confirm"]=json!(true);}
 let value=json!({"ok":true,"nativeMarker":tool});let(origin,t)=server(200,value.clone());let api=ApiClient::new(&origin,true,None).unwrap();let out=crate::tools::call(&api,tool,args.clone()).await.unwrap();assert_eq!(out["structuredContent"],json!({"ok":true,"data":value}));let(header,actual)=t.join().unwrap();assert!(header.starts_with(&format!("POST /api/companies/synthetic/invoices/{route} ")));assert_eq!(actual,native_body);
 let api=ApiClient::new("http://127.0.0.1:9",true,None).unwrap();for(key,v)in[("documentId",json!(7)),("asOf",json!("2026-01-01")),("payload",json!({})),("referenceRate",json!(2))]{let mut mixed=args.clone();mixed[key]=v;assert!(crate::tools::call(&api,tool,mixed).await.is_err());}let mut null=args.clone();null["body"]=Value::Null;assert!(crate::tools::call(&api,tool,null).await.is_err());
 let listed=crate::tools::list();let descriptor=listed["tools"].as_array().unwrap().iter().find(|t|t["name"]==tool).unwrap();let alternatives=descriptor["inputSchema"]["oneOf"].as_array().unwrap();assert_eq!(alternatives.len(),2);let original=crate::invoice_compensation::tools().into_iter().chain(crate::invoice_interest::tools()).find(|t|t["name"]==tool).unwrap();assert_eq!(alternatives[0],original["inputSchema"]);assert_eq!(alternatives[1],schema(kind));
 }
 });
    }
    #[test]
    fn all_original_claim_tools_preserve_422_receipts_and_refuse_untrusted_errors() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            for kind in Kind::ALL {
                let mut args = body(kind);
                args["company"] = json!("synthetic");
                if kind == Kind::Interest { args["referenceRate"] = json!(2.65); }
                let receipt = json!({"ok":false,"code":" server_claim_gate ","errors":[" exact server error ","second error"],"appliedRules":["source-rule"]});
                for (status, value, accepted) in [
                    (422,receipt.clone(),true),
                    (200,json!({"ok":false,"errors":["legacy refusal"]}),true),
                    (403,receipt.clone(),false),
                    (422,json!({"ok":true,"data":{},"code":"remote-secret","errors":[]}),false),
                    (422,json!({"ok":false,"data":{},"code":"remote-secret","errors":["remote-secret"]}),false),
                    (422,json!({"ok":false,"errors":["remote-secret"]}),false),
                    (422,json!({"ok":false,"code":" ","errors":["remote-secret"]}),false),
                    (422,json!({"ok":false,"code":"remote-secret","errors":[12]}),false),
                    (422,json!({"ok":false,"code":"remote-secret","errors":["remote-secret"],"appliedRules":false}),false),
                ] {
                    let (origin,t) = server(status,value.clone());
                    let api = ApiClient::new(&origin,true,None).unwrap();
                    let out = crate::tools::call(&api,name(kind),args.clone()).await.unwrap();
                    assert_eq!(out["isError"],true);
                    let actual = &out["structuredContent"];
                    assert!(actual.get("data").is_none());
                    assert_eq!(serde_json::from_str::<Value>(out["content"][0]["text"].as_str().unwrap()).unwrap(),*actual);
                    if accepted { assert_eq!(*actual,value); }
                    else {
                        assert_eq!(actual["code"],"server_refused");
                        assert!(!out.to_string().contains("remote-secret"));
                        assert!(!out.to_string().contains("server_claim_gate"));
                    }
                    let (_,mut actual_body) = t.join().unwrap();
                    let mut expected = args.clone();
                    expected.as_object_mut().unwrap().remove("company");
                    assert_eq!(actual_body,expected);
                    if kind.is_read_only() { assert!(actual_body.as_object_mut().unwrap().remove("confirm").is_none()); }
                }
            }
        });
    }
}
