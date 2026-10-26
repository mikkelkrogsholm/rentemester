//! Original tasks family, forwarding typed task DTOs to the single Rust server.
use rentemester_client::{
    ApiClient, MAX_TASK_BODY, TASK_OPERATIONS, task_input_schema, task_writes,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn help() -> Value {
    json!({"usage":"tasks list|get|history|receipt|create|update|move|complete|reopen --server HTTPS_ORIGIN (--cookie-file PRIVATE_PATH | --service-key-file PRIVATE_PATH) [--input task.json] [--confirm]. Global --local-profile joins an existing local server instead. Input is original typed JSON, from --input or stdin (list defaults to {}).","policy":"Current verified identity and all company memberships before receipt replay; caller idempotencyKey and positive expectedVersion on existing-task writes. No caller actor/local grant, ledger opening, background workers, generated keys or automatic retry. After outcome_unknown retain the exact input/key; inspect receipt/history before another action. Source completion remains refused until native authoritative sources are ported. Custom boards use explicit scoped preview and confirmed versioned save; automatic runtime remains unfinished. Series use explicit projection/materialization, never automatic retries.","inputSchemas":TASK_OPERATIONS.iter().filter(|op| !op.starts_with("series-") && !op.starts_with("boards-")).map(|op|(*op,task_input_schema(op))).collect::<BTreeMap<_,_>>()})
}
pub fn series_help() -> Value {
    let mut value = help();
    value["usage"] = json!(
        "task-series list|save|project|materialize --server HTTPS_ORIGIN (--cookie-file PRIVATE_PATH | --service-key-file PRIVATE_PATH) [--input series.json] [--confirm]. Same private local-profile transport; input from --input or stdin (list defaults to {}). Save requires expectedVersion 0 for new series or current positive version; writes retain the caller idempotencyKey."
    );
    value["inputSchemas"] = json!(
        TASK_OPERATIONS
            .iter()
            .filter(|op| op.starts_with("series-"))
            .map(|op| (op.trim_start_matches("series-"), task_input_schema(op)))
            .collect::<BTreeMap<_, _>>()
    );
    value
}
pub fn run_boards(args: &[String]) -> Result<Value, Failure> {
    if args.is_empty() || args == ["--help"] || args.get(1).is_some_and(|s| s == "--help") {
        let mut value = help();
        value["usage"] = json!(
            "task-boards list|preview|save --server HTTPS_ORIGIN (--cookie-file PRIVATE_PATH | --service-key-file PRIVATE_PATH) [--input board.json] [--confirm]. Preview is read-only and refuses confirmation; save requires expectedVersion0/new or current version and original idempotencyKey."
        );
        value["inputSchemas"] = json!(
            TASK_OPERATIONS
                .iter()
                .filter(|op| op.starts_with("boards-"))
                .map(|op| (op.trim_start_matches("boards-"), task_input_schema(op)))
                .collect::<BTreeMap<_, _>>()
        );
        return Ok(value);
    }
    if !["list", "preview", "save"].contains(&args[0].as_str()) {
        return Err(invalid("unsupported native task-boards operation"));
    }
    let mut mapped = args.to_vec();
    mapped[0] = format!("boards-{}", args[0]);
    run_operation(&mapped)
}
pub fn run_series(args: &[String]) -> Result<Value, Failure> {
    if args.is_empty() || args == ["--help"] || args.get(1).is_some_and(|s| s == "--help") {
        return Ok(series_help());
    }
    if !["list", "save", "project", "materialize"].contains(&args[0].as_str()) {
        return Err(invalid(
            "unsupported native task-series operation; see task-series --help",
        ));
    }
    let mut mapped = args.to_vec();
    mapped[0] = format!("series-{}", args[0]);
    run_operation(&mapped)
}
pub fn run(args: &[String]) -> Result<Value, Failure> {
    if args
        .first()
        .is_some_and(|s| s.starts_with("series-") || s.starts_with("boards-"))
    {
        return Err(invalid("use task-series list|save|project|materialize"));
    }
    run_operation(args)
}
fn run_operation(args: &[String]) -> Result<Value, Failure> {
    if args.is_empty() || args == ["--help"] || args.get(1).is_some_and(|s| s == "--help") {
        return Ok(help());
    }
    let op = &args[0];
    if !TASK_OPERATIONS.contains(&op.as_str()) {
        return Err(invalid(
            "unsupported native task operation; see tasks --help",
        ));
    }
    let mut flags = BTreeMap::new();
    let mut confirm = false;
    let mut allow = false;
    let mut cursor = 1;
    while cursor < args.len() {
        let arg = args[cursor].as_str();
        if arg == "--confirm" {
            if confirm {
                return Err(invalid("duplicate confirmation"));
            }
            confirm = true;
            cursor += 1;
            if args.get(cursor).is_some_and(|s| s == "yes") {
                cursor += 1;
            }
            continue;
        }
        if arg == "--allow-loopback-http" {
            if allow {
                return Err(invalid("duplicate loopback flag"));
            }
            allow = true;
            cursor += 1;
            continue;
        }
        if !["--server", "--cookie-file", "--service-key-file", "--input"].contains(&arg)
            || flags.contains_key(arg)
        {
            return Err(invalid("unknown/duplicate task flag"));
        }
        let value = args
            .get(cursor + 1)
            .filter(|s| !s.starts_with("--"))
            .ok_or_else(|| invalid("missing task flag value"))?;
        flags.insert(arg, value.as_str());
        cursor += 2;
    }
    if confirm != task_writes(op) {
        return Err(invalid(
            "writes require --confirm; reads refuse confirmation",
        ));
    }
    let mut bytes = vec![];
    if let Some(path) = flags.get("--input") {
        std::fs::File::open(path)
            .map_err(|_| invalid("cannot read task input"))?
            .take(MAX_TASK_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("cannot read task input"))?;
    } else if !matches!(op.as_str(), "list" | "series-list" | "boards-list")
        || !std::io::IsTerminal::is_terminal(&std::io::stdin())
    {
        std::io::stdin()
            .take(MAX_TASK_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("cannot read task stdin"))?;
    }
    if bytes.len() > MAX_TASK_BODY {
        return Err(invalid("task input exceeds128KiB"));
    }
    let input = if bytes.is_empty() && matches!(op.as_str(), "list" | "series-list" | "boards-list")
    {
        json!({})
    } else {
        serde_json::from_slice(&bytes).map_err(|_| invalid("task JSON required"))?
    };
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit server or local-profile required"))?;
    if flags.contains_key("--cookie-file") == flags.contains_key("--service-key-file") {
        return Err(invalid(
            "exactly one private cookie/service-key file required",
        ));
    }
    let api = if let Some(path) = flags.get("--cookie-file") {
        ApiClient::new(
            server,
            allow,
            Some(
                rentemester_client::read_cookie_file(std::path::Path::new(path))
                    .map_err(crate::remote::client_error)?,
            ),
        )
    } else {
        ApiClient::with_service_key(
            server,
            allow,
            rentemester_client::read_service_key_file(std::path::Path::new(
                flags["--service-key-file"],
            ))
            .map_err(crate::remote::client_error)?,
        )
    }
    .map_err(crate::remote::client_error)?;
    let rt = tokio::runtime::Runtime::new().map_err(|_| invalid("HTTP runtime unavailable"))?;
    let value = rt
        .block_on(api.task_operation(op, &input, confirm))
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":true,"errors":[],"data":value}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn series_help_and_refusals() {
        assert_eq!(
            run_series(&["--help".into()]).unwrap()["inputSchemas"]
                .as_object()
                .unwrap()
                .len(),
            4
        );
        for args in [
            vec!["save"],
            vec!["materialize"],
            vec!["list", "--confirm"],
            vec!["run"],
            vec!["save", "--confirm", "--actor", "user:owner"],
        ] {
            assert!(run_series(&args.into_iter().map(String::from).collect::<Vec<_>>()).is_err());
        }
    }
    #[test]
    fn no_implicit_writes_or_authority_flags() {
        for args in [
            vec!["create"],
            vec!["list", "--confirm"],
            vec!["create", "--confirm", "--actor", "user:owner"],
            vec!["create", "--confirm", "--local", "true"],
        ] {
            assert!(run(&args.into_iter().map(String::from).collect::<Vec<_>>()).is_err());
        }
    }
}
