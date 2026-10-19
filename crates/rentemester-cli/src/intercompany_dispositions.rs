//! Explicit evidence-only lifecycle commands; no ledger posting, generated keys or automatic retries.
use rentemester_client::{ApiClient, DispositionAction as Action, MAX_DISPOSITION_BODY};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
fn action(command: &str) -> Option<Action> {
    Some(match command.strip_suffix("-group-disposition")? {
        "plan" => Action::Plan,
        "propose" => Action::Propose,
        "approve" => Action::Approve,
        "link" => Action::Link,
        "settle" => Action::Settle,
        "supersede" => Action::Supersede,
        "reopen" => Action::Reopen,
        _ => return None,
    })
}
pub fn accepts(command: &str) -> bool {
    command == "group-disposition-status" || action(command).is_some()
}
pub fn help() -> Value {
    let mut schemas = serde_json::Map::new();
    for op in [
        Action::Plan,
        Action::Propose,
        Action::Approve,
        Action::Link,
        Action::Settle,
        Action::Supersede,
        Action::Reopen,
    ] {
        schemas.insert(
            format!("{}-group-disposition", op.suffix()),
            rentemester_client::disposition_body_schema(op),
        );
    }
    json!({"usage":"group-disposition-status --id EXACT_ID [--as-of DATE] is global, no company/confirm/body. plan-group-disposition --company ANCHOR reads exact native32KiB stdin JSON without confirmation; propose/approve/link/settle/supersede/reopen-group-disposition require --company ANCHOR --confirm and inner confirm:true. No generated IDs, keys, heads, money or automatic retry. Propose the exact normalized disposition returned by plan, including its ID, so unknown outcomes can be reconciled by readonly exact-ID status. Omitted proposal IDs preserve source server generation but lose direct recovery if the reply is lost. Lifecycle replay is state-specific; settled balanced is false; settlement records evidence only. Both company ledgers must be posted separately before linking either. Later journal writes stale the retained heads; no refresh/waiver exists.","bodySchemas":schemas})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow_http: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, (u8, Error)> {
    let invalid = |s| (2, Error::new("invalid_input", s));
    let status = command == "group-disposition-status";
    let op = action(command);
    let write = op.is_some_and(Action::is_write);
    if confirmed != write
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file"].contains(k)
                || status && ["--id", "--as-of"].contains(k)
                || !status && *k == "--company")
        })
    {
        return Err(invalid(
            "status requires exact --id without company/confirmation; plan forbids confirmation; writes require --confirm and native stdin JSON",
        ));
    }
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let file = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("explicit --cookie-file required"))?;
    let id = if status {
        Some(
            *flags
                .get("--id")
                .ok_or_else(|| invalid("exact --id required"))?,
        )
    } else {
        None
    };
    let company = if status {
        None
    } else {
        Some(
            *flags
                .get("--company")
                .ok_or_else(|| invalid("explicit anchor --company required"))?,
        )
    };
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(file))
        .map_err(crate::remote::client_error)?;
    let api =
        ApiClient::new(server, allow_http, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = Vec::new();
    if !status {
        std::io::stdin()
            .take(MAX_DISPOSITION_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact native disposition JSON"))?;
    }
    let recovery_id = serde_json::from_slice::<Value>(&bytes).ok().and_then(|v| {
        v.get("dispositionId")
            .or_else(|| v.get("disposition").and_then(|d| d.get("dispositionId")))
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let result = runtime.block_on(async {
        if status {
            api.group_disposition_status(id.unwrap(), flags.get("--as-of").copied())
                .await
        } else {
            api.intercompany_disposition(company.unwrap(), op.unwrap(), bytes, confirmed)
                .await
        }
    });
    if result == Err(rentemester_client::Error::OutcomeUnknown) {
        return Ok(
            json!({"ok":false,"code":"outcome_unknown","dispositionId":recovery_id,"errors":["Read canonical exact-ID disposition status before another action; retain original action/body. Never resend automatically or replace a lost server-generated ID."]}),
        );
    }
    let value = result.map_err(crate::remote::client_error)?;
    Ok(json!({"ok":value["ok"].as_bool().unwrap_or(false),"data":value}))
}
