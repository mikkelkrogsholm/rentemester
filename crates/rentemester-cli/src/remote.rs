use rentemester_client::{
    ApiClient, Error as ClientError, JournalMutation, LoginProgress, MfaFactor, Report,
};
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
            "commands":["health","me","companies","journal","trial-balance","income-statement","balance","post-journal","reverse-journal","login"],
            "authentication":"Supply a valid MFA-derived browser session as one Cookie request-header value in an owner-only regular file (0600). No Set-Cookie attributes. All commands except health/login require --cookie-file. Login uses --email EMAIL --factor totp|backup --session-out NEW_PATH, reads password then factor code as two newline-terminated stdin lines; stdin must be piped/private. Output file is created0600 and never overwritten. MFA enrollment remains in browser.",
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
                "--email",
                "--factor",
                "--session-out",
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
    if command == "login" {
        return login(&flags, allow_http);
    }
    if flags.contains_key("--email")
        || flags.contains_key("--factor")
        || flags.contains_key("--session-out")
    {
        return Err(invalid("email/factor/session-out allowed only with login"));
    }
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
        Some(path) => Some(
            rentemester_client::read_cookie_file(std::path::Path::new(path))
                .map_err(client_error)?,
        ),
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

fn login(flags: &BTreeMap<&str, &str>, allow_http: bool) -> Result<Value, Failure> {
    use std::io::{BufRead, IsTerminal};
    if flags
        .keys()
        .any(|key| !["--server", "--email", "--factor", "--session-out"].contains(key))
    {
        return Err(invalid(
            "login cannot use existing cookie/company/report flags",
        ));
    }
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server origin required"))?;
    let email = flags
        .get("--email")
        .ok_or_else(|| invalid("login requires --email"))?;
    let factor = match flags.get("--factor").copied() {
        Some("totp") => MfaFactor::Totp,
        Some("backup") => MfaFactor::BackupCode,
        _ => return Err(invalid("login requires --factor totp or backup")),
    };
    let output = std::path::Path::new(
        flags
            .get("--session-out")
            .ok_or_else(|| invalid("login requires --session-out NEW_PATH"))?,
    );
    match std::fs::symlink_metadata(output) {
        Ok(_) => {
            return Err(invalid(
                "session output already exists; choose a new private path",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(invalid("cannot inspect session output path")),
    }
    let client = ApiClient::new(server, allow_http, None).map_err(client_error)?;
    let input = std::io::stdin();
    if input.is_terminal() {
        return Err(invalid(
            "pipe password and factor code via private stdin; terminal echo is refused",
        ));
    }
    let mut input = input.lock();
    fn secret(input: &mut impl BufRead) -> Result<String, Failure> {
        let mut bytes = Vec::new();
        input
            .by_ref()
            .take(4099)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| invalid("could not read secret stdin line"))?;
        if bytes.pop() != Some(b'\n') {
            return Err(invalid("secret stdin lines must end in newline"));
        }
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        if bytes.is_empty() || bytes.len() > 4096 {
            return Err(invalid("secret stdin line outside server input bounds"));
        }
        String::from_utf8(bytes).map_err(|_| invalid("secret stdin line must be UTF-8"))
    }
    let password = secret(&mut input)?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let pending = match runtime
        .block_on(client.begin_login(email, password))
        .map_err(client_error)?
    {
        LoginProgress::MfaRequired(pending) => pending,
        LoginProgress::EnrollmentRequired => {
            return Ok(
                json!({"ok":false,"code":"mfa_enrollment_required","errors":["MFA enrollment required; complete enrollment in server UI"]}),
            );
        }
    };
    let code = secret(&mut input)?;
    let session = runtime
        .block_on(client.complete_login(&pending, factor, code))
        .map_err(client_error)?;
    session
        .write_cookie_file_new(output)
        .map_err(client_error)?;
    Ok(json!({"ok":true,"data":{"status":"authenticated"}}))
}
