//! Composition of the native owner and existing HTTP identity boundary.
//! This development executable never proxies requests to the TypeScript backend.
use rentemester_application::{email_verification, mfa_login, verification_request};
use rentemester_host::{AuthAttempt, Owner};
use rentemester_runtime::{
    auth::{CookieProfile, CookieVerifier, SystemClock},
    email_token::EmailTokenSigner,
    mfa::MfaKeyring,
    recovery::RecoveryLinks,
};
use rentemester_server::auth_http::{AuthHandlers, EnrollmentHandlers, VerificationHandlers};
use serde::Deserialize;
use std::{
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
    #[serde(default)]
    runtime_profile: rentemester_runtime::namespace::NamespaceProfile,
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
    #[serde(default)]
    backup_signing_key_file: Option<PathBuf>,
    #[serde(default)]
    transfer_trusted_key_file: Option<PathBuf>,
    #[serde(default)]
    restore_actor: Option<String>,
    #[serde(default)]
    auth_mail: Option<crate::auth_delivery::Config>,
    #[serde(default)]
    document_scanner: Option<crate::document_scanner::Config>,
    #[serde(default)]
    invoice_extractor: Option<crate::invoice_extractor::Config>,
    #[serde(default)]
    imap_transport: Option<crate::imap_transport::Config>,
    #[serde(default)]
    document_pdf_worker: Option<crate::document_pdf::Config>,
    #[serde(default)]
    vies: Option<rentemester_runtime::vies::Config>,
    #[serde(default, rename = "publicEInvoice")]
    public_einvoice: std::collections::BTreeMap<String, rentemester_runtime::public_einvoice::Config>,
    #[serde(default)]
    invoice_mail: std::collections::BTreeMap<String, rentemester_runtime::invoice_mail::Config>,
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

pub struct RestoreRequest {
    pub archive: PathBuf,
    pub trusted_key: PathBuf,
    pub confirmed_target: PathBuf,
    pub transfer: bool,
}

pub struct Backend {
    pub(crate) owner: Mutex<Owner>,
    pub(crate) document_scanner: Option<rentemester_runtime::document_scanner::HttpJsonScanner>,
    pub(crate) invoice_extractor: Option<rentemester_runtime::invoice_extractor::HttpJsonExtractor>,
    pub(crate) document_pdf_worker: Option<rentemester_runtime::document_pdf::Config>,
    pub(crate) cookies: Arc<CookieVerifier>,
    pub(crate) setup: Option<crate::setup_http::Installer>,
    pub(crate) setup_pending: std::sync::atomic::AtomicBool,
    pub(crate) backup_signer: Option<rentemester_runtime::backup_artifact::SigningKey>,
    pub(crate) transfer_trusted_key: Option<rentemester_runtime::backup_artifact::TrustedPublicKey>,
    pub(crate) imap_transport: rentemester_runtime::imap::OperatorConfig,
    pub(crate) mail_executor: Option<Arc<rentemester_runtime::service_key::ServiceKey>>,
    pub(crate) scheduled_executor: Option<Arc<rentemester_runtime::service_key::ServiceKey>>,
    pub(crate) vies: rentemester_runtime::vies::Client,
    pub(crate) public_einvoice: std::collections::BTreeMap<String, rentemester_runtime::public_einvoice::Client>,
    pub(crate) invoice_mail: std::collections::BTreeMap<String, Mutex<rentemester_runtime::invoice_mail::SmtpSender>>,
    keys: MfaKeyring,
    pub(crate) signer: EmailTokenSigner,
    pub(crate) links: RecoveryLinks,
    pub(crate) delivery: Mutex<crate::auth_delivery::Delivery>,
    profile: CookieProfile,
    pub(crate) invitation_key: rentemester_runtime::invitation::InvitationKey,
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
    /// Offline operator entry; never reachable from HTTP and never starts workers.
    pub fn adopt_legacy(path: &Path, source: &Path, target: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let text = Zeroizing::new(std::fs::read_to_string(path)?);
        let config: Config = serde_json::from_str(&text).map_err(|_| "invalid server configuration")?;
        if target != config.root { return Err("exact configured fresh target required".into()); }
        let actor = config.first_owner.as_ref().map(|x| x.actor.as_str()).or(config.setup_actor.as_deref())
            .ok_or("fresh firstOwner or setupActor required")?;
        if config.setup_actor.as_deref().is_some_and(|a| a != actor) {
            return Err("bootstrap operator must be unambiguous".into());
        }
        if config.imap_transport.is_some() || !config.public_einvoice.is_empty() || !config.invoice_mail.is_empty() {
            return Err("adoption config must start with mail and delivery providers disabled".into());
        }
        CookieVerifier::new(&config.workspace, CookieProfile::Development, &config.cookie_key)?;
        MfaKeyring::new(&[(1, &config.mfa_key)], None).map_err(|_| "invalid fresh MFA key")?;
        let operator = rentemester_runtime::bootstrap::Operator::from_policy(&config.workspace, actor, &config.operator_actors)?;
        let prepared = rentemester_host::legacy_adoption::prepare(source, target, &config.workspace, &config.company_slug)?;
        let binding = rentemester_storage::legacy_retirement::RetirementBinding {
            operation_id: prepared.expected_operation_id().to_owned(),
            source_workspace_id: prepared.source_workspace_id().to_owned(),
            operator_actor: actor.to_owned(), source_stopped_confirmed: true,
        };
        crate::legacy_adoption_cli::confirm("retire-source", prepared.review(), prepared.review_hash(), &mut std::io::stdin().lock(), &mut std::io::stdout().lock())?;
        let reviewed = prepared.review_hash().to_owned();
        let retired = match rentemester_host::legacy_adoption::retire_source(prepared, &operator, &binding, &reviewed, true) {
            Ok(retired) => retired,
            Err(failure) => {
                eprintln!("{}", serde_json::json!({"ok":false,"code":failure.error.code,"retirementProgress":failure.progress,"sourceMayBePartlyRetired":true}));
                return Err(failure.into());
            }
        };
        crate::legacy_adoption_cli::confirm("publish-target", retired.review(), retired.review_hash(), &mut std::io::stdin().lock(), &mut std::io::stdout().lock())?;
        let reviewed = retired.review_hash().to_owned();
        let owner = rentemester_host::legacy_adoption::adopt(retired, target, config.broker_uid, config.runtime_profile, &operator, &reviewed)?;
        owner.try_close().map_err(|_| "adopted workspace close refused")?;
        println!("{}", serde_json::json!({"ok":true,"target":target,"next":"Start normally with the same private config; complete fresh owner verification and MFA. The legacy source remains retired."}));
        Ok(())
    }
    pub(crate) fn lock_company(&self,slug:&str)->Result<std::sync::MutexGuard<'_,Owner>,()> {
        let mut owner=self.owner.lock().map_err(|_|())?;
        owner.select_company(slug).map_err(|_|())?;
        Ok(owner)
    }
    /// Explicit schema operation on the configured protected namespace; no HTTP or bootstrap.
    pub fn upgrade_mail_control(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let text = Zeroizing::new(std::fs::read_to_string(path)?);
        let config: Config = serde_json::from_str(&text).map_err(|_| "invalid server configuration")?;
        let mut owner = Owner::open_with_profile(&config.root, config.broker_uid,
            &config.workspace, &config.company_slug, config.runtime_profile)?;
        owner.upgrade_mail_control_schema()?;
        owner.try_close().map_err(|_| "schema owner close refused")?;
        Ok(())
    }
    pub fn load(
        path: &Path,
        origin: &str,
        create: bool,
        loopback_listener: bool,
    ) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        Self::load_with_restore(path, origin, create, loopback_listener, None)
    }
    pub fn load_with_restore(
        path: &Path,
        origin: &str,
        create: bool,
        loopback_listener: bool,
        restore: Option<&RestoreRequest>,
    ) -> Result<Arc<Self>, Box<dyn std::error::Error>> {
        let text = Zeroizing::new(std::fs::read_to_string(path)?);
        let config: Config =
            serde_json::from_str(&text).map_err(|_| "invalid server configuration")?;
        if config.runtime_profile
            == rentemester_runtime::namespace::NamespaceProfile::LocalUserMacos
        {
            let url = url::Url::parse(origin).map_err(|_| "invalid local server origin")?;
            let local_origin = url.host_str().is_some_and(|host| {
                host == "localhost"
                    || host
                        .trim_matches(['[', ']'])
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            });
            if !loopback_listener || !local_origin {
                return Err("local-user-macos requires a loopback listener and origin".into());
            }
        }
        let profile = cookie_profile(origin)?;
        let document_scanner = crate::document_scanner::build(config.document_scanner.as_ref())?;
        let invoice_extractor = crate::invoice_extractor::build(config.invoice_extractor.as_ref())?;
        let imap_transport = crate::imap_transport::build(config.imap_transport.as_ref())?;
        let document_pdf_worker = crate::document_pdf::build(config.document_pdf_worker.as_ref())?;
        let mut delivery=crate::auth_delivery::Delivery::new(config.auth_mail.as_ref(), profile == CookieProfile::Development, &config.root.join(".rentemester"))?;
        let vies = rentemester_runtime::vies::Client::new(config.vies.as_ref().unwrap_or(&Default::default()))?;
        let public_einvoice=config.public_einvoice.iter().map(|(slug,config)| {
            rentemester_runtime::public_einvoice::Client::new(config.clone()).map(|client|(slug.clone(),client))
        }).collect::<Result<std::collections::BTreeMap<_,_>,_>>()?;
        let invoice_mail=config.invoice_mail.iter().map(|(slug,config)| {
            rentemester_runtime::invoice_mail::SmtpSender::new(config).map(|sender|(slug.clone(),Mutex::new(sender)))
        }).collect::<Result<std::collections::BTreeMap<_,_>,_>>()?;
        let cookies = Arc::new(CookieVerifier::new(
            &config.workspace,
            profile,
            &config.cookie_key,
        )?);
        let keys = MfaKeyring::new(&[(1, &config.mfa_key)], None).map_err(|_| "invalid MFA key")?;
        let signer = EmailTokenSigner::new(&config.workspace, &config.cookie_key)?;
        let links = RecoveryLinks::new(origin, &[origin], profile)?;
        let backup_signer = config
            .backup_signing_key_file
            .as_ref()
            .map(|p| rentemester_runtime::backup_keys::signing_key(p, &config.root))
            .transpose()?;
        let transfer_trusted_key = config.transfer_trusted_key_file.as_deref()
            .map(rentemester_runtime::backup_keys::trusted_key).transpose()?;
        let mut owner = if let Some(request) = restore {
            if create || request.confirmed_target != config.root || config.first_owner.is_none() {
                return Err("restore requires exact configured fresh target, a new firstOwner and no --create".into());
            }
            let operator = rentemester_runtime::bootstrap::Operator::from_policy(
                &config.workspace,
                config
                    .restore_actor
                    .as_deref()
                    .ok_or("restoreActor required in private config")?,
                &config.operator_actors,
            )?;
            let trusted = rentemester_runtime::backup_keys::trusted_key(&request.trusted_key)?;
            if request.transfer && !transfer_trusted_key.as_ref().is_some_and(|configured| {
                configured.public_key_bytes() == trusted.public_key_bytes()
            }) {
                return Err("transferTrustedKeyFile must independently configure the same trusted source key".into());
            }
            let (verified, wire_hash) = crate::portable_backup_cli::verified_with_hash(&request.archive, &trusted)?;
            if request.transfer {
                Owner::stage_transfer_new(
                    &config.root, config.broker_uid, &config.workspace,
                    &config.company_slug, config.runtime_profile, &verified,
                    &operator, &wire_hash, origin,
                )?
            } else { Owner::restore_new(
                &config.root,
                config.broker_uid,
                &config.workspace,
                &config.company_slug,
                config.runtime_profile,
                &verified,
                &operator,
            )? }
        } else if create {
            Owner::create_with_chart_profile(
                &config.root,
                config.broker_uid,
                &config.workspace,
                &config.company_slug,
                &config.company_name,
                &chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
                    .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                &config.accounts,
                config.runtime_profile,
            )?
        } else {
            Owner::open_with_profile(
                &config.root,
                config.broker_uid,
                &config.workspace,
                &config.company_slug,
                config.runtime_profile,
            )?
        };
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
                &mut delivery,
                rentemester_application::bootstrap::Input {
                    email: &first.email,
                    name: &first.name,
                    password: &first.password,
                },
            )?;
            let reservation = result
                .outcome
                .map_err(|f| format!("first owner bootstrap refused: {}", f.error.code))?;
            if restore.is_some() {
                owner.grant_restored_first_owner(&operator, &reservation)?;
            } else if owner.has_legacy_adoption()? {
                owner.grant_adopted_first_owner(&operator, &reservation)?;
            }
            println!(
                "First owner initialized; verification delivery requested."
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
            document_scanner,
            invoice_extractor,
            document_pdf_worker,
            setup,
            setup_pending,
            cookies,
            keys,
            backup_signer,
            transfer_trusted_key,
            scheduled_executor: None,
            mail_executor: None,
            imap_transport,
            invoice_mail,
            public_einvoice,
            vies,
            signer,
            links,
            delivery: Mutex::new(delivery),
            profile,
            invitation_key: rentemester_runtime::invitation::InvitationKey::new(
                &config.workspace,
                &config.cookie_key,
            )?,
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
        let mut owner = self.owner.lock().map_err(|_| "owner unavailable".to_owned())?;
        let reservation = owner.bootstrap_first_identity(
                &installer.operator,
                &runtime,
                &mut *self.delivery.lock().map_err(|_| "delivery unavailable".to_owned())?,
                input,
            )
            .map_err(|_| "owner unavailable".to_owned())?
            .outcome
            .map_err(|f| f.error.code.to_owned())?;
        if owner.has_legacy_adoption().map_err(|_| "adoption receipt unavailable".to_owned())? {
            owner.grant_adopted_first_owner(&installer.operator, &reservation).map_err(|_| "adopted owner grant refused".to_owned())?;
        }
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
        let service_context = self.clone();
        let service_lifecycle = self.clone();
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
                let mut owner=context.owner.lock().map_err(|_|unavailable())?;
                let catalog=owner.catalog().map_err(|_|unavailable())?;
                unwrap(owner.workspace_context(&context.cookies,&SystemClock,&proof,&catalog))
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
                            &mut *request.delivery.lock().map_err(|_| fail())?,
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
            service_context: Some(Box::new(move |key| {
                unwrap(service_context.owner.lock().map_err(|_| unavailable())?
                    .service_context(&SystemClock, &key))
            })),
            service_lifecycle: Some(Box::new(move |input| {
                unwrap(service_lifecycle.owner.lock().map_err(|_| unavailable())?
                    .service_lifecycle(&service_lifecycle.cookies, &SystemClock, input.proof(), input.operation()))
            })),
            session_control: None,
        };
        let reset_request = self.clone();
        let reset_callback = self.clone();
        let reset_complete = self.clone();
        rentemester_server::recovery_identity_router(origin, self.profile,
            move |input| {
                let runtime=rentemester_application::reset_request::Runtime {cookies:&reset_request.cookies,clock:&SystemClock,links:&reset_request.links,audit_key_version:1};
                let fail=||rentemester_application::reset_request::Failure {stage:rentemester_application::reset_request::Stage::NotCommitted,error:rentemester_engine::Error::new("connection_refused","native owner unavailable"),connection_reusable:false};
                reset_request.owner.lock().map_err(|_|fail())?
                    .request_password_reset(&runtime,&mut *reset_request.delivery.lock().map_err(|_|fail())?,input.email(),input.redirect())
                    .map_err(|_|fail())?.outcome
            },
            move |input| {
                let runtime=rentemester_application::reset_request::Runtime {cookies:&reset_callback.cookies,clock:&SystemClock,links:&reset_callback.links,audit_key_version:1};
                unwrap(reset_callback.owner.lock().map_err(|_|unavailable())?
                    .password_reset_callback(&runtime,input.token(),Some(input.redirect())))
            },
            move |input| {
                let runtime=rentemester_application::password_reset::Runtime {clock:&SystemClock,audit_key_version:1};
                unwrap(reset_complete.owner.lock().map_err(|_|unavailable())?
                    .complete_password_reset(&runtime,input.token(),input.password()))
            },handlers)
    }
}
/// Origin validation also fixes cookie naming and HTTPS-only flags for all identity routes.
pub(crate) fn cookie_profile(origin: &str) -> Result<CookieProfile, rentemester_runtime::auth::AuthError> {
    let url=url::Url::parse(origin).map_err(|_|rentemester_runtime::auth::AuthError::InvalidConfig)?;
    let profile=if url.scheme()=="https" {CookieProfile::Secure} else {CookieProfile::Development};
    RecoveryLinks::new(origin,&[origin],profile)?;
    Ok(profile)
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    #[test]
    fn canonical_https_uses_secure_cookie_profile_and_http_stays_loopback_only() {
        assert!(matches!(cookie_profile("https://accounting.example.invalid"),Ok(CookieProfile::Secure)));
        assert!(matches!(cookie_profile("HTTPS://accounting.example.invalid"),Ok(CookieProfile::Secure)));
        assert!(matches!(cookie_profile("http://127.0.0.1:4319"),Ok(CookieProfile::Development)));
        for origin in ["http://accounting.example.invalid","https://u@example.invalid","https://example.invalid/path","https://example.invalid?secret=x"] {assert!(cookie_profile(origin).is_err());}
    }
    #[test]
    fn public_einvoice_operator_config_uses_documented_camelcase() {
        let mut value=serde_json::json!({"root":"/synthetic/workspace","brokerUid":501,"workspace":"synthetic","companySlug":"synthetic","companyName":"Synthetic","cookieKey":"synthetic","mfaKey":"synthetic","publicEInvoice":{"synthetic":{"apiLicenseKey":"synthetic-license-key","environment":"test"}}});
        let config:Config=serde_json::from_value(value.clone()).unwrap();
        assert_eq!(config.public_einvoice.len(),1);
        let map=value.as_object_mut().unwrap();
        let provider=map.remove("publicEInvoice").unwrap();
        map.insert("publicEinvoice".into(),provider);
        assert!(serde_json::from_value::<Config>(value).is_err());
    }

}
