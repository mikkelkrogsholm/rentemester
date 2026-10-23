//! Thin exact-body mileage tools; the server is the only accounting authority.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_MILEAGE_POSTING_BODY, MileagePostingOperation as Op,
    mileage_posting_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(
        s,
        "mileage_rate_versions"
            | "configure_mileage_rates"
            | "preview_mileage_post"
            | "post_mileage"
            | "mileage_postings"
    )
}
pub fn tools() -> Vec<Value> {
    ["mileage_rate_versions","configure_mileage_rates","preview_mileage_post","post_mileage","mileage_postings"].into_iter().map(|name| {
        let op=operation(name);
        let write=matches!(op,Some(Op::Configure|Op::Post));
        let mut props=json!({"company":company_schema()});let mut required=vec!["company"];
        if let Some(op)=op {let mut body=native_body(mileage_posting_body_schema(op));body["maxLength"]=json!(MAX_MILEAGE_POSTING_BODY);props["body"]=body;required.push("body");}
        if write {props["confirm"]=json!({"const":true});required.push("confirm");}
        tool(name,"Current human cookie only; service routes not admitted, no credential fallback. Explicit rate version/config/source/original and current company permissions required. Exact32KiB body string; configure/post require inner and outer confirm:true. Preview is read-only. Logging never posts. Server owns rates, arithmetic, current evidence, journal and duplicate-protected receipt. No inferred rates, generated keys, automatic posting or retry. On outcome_unknown inspect mileage_postings before any further action.",json!({"type":"object","properties":props,"required":required,"additionalProperties":false}),!write)
    }).collect()
}
fn operation(name: &str) -> Option<Op> {
    match name {
        "configure_mileage_rates" => Some(Op::Configure),
        "preview_mileage_post" => Some(Op::Preview),
        "post_mileage" => Some(Op::Post),
        _ => None,
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preview {
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
    if !accepts(name) {
        return Err("unknown mileage posting tool");
    }
    match operation(name) {
        None => {
            let r: Read = serde_json::from_value(args).map_err(|_| "only company allowed")?;
            Ok(if name == "mileage_postings" {
                api.mileage_postings(&r.company).await
            } else {
                api.mileage_rate_versions(&r.company).await
            })
        }
        Some(Op::Preview) => {
            let r: Preview =
                serde_json::from_value(args).map_err(|_| "only company/body allowed")?;
            Ok(api
                .preview_mileage_post(&r.company, r.body.into_bytes())
                .await)
        }
        Some(op) => {
            let r: Write =
                serde_json::from_value(args).map_err(|_| "company/body/confirm required")?;
            Ok(if op == Op::Configure {
                api.configure_mileage_rates(&r.company, r.body.into_bytes(), r.confirm)
                    .await
            } else {
                api.post_mileage(&r.company, r.body.into_bytes(), r.confirm)
                    .await
            })
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mileage_posting_thin_catalogue() {
        let ts = tools();
        assert_eq!(ts.len(), 5);
        for t in ts {
            let name = t["name"].as_str().unwrap();
            assert!(accepts(name));
            let p = &t["inputSchema"]["properties"];
            assert!(p.get("company").is_some());
            assert!(p.get("actor").is_none());
            if operation(name).is_some() {
                assert_eq!(p["body"]["type"], "string");
            }
        }
    }
    #[test]
    fn mileage_posting_outer_validation_without_transport() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let api = ApiClient::new("http://127.0.0.1:1", true, None).unwrap();
            assert!(
                call(
                    &api,
                    "mileage_postings",
                    json!({"company":"s","confirm":true})
                )
                .await
                .is_err()
            );
            assert!(
                call(
                    &api,
                    "preview_mileage_post",
                    json!({"company":"s","body":"{}","actor":"forged"})
                )
                .await
                .is_err()
            );
            assert_eq!(
                call(
                    &api,
                    "post_mileage",
                    json!({"company":"s","body":"{}","confirm":false})
                )
                .await
                .unwrap(),
                Err(Error::InvalidInput("confirmation required"))
            );
        });
    }
}
