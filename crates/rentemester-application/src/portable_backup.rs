//! Current human workspace + every company admin authorization, on one held control writer.
use crate::access::{CompanyRole, Permission, WorkspaceRole};
use rentemester_engine::Error;
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::workspace_context::Catalog;
use rusqlite::Transaction;
pub fn authorize(
    tx: &Transaction<'_>,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
) -> Result<(), Error> {
    let denied = || {
        Error::new(
            "access_denied",
            "current workspace management and company administration required for every registered company",
        )
    };
    if !cookies.accepts_session(proof) || cookies.workspace() != catalog.workspace {
        return Err(denied());
    }
    rentemester_storage::workspace_context::validate_catalog(catalog)?;
    let current = rentemester_storage::cookie_session::resolve(
        tx,
        proof.token(),
        clock.now_unix_ms().map_err(|_| denied())?,
        rentemester_storage::cookie_session::SessionRequirement::Current,
    )?;
    rentemester_storage::session_control::authorize(tx, &current)?;
    let role =
        rentemester_storage::workspace_members::access(tx, &current.subject)?.ok_or_else(denied)?;
    let role: WorkspaceRole =
        serde_json::from_value(serde_json::json!(role)).map_err(|_| denied())?;
    if !crate::access::workspace_role_allows(role, Permission::WorkspaceManage) {
        return Err(denied());
    }
    for company in &catalog.companies {
        let role = rentemester_storage::workspace_members::membership(
            tx,
            &current.subject,
            &company.slug,
        )?
        .ok_or_else(denied)?;
        let role: CompanyRole =
            serde_json::from_value(serde_json::json!(role)).map_err(|_| denied())?;
        if !crate::access::company_role_allows(role, Permission::CompanyAdmin) {
            return Err(denied());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use rentemester_runtime::auth::{AuthError, CookieProfile};
    use rusqlite::{Connection, TransactionBehavior};
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1_767_226_200_000)
        }
    }
    #[test]
    fn every_company_including_archived_requires_current_owner_and_workspace_management() {
        let mut c = Connection::open_in_memory().unwrap();
        let mut l = Connection::open_in_memory().unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic portable")
            .unwrap();
        c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified) VALUES('f','synthetic','synthetic','u',1);INSERT INTO rm_workspace_mfa_events(user_id,event_type,actor,created_at)VALUES('u','mfa_enabled','system:better-auth','2026-01-01T00:00:00.000Z');INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:01:00.000Z','2026-01-01T00:01:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','system:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','system:synthetic');").unwrap();
        let vectors: serde_json::Value = serde_json::from_str(include_str!(
            "../../rentemester-runtime/fixtures/session-cookie.json"
        ))
        .unwrap();
        let cookies =
            CookieVerifier::new("w", CookieProfile::Secure, vectors["key"].as_str().unwrap())
                .unwrap();
        let proof = cookies
            .verify(vectors["vectors"][0]["header"].as_str(), None)
            .unwrap();
        let catalog = Catalog {
            workspace: "w".into(),
            companies: ["a", "b"]
                .into_iter()
                .map(
                    |s| rentemester_storage::workspace_context::RegisteredCompany {
                        slug: s.into(),
                        name: format!("Synthetic {s}"),
                        purpose: Default::default(), archived: s == "b",
                    },
                )
                .collect(),
        };
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(
            authorize(&tx, &cookies, &Fixed, &proof, &catalog)
                .unwrap_err()
                .code,
            "access_denied"
        );
        tx.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','grant','reader','system:synthetic')",[]).unwrap();
        assert_eq!(
            authorize(&tx, &cookies, &Fixed, &proof, &catalog)
                .unwrap_err()
                .code,
            "access_denied"
        );
        tx.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','grant','owner','system:synthetic')",[]).unwrap();
        authorize(&tx, &cookies, &Fixed, &proof, &catalog).unwrap();
        tx.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','system:synthetic')",[]).unwrap();
        assert_eq!(
            authorize(&tx, &cookies, &Fixed, &proof, &catalog)
                .unwrap_err()
                .code,
            "access_denied"
        );
        tx.rollback().unwrap();
    }
}
