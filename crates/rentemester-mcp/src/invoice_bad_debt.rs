use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, MAX_INVOICE_BAD_DEBT_BODY, invoice_bad_debt_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(name: &str) -> bool {
    name == "invoice_write_off_bad_debt"
}
pub fn tools() -> Vec<Value> {
    let mut body = native_body(invoice_bad_debt_body_schema());
    body["maxLength"] = json!(MAX_INVOICE_BAD_DEBT_BODY);
    let schema = json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false});
    let mut descriptor = tool(
        "invoice_write_off_bad_debt",
        "Original alternative {company,payload,confirm:true} resolves invoiceNumber or invoiceDocumentId on server and checks optional debtor/VAT account assertions. Original payload and native body are disjoint. Native form: current human MFA CompanyLedgerPost only; exact16KiB JSON string once, inner and outer confirm:true. Operator confirms documented established loss; server does not establish legal evidence. Optional grossAmount is ORIGINAL invoice currency, omission writes off current open principal. Explicit compatible expense account; original debtor and output VAT accounts/amounts/current dated carrying are server-derived. Read original-currency amounts and actual DKK journal effects separately. Does not extinguish reminder/compensation/interest claims. No email, arbitrary accounts, actor/key, recovery feature or automatic retry. Unknown outcome requires canonical invoice_reminder_claims/invoice/journal reads and manual resolution.",
        json!({"type":"object","oneOf":[schema,original_schema()]}),
        false,
    );
    descriptor["annotations"]["openWorldHint"] = json!(false);
    vec![descriptor]
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
        return Err("unknown invoice bad-debt operation");
    }
    let i: Write =
        serde_json::from_value(args).map_err(|_| "write requires only company/body/confirm")?;
    if i.body.len() > MAX_INVOICE_BAD_DEBT_BODY {
        return Err("bad-debt body exceeds native16KiB");
    }
    Ok(api
        .invoice_bad_debt(&i.company, i.body.into_bytes(), i.confirm)
        .await)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documented_loss_write_is_never_readonly_or_idempotent() {
        let t = tools().remove(0);
        assert_eq!(t["annotations"]["readOnlyHint"], false);
        assert_eq!(t["annotations"]["idempotentHint"], false);
        assert_eq!(t["annotations"]["openWorldHint"], false);
        assert_eq!(
            t["inputSchema"]["oneOf"][0]["required"],
            json!(["company", "body", "confirm"])
        );
        assert_eq!(t["inputSchema"]["oneOf"][0]["additionalProperties"], false);
        assert!(
            serde_json::from_value::<Write>(
                json!({"company":"synthetic","body":"{}","confirm":true,"actor":"forged"})
            )
            .is_err()
        );
    }
}

fn original_schema() -> Value {
    let mut schema = rentemester_client::invoice_bad_debt_original_schema();
    schema["properties"]["company"] = company_schema();
    schema["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("company"));
    schema
}
pub async fn call_original(api: &ApiClient, args: Value) -> Result<Value, &'static str> {
    let mut args = args.as_object().ok_or("expected original object")?.clone();
    let company = args
        .remove("company")
        .and_then(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned))
        .ok_or("company required")?;
    let bytes = serde_json::to_vec(&args).map_err(|_| "invalid payload")?;
    rentemester_client::validate_invoice_bad_debt_original(&bytes).map_err(
        |_| "exact original payload and confirm:true required; no native body or identity fields",
    )?;
    let value = match api.invoice_bad_debt_original(&company, bytes).await {
        Ok(data) => {
            json!({"ok":data["ok"]==true,"errors":data.get("errors").cloned().unwrap_or(json!([])),"appliedRules":data.get("appliedRules").cloned().unwrap_or(json!([])),"data":data})
        }
        Err(error) => {
            json!({"ok":false,"errors":[error.to_string()],"code":match error {Error::InvalidInput(_)=>"invalid_input",Error::OutcomeUnknown=>"outcome_unknown",Error::Transport=>"transport",Error::Protocol=>"protocol",Error::Refused{..}=>"server_refused"}})
        }
    };
    Ok(
        json!({"content":[{"type":"text","text":value.to_string()}],"isError":value["ok"]!=true,"structuredContent":value}),
    )
}

#[cfg(test)]
mod original_tests {
    use super::*;
    #[test]
    fn catalogue_is_unique_and_mixed_forms_refuse() {
        let tools = crate::tools::list();
        let all = tools["tools"].as_array().unwrap();
        let matching = all
            .iter()
            .filter(|v| v["name"] == "invoice_write_off_bad_debt")
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        assert_eq!(
            matching[0]["inputSchema"]["oneOf"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let mixed = json!({"company":"synthetic","body":"{}","payload":{},"confirm":true});
        assert!(serde_json::from_value::<Write>(mixed).is_err());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let api = ApiClient::new("http://127.0.0.1:1", true, None).unwrap();
        assert!(
            rt.block_on(call_original(
                &api,
                json!({"company":"synthetic","payload":{},"confirm":false})
            ))
            .is_err()
        );
    }
}
