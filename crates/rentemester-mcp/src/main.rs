//! Bounded MCP 2025-11-25 stdio adapter. Wire references:
//! https://modelcontextprotocol.io/specification/2025-11-25/basic/transports
//! https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle
//! https://modelcontextprotocol.io/specification/2025-11-25/server/tools
mod tools;
use rentemester_client::{ApiClient, read_cookie_file};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, Read, Write},
    path::Path,
    process::ExitCode,
};
const PROTOCOL: &str = "2025-11-25";
const MAX_LINE: usize = 1_048_576;

fn help() -> &'static str {
    "rentemester-mcp --server HTTPS_ORIGIN --cookie-file PATH [--allow-loopback-http]\nMCP 2025-11-25 over newline-delimited JSON-RPC stdio. Requires initialize then notifications/initialized.\nCookie file: one valid MFA-derived browser Cookie request-header value; private regular file (chmod 600 on Unix). Login/enrollment remain in browser.\nTools: me {}, companies {}, journal/trial_balance/income_statement/balance {company,year?,account?}; post_journal/reverse_journal {company,envelope}.\ncompany is an explicit company slug. envelope is an exact JSON string containing the server's confirmed request and caller idempotencyKey; never retried or rewritten.\nHTTP requires explicit --allow-loopback-http and literal loopback IP. No local fallback. Unknown write outcomes require reconciliation with original idempotency key.\nStdin messages limited to 1 MiB; string request IDs to 256 bytes; sessions to 100000 requests. Tools execute sequentially with bounded HTTP timeouts. Close stdin to stop. stdout contains protocol messages only during server operation."
}
fn config(args: Vec<String>) -> Result<ApiClient, &'static str> {
    let mut flags = BTreeMap::new();
    let mut allow_http = false;
    let mut cursor = 0;
    while cursor < args.len() {
        let arg = args[cursor].as_str();
        if arg == "--allow-loopback-http" {
            if allow_http {
                return Err("duplicate loopback flag");
            }
            allow_http = true;
            cursor += 1;
            continue;
        }
        if !["--server", "--cookie-file"].contains(&arg) {
            return Err("unknown flag; see --help");
        }
        let value = args
            .get(cursor + 1)
            .filter(|v| !v.is_empty() && !v.starts_with("--"))
            .ok_or("flag requires value")?;
        if flags.insert(arg, value.as_str()).is_some() {
            return Err("duplicate flag");
        }
        cursor += 2;
    }
    let server = flags
        .get("--server")
        .ok_or("explicit --server origin required")?;
    let path = flags
        .get("--cookie-file")
        .ok_or("--cookie-file with valid MFA-derived session required")?;
    let cookie = read_cookie_file(Path::new(path)).map_err(
        |_| "cookie file unavailable or invalid; require private regular file (chmod 600)",
    )?;
    ApiClient::new(server, allow_http, Some(cookie))
        .map_err(|_| "invalid server origin or Cookie header; see --help")
}
#[derive(Default)]
struct Session {
    initialized: bool,
    ready: bool,
    request_ids: BTreeSet<String>,
}
fn failure(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
fn success(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}
impl Session {
    async fn handle(&mut self, value: Value, api: &ApiClient) -> Option<Value> {
        let object = match value.as_object() {
            Some(v) => v,
            None => return Some(failure(Value::Null, -32600, "expected one JSON-RPC object")),
        };
        let id = object.get("id");
        let valid_id = id.is_some_and(|v| {
            v.as_str().is_some_and(|s| s.len() <= 256)
                || v.as_i64().is_some()
                || v.as_u64().is_some()
        });
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || object.get("method").and_then(Value::as_str).is_none()
            || (id.is_some() && !valid_id)
        {
            return Some(failure(
                if valid_id {
                    id.unwrap().clone()
                } else {
                    Value::Null
                },
                -32600,
                "invalid JSON-RPC request",
            ));
        }
        let method = object["method"].as_str().unwrap();
        if id.is_none() {
            if method == "notifications/initialized" && self.initialized {
                self.ready = true;
            }
            // Notifications never execute tools or produce responses. Cancellation is
            // advisory; sequential bounded HTTP operations complete before input resumes.
            return None;
        }
        let id = id.unwrap().clone();
        // Cap retained session state; reconnect rather than grow indefinitely.
        if self.request_ids.len() >= 100_000 {
            return Some(failure(
                id,
                -32000,
                "session request limit reached; reconnect",
            ));
        }
        if !self.request_ids.insert(id.to_string()) {
            return Some(failure(id, -32600, "request id already used"));
        }
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
        if !params.is_object() {
            return Some(failure(id, -32602, "expected params object"));
        }
        if method == "ping" {
            return Some(success(id, json!({})));
        }
        if method == "initialize" {
            if self.initialized {
                return Some(failure(id, -32600, "already initialized"));
            }
            if params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .is_none()
                || !params.get("capabilities").is_some_and(Value::is_object)
                || params
                    .pointer("/clientInfo/name")
                    .and_then(Value::as_str)
                    .is_none()
                || params
                    .pointer("/clientInfo/version")
                    .and_then(Value::as_str)
                    .is_none()
            {
                return Some(failure(
                    id,
                    -32602,
                    "initialize requires protocolVersion, capabilities and clientInfo",
                ));
            }
            // Only this version is supported. Per MCP negotiation, respond with the
            // supported version when requested version differs; client may disconnect.
            self.initialized = true;
            return Some(success(
                id,
                json!({"protocolVersion":PROTOCOL,"capabilities":{"tools":{}},"serverInfo":{"name":"rentemester-rust","version":env!("CARGO_PKG_VERSION")},"instructions":"Authenticated remote Rust server adapter. Discover companies before selecting an explicit slug. Keep original idempotency key when reconciling outcome_unknown; no automatic retries."}),
            ));
        }
        if !self.ready {
            return Some(failure(
                id,
                -32002,
                "initialize and notifications/initialized required",
            ));
        }
        match method {
            "tools/list" => Some(success(id, tools::list())),
            "tools/call" => {
                if params.get("task").is_some() {
                    return Some(failure(id, -32602, "task execution not supported"));
                }
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(failure(id, -32602, "tool name required"));
                };
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                match tools::call(api, name, args).await {
                    Ok(result) => Some(success(id, result)),
                    Err(message) => Some(failure(id, -32602, message)),
                }
            }
            _ => Some(failure(id, -32601, "method not found")),
        }
    }
}
fn serve(api: ApiClient) -> Result<(), &'static str> {
    let runtime = tokio::runtime::Runtime::new().map_err(|_| "could not start HTTP runtime")?;
    let mut session = Session::default();
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    loop {
        let mut line = Vec::new();
        let read = input
            .by_ref()
            .take((MAX_LINE + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|_| "stdin read failed")?;
        if read == 0 {
            return Ok(());
        }
        let oversized = line.len() > MAX_LINE;
        let response = if oversized || !line.ends_with(b"\n") {
            Some(failure(
                Value::Null,
                -32700,
                "invalid or oversized newline-delimited message",
            ))
        } else {
            match serde_json::from_slice(&line) {
                Ok(value) => runtime.block_on(session.handle(value, &api)),
                Err(_) => Some(failure(Value::Null, -32700, "invalid JSON")),
            }
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut output, &response).map_err(|_| "stdout write failed")?;
            output
                .write_all(b"\n")
                .and_then(|_| output.flush())
                .map_err(|_| "stdout write failed")?;
        }
        if oversized {
            return Err("message limit exceeded");
        }
    }
}
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--help"] {
        println!("{}", help());
        return ExitCode::SUCCESS;
    }
    let api = match config(args) {
        Ok(api) => api,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match serve(api) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}
