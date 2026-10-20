//! Current company visibility and report reads; no read-time discovery or writes.
use crate::cfo_analytics::{self, Authorized};
use rentemester_engine::{Error, cfo_analytics::Query, fiscal_year::Date};
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::workspace_context::Catalog;
use rusqlite::Transaction;

/// Reuse the admitted workspace-read and current company membership policy.
/// The internal selection's hidden-company count is never serialized.
pub fn authorize_portfolio(
    tx: &Transaction<'_>,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    as_of: &str,
) -> Result<Authorized, Error> {
    Date::parse(as_of)?;
    let query = Query {
        scope: "portfolio".into(),
        from: as_of.into(),
        to: as_of.into(),
        company_slugs: vec![],
        group_profile_id: None,
        account: None,
        party: None,
        dimension: None,
        currency: None,
        cursor: None,
        aggregate: "none".into(),
        limit: 100,
    };
    let authorized = cfo_analytics::authorize(tx, cookies, clock, proof, catalog, &query)?;
    if authorized.companies.is_empty() && !catalog.companies.is_empty() {
        return Err(Error::new(
            "access_denied",
            "No accessible portfolio companies",
        ));
    }
    Ok(authorized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rentemester_runtime::auth::{AuthError, CookieProfile};
    use rentemester_storage::workspace_context::RegisteredCompany;
    use rusqlite::Connection;
    use serde_json::Value;

    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1_767_225_600_000)
        }
    }

    #[test]
    fn portfolio_rechecks_current_visibility_without_control_effects() {
        let dir = tempfile::tempdir().unwrap();
        let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
        let mut ledger = Connection::open(dir.path().join("ledger.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut ledger,
            "Synthetic Portfolio",
        )
        .unwrap();
        control.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);
            INSERT INTO twoFactor(id,secret,backupCodes,userId,verified) VALUES('f','synthetic','synthetic','u',1);
            INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');
            INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');
            INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','visible','grant','reader','agent:synthetic');").unwrap();
        let fixture: Value = serde_json::from_str(include_str!(
            "../../rentemester-runtime/fixtures/session-cookie.json"
        ))
        .unwrap();
        let verifier =
            CookieVerifier::new("w", CookieProfile::Secure, fixture["key"].as_str().unwrap())
                .unwrap();
        let proof = verifier
            .verify(fixture["vectors"][0]["header"].as_str(), None)
            .unwrap();
        let catalog = Catalog {
            workspace: "w".into(),
            companies: vec![
                RegisteredCompany {
                    slug: "visible".into(),
                    name: "Synthetic visible".into(),
                    archived: false,
                },
                RegisteredCompany {
                    slug: "hidden".into(),
                    name: "Synthetic hidden".into(),
                    archived: false,
                },
            ],
        };
        let initial = control.total_changes();
        let tx = control
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let allowed =
            authorize_portfolio(&tx, &verifier, &Fixed, &proof, &catalog, "2026-01-01").unwrap();
        assert_eq!(allowed.companies, ["visible"]);
        assert!(
            authorize_portfolio(&tx, &verifier, &Fixed, &proof, &catalog, "2026-02-30").is_err()
        );
        tx.rollback().unwrap();
        assert_eq!(control.total_changes(), initial);
        control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','visible','revoke',NULL,'agent:synthetic');").unwrap();
        let tx = control
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(
            authorize_portfolio(&tx, &verifier, &Fixed, &proof, &catalog, "2026-01-01")
                .err()
                .unwrap()
                .code,
            "access_denied"
        );
        tx.rollback().unwrap();
    }
}
