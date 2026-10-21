use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{
    ApiClient, Error, InvoiceInterestOperation as Op, MAX_INVOICE_INTEREST_BODY,
    invoice_interest_body_schema,
};
use serde::Deserialize;
use serde_json::{Value, json};
fn operation(name: &str) -> Option<Op> {
    match name {
        "invoice_interest_calc" => Some(Op::Calculate),
        "invoice_claim_interest" => Some(Op::Register),
        "invoice_post_interest" => Some(Op::Post),
        "invoice_interest_correction_calc" => Some(Op::Correction),
        "invoice_post_interest_correction" => Some(Op::PostCorrection),
        _ => None,
    }
}
pub fn accepts(name: &str) -> bool {
    operation(name).is_some()
}
pub fn tools() -> Vec<Value> {
    [("invoice_interest_calc",Op::Calculate),("invoice_claim_interest",Op::Register),("invoice_post_interest",Op::Post),("invoice_interest_correction_calc",Op::Correction),("invoice_post_interest_correction",Op::PostCorrection)].into_iter().map(|(name,op)|{
        let read=op.is_read_only();let mut body=native_body(invoice_interest_body_schema(op));body["maxLength"]=json!(MAX_INVOICE_INTEREST_BODY);
        let mut schema=json!({"type":"object","required":["company","body"],"properties":{"company":company_schema(),"body":body},"additionalProperties":false});
        if !read {schema["required"]=json!(["company","body","confirm"]);schema["properties"]["confirm"]=json!({"const":true});}
        let mut descriptor=tool(name,"Current human MFA cookie only; exact native16KiB JSON once. Calculate and correction are readonly CompanyRead. Optional referenceRatePercent omission preserves the statutory-table default; never infer a rate. Register: DraftWrite, separate no-journal claim. Post/PostCorrection: LedgerPost; both inner/outer confirm:true. Post uses explicit active compatible receivable/income accounts; claimId optional selects server oldest unposted. Correction hasProposal alone is not permission: inspect canPost/requiresRefund/proposedLines; correction post accepts reason/date only and server derives amounts/original accounts/causal allocation. Currently native interest is DKK; foreign interest may refuse explicitly. No email, keys, accounts/math defaults or automatic retry; unknown write requires canonical calculate/correction/claims and manual reconciliation",schema,read);
        descriptor["annotations"]["openWorldHint"]=json!(false);descriptor
    }).collect()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
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
    let op = operation(name).ok_or("unknown interest operation")?;
    let (company, body, confirm) = if op.is_read_only() {
        let i: Read =
            serde_json::from_value(args).map_err(|_| "calculate requires only company/body")?;
        (i.company, i.body, false)
    } else {
        let i: Write =
            serde_json::from_value(args).map_err(|_| "write requires company/body/confirm")?;
        (i.company, i.body, i.confirm)
    };
    if body.len() > MAX_INVOICE_INTEREST_BODY {
        return Err("interest body exceeds native16KiB");
    }
    Ok(api
        .invoice_interest(&company, op, body.into_bytes(), confirm)
        .await)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_calculation_is_read_only_and_writes_have_explicit_confirmation() {
        let tools = tools();
        assert_eq!(tools.len(), 5);
        for (i, t) in tools.iter().enumerate() {
            assert_eq!(t["annotations"]["readOnlyHint"], matches!(i, 0 | 3));
            assert_eq!(t["annotations"]["idempotentHint"], matches!(i, 0 | 3));
            assert_eq!(t["annotations"]["openWorldHint"], false);
            assert_eq!(t["inputSchema"]["additionalProperties"], false);
            assert_eq!(
                t["inputSchema"]["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("confirm")),
                !matches!(i, 0 | 3)
            );
        }
    }
}
