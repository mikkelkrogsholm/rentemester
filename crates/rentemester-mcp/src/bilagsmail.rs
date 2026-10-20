//! Current mailbox status and explicit receive-only configuration/polling. No stored human credentials.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, BilagsmailMutation as Action, Error, MAX_BILAGSMAIL_BODY, bilagsmail_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "bilagsmail"
            | "configure_bilagsmail"
            | "delete_bilagsmail_config"
            | "set_bilagsmail_alias"
            | "poll_bilagsmail"
    )
}
pub fn tools() -> Vec<Value> {
    let mut tools = vec![tool(
        "bilagsmail",
        "Read authorized safe mailbox configuration status, alias, retained document rows and polling status. Never provider I/O, no password/config secrets or writes.",
        json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
        true,
    )];
    for (name, action) in [
        ("configure_bilagsmail", Action::Configure),
        ("delete_bilagsmail_config", Action::DeleteConfig),
        ("set_bilagsmail_alias", Action::Alias),
        ("poll_bilagsmail", Action::Poll),
    ] {
        let mut schema = json!({"type":"object","required":["company","confirm"],"properties":{"company":company_schema(),"confirm":{"const":true}},"additionalProperties":false});
        if action != Action::DeleteConfig {
            let mut body = native_body(bilagsmail_body_schema(action));
            body["maxLength"] = json!(MAX_BILAGSMAIL_BODY);
            if action == Action::Configure {
                body["writeOnly"] = json!(true);
            }
            schema["properties"]["body"] = body;
            schema["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("body"));
        }
        tools.push(tool(name,match action{
            Action::Configure=>"Current company-admin confirmation required. Save exact source mailbox configuration from bounded16KiB private JSON body; password never returned or copied to examples. No provider I/O on save, no fabricated human session or implicit accounting. Optional receive-only service binding is operator/current-authority controlled.",
            Action::DeleteConfig=>"Explicit current company-admin confirmation deletes mailbox configuration, invalidating admitted polls. No body, key or automatic retry. Read status/inbox after uncertain outcome.",
            Action::Alias=>"Set/clear explicit nullable mailbox alias from exact16KiB native JSON {alias}; current company-admin confirmation. No inferred routing or provider work.",
            Action::Poll=>"One explicit receive-only IMAP pass, exact {confirm:true} body plus outer confirm:true, current company-admin/MFA. Retains original EML/attachments and truthful exceptions; requires separate metadata review/handoff. No metadata inference, extraction, posting or email sending. After uncertainty read status/inbox before another explicit operation; never automatic retry."
        },schema,false));
    }
    tools
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Delete {
    company: String,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    company: String,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    if name == "bilagsmail" {
        let i: Read =
            serde_json::from_value(args).map_err(|_| "bilagsmail requires only company")?;
        return Ok(api.bilagsmail(&i.company).await);
    }
    if name == "delete_bilagsmail_config" {
        let i: Delete = serde_json::from_value(args)
            .map_err(|_| "mailbox delete requires only company/confirm:true")?;
        return Ok(api
            .mutate_bilagsmail(&i.company, Action::DeleteConfig, vec![], i.confirm)
            .await);
    }
    let i: Write = serde_json::from_value(args)
        .map_err(|_| "mailbox write requires only company/exact bounded body/confirm:true")?;
    let action = match name {
        "configure_bilagsmail" => Action::Configure,
        "set_bilagsmail_alias" => Action::Alias,
        "poll_bilagsmail" => Action::Poll,
        _ => return Err("unknown mailbox operation"),
    };
    Ok(api
        .mutate_bilagsmail(&i.company, action, i.body.into_bytes(), i.confirm)
        .await)
}
