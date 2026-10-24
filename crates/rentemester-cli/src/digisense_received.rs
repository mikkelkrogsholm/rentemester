//! Explicit remote receive; never local provider configuration or replay.
use rentemester_client::{ApiClient, MAX_DIGISENSE_RECEIVED_BODY, digisense_received_schema};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
pub const COMMANDS: [&str; 2] = ["efaktura-modtag", "efaktura-modtag-workspace"];
pub fn accepts(command: &str) -> bool {
    COMMANDS.contains(&command)
}
pub fn help() -> Value {
    json!({"commands":COMMANDS,"usage":"efaktura-modtag --server HTTPS_ORIGIN --cookie-file PRIVATE_COOKIE --company SLUG --confirm < native.json; efaktura-modtag-workspace --server HTTPS_ORIGIN --cookie-file PRIVATE_COOKIE --confirm < native.json","policy":"Exact bounded16KiB stdin JSON once, inner+outer confirm:true. Explicit poll only. Current server authority and backup fences, no files/provider credentials/keys/timer/posting/send. Workspace body is only {confirm:true}. After outcome_unknown inspect authoritative documents/dedup; no automatic replay.","bodySchemas":{"company":digisense_received_schema(false),"workspace":digisense_received_schema(true)}})
}
fn invalid() -> (u8, Error) {
    (
        2,
        Error::new(
            "invalid_input",
            "receive requires explicit server/private cookie, company only for company poll, --confirm and bounded native stdin JSON",
        ),
    )
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let workspace = command == COMMANDS[1];
    if !accepts(command)
        || !confirmed
        || preview
        || flags
            .keys()
            .any(|k| !["--server", "--cookie-file", "--company"].contains(k))
        || workspace && flags.contains_key("--company")
    {
        return Err(invalid());
    }
    let get = |key| flags.get(key).copied().ok_or_else(invalid);
    let server = get("--server")?;
    let company = if workspace {
        None
    } else {
        Some(get("--company")?)
    };
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(get("--cookie-file")?))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = vec![];
    std::io::stdin()
        .take(MAX_DIGISENSE_RECEIVED_BODY as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid())?;
    let value = rt.block_on(async {
        if let Some(company) = company {
            api.digisense_received(company, bytes, confirmed).await
        } else {
            api.digisense_received_workspace(bytes, confirmed).await
        }
    });
    if value == Err(rentemester_client::Error::OutcomeUnknown) {
        return Ok(
            json!({"ok":false,"code":"outcome_unknown","errors":["Inspect authoritative documents/dedup before further action; no automatic replay."]}),
        );
    }
    let value = value.map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn digisense_received_confirmation_selectors() {
        assert!(accepts("efaktura-modtag"));
        assert!(run("efaktura-modtag", &BTreeMap::new(), false, false, false).is_err());
        let flags = BTreeMap::from([("--company", "synthetic")]);
        assert!(run("efaktura-modtag-workspace", &flags, false, true, false).is_err());
    }
}
