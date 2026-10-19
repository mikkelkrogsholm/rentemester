//! Native planning tool contracts. Confirmation and original domain proof stay caller-owned.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, CommitmentMutation as Mutation, Error, commitment_body_schema,
    commitment_plan_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "supplier_commitments"
            | "supplier_commitment_matches"
            | "plan_supplier_commitment"
            | "approve_supplier_commitment"
            | "change_supplier_commitment"
            | "match_supplier_commitment"
    )
}
pub fn tools() -> Vec<Value> {
    let company = company_schema();
    let mut result = vec![
        tool(
            "supplier_commitments",
            "Read native supplier commitments, original snapshots, current payloadHash/status, immutable history, unmatched scheduled occurrences, canonical evidenceOptions, and13-week forecast with startDate selector. CompanyRead. Coverage is explicitly incomplete; unavailable cash stays null, foreign FX excluded, undated budgets informational only. Read before reconciling uncertain writes",
            json!({"type":"object","required":["company"],"properties":{"company":company,"startDate":{"type":"string","minLength":10,"maxLength":10,"pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$","description":"Real ISO date; omitted uses server trusted Danish date. No fiscal year selector"}},"additionalProperties":false}),
            true,
        ),
        tool(
            "supplier_commitment_matches",
            "Read persisted canonical occurrence/evidence links and immutable variance; CompanyRead, no selectors",
            json!({"type":"object","required":["company"],"properties":{"company":company},"additionalProperties":false}),
            true,
        ),
        tool(
            "plan_supplier_commitment",
            "Read-only native POST: validate explicit existing native vendor:<id>, source document:<id>, amount/currency, cadence/dates and purpose; return plan.payloadHash plus exact schedule/source snapshot/warnings. CompanyRead. Creates no approval/event/accounting. Review result before separate approval; never invent proof or automatically approve",
            json!({"type":"object","required":["company","body"],"properties":{"company":company,"body":native_body(commitment_plan_schema())},"additionalProperties":false}),
            true,
        ),
    ];
    for (name, op, description) in [
        (
            "approve_supplier_commitment",
            Mutation::Apply,
            "Approve the exact reviewed commitment and returned plan.payloadHash; stale vendor/source/prior revision refused. Optional native idempotencyKey must be caller supplied, never injected",
        ),
        (
            "change_supplier_commitment",
            Mutation::Change,
            "Reasoned pause/end/supersede of current commitment; use current expectedPayloadHash from supplier_commitments. History retained; future planned occurrences removed",
        ),
        (
            "match_supplier_commitment",
            Mutation::Match,
            "Match one real scheduled occurrence using canonical evidence kind+id from evidenceOptions. Server resolves original hash, date, amount, currency and identity; one occurrence/evidence link, duplicates refused. No factual overrides and no accounting/payment creation",
        ),
    ] {
        result.push(tool(name,&format!("{description}. CompanyDraftWrite. Exact native body preserved byte-for-byte; confirm:true required both inside body and outer tool args. One attempt, no automatic retry or fallback. On outcome_unknown reconcile current supplier_commitments/history/matches and original approval key/proof before action."),json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company,"body":native_body(commitment_body_schema(op)),"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    result
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Read {
    company: String,
    start_date: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Company {
    company: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    company: String,
    body: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Write {
    company: String,
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    Ok(match name {
        "supplier_commitments" => {
            if args.get("startDate").is_some_and(|v| !v.is_string()) {
                return Err("startDate must be a real ISO date string");
            }
            let a: Read = serde_json::from_value(args)
                .map_err(|_| "supplier_commitments accepts company and optional startDate only")?;
            api.supplier_commitments(&a.company, a.start_date.as_deref())
                .await
        }
        "supplier_commitment_matches" => {
            let a: Company =
                serde_json::from_value(args).map_err(|_| "matches read accepts company only")?;
            api.supplier_commitment_matches(&a.company).await
        }
        "plan_supplier_commitment" => {
            let a: Plan = serde_json::from_value(args).map_err(
                |_| "read-only plan accepts company and exact body only, no confirmation",
            )?;
            api.plan_supplier_commitment(&a.company, a.body.into_bytes())
                .await
        }
        _ => {
            let a: Write = serde_json::from_value(args)
                .map_err(|_| "commitment write requires company, exact body and confirm:true")?;
            if !a.confirm {
                return Err("explicit commitment confirmation required");
            }
            let op = match name {
                "approve_supplier_commitment" => Mutation::Apply,
                "change_supplier_commitment" => Mutation::Change,
                "match_supplier_commitment" => Mutation::Match,
                _ => return Err("unknown commitment tool"),
            };
            api.mutate_supplier_commitment(&a.company, op, a.body.into_bytes(), a.confirm)
                .await
        }
    })
}
