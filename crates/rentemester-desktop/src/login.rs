//! Owns the shared client's origin-bound ceremony; no secret-bearing Debug or logs.
use crate::{Session, readable};
use rentemester_client::{ApiClient, LoginProgress, MfaFactor, PendingLogin};

pub struct Pending {
    session: Session,
    challenge: PendingLogin,
    pub origin: String,
}
pub enum Step {
    Factor(Pending),
    Enrollment(String),
}
pub struct FactorFailure {
    pub message: String,
    /// Only an explicit safe refusal permits entering another code.
    pub retry: Option<Pending>,
}
fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|_| "HTTP-runtime kunne ikke startes".into())
}
pub fn begin(
    origin: String,
    allow_http: bool,
    email: String,
    password: String,
) -> Result<Step, String> {
    let runtime = runtime()?;
    let client = {
        let _guard = runtime.enter();
        ApiClient::new(&origin, allow_http, None).map_err(readable)?
    };
    match runtime
        .block_on(client.begin_login(&email, password))
        .map_err(login_error)?
    {
        LoginProgress::MfaRequired(challenge) => Ok(Step::Factor(Pending {
            session: Session { client, runtime },
            challenge,
            origin,
        })),
        LoginProgress::EnrollmentRequired => Ok(Step::Enrollment(origin)),
    }
}
pub fn complete(
    mut pending: Pending,
    backup: bool,
    code: String,
) -> Result<Session, FactorFailure> {
    let result = pending
        .session
        .runtime
        .block_on(pending.session.client.complete_login(
            &pending.challenge,
            if backup {
                MfaFactor::BackupCode
            } else {
                MfaFactor::Totp
            },
            code,
        ));
    match result {
        Ok(session) => {
            pending
                .session
                .client
                .adopt_session(session)
                .map_err(|error| FactorFailure {
                    message: login_error(error),
                    retry: None,
                })?;
            Ok(pending.session)
        }
        Err(error) => {
            let safe = matches!(
                error,
                rentemester_client::Error::Refused {
                    status: 400 | 401 | 403 | 422 | 429
                } | rentemester_client::Error::InvalidInput(_)
            );
            Err(FactorFailure {
                message: login_error(error),
                retry: if safe { Some(pending) } else { None },
            })
        }
    }
}
fn login_error(error: rentemester_client::Error) -> String {
    match error {
        rentemester_client::Error::Refused{status:400|401|403|422}=>"Login blev afvist. Kontrollér e-mail, adgangskode eller MFA-kode.".into(),
        rentemester_client::Error::Refused{status:429}=>"For mange loginforsøg. Vent, før du prøver igen.".into(),
        rentemester_client::Error::OutcomeUnknown=>"Loginresultatet kunne ikke bekræftes. Start login forfra; et svar eller en kode kan være blevet brugt.".into(),
        error=>readable(error),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uncertain_auth_outcome_has_explicit_recovery_without_secret_details() {
        assert!(
            login_error(rentemester_client::Error::OutcomeUnknown).contains("Start login forfra")
        );
        assert!(login_error(rentemester_client::Error::Refused { status: 429 }).contains("Vent"));
        assert!(
            begin(
                "http://example.invalid".into(),
                true,
                "synthetic@example.invalid".into(),
                "synthetic-password".into()
            )
            .is_err()
        );
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use serde_json::json;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };
    fn response(status: &str, cookie: Option<&str>, body: serde_json::Value) -> String {
        let body = body.to_string();
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{body}",
            body.len(),
            cookie
                .map(|value| format!("Set-Cookie: {value}\r\n"))
                .unwrap_or_default()
        )
    }
    fn server(replies: Vec<(&'static str, String)>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            for (path, response) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                loop {
                    let mut part = [0; 4096];
                    let size = stream.read(&mut part).unwrap();
                    assert!(size > 0);
                    request.extend_from_slice(&part[..size]);
                    if let Some(end) = request.windows(4).position(|p| p == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&request[..end]).to_lowercase();
                        let length = header
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .and_then(|value| value.parse::<usize>().ok())
                            .unwrap_or(0);
                        if request.len() >= end + 4 + length {
                            assert!(header.lines().next().unwrap().contains(path));
                            break;
                        }
                    }
                }
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (origin, task)
    }
    fn challenge() -> String {
        response(
            "200 OK",
            Some("better-auth.two_factor=synthetic-challenge; Path=/; HttpOnly; SameSite=Lax"),
            json!({"twoFactorRedirect":true}),
        )
    }
    fn pending(origin: String) -> Pending {
        match begin(
            origin,
            true,
            "synthetic@example.invalid".into(),
            "synthetic-password".into(),
        )
        .unwrap_or_else(|_| panic!("synthetic password step failed"))
        {
            Step::Factor(pending) => pending,
            _ => panic!("expected factor challenge"),
        }
    }
    #[test]
    fn refused_code_keeps_challenge_for_explicit_backup_retry_and_success_adopts_session() {
        let (origin, task) = server(vec![
            ("/api/auth/sign-in/email", challenge()),
            (
                "/api/auth/two-factor/verify-totp",
                response("401 Unauthorized", None, json!({"ok":false})),
            ),
            (
                "/api/auth/two-factor/verify-backup-code",
                response(
                    "200 OK",
                    Some(
                        "better-auth.session_token=synthetic-session; Path=/; HttpOnly; SameSite=Lax",
                    ),
                    json!({"status":true}),
                ),
            ),
            (
                "/api/companies",
                response("200 OK", None, json!({"ok":true,"companies":[]})),
            ),
        ]);
        let pending = pending(origin);
        let failure = match complete(pending, false, "000000".into()) {
            Err(failure) => failure,
            Ok(_) => panic!("wrong code accepted"),
        };
        let session = complete(
            failure.retry.expect("refused code permits explicit retry"),
            true,
            "synthetic-backup".into(),
        )
        .unwrap_or_else(|_| panic!("backup step failed"));
        assert_eq!(
            session
                .runtime
                .block_on(session.client.companies())
                .unwrap()["ok"],
            true
        );
        task.join().unwrap();
    }
    #[test]
    fn enrollment_is_explicit_and_uncertain_mfa_drops_pending_challenge() {
        let (origin, task) = server(vec![(
            "/api/auth/sign-in/email",
            response(
                "409 Conflict",
                None,
                json!({"code":"MFA_ENROLLMENT_REQUIRED"}),
            ),
        )]);
        assert!(
            matches!(begin(origin.clone(),true,"synthetic@example.invalid".into(),"synthetic-password".into()),Ok(Step::Enrollment(value)) if value==origin)
        );
        task.join().unwrap();
        let (origin, task) = server(vec![
            ("/api/auth/sign-in/email", challenge()),
            (
                "/api/auth/two-factor/verify-totp",
                response("503 Unavailable", None, json!({"ok":false})),
            ),
        ]);
        let failure = match complete(pending(origin), false, "123456".into()) {
            Err(failure) => failure,
            Ok(_) => panic!("uncertain outcome accepted"),
        };
        assert!(failure.retry.is_none());
        assert!(failure.message.contains("Start login forfra"));
        task.join().unwrap();
    }
}
