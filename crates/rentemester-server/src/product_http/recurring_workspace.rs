//! Explicit original workspace run. One company transaction at a time; no timer or retries.
use super::*;
use rentemester_application::{control_fence::FailureStage, recurring_workspace::Action};
use rentemester_runtime::auth::VerifiedCookie;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    workspace: String,
    as_of_date: String,
    max_generations: Option<usize>,
    confirm: bool,
}
#[derive(Debug)]
struct RunError {
    unknown: bool,
    attempted: bool,
}
fn unknown() -> RunError {
    RunError {
        unknown: true,
        attempted: false,
    }
}
fn known() -> RunError {
    RunError {
        unknown: false,
        attempted: false,
    }
}
fn phase(
    state: &Product,
    slug: &str,
    cookie: &VerifiedCookie,
    action: Action<'_>,
    today: &str,
    now: &str,
) -> Result<Value, RunError> {
    let mut owner = state.native.lock_company(slug).map_err(|_| unknown())?;
    let attempt = owner
        .recurring_workspace(cookie, &SystemClock, action, today, now)
        .map_err(|_| unknown())?;
    if attempt.retired {
        return Err(unknown());
    }
    match attempt.outcome {
        Ok(c) if c.control_reusable => Ok(c.value),
        Ok(_) => Err(unknown()),
        Err(f) => Err(RunError {
            unknown: f.stage == FailureStage::OutcomeUnknown
                || !f.control_reusable
                || !f.company_reusable,
            attempted: false,
        }),
    }
}
fn email_preview(
    state: &Product,
    slug: &str,
    cookie: &VerifiedCookie,
    row: &Value,
    today: &str,
    now: &str,
) -> Result<(), RunError> {
    let identity = state
        .native
        .recurring_email_dry_run
        .get(slug)
        .ok_or_else(known)?;
    let recipient = row["emailRecipient"].as_str().ok_or_else(known)?;
    let number = row["invoiceNumber"].as_str().ok_or_else(unknown)?;
    let id = row["documentId"].as_i64().ok_or_else(unknown)?;
    let mut owner = state.native.lock_company(slug).map_err(|_| unknown())?;
    let attempt = owner
        .sales(
            cookie,
            &SystemClock,
            rentemester_application::sales::Action::Pdf(id),
            today,
            now,
            None,
        )
        .map_err(|_| unknown())?;
    if attempt.retired {
        return Err(unknown());
    }
    let bytes = match attempt.outcome {
        Ok(c) if c.control_reusable => match c.value {
            rentemester_application::sales::Output::Pdf(bytes) => bytes,
            _ => return Err(unknown()),
        },
        Err(f) => {
            return Err(RunError {
                unknown: f.stage == FailureStage::OutcomeUnknown
                    || !f.control_reusable
                    || !f.company_reusable,
                attempted: false,
            });
        }
        _ => return Err(unknown()),
    };
    // Pure native MIME construction validates config, recipient and admitted original PDF.
    // No SMTP transport or native delivery receipt is created for a source dry-run.
    rentemester_runtime::invoice_mail::build(
        identity,
        recipient,
        "invoice",
        number,
        &bytes,
        &format!("{number}.pdf"),
    )
    .map_err(|_| known())?;
    Ok(())
}
/// Reuse the native local admission before reserving the source attempt.
/// Dropping this validation admission performs no provider I/O or reservation.
fn public_preflight(
    state: &Product,
    slug: &str,
    cookie: &VerifiedCookie,
    document: i64,
) -> Result<(), RunError> {
    use rentemester_application::public_einvoice::{Intent, Next, Request};
    let client = state.native.public_einvoice.get(slug).ok_or_else(known)?;
    let environment = match client.environment() {
        rentemester_runtime::public_einvoice::Environment::Test => "test",
        rentemester_runtime::public_einvoice::Environment::Production => "production",
    };
    let mut owner = state.native.lock_company(slug).map_err(|_| unknown())?;
    let attempt = owner
        .start_public_einvoice(cookie, &SystemClock, Intent::Send(document), environment)
        .map_err(|_| unknown())?;
    if attempt.retired {
        return Err(unknown());
    }
    match attempt.outcome {
        Ok(c) if c.control_reusable => match c.value {
            Next::Pending(admission) if matches!(admission.request(), Request::Validate { .. }) => {
                Ok(())
            }
            _ => Err(known()),
        },
        Ok(_) => Err(unknown()),
        Err(f) => Err(RunError {
            unknown: f.stage == FailureStage::OutcomeUnknown
                || !f.control_reusable
                || !f.company_reusable,
            attempted: false,
        }),
    }
}
fn deliver(
    state: &Product,
    slug: &str,
    cookie: &VerifiedCookie,
    row: &Value,
    today: &str,
    now: &str,
) -> Result<bool, RunError> {
    let generation = row["generationId"].as_i64().ok_or_else(unknown)?;
    let document = row["documentId"].as_i64().ok_or_else(unknown)?;
    let channel = row["channel"].as_str().ok_or_else(unknown)?;
    let latest = row["latestEvent"].as_str().unwrap_or("");
    if matches!(latest, "attempted" | "uncertain" | "terminal_failed") {
        return Err(known());
    }
    let observing = latest == "accepted_pending";
    if observing
        && (channel != "digisense"
            || row["canObserve"] != true
            || row["providerId"].as_str().is_none())
    {
        return Err(known());
    }
    if !observing {
        let preflight = match channel {
            "email" => email_preview(state, slug, cookie, row, today, now),
            "digisense" => public_preflight(state, slug, cookie, document),
            _ => Err(known()),
        };
        if let Err(f) = preflight {
            if f.unknown {
                return Err(f);
            }
            let message = if channel == "email" {
                if state.native.invoice_mail.contains_key(slug)
                    && !state.native.recurring_email_dry_run.contains_key(slug)
                {
                    "live email automation is unavailable; configure an explicit recurring dry-run"
                } else {
                    "email dry-run configuration, recipient or original is unavailable"
                }
            } else {
                "configured public delivery adapter is unavailable"
            };
            phase(
                state,
                slug,
                cookie,
                Action::Event {
                    generation_id: generation,
                    event_type: "preflight_failed",
                    provider_id: None,
                    message,
                },
                today,
                now,
            )?;
            return Err(known());
        }
        let reserved = phase(
            state,
            slug,
            cookie,
            Action::Reserve {
                generation_id: generation,
            },
            today,
            now,
        )?;
        if reserved["reserved"] != true {
            return Ok(false);
        }
    }
    let outcome = (|| {
        if channel == "email" {
            phase(
                state,
                slug,
                cookie,
                Action::Event {
                    generation_id: generation,
                    event_type: "acknowledged",
                    provider_id: None,
                    message: "email dry-run completed; no actual delivery",
                },
                today,
                now,
            )?;
            return Ok(true);
        }
        let intent = if observing {
            rentemester_application::public_einvoice::Intent::Status(document)
        } else {
            rentemester_application::public_einvoice::Intent::Send(document)
        };
        match super::public_einvoice::drive(state, slug, cookie, intent) {
            Ok(value) => {
                let submission = value.get("submission").ok_or_else(unknown)?;
                let provider_id = submission["transmissionId"].as_str();
                let (kind, message) = match submission["status"].as_str() {
                    Some("acknowledged") => ("acknowledged", "public delivery acknowledged"),
                    Some("queued") | Some("prepared") if provider_id.is_some() => {
                        ("accepted_pending", "public delivery accepted and pending")
                    }
                    Some("failed") => (
                        "terminal_failed",
                        "public delivery failed; manual reconciliation required",
                    ),
                    _ => (
                        "uncertain",
                        "public delivery outcome uncertain; manual reconciliation required",
                    ),
                };
                phase(
                    state,
                    slug,
                    cookie,
                    Action::Event {
                        generation_id: generation,
                        event_type: kind,
                        provider_id,
                        message,
                    },
                    today,
                    now,
                )?;
                if matches!(kind, "terminal_failed" | "uncertain") {
                    return Err(known());
                }
                Ok(!observing)
            }
            Err(response) => {
                // Accepted pending never falls back to send after an observation failure.
                let kind = if observing {
                    "accepted_pending"
                } else {
                    "uncertain"
                };
                phase(
                    state,
                    slug,
                    cookie,
                    Action::Event {
                        generation_id: generation,
                        event_type: kind,
                        provider_id: if observing {
                            row["providerId"].as_str()
                        } else {
                            None
                        },
                        message: "public delivery check failed; reconcile before any new action",
                    },
                    today,
                    now,
                )?;
                if response.status().is_server_error() {
                    Err(unknown())
                } else {
                    Err(known())
                }
            }
        }
    })();
    outcome.map_err(|mut failure: RunError| {
        failure.attempted = !observing;
        failure
    })
}
fn company(
    state: &Product,
    slug: &str,
    cookie: &VerifiedCookie,
    input: &Input,
    today: &str,
    now: &str,
) -> (Value, bool) {
    let mut generated = 0u64;
    let mut attempted = 0u64;
    let mut remaining = 0u64;
    let mut failed = false;
    let mut stop = false;
    let run = (|| -> Result<(), RunError> {
        let plan = phase(
            state,
            slug,
            cookie,
            Action::Plan {
                as_of: &input.as_of_date,
                max: input.max_generations.unwrap_or(500),
            },
            today,
            now,
        )?;
        remaining = plan["initialMissing"].as_u64().ok_or_else(unknown)?;
        for item in plan["missing"].as_array().ok_or_else(unknown)? {
            let action = Action::Generate {
                template_id: item["templateId"].as_i64().ok_or_else(unknown)?,
                period_index: item["periodIndex"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(unknown)?,
                as_of: &input.as_of_date,
            };
            match phase(state, slug, cookie, action, today, now) {
                Ok(v) if v["ok"] == true => {
                    remaining = remaining.saturating_sub(1);
                    if v["created"] == true {
                        generated += 1;
                    }
                }
                Ok(_) => failed = true,
                Err(f) if !f.unknown => failed = true,
                Err(f) => return Err(f),
            }
        }
        // Re-read candidates after generation, under current company authority and backup gate.
        let plan = phase(
            state,
            slug,
            cookie,
            Action::Plan {
                as_of: &input.as_of_date,
                max: input.max_generations.unwrap_or(500),
            },
            today,
            now,
        )?;
        for row in plan["deliveries"].as_array().ok_or_else(unknown)? {
            match deliver(state, slug, cookie, row, today, now) {
                Ok(true) => attempted += 1,
                Ok(false) => {}
                Err(f) if !f.unknown => {
                    attempted += u64::from(f.attempted);
                    failed = true;
                }
                Err(f) => {
                    attempted += u64::from(f.attempted);
                    return Err(f);
                }
            }
        }
        Ok(())
    })();
    if let Err(f) = run {
        failed = true;
        stop = f.unknown;
    }
    let mut result = json!({"slug":slug,"status":if failed {"failed"} else {"processed"},"generated":generated,"attempted":attempted,"hasMore":remaining>0,"remainingGenerations":remaining});
    if failed {
        result["reason"] = json!("run failed");
    }
    if remaining > 0 {
        result["continuation"] = json!({"remainingGenerations":remaining});
    }
    (result, stop)
}
pub(super) async fn handle(State(state): State<Product>, request: Request) -> Response {
    if request.headers().contains_key("x-rentemester-service-key")
        || request.headers().contains_key("authorization")
    {
        return refused(StatusCode::UNAUTHORIZED, "human_cookie_required");
    }
    let cookie = match proof(&state, &request, true) {
        Ok(c) => c,
        Err(r) => return *r,
    };
    if request.uri().query().is_some()
        || request.headers().contains_key("content-encoding")
        || !one(request.headers(), "content-type")
            .is_some_and(|s| s.eq_ignore_ascii_case("application/json"))
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(bytes) = to_bytes(request.into_body(), 16 * 1024).await else {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "invalid_input");
    };
    let Ok(input) = serde_json::from_slice::<Input>(&bytes) else {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    };
    if !input.confirm
        || input.workspace.is_empty()
        || input.workspace.len() > 4096
        || input.workspace.chars().any(char::is_control)
        || !(1..=5000).contains(&input.max_generations.unwrap_or(500))
        || rentemester_engine::fiscal_year::Date::parse(&input.as_of_date).is_err()
    {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(ms) = rentemester_runtime::auth::Clock::now_unix_ms(&SystemClock) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    let (Ok(today), Ok(now)) = (
        rentemester_runtime::posting_time::latest_date(ms, 0),
        rentemester_engine::saft::timestamp(ms),
    ) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "clock_unavailable");
    };
    if input.as_of_date > today {
        return refused(StatusCode::BAD_REQUEST, "invalid_input");
    }
    let Ok(permit) = state.workers.clone().try_acquire_owned() else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "busy");
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit=permit;
        let catalog = {
            let Ok(mut owner) = state.native.owner.lock() else { return refused(StatusCode::SERVICE_UNAVAILABLE,"owner_unavailable"); };
            let Ok(attempt)=owner.recurring_workspace_catalog(&state.native.cookies,&cookie,&SystemClock,&input.workspace) else { return refused(StatusCode::SERVICE_UNAVAILABLE,"owner_unavailable"); };
            if attempt.retired { return refused(StatusCode::SERVICE_UNAVAILABLE,"outcome_unknown"); }
            match attempt.outcome { Ok(v)=>v,Err(_)=>return refused(StatusCode::FORBIDDEN,"workspace_access_denied") }
        };
        let Some(entries)=catalog["companies"].as_array() else { return refused(StatusCode::SERVICE_UNAVAILABLE,"catalog_unconfirmed"); };
        let mut entries=entries.clone(); entries.sort_by(|a,b|a["slug"].as_str().cmp(&b["slug"].as_str()));
        let mut companies=vec![]; let mut targets=vec![];
        for entry in entries {
            let Some(slug)=entry["slug"].as_str() else { return refused(StatusCode::SERVICE_UNAVAILABLE,"catalog_unconfirmed"); };
            if entry["archived"]==true { companies.push(json!({"slug":slug,"status":"skipped","reason":"archived"})); }
            else if entry["initialized"]==false { companies.push(json!({"slug":slug,"status":"skipped","reason":"not initialized"})); }
            else { targets.push(slug.to_owned()); }
        }
        let mut errors=vec![];let mut stopped=false;
        for slug in targets {
            let (value,stop)=company(&state,&slug,&cookie,&input,&today,&now);
            if value["status"]=="failed" { errors.push(format!("recurring run failed for company {slug}")); }
            companies.push(value);
            if stop { stopped=true;errors.push("workspace outcome unknown; reconcile partial results before any further action".into());break; }
        }
        let continuations=companies.iter().filter(|c|c["hasMore"]==true).map(|c|json!({"slug":c["slug"],"remainingGenerations":c["remainingGenerations"]})).collect::<Vec<_>>();
        let remaining=continuations.iter().filter_map(|c|c["remainingGenerations"].as_u64()).sum::<u64>();
        let mut value=json!({"ok":errors.is_empty(),"companies":companies,"errors":errors,"hasMore":remaining>0,"remainingGenerations":remaining});
        if remaining>0 {value["continuation"]=json!({"remainingGenerations":remaining,"companies":continuations});}
        if stopped {value["code"]=json!("outcome_unknown");}
        response(StatusCode::OK,value)
    }).await;
    match result {
        Ok(r) => r,
        Err(_) => refused(StatusCode::SERVICE_UNAVAILABLE, "outcome_unknown"),
    }
}
