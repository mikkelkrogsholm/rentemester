//! Canonical native archive source transport, without client parsing or automatic retries.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, MAX_ARCHIVE_IMPORT_BODY, archive_import_body_schema};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    [
        "archive_import_plan",
        "archive_import_apply",
        "archives",
        "archive_import_original",
    ]
    .contains(&name)
}
pub fn tools() -> Vec<Value> {
    let mut out = vec![];
    for (name, apply) in [
        ("archive_import_plan", false),
        ("archive_import_apply", true),
    ] {
        let mut body = native_body(archive_import_body_schema(apply));
        body["maxLength"] = json!(MAX_ARCHIVE_IMPORT_BODY);
        let mut schema = json!({"type":"object","properties":{"company":company_schema(),"body":body},"required":["company","body"],"additionalProperties":false});
        if apply {
            schema["properties"]["confirm"] = json!({"const":true});
            schema["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("confirm"));
        }
        out.push(tool(name,"Review/install closed-history Dinero source archives only. Exact native35MiB source JSON; apply requires reviewed planHash, caller idempotencyKey, inner+outer confirm:true. Server owns strict CSV/source attestation/current admin/fences. No live journal migration, inferred currency, parsing/math/generatedkeys/retry. Reconcile unknown apply via canonical archives and original source hash before deliberate exact-key action.",schema,!apply));
    }
    out.push(tool("archives","Read original-backed closed history years or one exact fiscalYear, preserving raw six-decimal source values and separately quantized money/provenance. No mutation or inferred balances.",json!({"type":"object","properties":{"company":company_schema(),"fiscalYear":{"type":"integer","minimum":1000,"maximum":9999}},"required":["company"],"additionalProperties":false}),true));
    out.push(tool("archive_import_original","Read exact retained source package as application/json integer bytes, max25MiB. Current company/document read authority; no local file or verification claim.",json!({"type":"object","properties":{"company":company_schema(),"importId":{"type":"integer","minimum":1,"maximum":9007199254740991u64}},"required":["company","importId"],"additionalProperties":false}),true));
    out
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    company: String,
    body: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    company: String,
    body: String,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Years {
    company: String,
    fiscal_year: Option<u16>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Original {
    company: String,
    import_id: i64,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    Ok(match name {
        "archive_import_plan" => {
            let v: Plan = serde_json::from_value(args)
                .map_err(|_| "exact company and read-only body required")?;
            if v.body.len() > MAX_ARCHIVE_IMPORT_BODY {
                return Err("native35MiB source body required");
            }
            api.archive_import(&v.company, v.body.into_bytes(), false, false)
                .await
        }
        "archive_import_apply" => {
            let v: Apply = serde_json::from_value(args)
                .map_err(|_| "exact company, body and confirm:true required")?;
            if !v.confirm || v.body.len() > MAX_ARCHIVE_IMPORT_BODY {
                return Err("explicit confirmation and native35MiB source body required");
            }
            api.archive_import(&v.company, v.body.into_bytes(), true, v.confirm)
                .await
        }
        "archives" => {
            let v: Years = serde_json::from_value(args)
                .map_err(|_| "exact company and optional fiscalYear required")?;
            api.archives(&v.company, v.fiscal_year).await
        }
        "archive_import_original" => {
            let v: Original =
                serde_json::from_value(args).map_err(|_| "exact company and importId required")?;
            api.archive_import_original(&v.company, v.import_id).await
        }
        _ => return Err("unknown archive tool"),
    })
}
