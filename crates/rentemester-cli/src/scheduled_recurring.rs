use rentemester_client::{
    ApiClient, MAX_SCHEDULED_RECURRING_BODY, ScheduledRecurringMutation,
    scheduled_recurring_body_schema,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
type Failure = (u8, Error);
fn invalid(s: &str) -> Failure {
    (2, Error::new("invalid_input", s))
}
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "scheduled-recurring"
            | "scheduled-recurring-plan"
            | "enable-scheduled-recurring"
            | "pause-scheduled-recurring"
            | "resume-scheduled-recurring"
            | "cancel-scheduled-recurring"
            | "catch-up-scheduled-recurring"
            | "skip-scheduled-recurring"
            | "dispatch-scheduled-recurring"
            | "reconsent-scheduled-recurring"
    )
}
pub fn help() -> Value {
    json!({"usage":"scheduled-recurring --company SLUG; scheduled-recurring-plan --company SLUG --id TEMPLATE_ID; enable/pause/resume/cancel/catch-up/skip/dispatch/reconsent-scheduled-recurring --company SLUG --confirm < exact-native.json. Explicit --server/private MFA --cookie-file; native16KiB.","policy":"Status/plan GETs are read-only and never dispatch. Current CompanyRead; restored scheduler reconsent requires current CompanyAdmin and ONLY records explicit consent, never dispatches or starts a timer; other mutations current CompanyDraftWrite, server-bound consenting owner and exact current template/state/plan hashes. Catch-up queues only; skip audits only. Dispatch is caller-driven with an active authorized session and creates actual issued JSON/PDF UNPOSTED/UNSENT. No hidden local service credentials, period/date/currency/template/principal defaults, calculation, key injection or automatic retry. After outcome_unknown read canonical status/plan before any further manual action.","bodySchemas":{"reconsent":scheduled_recurring_body_schema(ScheduledRecurringMutation::Reconsent),"enable":scheduled_recurring_body_schema(ScheduledRecurringMutation::Enable),"state":scheduled_recurring_body_schema(ScheduledRecurringMutation::Pause),"catchUp":scheduled_recurring_body_schema(ScheduledRecurringMutation::CatchUp),"skip":scheduled_recurring_body_schema(ScheduledRecurringMutation::Skip),"dispatch":scheduled_recurring_body_schema(ScheduledRecurringMutation::Dispatch)}})
}
pub fn run(
    command: &str,
    flags: &BTreeMap<&str, &str>,
    allow: bool,
    confirmed: bool,
    preview: bool,
) -> Result<Value, Failure> {
    let read = matches!(command, "scheduled-recurring" | "scheduled-recurring-plan");
    let needs_id = command == "scheduled-recurring-plan";
    if confirmed == read
        || preview
        || flags.keys().any(|k| {
            !(["--server", "--cookie-file", "--company"].contains(k) || needs_id && *k == "--id")
        })
    {
        return Err(invalid(
            "scheduled status has no selector/confirmation, plan requires --id only; writes require --confirm and exact stdin body",
        ));
    }
    let id = if needs_id {
        flags
            .get("--id")
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|n| (1..=9_007_199_254_740_991).contains(n))
            .ok_or_else(|| invalid("positive exact --id required"))?
    } else {
        0
    };
    let company = flags
        .get("--company")
        .ok_or_else(|| invalid("scheduled recurring requires --company"))?;
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server required"))?;
    let cookie = flags
        .get("--cookie-file")
        .ok_or_else(|| invalid("private --cookie-file required"))?;
    let cookie = rentemester_client::read_cookie_file(std::path::Path::new(cookie))
        .map_err(crate::remote::client_error)?;
    let api = ApiClient::new(server, allow, Some(cookie)).map_err(crate::remote::client_error)?;
    let mut bytes = vec![];
    if !read {
        std::io::stdin()
            .take(MAX_SCHEDULED_RECURRING_BODY as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("could not read exact scheduled recurring body"))?;
    }
    let runtime =
        tokio::runtime::Runtime::new().map_err(|_| invalid("could not start HTTP runtime"))?;
    let result = runtime
        .block_on(async {
            if command == "scheduled-recurring" {
                api.scheduled_recurring(company).await
            } else if needs_id {
                api.scheduled_recurring_plan(company, id).await
            } else {
                api.mutate_scheduled_recurring(
                    company,
                    match command {
                        "enable-scheduled-recurring" => ScheduledRecurringMutation::Enable,
                        "pause-scheduled-recurring" => ScheduledRecurringMutation::Pause,
                        "resume-scheduled-recurring" => ScheduledRecurringMutation::Resume,
                        "cancel-scheduled-recurring" => ScheduledRecurringMutation::Cancel,
                        "catch-up-scheduled-recurring" => ScheduledRecurringMutation::CatchUp,
                        "skip-scheduled-recurring" => ScheduledRecurringMutation::Skip,
                        "reconsent-scheduled-recurring" => ScheduledRecurringMutation::Reconsent,
                        _ => ScheduledRecurringMutation::Dispatch,
                    },
                    bytes,
                    confirmed,
                )
                .await
            }
        })
        .map_err(crate::remote::client_error)?;
    Ok(json!({"ok":result.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":result}))
}
