//! CompanyRead, current human and read-only company/control fence.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rusqlite::Connection;
use serde_json::{Value, json};
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    as_of: &str,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    let workspace = context.selection.workspace;
    let clock = context.clock;
    control_fence::execute_cookie_both(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyRead,
        true,
        false,
        |control, tx, subject| {
            let mut data = rentemester_storage::compliance::read(tx, slug, as_of)?;
            let role =
                rentemester_storage::control_access::observe_company_role(control, subject, slug)?;
            let detail_allowed = role.as_deref() == Some("owner");
            data["backup"]["detailPermissionGranted"] = json!(detail_allowed);
            if detail_allowed {
                data["backup"] = match crate::backup_status::data(
                    control, tx, workspace, slug, clock, true,
                ) {
                    Ok(value) => {
                        json!({"available":true,"detailPermissionGranted":true,"localRetainedEvidence":value,"destinationGovernanceAvailable":false,"reason":"Destination declarations and remote governance are not collected by this report; no provider requests or invented remote evidence."})
                    }
                    Err(error) => {
                        json!({"available":false,"detailPermissionGranted":true,"reason":error.message,"code":error.code})
                    }
                };
            }
            let html = rentemester_engine::compliance_report::render(&data)?;
            let hash = rentemester_engine::compliance_report::fingerprint(&html);
            Ok(
                json!({"contentType":"text/html; charset=utf-8","html":html,"sha256":hash,"data":data}),
            )
        },
    )
}
#[cfg(test)]
mod tests;
