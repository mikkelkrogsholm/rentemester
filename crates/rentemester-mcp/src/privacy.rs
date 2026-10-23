use crate::tools::{company_schema, native_body, tool};
use rentemester_client::{ApiClient, Error, MAX_PRIVACY_BODY, privacy_body_schema, PrivacyOperation};
use serde::Deserialize;
use serde_json::{Value, json};
pub fn accepts(s: &str) -> bool {
    matches!(s, "retention" | "gdpr_export" | "gdpr_erase" | "gdpr_discover" | "gdpr_forget" | "gdpr_legal_hold" | "gdpr_audit_log")
}
pub fn tools() -> Vec<Value> {
    let mut out = vec![tool(
        "retention",
        "CompanyRead: full current server-date retention report, unknown deadlines and legal source unchanged. No query/asOf/confirmation or file/ledger effects. No client retention-law calculation.",
        json!({"type":"object","required":["company"],"properties":{"company":company_schema()},"additionalProperties":false}),
        true,
    )];
    for (name, erase) in [("gdpr_export", false), ("gdpr_erase", true)] {
        let mut body = native_body(privacy_body_schema(erase));
        body["maxLength"] = json!(MAX_PRIVACY_BODY);
        out.push(tool(name,if erase{"CompanyAdmin: explicit reviewed scoped retention-bounded overlay. Exact native16KiB subject cvr?/name?/confirm:true, outer confirm:true; asOf forbidden even null, trusted server current date only. Review full prior export erasable/retained/unknown evidence for same subject. Preserve all refused rows, coverage.complete:false and unknown deadlines. No universal physical deletion, protected original/journal rewrite or backup erasure guarantee. No caller keys/automatic erase/retries; uncertain outcomes require canonical reads before any further action."}else{"CompanyExport: audited subject access export, may strengthen subject aliases. NOT read-only: exact native16KiB cvr?/name?/asOf?/confirm:true and outer confirm:true. At least one nonblank identity; both must identify same subject. Preserve all records including audit rows, coverage.complete:false, retained/erasable/unknown deadlines, company/rules. Optional asOf is export review only and cannot authorize erasure. No caller keys/retries; uncertainty requires reconciliation, each explicit export creates audit evidence."},json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    for (name, operation) in [("gdpr_discover",PrivacyOperation::Discover),("gdpr_forget",PrivacyOperation::Forget),("gdpr_legal_hold",PrivacyOperation::LegalHold),("gdpr_audit_log",PrivacyOperation::AuditLog)] {
        let mut body=native_body(operation.schema()); body["maxLength"]=json!(MAX_PRIVACY_BODY);
        out.push(tool(name,"Current human cookie only. CompanyExport for audited discovery/audit; CompanyAdmin for forget/hold. Exact confirmed native bytes once; no retries or actor identity overrides. Server current permission/date and retention/hold gates apply. Audit embedded key is not independently trusted verification. Preserve complete refusal/unknown evidence.",json!({"type":"object","required":["company","body","confirm"],"properties":{"company":company_schema(),"body":body,"confirm":{"const":true}},"additionalProperties":false}),false));
    }
    out
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    company: String,
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
    if name == "retention" {
        let r: Read =
            serde_json::from_value(args).map_err(|_| "retention requires only company")?;
        return Ok(api.retention(&r.company).await);
    }
    let r: Write =
        serde_json::from_value(args).map_err(|_| "privacy requires only company/body/confirm")?;
    if r.body.len() > MAX_PRIVACY_BODY {
        return Err("privacy body exceeds native16KiB");
    }
    Ok(if name == "gdpr_export" {
        api.gdpr_export(&r.company, r.body.into_bytes(), r.confirm)
            .await
    } else if name == "gdpr_erase" {
        api.gdpr_erase(&r.company, r.body.into_bytes(), r.confirm)
            .await
    } else {
        api.privacy_operation(&r.company,match name {"gdpr_discover"=>PrivacyOperation::Discover,"gdpr_forget"=>PrivacyOperation::Forget,"gdpr_legal_hold"=>PrivacyOperation::LegalHold,"gdpr_audit_log"=>PrivacyOperation::AuditLog,_=>return Err("unknown privacy tool")},r.body.into_bytes(),r.confirm).await
    })
}
