use super::*;
pub(crate) fn tools() -> Vec<Value> {
    let mut tools = vec![];
    for (name, title, read, workspace) in [
        (
            "efaktura_onboarding_status",
            "DigiSense onboarding-status",
            true,
            false,
        ),
        (
            "efaktura_onboard",
            "Onboard ledger company with DigiSense",
            false,
            false,
        ),
        (
            "efaktura_registrer",
            "Registrér virksomhed i NemHandel via Digisense",
            false,
            false,
        ),
        (
            "efaktura_modtag",
            "Modtag e-fakturaer via Digisense",
            false,
            false,
        ),
        (
            "efaktura_modtag_workspace",
            "Poll DigiSense inbound for active workspace companies",
            false,
            true,
        ),
    ] {
        let selector = if workspace { "workspace" } else { "company" };
        let mut properties = json!({selector:{"type":"string","minLength":1}});
        let mut required = vec![selector];
        if !read {
            properties["confirm"] = json!({"type":"boolean"});
        }
        if name == "efaktura_registrer" {
            properties["cvr"] = json!({"type":"string","minLength":1});
            properties["companyName"] = json!({"type":"string","minLength":1});
            properties["network"] = json!({"type":"string","enum":["nemhandel","peppol"]});
            required.extend(["cvr", "companyName"]);
        }
        if name == "efaktura_modtag" {
            properties["digisenseCompanyKey"] = json!({"type":"string","minLength":1});
            properties["limit"] = json!({"type":"integer","minimum":1,"maximum":100});
            properties["maxTimestamp"] = json!({"type":"string","minLength":1});
            properties["metadata"] =
                rentemester_client::digisense_received_schema(false)["properties"]["metadata"]
                    .clone();
            properties["force"] = json!({"type":"boolean"});
        }
        tools.push(json!({"name":name,"title":title,"description":title,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"outputSchema":{"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"data":{},"code":{"type":"string"}}},"annotations":{"readOnlyHint":read,"destructiveHint":false,"idempotentHint":true,"openWorldHint":!read}}));
    }
    tools
}
pub(super) fn validate(name: &str, args: &Value) -> bool {
    let Some(schema) = tools()
        .into_iter()
        .find(|t| t["name"] == name)
        .map(|t| t["inputSchema"].clone())
    else {
        return false;
    };
    let Some(o) = args.as_object() else {
        return false;
    };
    if !schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .all(|k| o.contains_key(k.as_str().unwrap()))
    {
        return false;
    }
    o.iter().all(|(k, v)| {
        let Some(s) = schema["properties"].get(k) else {
            return false;
        };
        match s["type"].as_str() {
            Some("boolean") => v.is_boolean(),
            Some("string") => {
                v.as_str().is_some_and(|v| !v.is_empty())
                    && s.get("enum")
                        .is_none_or(|a| a.as_array().unwrap().contains(v))
            }
            Some("integer") => v.as_u64().is_some_and(|n| (1..=100).contains(&n)),
            Some("object") => v.is_object(),
            _ => false,
        }
    })
}
pub(crate) async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if !validate(name, &args) {
        return Err("invalid original DigiSense flat fields");
    }
    // Missing/false is a normal ToolResult, never a transport or parser failure.
    if name != "efaktura_onboarding_status" && args["confirm"] != true {
        return Ok(Ok(
            json!({"ok":false,"errors":["confirm: true is required"],"code":"CONFIRM_REQUIRED"}),
        ));
    }
    Ok(api.original717_digisense(name, &args).await)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original717_digisense_flat_catalogue_and_confirmation() {
        for (name, args) in [
            ("efaktura_onboarding_status", json!({"company":"synthetic"})),
            ("efaktura_onboard", json!({"company":"synthetic"})),
            (
                "efaktura_registrer",
                json!({"company":"synthetic","cvr":"DK12345678","companyName":"Synthetic","confirm":false}),
            ),
            (
                "efaktura_modtag",
                json!({"company":"synthetic","force":true,"limit":2,"maxTimestamp":"2026-01-01"}),
            ),
            (
                "efaktura_modtag_workspace",
                json!({"workspace":"synthetic","confirm":true}),
            ),
        ] {
            assert!(validate(name, &args));
        }
        assert!(!validate(
            "efaktura_onboard",
            &json!({"company":"synthetic","confirm":"true"})
        ));
        assert!(!validate(
            "efaktura_modtag",
            &json!({"company":"synthetic","body":"{}","confirm":true})
        ));
        for t in tools() {
            assert_eq!(t["annotations"]["destructiveHint"], false);
            assert_eq!(t["annotations"]["idempotentHint"], true);
        }
    }
}

/// Root protocol entrypoint: bypass the native tools.rs double-data wrapper.
pub async fn tool_result(api: &ApiClient, name: &str, args: Value) -> Result<Value, &'static str> {
    let core = match call(api, name, args).await? {
        Ok(v) => v,
        Err(e) => {
            let code = match &e {
                Error::InvalidInput(_) => "invalid_input",
                Error::Transport => "transport",
                Error::Refused { .. } => "server_refused",
                Error::Protocol => "protocol",
                Error::OutcomeUnknown => "outcome_unknown",
            };
            json!({"ok":false,"errors":[e.to_string()],"code":code})
        }
    };
    let envelope = if core["ok"] == false {
        let mut e = json!({"ok":false,"errors":core["errors"]});
        if let Some(code) = core.get("code") {
            e["code"] = code.clone();
        }
        // Failed batches retain partial/unknown evidence for recovery.
        if core.get("documents").is_some() {
            e["data"] = core.clone();
        }
        e
    } else {
        let mut data = core;
        if let Some(o) = data.as_object_mut() {
            o.remove("ok");
        }
        json!({"ok":true,"errors":[],"data":data})
    };
    Ok(
        json!({"isError":envelope["ok"]==false,"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope}),
    )
}
