use super::*;
use rentemester_runtime::{
    auth::{AuthError, CookieProfile},
    email_token::EmailTokenSigner,
    recovery::{DeliveryUnconfirmed, RecoveryLinks, VerificationMessage},
};
use rusqlite::params;
const KEY: &str = "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE";
struct Fixed(i64);
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
#[derive(Default)]
struct Mail(Vec<String>, bool);
impl VerificationSender for Mail {
    fn enabled(&self) -> bool {
        true
    }
    fn send(&mut self, m: &VerificationMessage) -> Result<(), DeliveryUnconfirmed> {
        self.0.push(m.token().into());
        if self.1 {
            Err(DeliveryUnconfirmed)
        } else {
            Ok(())
        }
    }
}
fn fixture() -> (tempfile::TempDir, Connection, Catalog) {
    let root = tempfile::tempdir().unwrap();
    let mut db = Connection::open(root.path().join("control")).unwrap();
    let mut company = Connection::open(root.path().join("company")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(&mut db, &mut company, "Synthetic").unwrap();
    let date = "2026-01-01T00:00:00.000Z";
    db.execute("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES('owner','Synthetic Owner','owner@example.invalid',1,1,?1,?1)",[date]).unwrap();
    db.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('owner','activate','workspace_owner','user:owner')",[]).unwrap();
    db.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('owner','a','grant','owner','user:owner')",[]).unwrap();
    (
        root,
        db,
        Catalog {
            workspace: "w".into(),
            companies: vec![rentemester_storage::workspace_context::RegisteredCompany {
                slug: "a".into(),
                name: "Synthetic".into(),
                archived: false,
            }],
        },
    )
}
fn issued(db: &mut Connection, catalog: &Catalog, key: &InvitationKey, now: i64) -> String {
    let token = key.generate().unwrap().1;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    invites::issue(
        &tx,
        catalog,
        &Issue {
            email: "invited@example.invalid".into(),
            workspace_role: "member".into(),
            company_slug: "a".into(),
            company_role: "reviewer".into(),
        },
        invites::Issuance {
            id: "11111111-1111-4111-a111-111111111111",
            token_hash: &key.digest("token", token.as_str()).unwrap(),
            email_hash: &key.digest("email", "invited@example.invalid").unwrap(),
            now,
        },
        "user:owner",
    )
    .unwrap();
    invites::status(
        &tx,
        "11111111-1111-4111-a111-111111111111",
        "delivery_confirmed",
        "user:owner",
    )
    .unwrap();
    tx.commit().unwrap();
    token.as_str().into()
}
#[test]
fn claimed_identity_needs_real_verification_and_mfa_has_no_session_and_single_use() {
    let (_root, mut db, catalog) = fixture();
    let clock = Fixed(1767225600000);
    let key = InvitationKey::new("w", KEY).unwrap();
    let cookies = CookieVerifier::new("w", CookieProfile::Development, KEY).unwrap();
    let token = issued(&mut db, &catalog, &key, clock.0);
    let r = Runtime {
        cookies: &cookies,
        clock: &clock,
        key: &key,
        catalog: &catalog,
    };
    let signer = EmailTokenSigner::new("w", KEY).unwrap();
    let links = RecoveryLinks::new(
        "http://127.0.0.1:4319",
        &["http://127.0.0.1:4319"],
        CookieProfile::Development,
    )
    .unwrap();
    let er = crate::email_verification::Runtime {
        signer: &signer,
        clock: &clock,
        audit_key_version: 1,
    };
    let vr = verification_request::Runtime {
        verification: &er,
        cookies: &cookies,
        links: &links,
    };
    let mut mail = Mail(Vec::new(), true);
    assert!(
        claim(
            &mut db,
            &r,
            &vr,
            &mut mail,
            Claim {
                token: &token,
                name: "Synthetic Invited",
                password: "synthetic-invited-password"
            }
        )
        .is_err()
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM rm_company_membership_events WHERE user_id<>'owner'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    mail.1 = false;
    let result = claim(
        &mut db,
        &r,
        &vr,
        &mut mail,
        Claim {
            token: &token,
            name: "Synthetic Invited",
            password: "synthetic-invited-password",
        },
    )
    .unwrap();
    assert_eq!(result["accessReady"], false);
    assert_eq!(mail.0.len(), 2);
    assert_eq!(
        db.query_row("SELECT count(*) FROM session", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM twoFactor", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(
        crate::password_login::execute(
            &mut db,
            "invited@example.invalid",
            "synthetic-invited-password",
            |_, f| Ok(f)
        )
        .is_err()
    );
    verification_request::complete(&mut db, &vr, &mail.0[1], None).unwrap();
    let first = crate::password_login::execute(
        &mut db,
        "invited@example.invalid",
        "synthetic-invited-password",
        |_, f| Ok(f),
    )
    .unwrap();
    assert_eq!(
        first.next,
        rentemester_storage::password_credential::NextFactor::Enrollment
    );
    assert!(
        claim(
            &mut db,
            &r,
            &vr,
            &mut mail,
            Claim {
                token: &token,
                name: "Changed",
                password: "different-synthetic-password"
            }
        )
        .is_err()
    );
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let list = invites::list(&tx, &catalog, "owner").unwrap();
    let json = serde_json::to_string(&list).unwrap();
    assert!(!json.contains(&token));
    assert!(!json.contains("tokenHash"));
    assert_eq!(list[0].status, "accepted");
}
#[test]
fn expiry_cancel_last_owner_and_existing_credential_are_preserved() {
    let (_root, mut db, catalog) = fixture();
    let clock = 1767225600000;
    let key = InvitationKey::new("w", KEY).unwrap();
    let token = issued(&mut db, &catalog, &key, clock);
    let digest = key.digest("token", &token).unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(invites::inspect(&tx, &digest, clock + 7 * 86400000, &catalog).is_err());
    assert!(members::change(&tx, &catalog, "owner", None, Some("member"), "user:owner").is_err());
    assert!(members::change(&tx, &catalog, "owner", Some("a"), None, "user:owner").is_err());
    let i = invites::inspect(&tx, &digest, clock, &catalog).unwrap();
    assert!(invites::accept(&tx, &i, "owner").is_err());
    let hash = rentemester_runtime::password::hash_password("synthetic-original-password").unwrap();
    let id = invites::identity(
        &tx,
        &i,
        invites::Identity {
            user: "invited",
            account: "credential",
            name: "Synthetic",
            hash: hash.as_str(),
        },
        clock,
    )
    .unwrap();
    let replacement =
        rentemester_runtime::password::hash_password("synthetic-replacement-password").unwrap();
    assert_eq!(
        invites::identity(
            &tx,
            &i,
            invites::Identity {
                user: "new",
                account: "new",
                name: "Other",
                hash: replacement.as_str()
            },
            clock
        )
        .unwrap(),
        id
    );
    let actual: String = tx
        .query_row("SELECT password FROM account WHERE userId=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(actual, hash.as_str());
    invites::status(&tx, &i.invitation_id, "cancelled", "user:owner").unwrap();
    assert!(invites::inspect(&tx, &digest, clock, &catalog).is_err());
    tx.commit().unwrap();
}
#[test]
fn unauthenticated_or_stale_owner_cannot_manage_or_disclose() {
    let (_root, mut db, catalog) = fixture();
    let clock = Fixed(1767225600000);
    let key = InvitationKey::new("w", KEY).unwrap();
    let cookies = CookieVerifier::new("w", CookieProfile::Development, KEY).unwrap();
    let session = cookies.issue_session("synthetic-no-session").unwrap();
    let header = session.set_cookie().split(';').next().unwrap().to_owned();
    let proof = cookies.verify(Some(&header), None).unwrap();
    let r = Runtime {
        cookies: &cookies,
        clock: &clock,
        key: &key,
        catalog: &catalog,
    };
    assert!(
        execute(&mut db, &r, &proof, Action::Members, &mut |_, _| panic!(
            "must not send"
        ))
        .is_err()
    );
    assert!(
        execute(
            &mut db,
            &r,
            &proof,
            Action::Invitations,
            &mut |_, _| panic!("must not send")
        )
        .is_err()
    );
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(
        members::change(
            &tx,
            &catalog,
            "owner",
            Some("outside"),
            Some("reader"),
            "user:owner"
        )
        .is_err()
    );
    tx.execute("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES('pending','Pending','pending@example.invalid',0,0,?1,?1)",params!["2026-01-01T00:00:00.000Z"]).unwrap();
    members::change(
        &tx,
        &catalog,
        "pending",
        None,
        Some("workspace_owner"),
        "user:owner",
    )
    .unwrap();
    members::change(
        &tx,
        &catalog,
        "pending",
        Some("a"),
        Some("owner"),
        "user:owner",
    )
    .unwrap();
    assert!(members::change(&tx, &catalog, "owner", None, None, "user:owner").is_err());
}

#[test]
fn concurrent_last_effective_owner_removals_serialize() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    // Only this test installs this handler. It proves real SQLite contention,
    // rather than assuming that two spawned threads happened to overlap.
    static CONTENDED: AtomicBool = AtomicBool::new(false);
    for company in [false, true] {
        for demote in [false, true] {
            for first in ["owner", "other"] {
                let (root, mut db, catalog) = fixture();
                let date = "2026-01-01T00:00:00.000Z";
                for (user, ready) in [("other", 1), ("pending", 0)] {
                    db.execute("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES(?1,?1,?2,?3,?3,?4,?4)",params![user,format!("{user}@example.invalid"),ready,date]).unwrap();
                    db.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES(?1,'activate','workspace_owner','user:owner')",[user]).unwrap();
                    db.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,'a','grant','owner','user:owner')",[user]).unwrap();
                }
                let count = |db: &Connection, table: &str| -> i64 {
                    db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                        .unwrap()
                };
                let tables = [
                    "rm_workspace_user_access_events",
                    "rm_company_membership_events",
                    "rm_workspace_security_events",
                    "workspace_audit",
                ];
                let before = tables.map(|t| count(&db, t));
                let company_bytes = || {
                    ["company", "company-wal"].map(|name| {
                        let path = root.path().join(name);
                        path.exists().then(|| std::fs::read(path).unwrap())
                    })
                };
                let company_before = company_bytes();
                let second = if first == "owner" { "other" } else { "owner" };
                let slug = company.then_some("a");
                let role = demote.then_some(if company { "reader" } else { "member" });
                let mut other = Connection::open(root.path().join("control")).unwrap();
                other.pragma_update(None, "foreign_keys", true).unwrap();
                other.pragma_update(None, "synchronous", "FULL").unwrap();
                other
                    .busy_handler(Some(|attempt| {
                        CONTENDED.store(true, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(1));
                        attempt < 5000
                    }))
                    .unwrap();
                CONTENDED.store(false, Ordering::SeqCst);
                let tx = db
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .unwrap();
                std::thread::scope(|scope| {
                    let pending = scope.spawn(|| {
                        transact(&mut other, |t| {
                            members::change(
                                t,
                                &catalog,
                                second,
                                slug,
                                role,
                                &format!("user:{second}"),
                            )
                        })
                    });
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while !CONTENDED.load(Ordering::SeqCst) && Instant::now() < deadline {
                        std::thread::yield_now();
                    }
                    assert!(
                        CONTENDED.load(Ordering::SeqCst),
                        "second connection must contend on the held writer"
                    );
                    members::change(&tx, &catalog, first, slug, role, &format!("user:{first}"))
                        .unwrap();
                    tx.commit().unwrap();
                    let failure = pending
                        .join()
                        .unwrap()
                        .err()
                        .expect("final effective owner must remain");
                    assert_eq!(failure.error.code, "last_owner");
                    assert!(failure.connection_reusable);
                });
                drop(other);
                drop(db);
                let mut reopened = Connection::open(root.path().join("control")).unwrap();
                let after = tables.map(|t| count(&reopened, t));
                assert_eq!(
                    after,
                    [
                        before[0] + i64::from(!company),
                        before[1] + i64::from(company),
                        before[2] + i64::from(!company && !demote),
                        before[3] + 1
                    ]
                );
                let tx = reopened
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .unwrap();
                let effective = |user| {
                    let ready: bool = tx
                        .query_row(
                            "SELECT emailVerified=1 AND twoFactorEnabled=1 FROM user WHERE id=?1",
                            [user],
                            |r| r.get(0),
                        )
                        .unwrap();
                    ready
                        && members::access(&tx, user).unwrap().as_deref() == Some("workspace_owner")
                        && members::membership(&tx, user, "a").unwrap().as_deref() == Some("owner")
                };
                assert!(!effective(first));
                assert!(effective(second));
                assert!(!effective("pending"));
                tx.rollback().unwrap();
                assert_eq!(company_bytes(), company_before);
            }
        }
    }
}
