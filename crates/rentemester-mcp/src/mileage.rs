use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, MAX_MILEAGE_BODY, mileage_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(s, "mileage" | "register_mileage")
}
pub fn tools() -> Vec<Value> {
    let mut body = native_body(mileage_body_schema());
    body["maxLength"] = json!(MAX_MILEAGE_BODY);
    vec![
        tool(
            "mileage",
            "CompanyRead: full mileage fiscal/month report unchanged. Explicit native selectedYear/fiscalYears label required. No amount/rate/tax eligibility inference or journal effects.",
            json!({"type":"object","required":["company","year"],"properties":{"company":company_schema(),"year":{"type":"string","minLength":1,"maxLength":16}},"additionalProperties":false}),
            true,
        ),
        tool(
            "register_mileage",
            "CompanyLedgerPost: exact native32KiB caller-chosen trip/route/driver/vehicle/kilometers/ratePerKm/rateBasis once, inner/outer confirm:true. Server owns precision, amount and rate advisory; never select/default rates or infer tax eligibility. No journal posting, caller keys, deduplication or automatic retries. Unknown outcomes require canonical mileage reads before further action.",
            json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),
            false,
        ),
    ]
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
    year: String,
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
    if name == "mileage" {
        let r: Read =
            serde_json::from_value(args).map_err(|_| "mileage requires only company/year")?;
        return Ok(api.mileage(&r.company, &r.year).await);
    }
    let r: Write = serde_json::from_value(args)
        .map_err(|_| "register_mileage requires only company/body/confirm")?;
    if r.body.len() > MAX_MILEAGE_BODY {
        return Err("native mileage body exceeds32KiB");
    }
    Ok(api
        .register_mileage(&r.company, r.body.into_bytes(), r.confirm)
        .await)
}
