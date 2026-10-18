//! Composition of the native owner and existing HTTP identity boundary.
//! This development executable never proxies requests to the TypeScript backend.
use rentemester_application::{
    email_verification, mfa_login, verification_request,
    workspace_context::{Catalog, RegisteredCompany},
};
use rentemester_host::{AuthAttempt, Owner};
use rentemester_runtime::{
    auth::{CookieProfile, CookieVerifier, SystemClock},
    email_token::EmailTokenSigner,
    mfa::MfaKeyring,
    recovery::{DeliveryUnconfirmed, RecoveryLinks, VerificationMessage, VerificationSender},
};
use rentemester_server::auth_http::{AuthHandlers, EnrollmentHandlers, VerificationHandlers};
use serde::Deserialize;
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

type Failure = rentemester_application::password_login::Failure;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Config {
    root: PathBuf,
    broker_uid: u32,
    workspace: String,
    company_slug: String,
    company_name: String,
    cookie_key: String,
    mfa_key: String,
    #[serde(default)]
    first_owner: Option<FirstOwner>,
    #[serde(default)]
    setup_actor: Option<String>,
    #[serde(default)]
    operator_actors: Vec<String>,
    #[serde(default)]
    accounts: Vec<rentemester_storage::fresh::InitialAccount>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FirstOwner {
    actor: String,
    email: String,
    name: String,
    password: String,
}
impl Drop for FirstOwner {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.password.zeroize();
    }
}
impl Drop for Config {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.cookie_key.zeroize();
        self.mfa_key.zeroize();
    }
}

