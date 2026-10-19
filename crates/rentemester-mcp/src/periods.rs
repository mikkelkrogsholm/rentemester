//! Explicit review and commitment over existing server period controls.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, PeriodMutation, period_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(
        name,
        "vat"
            | "periods"
            | "period_close_readiness"
            | "review_period"
            | "close_period"
            | "reopen_period"
            | "report_period"
    )
}
pub fn tools() -> Vec<Value> {
    let company = company_schema();
    let mut result = vec![
        tool(
            "vat",
            "Read server-calculated VAT totals, registered cadence, period windows and filing state; optional fiscal year",
            json!({"type":"object","required":["company"],"properties":{"company":company,"year":{"type":"string","maxLength":128}},"additionalProperties":false}),
            true,
        ),
        tool(
            "periods",
            "List effective period status, original closePacketHash and immutable receipt reference; read before reconciling uncertain writes",
            json!({"type":"object","required":["company"],"properties":{"company":company},"additionalProperties":false}),
            true,
        ),
        tool(
            "period_close_readiness",
            "Read current close packet, controls, blockers and hash for explicit bounds; requires company-review permission. Review separately before closing",
            json!({"type":"object","required":["company","from","to"],"properties":{"company":company,"from":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},"to":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"}},"additionalProperties":false}),
            true,
        ),
    ];
    for (name, op, description) in [
        (
            "review_period",
            PeriodMutation::Review,
            "Append explicit review of current readiness controls. Returns review.id and review.packet.hash; inspect controls before close",
        ),
        (
            "close_period",
            PeriodMutation::Close,
            "Close using caller-selected current reviewId and packetHash; stale proof, blockers, future bounds and overlaps refused. Forced close unavailable. Lock prevents dated posting",
        ),
        (
            "reopen_period",
            PeriodMutation::Reopen,
            "Reopen closed unreported period with nonblank reason; appends audit without rewriting original close",
        ),
        (
            "report_period",
            PeriodMutation::Report,
            "Record receipt of externally completed filing using original closePacketHash from periods; reference immutable, reported period cannot reopen. No filing, email or network submission",
        ),
    ] {
        result.push(tool(name,&format!("{description}. Company-review authority. Exact body bytes, confirm:true inside body and outer confirm:true. No generic idempotency keys or automatic retries; reconcile outcome_unknown with periods/readiness before further action."),json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company,"body":native_body(period_body_schema(op)),"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    result
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
    year: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bounds {
    company: String,
    from: String,
    to: String,
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
        "vat" | "periods" => {
            if args.get("year").is_some_and(|v| !v.is_string())
                || (name == "periods" && args.get("year").is_some())
            {
                return Err("only VAT accepts string year");
            }
            let a: Read =
                serde_json::from_value(args).map_err(|_| "invalid period read arguments")?;
            if name == "vat" {
                api.vat(&a.company, a.year.as_deref()).await
            } else {
                api.periods(&a.company).await
            }
        }
        "period_close_readiness" => {
            let a: Bounds =
                serde_json::from_value(args).map_err(|_| "readiness requires company/from/to")?;
            api.period_close_readiness(&a.company, &a.from, &a.to).await
        }
        _ => {
            let a: Write = serde_json::from_value(args)
                .map_err(|_| "period write requires company, exact native body and confirm:true")?;
            if !a.confirm {
                return Err("explicit confirmation required");
            }
            let op = match name {
                "review_period" => PeriodMutation::Review,
                "close_period" => PeriodMutation::Close,
                "reopen_period" => PeriodMutation::Reopen,
                "report_period" => PeriodMutation::Report,
                _ => return Err("unknown period tool"),
            };
            api.period_mutation(&a.company, op, a.body.into_bytes(), a.confirm)
                .await
        }
    })
}
