//! Cookie-only mileage posting, admitted current authority and physical originals.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::namespace::NamespaceGuard;
use rentemester_storage::{idempotency::PrincipalKind, mileage_posting as store};
use rusqlite::{Connection, Transaction};
use serde_json::Value;
pub use store::{Configure, Post, Preview};
pub enum Action<'a> {
    RateVersions,
    Configure(&'a Configure),
    Preview(&'a Preview),
    Post(&'a Post),
    Postings,
}
fn original(tx: &Transaction<'_>, guard: &mut NamespaceGuard, id: i64) -> Result<String, Error> {
    if !(1..=9_007_199_254_740_991).contains(&id) {
        return Err(store::refused("Positive safe document ID required"));
    }
    rentemester_storage::invoice_extraction::validate_history(tx)?;
    let evidence = rentemester_storage::documents::evidence(tx, id)
        .map_err(|_| store::refused("Mileage original missing"))?;
    let bytes = guard
        .read_document(&evidence.path, &evidence.hash)
        .map_err(|_| store::refused("Mileage physical original unavailable or changed"))?;
    if bytes.is_empty() {
        return Err(store::refused("Empty mileage physical original"));
    }
    for source in rentemester_storage::invoice_extraction::sources(tx)? {
        if source.document_id == id && (source.sha256 != evidence.hash || source.pdf_bytes != bytes)
        {
            return Err(store::refused("Retained mileage source changed"));
        }
    }
    Ok(evidence.hash)
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    action: Action<'_>,
    today: &str,
) -> Result<Committed<Value>, Failure> {
    let permission = match action {
        Action::Configure(_) => Permission::CompanyAdmin,
        Action::Post(_) => Permission::CompanyLedgerPost,
        _ => Permission::CompanyRead,
    };
    let readonly = !matches!(action, Action::Configure(_) | Action::Post(_));
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        readonly,
        |tx, subject| {
            // Date is the owner's current posting horizon, never a caller posting date.
            let latest = Date::parse(today)?;
            match action {
                Action::RateVersions => store::rate_versions(tx),
                Action::Configure(input) => {
                    store::configure(tx, input, subject, PrincipalKind::User)
                }
                Action::Preview(input) => {
                    let hash = original(tx, guard, input.document_id)?;
                    store::preview(tx, input, &hash, latest)
                }
                Action::Post(input) => {
                    let hash = original(tx, guard, input.document_id)?;
                    store::post(tx, input, subject, PrincipalKind::User, &hash, latest)
                }
                Action::Postings => {
                    for id in store::posting_documents(tx)? {
                        original(tx, guard, id)?;
                    }
                    store::postings(tx, latest)
                }
            }
        },
    )
}

#[cfg(all(test, target_os = "macos"))]
mod mileage_posting_tests {

    use super::*;
    use rentemester_runtime::{
        auth::{AuthError, Clock, CookieProfile, CookieVerifier, VerifiedCookie},
        namespace::{NamespaceGuard, NamespaceProfile},
    };
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1767225600000)
        }
    }
    struct Fixture {
        _dir: tempfile::TempDir,
        control: Connection,
        company: Connection,
        guard: NamespaceGuard,
        cookie: VerifiedCookie,
    }
    fn fixture(role: &str) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap().join("workspace");
        let uid = String::from_utf8(
            std::process::Command::new("id")
                .arg("-u")
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .parse()
        .unwrap();
        let guard = NamespaceGuard::create_fresh_with_profile(
            &root,
            uid,
            "w",
            "a",
            "Synthetic CVR",
            "2026-01-01T00:00:00.000Z",
            NamespaceProfile::LocalUserMacos,
        )
        .unwrap();
        let control_path = root.join(".rentemester/workspace-control.sqlite");
        let company_path = root.join("a/data/ledger.sqlite");
        let mut control = Connection::open(&control_path).unwrap();
        let mut company = Connection::open(&company_path).unwrap();
        for db in [&control, &company] {
            db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
                .unwrap();
        }
        rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut company,
            "Synthetic Local",
        )
        .unwrap();
        control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');").unwrap();
        control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant',?1,'user:u')",[role]).unwrap();
        company
            .execute("UPDATE companies SET cvr='87654321' WHERE id=1", [])
            .unwrap();
        let verifier = CookieVerifier::new(
            "w",
            CookieProfile::Development,
            "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
        )
        .unwrap();
        let issued = verifier.issue_session("synthetic-token").unwrap();
        let cookie = verifier
            .verify(Some(issued.set_cookie().split(';').next().unwrap()), None)
            .unwrap();
        Fixture {
            _dir: dir,
            control,
            company,
            guard,
            cookie,
        }
    }
    fn context(cookie: &VerifiedCookie) -> Context<'_> {
        Context {
            cookie,
            clock: &Fixed,
            selection: control_fence::CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
            },
        }
    }

    #[test]
    fn mileage_posting_current_reader_revocation_and_missing_original() {
        let mut f = fixture("reader");
        let versions = execute(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            context(&f.cookie),
            Action::RateVersions,
            "2026-01-01",
        )
        .unwrap();
        assert_eq!(versions.value, serde_json::json!({"rateVersions":[]}));
        let input:Configure=serde_json::from_value(serde_json::json!({"version":"synthetic","validFrom":"2026-01-01","validTo":"2026-12-31","vehicle":"Car","ratePerKm":"2.00","basis":"Synthetic","source":"Synthetic","expenseAccountNo":"E","liabilityAccountNo":"L","confirm":true})).unwrap();
        assert_eq!(
            execute(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                context(&f.cookie),
                Action::Configure(&input),
                "2026-01-01"
            )
            .err()
            .unwrap()
            .error
            .code,
            "access_denied"
        );
        let post = Post {
            mileage_entry_id: 1,
            rate_version: "synthetic".into(),
            document_id: 1,
            expected_preview_hash: "a".repeat(64),
            operation_key: "key".into(),
            confirm: true,
        };
        assert_eq!(
            execute(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                context(&f.cookie),
                Action::Post(&post),
                "2026-01-01"
            )
            .err()
            .unwrap()
            .error
            .code,
            "access_denied"
        );
        let preview = Preview {
            mileage_entry_id: 1,
            rate_version: "synthetic".into(),
            document_id: 1,
        };
        assert_eq!(
            execute(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                context(&f.cookie),
                Action::Preview(&preview),
                "2026-01-01"
            )
            .err()
            .unwrap()
            .error
            .code,
            "mileage_posting_refused"
        );
        f.control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')").unwrap();
        assert_eq!(
            execute(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                context(&f.cookie),
                Action::RateVersions,
                "2026-01-01"
            )
            .err()
            .unwrap()
            .error
            .code,
            "access_denied"
        );
        assert_eq!(
            f.company
                .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
