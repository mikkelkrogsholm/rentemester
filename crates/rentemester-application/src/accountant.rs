//! Verified original bytes and accountant-export audit under current company export authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rentemester_runtime::namespace::NamespaceGuard;
use rusqlite::Connection;
pub fn export(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    input: &rentemester_storage::accountant::Input,
) -> Result<Committed<rentemester_storage::accountant::Download>, Failure> {
    execute(control, company, context, input, |original| {
        guard.read_document(&original.path,&original.hash).map_err(|_|Error::new("document_evidence_unavailable","accountant export requires every registered original; missing or altered document evidence"))
    })
}
fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    input: &rentemester_storage::accountant::Input,
    mut read: impl FnMut(&rentemester_storage::accountant::Original) -> Result<Vec<u8>, Error>,
) -> Result<Committed<rentemester_storage::accountant::Download>, Failure> {
    let slug = context.selection.slug;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyExport,
        |tx, actor| {
            let prepared = rentemester_storage::accountant::prepare(tx, slug, input)?;
            let mut originals = vec![];
            for original in &prepared.originals {
                originals.push(read(original)?);
            }
            rentemester_storage::accountant::finish(tx, prepared, originals, actor)
        },
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_fence::CompanySelection;
    use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1_767_225_600_000)
        }
    }
    #[test]
    fn export_permission_confirmation_revocation_and_evidence_failures_are_fenced() {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Connection::open(dir.path().join("c.sqlite")).unwrap();
        let mut l = Connection::open(dir.path().join("l.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Accountant")
            .unwrap();
        c.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);
            INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');
            INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');
            INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','reader','agent:synthetic');").unwrap();
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../rentemester-runtime/fixtures/session-cookie.json"
        ))
        .unwrap();
        let cookie =
            CookieVerifier::new("w", CookieProfile::Secure, fixture["key"].as_str().unwrap())
                .unwrap()
                .verify(fixture["vectors"][0]["header"].as_str(), None)
                .unwrap();
        let context = |slug| Context {
            cookie: &cookie,
            clock: &Fixed,
            selection: CompanySelection {
                workspace: "w",
                slug,
                registered: true,
                archived: false,
            },
        };
        let mut input = rentemester_storage::accountant::Input {
            period_start: "2026-01-01".into(),
            period_end: "2026-12-31".into(),
            confirm: false,
            ..Default::default()
        };
        assert!(
            execute(&mut c, &mut l, context("a"), &input, |_| panic!(
                "must not read evidence"
            ))
            .is_err()
        );
        input.confirm = true;
        // Reader has company.export; it does not require ledger-post permission.
        execute(&mut c, &mut l, context("a"), &input, |_| {
            panic!("empty archive has no originals")
        })
        .unwrap();
        assert_eq!(
            l.query_row(
                "SELECT actor FROM audit_log WHERE event_type='accountant_handoff_export'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "user:u"
        );
        l.execute_batch("INSERT INTO documents(source,document_type,sha256_hash,stored_path,invoice_date) VALUES('synthetic','cash_register_receipt','synthetic-hash','documents/originals/synthetic.txt','2026-01-01')").unwrap();
        assert!(
            execute(&mut c, &mut l, context("a"), &input, |_| Err(Error::new(
                "document_evidence_unavailable",
                "synthetic missing original"
            )))
            .is_err()
        );
        assert!(
            execute(&mut c, &mut l, context("other"), &input, |_| panic!(
                "wrong company must not read originals"
            ))
            .is_err()
        );
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','revoke',NULL,'agent:synthetic');").unwrap();
        assert!(
            execute(&mut c, &mut l, context("a"), &input, |_| panic!(
                "revoked membership must not read originals"
            ))
            .is_err()
        );
        assert_eq!(
            l.query_row(
                "SELECT count(*) FROM audit_log WHERE event_type='accountant_handoff_export'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            l.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
