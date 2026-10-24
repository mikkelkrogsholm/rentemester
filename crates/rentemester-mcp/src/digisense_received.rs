//! Exact native receive body; server owns intake/authority/dedup.
use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_DIGISENSE_RECEIVED_BODY, digisense_received_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    matches!(name, "efaktura_modtag" | "efaktura_modtag_workspace")
}
pub fn tools() -> Vec<Value> {
    [false,true].into_iter().map(|workspace| {
        let mut body=native_body(digisense_received_schema(workspace));body["maxLength"]=json!(MAX_DIGISENSE_RECEIVED_BODY);
        let mut properties=json!({"body":body,"confirm":{"const":true}});
        let mut required=vec!["body","confirm"];
        if !workspace {properties["company"]=company_schema();required.push("company");}
        tool(if workspace {"efaktura_modtag_workspace"} else {"efaktura_modtag"},"Explicit receive poll, exact native16KiB body once with inner+outer confirm:true. Company body: confirm, companyKey assertion?, limit1..100?, maxTimestamp bounded128?, metadata supported document overrides?, ingestOptions:{forceDuplicateLogicalIdentity?:bool}?, maxPages1..100?. Workspace body only {confirm:true}, all-target current permission+backup preflight before effects. No provider credentials/files/paths/keys/timer/posting/send; fixed digisense_modtag source. Durable quarantine and stable-ID dedup; individual row errors retain good siblings. After outcome_unknown inspect authoritative documents/dedup; no automatic replay.",json!({"type":"object","required":required,"properties":properties,"additionalProperties":false}),false)
    }).collect()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Company {
    company: String,
    body: String,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Workspace {
    body: String,
    confirm: bool,
}
pub async fn call(
    api: &ApiClient,
    name: &str,
    args: Value,
) -> Result<Result<Value, Error>, &'static str> {
    match name {
        "efaktura_modtag" => {
            let i: Company = serde_json::from_value(args)
                .map_err(|_| "receive requires company/body/confirm:true only")?;
            Ok(api
                .digisense_received(&i.company, i.body.into_bytes(), i.confirm)
                .await)
        }
        "efaktura_modtag_workspace" => {
            let i: Workspace = serde_json::from_value(args)
                .map_err(|_| "workspace receive requires body/confirm:true only")?;
            Ok(api
                .digisense_received_workspace(i.body.into_bytes(), i.confirm)
                .await)
        }
        _ => Err("unknown receive tool"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn digisense_received_catalogue_strict() {
        let t = tools();
        assert_eq!(t.len(), 2);
        assert_eq!(t[1]["inputSchema"]["additionalProperties"], false);
        assert!(
            serde_json::from_value::<Workspace>(json!({"company":"x","body":"{}","confirm":true}))
                .is_err()
        );
    }
}
