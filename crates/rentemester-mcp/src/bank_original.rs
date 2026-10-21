//! Original flat bank tools over canonical server envelopes and current authority.
use rentemester_client::{ApiClient, BankOriginalMutation, Error, bank_original_body_schema};
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    let mut tools = vec![];
    for name in [
        "bank_suggest_matches",
        "bank_reconciliation_correction_plan",
        "bank_reconciliation_correction_apply",
    ] {
        let apply = name.ends_with("apply");
        let mut schema = if apply {
            let mut s = bank_original_body_schema(BankOriginalMutation::CorrectionApply);
            s["properties"]
                .as_object_mut()
                .unwrap()
                .remove("expectedPrincipal");
            s["properties"]["confirm"] = json!({"type":"boolean"});
            s["properties"]["planHash"]["pattern"] = json!("^[a-f0-9]{64}$");
            s["properties"]["reason"]["maxLength"] = json!(1000);
            s["properties"]["idempotencyKey"]["maxLength"] = json!(200);
            s["required"] = json!([
                "company",
                "bankTransactionId",
                "replacementJournalEntryId",
                "expectedReconciliationId",
                "planHash",
                "reason",
                "idempotencyKey"
            ]);
            s
        } else {
            let mut properties = json!({"bankTransactionId":id});
            let required = if name == "bank_suggest_matches" {
                properties["max"] = id.clone();
                vec!["company"]
            } else {
                properties["replacementJournalEntryId"] = id.clone();
                vec!["company", "bankTransactionId", "replacementJournalEntryId"]
            };
            json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
        };
        schema["properties"]["company"] = json!({"type":"string","minLength":1});
        let mut tool = crate::tools::tool(
            name,
            "Original flat bank advice/correction contract. Exact server errors[], rules, full source-bound plan and principal-bound receipt. No local score/hash/math/actor, caller principal, automatic matching or retries. Apply requires confirm:true before network; preserve raw key/reason and inspect canonical state after uncertain outcome.",
            schema,
            !apply,
        );
        tool["annotations"] = json!({"readOnlyHint":!apply,"destructiveHint":false,"idempotentHint":!apply,"openWorldHint":false});
        tool["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"data":{"type":"object"},"errors":{"type":"array","items":{"type":"string"}},"appliedRules":{"type":"array","items":{"type":"string"}},"code":{"type":"string"}},"additionalProperties":false});
        tools.push(tool);
    }
    tools
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
    let apply = name == "bank_reconciliation_correction_apply";
    let suggestions = name == "bank_suggest_matches";
    let mut object = args
        .as_object()
        .ok_or("bank tool requires flat object")?
        .clone();
    let allowed = if apply {
        vec![
            "company",
            "bankTransactionId",
            "replacementJournalEntryId",
            "expectedReconciliationId",
            "planHash",
            "reason",
            "idempotencyKey",
            "confirm",
        ]
    } else if suggestions {
        vec!["company", "bankTransactionId", "max"]
    } else {
        vec!["company", "bankTransactionId", "replacementJournalEntryId"]
    };
    if object.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("unknown bank tool field, no actor/principal/body accepted");
    }
    let company = object
        .get("company")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("nonempty company required")?
        .to_owned();
    for key in ["bankTransactionId", "replacementJournalEntryId", "max"] {
        if object.get(key).is_some_and(|v| {
            v.as_i64()
                .is_none_or(|n| !(1..=9007199254740991).contains(&n))
        }) {
            return Err("positive safe integer selectors required");
        }
    }
    if !suggestions
        && ["bankTransactionId", "replacementJournalEntryId"]
            .iter()
            .any(|k| !object.contains_key(*k))
    {
        return Err("bank/replacement required");
    }
    if apply {
        for key in [
            "expectedReconciliationId",
            "planHash",
            "reason",
            "idempotencyKey",
        ] {
            let text = object
                .get(key)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or("nonempty correction fields required")?;
            if key == "planHash"
                && (text.len() != 64
                    || !text
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
                || key == "reason" && text.encode_utf16().count() > 1000
                || key == "idempotencyKey" && text.encode_utf16().count() > 200
            {
                return Err("original correction field bounds differ");
            }
        }
        if object.get("confirm").is_some_and(|v| !v.is_boolean()) {
            return Err("confirm must be boolean");
        }
        if object.get("confirm") != Some(&json!(true)) {
            return Ok(result(
                json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for write tool bank_reconciliation_correction_apply"]}),
            ));
        }
    }
    let outcome = if suggestions {
        api.bank_original_suggestions(
            &company,
            object.get("bankTransactionId").and_then(Value::as_i64),
            object.get("max").and_then(Value::as_u64),
        )
        .await
    } else if !apply {
        api.bank_original_correction_plan(
            &company,
            object["bankTransactionId"].as_i64().unwrap(),
            object["replacementJournalEntryId"].as_i64().unwrap(),
        )
        .await
    } else {
        object.remove("company");
        api.bank_original_mutation(
            &company,
            BankOriginalMutation::CorrectionApply,
            serde_json::to_vec(&object).map_err(|_| "correction JSON unavailable")?,
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
    fn strict_flat_schema_confirmation_and_registry_collisions() {
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let base = json!({"company":"synthetic","bankTransactionId":7,"replacementJournalEntryId":9,"expectedReconciliationId":"current","planHash":"a".repeat(64),"reason":"reason","idempotencyKey":"key"});
        for confirm in [None, Some(false)] {
            let mut args = base.clone();
            if let Some(confirm) = confirm {
                args["confirm"] = json!(confirm);
            }
            assert_eq!(
                rt.block_on(crate::tools::call(
                    &api,
                    "bank_reconciliation_correction_apply",
                    args
                ))
                .unwrap()["structuredContent"]["code"],
                "CONFIRM_REQUIRED"
            );
        }
        for (key, value) in [
            (
                "expectedPrincipal",
                json!({"kind":"user","subjectId":"forged"}),
            ),
            ("actor", json!("forged")),
            ("principal", json!("forged")),
            ("body", json!("{}")),
            ("confirm", Value::Null),
            ("reason", Value::Null),
            ("bankTransactionId", json!("7")),
        ] {
            let mut args = base.clone();
            args[key] = value;
            assert!(
                rt.block_on(call(&api, "bank_reconciliation_correction_apply", args))
                    .is_err()
            );
        }
        for name in [
            "bank_suggest_matches",
            "bank_reconciliation_correction_plan",
            "bank_reconciliation_correction_apply",
            "bank_reconciliation_correction",
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
        assert!(!tools().iter().any(|t| t["name"] == "bank_link_journal"));
    }
    #[test]
    fn canonical_original_reads_and_apply_preserve_exact_text_envelope_and_raw_key() {
        tokio::runtime::Runtime::new().unwrap().block_on(async{
 let base=json!({"company":"synthetic","bankTransactionId":7,"replacementJournalEntryId":9,"expectedReconciliationId":"current","planHash":"a".repeat(64),"reason":" raw reason ","idempotencyKey":" raw key ","confirm":true});
 for(name,args,value)in[("bank_suggest_matches",json!({"company":"synthetic","max":3}),json!({"ok":true,"data":{"count":0,"rows":[]},"errors":[]})),("bank_reconciliation_correction_plan",json!({"company":"synthetic","bankTransactionId":7,"replacementJournalEntryId":9}),json!({"ok":true,"data":{"plan":{"planHash":"server","sourceProof":{"kept":true}}},"errors":[]})),("bank_reconciliation_correction_apply",base,json!({"ok":false,"code":"stale","errors":["server refusal"],"appliedRules":["rule"]}))]{let(origin,t)=server(200,value.clone());let api=ApiClient::new(&origin,true,None).unwrap();let result=crate::tools::call(&api,name,args).await.unwrap();assert_eq!(result["structuredContent"],value);assert_eq!(serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),value);assert_eq!(result["isError"],value["ok"]!=true);let(header,body)=t.join().unwrap();assert!(header.contains("/bank/original/"));if name.ends_with("apply"){assert_eq!(body["reason"]," raw reason ");assert_eq!(body["idempotencyKey"]," raw key ");assert!(body.get("expectedPrincipal").is_none());assert!(body.get("company").is_none());}}
});
    }
}
