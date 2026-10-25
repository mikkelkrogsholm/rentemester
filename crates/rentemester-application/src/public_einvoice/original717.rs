//! Company secret capabilities are obtained only inside current admitted Owner.
use super::*;
pub fn configure(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    body: &Value,
) -> Result<Committed<Value>, Failure> {
    let mut publication_attempted = false;
    let result = control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Fresh,
        },
        selection(&context),
        Permission::CompanyAdmin,
        |tx, actor| {
            if body["confirm"] != true {
                return Ok(json!({"ok":false,"errors":["confirm: true is required"]}));
            }
            let secret = json!({"apiLicenseKey":body["apiLicenseKey"],"environment":body.get("environment").cloned().unwrap_or(json!("test"))});
            let bytes = serde_json::to_vec(&secret).map_err(|_| refused())?;
            rentemester_runtime::public_einvoice::original717::from_admitted_secret(&bytes)
                .map_err(|_| refused())?;
            let generation = rentemester_runtime::auth::new_token().map_err(|_| refused())?;
            publication_attempted = true;
            guard
                .write_digisense_config(&bytes)
                .map_err(|_| refused())?;
            tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('original717_digisense_config','private_config','digisense',?,?)",rusqlite::params![json!({"generation":generation.as_str(),"sha256":rentemester_storage::invoice_email::sha(&bytes)}).to_string(),actor]).map_err(|_|refused())?;

            Ok(
                json!({"ok":true,"configPath":"config/digisense.json","environment":secret["environment"]}),
            )
        },
    );
    result.map_err(|mut failure| {
        // Atomic private-file publication is an external effect relative to the
        // SQLite transaction. A later audit/commit failure cannot claim no effect.
        if publication_attempted {
            failure.stage = FailureStage::OutcomeUnknown;
        }
        failure
    })
}
/// Never returned through HTTP; caller constructs a fresh immutable provider client.
pub fn secret(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
) -> Result<Committed<Option<(Vec<u8>, String)>>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Fresh,
        },
        selection(&context),
        Permission::CompanyExternalSend,
        |tx, _| read_secret(tx, guard),
    )
}

fn read_secret(
    tx: &rusqlite::Transaction<'_>,
    guard: &mut NamespaceGuard,
) -> Result<Option<(Vec<u8>, String)>, rentemester_engine::Error> {
    use rusqlite::OptionalExtension;
    let generation:Option<String>=tx.query_row("SELECT message FROM audit_log WHERE event_type='original717_digisense_config' AND entity_type='private_config' AND entity_id='digisense' ORDER BY id DESC LIMIT 1",[],|r|r.get(0)).optional().map_err(|_|refused())?;
    let bytes = guard.read_digisense_config().map_err(|_| refused())?;
    match (bytes, generation) {
        (Some(bytes), Some(generation)) => {
            let proof: Value = serde_json::from_str(&generation).map_err(|_| refused())?;
            if proof["sha256"] != rentemester_storage::invoice_email::sha(&bytes) {
                return Err(refused());
            }
            Ok(Some((bytes, generation)))
        }
        (None, None) => Ok(None),
        _ => Err(refused()),
    }
}
/// Read-only configuration uses the same saved generation as dispatch without
/// returning secret bytes or requiring an external-send grant merely to read UI.
pub fn configuration(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    fallback: Option<&str>,
) -> Result<Committed<Value>, Failure> {
    let observed = control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        selection(&context),
        Permission::CompanyRead,
        |tx, _| {
            read_secret(tx, guard)?
                .map(|(bytes, _)| {
                    let config =
                        rentemester_runtime::public_einvoice::original717::from_admitted_secret(
                            &bytes,
                        )
                        .map_err(|_| refused())?;
                    Ok::<_, rentemester_engine::Error>(match config.environment() {
                        rentemester_runtime::public_einvoice::Environment::Test => "test",
                        rentemester_runtime::public_einvoice::Environment::Production => {
                            "production"
                        }
                    })
                })
                .transpose()
        },
    )?;
    if !observed.control_reusable {
        return Err(Failure {
            stage: FailureStage::OutcomeUnknown,
            error: refused(),
            control_reusable: false,
            company_reusable: false,
        });
    }
    super::configuration(control, company, context, observed.value.or(fallback))
}

/// Exact source readiness uses a complete pair on one network, not an OR of directions.
pub fn onboarding(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    fallback: Option<&str>,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        selection(&context),
        Permission::CompanyRead,
        |tx, _| {
            let saved = read_secret(tx, guard)?;
            let environment = saved
                .as_ref()
                .map(|(bytes, _)| {
                    let config =
                        rentemester_runtime::public_einvoice::original717::from_admitted_secret(
                            bytes,
                        )
                        .map_err(|_| refused())?;
                    Ok::<_, rentemester_engine::Error>(match config.environment() {
                        rentemester_runtime::public_einvoice::Environment::Test => "test",
                        rentemester_runtime::public_einvoice::Environment::Production => {
                            "production"
                        }
                    })
                })
                .transpose()?
                .or(fallback);
            let identity = store::identity(tx).ok();
            let key = if identity.is_some() {
                environment
                    .map(|e| store::load_bound(tx, e))
                    .transpose()?
                    .flatten()
            } else {
                None
            };
            let mut pair = false;
            if let (Some(env), Some(key)) = (environment, key.as_deref()) {
                for network in ["nemhandel", "peppol"] {
                    pair |= store::direction_registered(tx, env, key, network, "inbound")?
                        && store::direction_registered(tx, env, key, network, "outbound")?;
                }
            }
            let mut blockers = vec![];
            if environment.is_none() {
                blockers.push("Digisense API credentials are not configured.");
            }
            if identity.is_none() {
                blockers.push("DigiSense requires the ledger's company profile to contain both CVR and legal company name before any network operation.");
            }
            if identity.is_some() && key.is_none() {
                blockers.push("No DigiSense company is registered for this ledger's profile CVR.");
            }
            if key.is_some() && !pair {
                blockers.push("Inbound participant is not registered.");
                blockers.push("Outbound participant is not registered.");
            }
            Ok(
                json!({"configured":environment.is_some(),"environment":environment,"identity":identity,"companyKeyConfigured":key.is_some(),"inboundReady":pair,"outboundReady":pair,"ready":blockers.is_empty(),"blockers":blockers}),
            )
        },
    )
}

pub fn registration_result(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    environment: &str,
    network: Network,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Fresh,
        },
        selection(&context),
        Permission::CompanyExternalSend,
        |tx, _| {
            let identity = store::identity(tx)?;
            let key = store::load_bound(tx, environment)?.ok_or_else(refused)?;
            let mut directions = vec![];
            for direction in ["outbound", "inbound"] {
                if store::direction_registered(tx, environment, &key, network.name(), direction)? {
                    directions.push(direction);
                }
            }
            Ok(
                json!({"ok":directions.len()==2,"companyKey":key,"directionsRegistered":directions,"network":network.name(),"participantType":"DK:CVR","participantId":identity.cvr}),
            )
        },
    )
}
