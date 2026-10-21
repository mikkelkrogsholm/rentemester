use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_REMINDER_CLAIM_BODY, ReminderClaimMutation as Op,
    reminder_claim_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "invoice_reminder_claims"
            | "register_invoice_reminder"
            | "post_invoice_reminder"
            | "settle_invoice_claims"
    )
}
pub fn tools() -> Vec<Value> {
    let mut out = vec![tool(
        "invoice_reminder_claims",
        "Current CompanyRead: reminder/principal balances, ledger-backed receivable accounts, prior receipts, server rule limits, exact bank source choices. No email or automatic action; compensation uses separate calculate/register/post steps; unported interest/corrections refuse explicitly.",
        json!({"type":"object","required":["company","id"],"properties":{"company":company_schema(),"id":{"type":"integer","minimum":1,"maximum":9007199254740991u64}},"additionalProperties":false}),
        true,
    )];
    for (name, op) in [
        ("register_invoice_reminder", Op::Register),
        ("post_invoice_reminder", Op::Post),
        ("settle_invoice_claims", Op::Settle),
    ] {
        let mut body = native_body(reminder_claim_body_schema(op));
        body["maxLength"] = json!(MAX_REMINDER_CLAIM_BODY);
        out.push(tool(name,"Exact confirmed native16KiB body once. Register under draft.write; SEPARATE post/claim receipt under ledger.post. Existing cap100/max3/10days and due/current balance rules remain server-owned. Explicit account choices, active invoice originals and archived bank evidence. Claim receipt only incomingDKK AFTER principal settled, may partially clear historical claim accounts; wrong direction/overclaim/stale/reversal/duplicate refuse. No SMTP/delivery, keys, automatic reminders, rate/math or retries. Unknown writes require canonical reads before further action.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    out
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
    id: i64,
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
    if name == "invoice_reminder_claims" {
        let i: Read = serde_json::from_value(args).map_err(|_| "requires company/id")?;
        return Ok(api.invoice_reminder_claims(&i.company, i.id).await);
    }
    let i: Write = serde_json::from_value(args).map_err(|_| "requires company/body/confirm")?;
    if i.body.len() > MAX_REMINDER_CLAIM_BODY {
        return Err("claim body exceeds native16KiB");
    }
    let op = match name {
        "register_invoice_reminder" => Op::Register,
        "post_invoice_reminder" => Op::Post,
        _ => Op::Settle,
    };
    Ok(api
        .reminder_claim_mutation(&i.company, op, i.body.into_bytes(), i.confirm)
        .await)
}
