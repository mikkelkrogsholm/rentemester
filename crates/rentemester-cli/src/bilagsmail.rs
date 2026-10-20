//! Receive-only mailbox commands. Credentials arrive only as bounded stdin JSON.
use rentemester_client::{
    ApiClient, BilagsmailMutation as Action, MAX_BILAGSMAIL_BODY, bilagsmail_body_schema,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid() -> Failure {
    (
        2,
        Error::new(
            "invalid_input",
            "expected bilagsmail command with explicit server/company/private cookie; writes require --confirm and bounded exact stdin JSON except bodyless delete",
        ),
    )
}
pub fn accepts(command: &str) -> bool {
    matches!(
        command,
        "bilagsmail"
            | "configure-bilagsmail"
            | "delete-bilagsmail-config"
            | "set-bilagsmail-alias"
            | "poll-bilagsmail"
    )
}
pub fn help() -> Value {
    json!({"usage":"bilagsmail --company SLUG; configure-bilagsmail/set-bilagsmail-alias/poll-bilagsmail --company SLUG --confirm < native.json; delete-bilagsmail-config --company SLUG --confirm (no body). All use explicit --server and private --cookie-file.","policy":"Mailbox credentials belong only in bounded16KiB stdin JSON, never command arguments or output. Configure uses source host/port/username/password/secure?/mailbox?; alias exact nullable alias; poll exact {confirm:true}. No generated keys, retries, local mail access, inferred metadata or posting. After unknown outcomes inspect bilagsmail and workspace-inbox before another explicit action. Status never contacts provider. Runtime configuration/grant and service executor are server-owned.","bodySchemas":{"configure":bilagsmail_body_schema(Action::Configure),"alias":bilagsmail_body_schema(Action::Alias),"poll":bilagsmail_body_schema(Action::Poll)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = command == "bilagsmail";
    if !accepts(command)
        || confirmed == read
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file", "--company"].contains(k))
    {
        return Err(invalid());
    }
    let field = |key| flags.get(key).copied().ok_or_else(invalid);
    let cookie =
        rentemester_client::read_cookie_file(std::path::Path::new(field("--cookie-file")?))
            .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(field("--server")?, allow, Some(cookie))
        .map_err(crate::remote::client_error)?;
    let company = field("--company")?;
    let action = match command {
        "configure-bilagsmail" => Action::Configure,
        "delete-bilagsmail-config" => Action::DeleteConfig,
        "set-bilagsmail-alias" => Action::Alias,
        _ => Action::Poll,
    };
    let mut bytes = Vec::new();
    if !read && action != Action::DeleteConfig {
        std::io::stdin()
            .take(MAX_BILAGSMAIL_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid())?;
    }
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid())?;
    let result = rt.block_on(async {
        if read {
            api.bilagsmail(company).await
        } else {
            api.mutate_bilagsmail(company, action, bytes, confirmed)
                .await
        }
    });
    if result == Err(rentemester_client::Error::OutcomeUnknown) {
        return Ok(
            json!({"ok":false,"code":"outcome_unknown","errors":["Mail operation outcome unknown; inspect bilagsmail/workspace-inbox before another explicit action. No automatic replay."]}),
        );
    }
    let result = result.map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result["ok"].as_bool().unwrap_or(false),"data":result}))
}
