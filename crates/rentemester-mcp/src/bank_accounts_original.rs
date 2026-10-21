//! Original flat bank_account_update over current shared master-data authority.
use rentemester_client::{
    ApiClient, BankAccountOriginalMutation, Error, bank_account_original_schema,
};
use serde_json::{Value, json};
pub fn tools() -> Vec<Value> {
    let mut schema = bank_account_original_schema(BankAccountOriginalMutation::Update);
    schema["properties"]["company"] = json!({"type":"string","minLength":1});
    schema["properties"]["confirm"] = json!({"type":"boolean"});
    schema["properties"]["name"]["minLength"] = json!(1);
    schema["required"] = json!(["company", "account"]);
    let mut tool = crate::tools::tool(
        "bank_account_update",
        "Update payment profile or active status under current CompanyMasterData authority. Omitted fields remain unchanged, empty payment strings clear. Identity/history and all validation stay on server; no slug mutation, delete, account lookup, actor or automatic retry. Missing/false confirmation refuses before network.",
        schema,
        false,
    );
    tool["annotations"] = json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false});
    tool["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"data":{"type":"object"},"code":{"type":"string"}},"additionalProperties":false});
    vec![tool]
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
pub async fn call(api: &ApiClient, args: Value) -> Result<Value, &'static str> {
    let mut object = args
        .as_object()
        .ok_or("bank_account_update requires object")?
        .clone();
    for (key, value) in &object {
        match key.as_str() {
            "confirm" | "active" => {
                if !value.is_boolean() {
                    return Err("confirm/active must be boolean");
                }
            }
            "company" | "account" | "name" | "bankName" | "registrationNo" | "accountNo"
            | "iban" | "bic" | "accountOwner" | "customerNo" | "currency" | "ledgerAccountNo" => {
                let text = value
                    .as_str()
                    .ok_or("optional strings reject null/nonstrings")?;
                if matches!(key.as_str(), "company" | "account" | "name") && text.is_empty() {
                    return Err("company/account/name must be nonempty");
                }
            }
            _ => return Err("unknown bank_account_update field"),
        }
    }
    for key in ["company", "account"] {
        if !object.contains_key(key) {
            return Err("company and account required");
        }
    }
    if object.get("confirm") != Some(&json!(true)) {
        return Ok(result(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for write tool bank_account_update"]}),
        ));
    }
    let company = object
        .remove("company")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let body = serde_json::to_vec(&object).map_err(|_| "account body unavailable")?;
    Ok(
        match api
            .bank_account_original(&company, BankAccountOriginalMutation::Update, body)
            .await
        {
            Ok(value) => result(value),
            Err(error) => failure(error),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_flat_schema_confirmation_and_no_create_alias() {
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        for confirm in [None, Some(false)] {
            let mut args = json!({"company":"synthetic","account":"8"});
            if let Some(confirm) = confirm {
                args["confirm"] = json!(confirm);
            }
            let result = rt
                .block_on(crate::tools::call(&api, "bank_account_update", args))
                .unwrap();
            assert_eq!(result["structuredContent"]["code"], "CONFIRM_REQUIRED");
            assert_eq!(result["isError"], true);
        }
        for args in [
            json!({"company":"synthetic","account":"8","active":"false"}),
            json!({"company":"synthetic","account":"8","iban":null}),
            json!({"company":"synthetic","account":"8","confirm":null}),
            json!({"company":"synthetic","account":"8","slug":"new"}),
            json!({"company":"synthetic","account":"8","name":""}),
            json!({"company":"synthetic","account":8}),
            json!({"company":"synthetic","account":"8","body":"{}"}),
        ] {
            assert!(rt.block_on(call(&api, args)).is_err());
        }
        let list = crate::tools::list();
        let tools = list["tools"].as_array().unwrap();
        assert_eq!(
            tools
                .iter()
                .filter(|t| t["name"] == "bank_account_update")
                .count(),
            1
        );
        for alias in [
            "bank_account_create",
            "bank_account_add",
            "bank_account_delete",
        ] {
            assert!(!tools.iter().any(|t| t["name"] == alias));
        }
        assert_eq!(super::tools()[0]["annotations"]["idempotentHint"], false);
    }
    async fn server_patch_and_exact_envelope() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        for value in [
            json!({"ok":true,"data":{"account":{"id":8,"slug":"stable","iban":null,"active":false,"createdAt":"server"}},"errors":[]}),
            json!({"ok":false,"errors":["server-owned refusal"]}),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let response = value.to_string();
            let thread =
                thread::spawn(move || {
                    let (mut s, _) = listener.accept().unwrap();
                    s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                        .unwrap();
                    let mut request = vec![];
                    let end = loop {
                        let mut b = [0; 1024];
                        let n = s.read(&mut b).unwrap();
                        assert!(n > 0);
                        request.extend_from_slice(&b[..n]);
                        if let Some(i) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                            break i + 4;
                        }
                    };
                    let header = String::from_utf8_lossy(&request[..end]);
                    assert!(header.starts_with(
                        "POST /api/companies/synthetic/bank/accounts-original/update "
                    ));
                    let length: usize = header
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap();
                    while request.len() < end + length {
                        let mut b = [0; 1024];
                        let n = s.read(&mut b).unwrap();
                        assert!(n > 0);
                        request.extend_from_slice(&b[..n]);
                    }
                    write!(
                    s,
                    "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{response}",
                    response.len()
                )
                .unwrap();
                    serde_json::from_slice::<Value>(&request[end..]).unwrap()
                });
            let api = ApiClient::new(&origin, true, None).unwrap();
            let result=call(&api,json!({"company":"synthetic","account":" 8 ","iban":"","active":false,"confirm":true})).await.unwrap();
            assert_eq!(result["structuredContent"], value);
            assert_eq!(
                serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap())
                    .unwrap(),
                value
            );
            assert_eq!(result["isError"], value["ok"] != true);
            assert_eq!(
                thread.join().unwrap(),
                json!({"confirm":true,"account":" 8 ","iban":"","active":false})
            );
        }
    }

    #[test]
    fn exact_patch_receipt_and_errors() {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(server_patch_and_exact_envelope());
    }
}
