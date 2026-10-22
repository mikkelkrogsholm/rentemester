use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, MAX_BUDGET_BODY, budget_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "budget" | "budget_vs_actual" | "budget_dimension_actuals" | "set_budget"
    )
}
pub fn tools() -> Vec<Value> {
    let year = json!({"type":"string","minLength":1,"maxLength":128,"description":"Exact native selectedYear/fiscalYears label"});
    let mut out = vec![];
    for name in ["budget", "budget_vs_actual", "budget_dimension_actuals"] {
        let mut props = json!({"company":company_schema(),"year":year});
        if name == "budget_dimension_actuals" {
            for k in ["dimension", "member"] {
                props[k] = json!({"type":"string","minLength":1,"maxLength":64});
            }
        }
        out.push(tool(name,"Read full server budget revisions/actuals/dimension comparison under CompanyRead. Explicit fiscal year; optional dimension/member only on dimension comparison, member requires dimension. Server owns totals/variance/actuals. Preserve accountBudget/accountActual/unallocated/other members, null unreviewed dimension budget/variance, selections/options and advisory notes. Never infer allocation or money.",json!({"type":"object","required":["company","year"],"properties":props,"additionalProperties":false}),true));
    }
    let comparison=out.iter_mut().find(|t|t["name"]=="budget_vs_actual").unwrap();
    let native=comparison["inputSchema"].clone();
    comparison["inputSchema"]=json!({"type":"object","oneOf":[native,crate::budget_original::comparison_schema()]});
    let mut body = native_body(budget_body_schema());
    body["maxLength"] = json!(MAX_BUDGET_BODY);
    out.push(tool("set_budget","CompanyAdmin: append one exact native16KiB {accountNo,period,amount,notes?} revision. Outer confirm:true required; native body has NO inner confirm/key/actor. No deduplication promise, journal effects or automatic retries. Unknown outcome requires canonical budget revision read before further action.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    out
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
    year: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Dimensions {
    company: String,
    year: String,
    dimension: Option<String>,
    member: Option<String>,
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
    if name == "set_budget" {
        let r: Write = serde_json::from_value(args)
            .map_err(|_| "set_budget requires only company/body/confirm")?;
        if r.body.len() > MAX_BUDGET_BODY {
            return Err("native budget body exceeds16KiB");
        }
        return Ok(api
            .set_budget(&r.company, r.body.into_bytes(), r.confirm)
            .await);
    }
    if name == "budget_dimension_actuals" {
        let r: Dimensions = serde_json::from_value(args)
            .map_err(|_| "dimension comparison requires company/year/dimension?/member? only")?;
        return Ok(api
            .budget_dimension_actuals(
                &r.company,
                &r.year,
                r.dimension.as_deref(),
                r.member.as_deref(),
            )
            .await);
    }
    let r: Read =
        serde_json::from_value(args).map_err(|_| "budget reads require only company/year")?;
    Ok(if name == "budget" {
        api.budget(&r.company, &r.year).await
    } else {
        api.budget_vs_actual(&r.company, &r.year).await
    })
}