pub struct Backend {
    pub(crate) owner: Mutex<Owner>,
    pub(crate) cookies: Arc<CookieVerifier>,
    pub(crate) setup: Option<crate::setup_http::Installer>,
    pub(crate) setup_pending: std::sync::atomic::AtomicBool,
    keys: MfaKeyring,
    signer: EmailTokenSigner,
    links: RecoveryLinks,
    spool: PathBuf,
    pub(crate) catalog: Catalog,
}
fn unavailable() -> Failure {
    Failure {
        stage: rentemester_application::password_login::FailureStage::NotCommitted,
        error: rentemester_engine::Error::new("connection_refused", "native owner unavailable"),
        connection_reusable: false,
    }
}
fn unwrap<T>(result: AuthAttempt<T>) -> Result<T, Failure> {
    // A committed outcome remains committed even when the owner is retired.
    result.map_err(|_| unavailable())?.outcome
}
impl Backend {
    pub fn load(
        path: &Path,
        origin: &str,
        create: bool,
    ) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let text = Zeroizing::new(std::fs::read_to_string(path)?);
        let config: Config =
            serde_json::from_str(&text).map_err(|_| "invalid server configuration")?;
        let cookies = Arc::new(CookieVerifier::new(
            &config.workspace,
            CookieProfile::Development,
            &config.cookie_key,
        )?);
        let keys = MfaKeyring::new(&[(1, &config.mfa_key)], None).map_err(|_| "invalid MFA key")?;
        let signer = EmailTokenSigner::new(&config.workspace, &config.cookie_key)?;
        let links = RecoveryLinks::new(origin, &[origin], CookieProfile::Development)?;
        let mut owner = if create {
            Owner::create_with_chart(
                &config.root,
                config.broker_uid,
                &config.workspace,
                &config.company_slug,
                &config.company_name,
                &chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                &config.accounts,
            )?
        } else {
            Owner::open_protected(
                &config.root,
                config.broker_uid,
                &config.workspace,
                &config.company_slug,
            )?
        };
        let spool = config.root.join(".rentemester");
        if let Some(first) = &config.first_owner {
            let key = rentemester_runtime::bootstrap::ReservationKey::new(
                &config.workspace,
                &config.cookie_key,
            )?;
            // Local process operator authority only. No request can select this actor or grant.
            let actor = first.actor.as_str();
            let operator = rentemester_runtime::bootstrap::Operator::from_policy(
                &config.workspace,
                actor,
                &config.operator_actors,
            )?;
            let verification = email_verification::Runtime {
                signer: &signer,
                clock: &SystemClock,
                audit_key_version: 1,
            };
            let request = verification_request::Runtime {
                verification: &verification,
                cookies: &cookies,
                links: &links,
            };
            let runtime = rentemester_application::bootstrap::Runtime {
                reservation_key: &key,
                verification: &request,
            };
            let result = owner.bootstrap_first_identity(
                &operator,
                &runtime,
                &mut Spool(spool.clone()),
                rentemester_application::bootstrap::Input {
                    email: &first.email,
                    name: &first.name,
                    password: &first.password,
                },
            )?;
            result
                .outcome
                .map_err(|f| format!("first owner bootstrap refused: {}", f.error.code))?;
            println!(
                "First owner initialized; verification links are in the private workspace .rentemester directory."
            );
        }
        let setup = if let Some(actor) = &config.setup_actor {
            let operator = rentemester_runtime::bootstrap::Operator::from_policy(
                &config.workspace,
                actor,
                &config.operator_actors,
            )?;
            let phase = unwrap(owner.first_identity_phase(&operator))
                .map_err(|_| "installer status unavailable")?;
            if phase == Some(rentemester_application::bootstrap::Phase::AccessCompleted) {
                None
            } else {
                let token = rentemester_runtime::auth::new_token()?;
                println!("Workspace setup: {origin}/#setup={}", token.as_str());
                Some(crate::setup_http::Installer::new(
                    operator,
                    rentemester_runtime::bootstrap::ReservationKey::new(
                        &config.workspace,
                        &config.cookie_key,
                    )?,
                    token,
                ))
            }
        } else {
            None
        };
        let setup_pending = std::sync::atomic::AtomicBool::new(setup.is_some());
        Ok(Arc::new(Self {
            owner: Mutex::new(owner),
            setup,
            setup_pending,
            cookies,
            keys,
            signer,
            links,
            spool,
            catalog: Catalog {
                workspace: config.workspace.clone(),
                companies: vec![RegisteredCompany {
                    slug: config.company_slug.clone(),
                    name: config.company_name.clone(),
                    archived: false,
                }],
            },
        }))
    }
    pub(crate) fn install_owner(
        &self,
        installer: &crate::setup_http::Installer,
        input: rentemester_application::bootstrap::Input<'_>,
    ) -> Result<(), String> {
        let verification = email_verification::Runtime {
            signer: &self.signer,
            clock: &SystemClock,
            audit_key_version: 1,
        };
        let request = verification_request::Runtime {
            verification: &verification,
            cookies: &self.cookies,
            links: &self.links,
        };
        let runtime = rentemester_application::bootstrap::Runtime {
            reservation_key: &installer.key,
            verification: &request,
        };
        self.owner
            .lock()
            .map_err(|_| "owner unavailable".to_owned())?
            .bootstrap_first_identity(
                &installer.operator,
                &runtime,
                &mut Spool(self.spool.clone()),
                input,
            )
            .map_err(|_| "owner unavailable".to_owned())?
            .outcome
            .map_err(|f| f.error.code.to_owned())?;
        self.setup_pending
            .store(false, std::sync::atomic::Ordering::Release);
        Ok(())
    }
    fn mfa(&self) -> mfa_login::Runtime<'_> {
        mfa_login::Runtime {
            cookies: &self.cookies,
            keys: &self.keys,
            clock: &SystemClock,
            audit_key_version: 1,
        }
    }
    pub fn identity(
        self: &Arc<Self>,
        origin: &str,
    ) -> Result<axum::Router, rentemester_server::InvalidConfig> {
        let login = self.clone();
        let totp = self.clone();
        let backup = self.clone();
        let session = self.clone();
        let logout = self.clone();
        let begin = self.clone();
        let confirm = self.clone();
        let context = self.clone();
        let request = self.clone();
        let complete = self.clone();
        let handlers = AuthHandlers {
            cookies: self.cookies.clone(),
            sign_in: Box::new(move |input| {
                unwrap(
                    login
                        .owner
                        .lock()
                        .map_err(|_| unavailable())?
                        .begin_password_login(&login.mfa(), input.email(), input.password()),
                )
            }),
            totp: Box::new(move |input| {
                unwrap(
                    totp.owner
                        .lock()
                        .map_err(|_| unavailable())?
                        .verify_login_factor(&totp.mfa(), input.challenge(), input.code(), false),
                )
            }),
            backup: Box::new(move |input| {
                unwrap(
                    backup
                        .owner
                        .lock()
                        .map_err(|_| unavailable())?
                        .verify_login_factor(&backup.mfa(), input.challenge(), input.code(), true),
                )
            }),
            session: Box::new(move |proof| {
                unwrap(
                    session
                        .owner
                        .lock()
                        .map_err(|_| unavailable())?
                        .current_session(&session.cookies, &SystemClock, &proof),
                )
            }),
            sign_out: Box::new(move |proof| {
                let runtime = rentemester_application::session_control::Runtime {
                    cookies: &logout.cookies,
                    clock: &SystemClock,
                    audit_key_version: 1,
                };
                unwrap(
                    logout
                        .owner
                        .lock()
                        .map_err(|_| unavailable())?
                        .sign_out(&runtime, &proof),
                )
            }),
            enrollment: Some(EnrollmentHandlers {
                begin: Box::new(move |input| {
                    unwrap(
                        begin
                            .owner
                            .lock()
                            .map_err(|_| unavailable())?
                            .begin_mfa_enrollment(
                                &begin.mfa(),
                                input.email(),
                                input.password(),
                                "Rentemester",
                            ),
                    )
                }),
                confirm: Box::new(move |input| {
                    unwrap(
                        confirm
                            .owner
                            .lock()
                            .map_err(|_| unavailable())?
                            .confirm_mfa_enrollment(
                                &confirm.mfa(),
                                input.challenge(),
                                input.code(),
                            ),
                    )
                }),
            }),
            workspace_context: Some(Box::new(move |proof| {
                unwrap(
                    context
                        .owner
                        .lock()
                        .map_err(|_| unavailable())?
                        .workspace_context(
                            &context.cookies,
                            &SystemClock,
                            &proof,
                            &context.catalog,
                        ),
                )
            })),
            verification: Some(VerificationHandlers {
                request: Box::new(move |input| {
                    let verification = email_verification::Runtime {
                        signer: &request.signer,
                        clock: &SystemClock,
                        audit_key_version: 1,
                    };
                    let runtime = verification_request::Runtime {
                        verification: &verification,
                        cookies: &request.cookies,
                        links: &request.links,
                    };
                    let fail = || rentemester_application::reset_request::Failure {
                        stage: rentemester_application::reset_request::Stage::NotCommitted,
                        error: rentemester_engine::Error::new(
                            "connection_refused",
                            "native owner unavailable",
                        ),
                        connection_reusable: false,
                    };
                    request
                        .owner
                        .lock()
                        .map_err(|_| fail())?
                        .request_email_verification(
                            &runtime,
                            &mut Spool(request.spool.clone()),
                            input.email(),
                            input.callback(),
                            input.caller(),
                        )
                        .map_err(|_| fail())?
                        .outcome
                }),
                complete: Box::new(move |input| {
                    let verification = email_verification::Runtime {
                        signer: &complete.signer,
                        clock: &SystemClock,
                        audit_key_version: 1,
                    };
                    let runtime = verification_request::Runtime {
                        verification: &verification,
                        cookies: &complete.cookies,
                        links: &complete.links,
                    };
                    unwrap(
                        complete
                            .owner
                            .lock()
                            .map_err(|_| unavailable())?
                            .complete_email_verification(&runtime, input.token(), input.callback()),
                    )
                    .map(|_| ())
                }),
            }),
            backup_management: None,
            password_change: None,
            service_context: None,
            service_lifecycle: None,
            session_control: None,
        };
        rentemester_server::identity_router(origin, CookieProfile::Development, handlers)
    }
}
// Explicit development delivery adapter. This is a private local outbox, not an email service.
struct Spool(PathBuf);
impl VerificationSender for Spool {
    fn enabled(&self) -> bool {
        true
    }
    fn send(&mut self, message: &VerificationMessage) -> Result<(), DeliveryUnconfirmed> {
        let name = rentemester_runtime::auth::new_token().map_err(|_| DeliveryUnconfirmed)?;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(self.0.join(format!("verification-{}.txt", name.as_str())))
            .map_err(|_| DeliveryUnconfirmed)?;
        file.write_all(message.action_url().as_bytes())
            .map_err(|_| DeliveryUnconfirmed)?;
        file.sync_all().map_err(|_| DeliveryUnconfirmed)
    }
}
