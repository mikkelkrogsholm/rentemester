use rentemester_client::{ApiClient, Error as ClientError, JournalMutation, Report};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};

type Failure = (u8, Error);
fn invalid(message: &str) -> Failure {
    (2, Error::new("invalid_input", message))
}

pub fn run(args: &[String]) -> Result<Value, Failure> {
    if args == ["--help"] {
        return Ok(json!({"ok":true,
            "usage":"remote --server ORIGIN [--allow-loopback-http] [--cookie-file PATH] COMMAND [--company SLUG] [--year YEAR] [--account CODE]",
            "commands":["health","me","companies","journal","trial-balance","income-statement","balance","post-journal","reverse-journal"],
            "authentication":"Supply a valid MFA-derived browser session as one Cookie request-header value in an owner-only regular file (0600). No Set-Cookie attributes; no login or enrollment command yet. All commands except health require --cookie-file.",
            "writes":"post-journal/reverse-journal read exact bounded server JSON envelope from stdin; requires confirm:true and input.idempotencyKey. Authenticated server session determines actor. No retry; outcome_unknown requires reconciliation using original key.",
            "transport":"HTTPS only unless --allow-loopback-http and literal loopback IP; --server is an origin without path/query/userinfo. No local fallback, redirects, ambient proxies or automatic retries.",
            "exitCodes":{"0":"success","1":"server/transport/protocol failure","2":"usage/configuration failure"},
            "example":"rentemester-rust remote --server http://127.0.0.1:8080 --allow-loopback-http --cookie-file /private/session.cookie trial-balance --company example --year 2026"}));
    }
    let mut flags = BTreeMap::new();
    let mut command = None;
    let mut allow_http = false;
    let mut cursor = 0;
    while cursor < args.len() {
        let arg = args[cursor].as_str();
        if arg == "--allow-loopback-http" {
            if allow_http {
                return Err(invalid("duplicate --allow-loopback-http"));
            }
            allow_http = true;
            cursor += 1;
            continue;
        }
        if arg.starts_with("--") {
            if ![
                "--server",
                "--cookie-file",
                "--company",
                "--year",
                "--account",
            ]
            .contains(&arg)
            {
                return Err(invalid("unknown remote flag; see remote --help"));
            }
            let value = args
                .get(cursor + 1)
                .filter(|s| !s.is_empty() && !s.starts_with("--"))
                .ok_or_else(|| invalid("remote flag requires a value"))?;
            if flags.insert(arg, value.as_str()).is_some() {
                return Err(invalid("duplicate remote flag"));
            }
            cursor += 2;
        } else {
            if command.replace(arg).is_some() {
                return Err(invalid("expected one remote command"));
            }
            cursor += 1;
        }
    }
    let command = command.ok_or_else(|| invalid("expected remote command; see remote --help"))?;
    let report = match command {
        "journal" => Some(Report::Journal),
        "trial-balance" => Some(Report::TrialBalance),
        "income-statement" => Some(Report::IncomeStatement),
        "balance" => Some(Report::Balance),
        "health" | "me" | "companies" | "post-journal" | "reverse-journal" => None,
        _ => return Err(invalid("unknown remote command")),
    };
    let mutation = matches!(command, "post-journal" | "reverse-journal");
    let company = flags.get("--company").copied();
    if (report.is_some() || mutation) != company.is_some()
        || (report.is_none() && (flags.contains_key("--year") || flags.contains_key("--account")))
    {
        return Err(invalid(
            "company required only for company operations; year/account allowed only on reports",
        ));
    }
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server origin required"))?;
    let cookie = match flags.get("--cookie-file") {
        Some(path) => Some(read_cookie(path)?),
        None if command == "health" => None,
        None => {
            return Err(invalid(
                "authenticated remote commands require --cookie-file",
            ));
        }
    };
    let client = ApiClient::new(server, allow_http, cookie).map_err(client_error)?;
    let body = if mutation {
        let mut body = Vec::new();
        std::io::stdin()
            .lock()
            .take(65_537)
            .read_to_end(&mut body)
            .map_err(|_| invalid("could not read request from stdin"))?;
        Some(body)
    } else {
        None
    };
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let response = runtime
        .block_on(async {
            if let Some(report) = report {
                client
                    .report(
                        company.unwrap(),
                        report,
                        flags.get("--year").copied(),
                        flags.get("--account").copied(),
                    )
                    .await
            } else {
                match command {
                    "health" => client.health().await,
                    "me" => client.me().await,
                    "companies" => client.companies().await,
                    _ => {
                        client
                            .manual_journal(
                                company.unwrap(),
                                if command == "post-journal" {
                                    JournalMutation::Post
                                } else {
                                    JournalMutation::Reverse
                                },
                                body.unwrap(),
                            )
                            .await
                    }
                }
            }
        })
        .map_err(client_error)?;
    Ok(json!({"ok":response.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":response}))
}
fn client_error(error: ClientError) -> Failure {
    let (exit, code) = match error {
        ClientError::InvalidInput(_) => (2, "invalid_input"),
        ClientError::Transport => (1, "transport"),
        ClientError::Refused { .. } => (1, "server_refused"),
        ClientError::Protocol => (1, "protocol"),
        ClientError::OutcomeUnknown => (1, "outcome_unknown"),
    };
    (exit, Error::new(code, error.to_string()))
}
fn read_cookie(path: &str) -> Result<String, Failure> {
    // Open first, then inspect the actual file descriptor to avoid path-replacement races.
    let mut file = std::fs::File::open(path).map_err(|_| invalid("could not open cookie file"))?;
    let metadata = file
        .metadata()
        .map_err(|_| invalid("could not inspect cookie file"))?;
    if !metadata.is_file() || metadata.len() > 8192 {
        return Err(invalid(
            "cookie must be a regular file of at most 8192 bytes",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 {
            return Err(invalid(
                "cookie file must have no group or other permissions (chmod 600)",
            ));
        }
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("could not read cookie file"))?;
    if bytes.len() > 8192 {
        return Err(invalid("cookie file exceeds limit"));
    }
    let value = String::from_utf8(bytes).map_err(|_| invalid("cookie file must be UTF-8"))?;
    Ok(value.trim_end_matches(['\r', '\n']).to_owned())
}
