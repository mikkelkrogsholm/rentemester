//! URL scope is admitted using fresh identity and knowledge permission before any page content.
use super::*;
use rentemester_engine::knowledge_pages::Scope as KnowledgeScope;
pub(super) fn read(
    tx: &Transaction<'_>,
    scope: &KnowledgeScope,
    reference: &str,
    access: &Access,
    identity: &Identity,
    stamp: &str,
) -> Result<Value, Error> {
    scope.validate()?;
    match scope {
        KnowledgeScope::Workspace => {
            if !access.owner {
                return Err(denied());
            }
        }
        KnowledgeScope::Company { company_slug } => {
            if !access.read.contains(company_slug) {
                return Err(denied());
            }
            let role = rentemester_storage::control_access::observe_company_role(
                tx,
                &identity.subject,
                company_slug,
            )?
            .ok_or_else(denied)?;
            let role: CompanyRole =
                serde_json::from_value(json!(role)).map_err(|_| error("invalid_control"))?;
            if !access::company_role_allows(role, Permission::CompanyKnowledgeRead) {
                return Err(denied());
            }
        }
    }
    let page = rentemester_storage::knowledge_pages::read_page(tx, scope, reference, stamp)?
        .ok_or_else(|| error("not_found"))?;
    Ok(json!({"page":page}))
}
