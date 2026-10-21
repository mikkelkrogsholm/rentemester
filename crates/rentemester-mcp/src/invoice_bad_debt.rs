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
        "Current human MFA CompanyLedgerPost only; exact16KiB JSON string once, inner and outer confirm:true. Operator confirms documented established loss; server does not establish legal evidence. Optional grossAmount is ORIGINAL invoice currency, omission writes off current open principal. Explicit compatible expense account; original debtor and output VAT accounts/amounts/current dated carrying are server-derived. Read original-currency amounts and actual DKK journal effects separately. Does not extinguish reminder/compensation/interest claims. No email, arbitrary accounts, actor/key, recovery feature or automatic retry. Unknown outcome requires canonical invoice_reminder_claims/invoice/journal reads and manual resolution.",
        schema,
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
            t["inputSchema"]["required"],
            json!(["company", "body", "confirm"])
        );
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert!(
            serde_json::from_value::<Write>(
                json!({"company":"synthetic","body":"{}","confirm":true,"actor":"forged"})
            )
            .is_err()
        );
    }
}
