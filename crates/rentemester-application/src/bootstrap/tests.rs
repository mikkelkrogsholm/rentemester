use super::*;
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier},
    bootstrap::ReservationKey,
    email_token::EmailTokenSigner,
    recovery::{DeliveryUnconfirmed, RecoveryLinks, VerificationMessage},
};
use rentemester_storage::password_credential::NextFactor;
const KEY: &str = "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE";
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1_767_225_600_000)
    }
}
#[derive(Default)]
struct Mail {
    fail: bool,
    tokens: Vec<String>,
}
impl VerificationSender for Mail {
    fn enabled(&self) -> bool {
        true
    }
    fn send(&mut self, message: &VerificationMessage) -> Result<(), DeliveryUnconfirmed> {
        assert_eq!(message.recipient(), "first@example.invalid");
        self.tokens.push(message.token().to_owned());
        if self.fail {
            Err(DeliveryUnconfirmed)
        } else {
            Ok(())
        }
    }
}
struct Config {
    key: ReservationKey,
    signer: EmailTokenSigner,
    cookies: CookieVerifier,
    links: RecoveryLinks,
}
impl Config {
    fn new() -> Self {
        Self {
            key: ReservationKey::new("w", KEY).unwrap(),
            signer: EmailTokenSigner::new("w", KEY).unwrap(),
            cookies: CookieVerifier::new("w", CookieProfile::Development, KEY).unwrap(),
            links: RecoveryLinks::new(
                "http://localhost:4319",
                &["http://localhost:4319"],
                CookieProfile::Development,
            )
            .unwrap(),
        }
    }
}
fn fixture(root: &std::path::Path) -> Connection {
    let mut control = Connection::open(root.join("control.sqlite")).unwrap();
    let mut company = Connection::open(root.join("company.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic")
        .unwrap();
    company.close().unwrap();
    control
}
fn operator(name: &str) -> Operator {
    Operator::from_policy(
        "w",
        name,
        &["user:installer".to_owned(), "agent:recovery".to_owned()],
    )
    .unwrap()
}
fn selection() -> CompanySelection<'static> {
    CompanySelection {
        workspace: "w",
        slug: "a",
        registered: true,
        archived: false,
    }
}
fn input() -> Input<'static> {
    Input {
        email: " FIRST@example.invalid ",
        name: " Synthetic Installer ",
        password: " synthetic-password-123 ",
    }
}
fn n(db: &Connection, table: &str) -> i64 {
    db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn with_runtime<T>(config: &Config, work: impl FnOnce(&Runtime<'_>) -> T) -> T {
    let verification = crate::email_verification::Runtime {
        signer: &config.signer,
        clock: &Fixed,
        audit_key_version: 1,
    };
    let mail = verification_request::Runtime {
        verification: &verification,
        cookies: &config.cookies,
        links: &config.links,
    };
    work(&Runtime {
        reservation_key: &config.key,
        verification: &mail,
    })
}
#[test]
fn complete_bootstrap_requires_real_email_completion_before_password_factor() {
    let root = tempfile::tempdir().unwrap();
    let mut db = fixture(root.path());
    let config = Config::new();
    let mut mail = Mail::default();
    let result = with_runtime(&config, |runtime| {
        execute(
            &mut db,
            &operator("user:installer"),
            runtime,
            &mut mail,
            selection(),
            input(),
        )
    })
    .unwrap();
    assert_eq!(result.phase, Phase::AccessCompleted);
    assert_eq!(mail.tokens.len(), 1);
    for table in [
        "user",
        "account",
        "rm_workspace_user_access_events",
        "rm_company_membership_events",
    ] {
        assert_eq!(n(&db, table), 1, "{table}");
    }
    assert_eq!(n(&db, "session"), 0);
    assert_eq!(n(&db, "twoFactor"), 0);
    assert_eq!(n(&db, "rm_workspace_bootstrap_events"), 4);
    assert_eq!(n(&db, "workspace_audit"), 4);
    assert_eq!(
        db.query_row("SELECT emailVerified FROM user", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    let before = crate::password_login::execute(
        &mut db,
        "first@example.invalid",
        "synthetic-password-123",
        |_, factor| Ok(factor),
    );
    assert!(before.is_err());
    with_runtime(&config, |runtime| {
        verification_request::complete(&mut db, runtime.verification, &mail.tokens[0], None)
    })
    .unwrap();
    let factor = crate::password_login::execute(
        &mut db,
        "first@example.invalid",
        "synthetic-password-123",
        |_, factor| Ok(factor),
    )
    .unwrap();
    assert_eq!(factor.next, NextFactor::Enrollment);
    assert_eq!(factor.subject, result.user_id.unwrap());
    assert_eq!(n(&db, "session"), 0);
    let audit: String = db
        .query_row(
            "SELECT group_concat(entity_id) FROM workspace_audit",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!audit.contains("@"));
    assert!(!audit.contains("password"));
}
#[test]
fn failed_handoff_keeps_progress_without_grants_and_other_operator_resumes_once() {
    let root = tempfile::tempdir().unwrap();
    let mut db = fixture(root.path());
    let config = Config::new();
    let mut mail = Mail {
        fail: true,
        ..Default::default()
    };
    let error = with_runtime(&config, |runtime| {
        execute(
            &mut db,
            &operator("user:installer"),
            runtime,
            &mut mail,
            selection(),
            input(),
        )
    })
    .unwrap_err();
    assert_eq!(error.last_committed_phase, Some(Phase::IdentityCreated));
    assert!(!error.commit_outcome_unknown);
    assert!(error.connection_reusable);
    assert_eq!(n(&db, "user"), 1);
    assert_eq!(n(&db, "account"), 1);
    assert_eq!(n(&db, "rm_workspace_bootstrap_events"), 2);
    assert_eq!(n(&db, "rm_workspace_user_access_events"), 0);
    assert_eq!(n(&db, "rm_company_membership_events"), 0);
    assert_eq!(n(&db, "session"), 0);
    let hash: String = db
        .query_row("SELECT password FROM account", [], |r| r.get(0))
        .unwrap();
    mail.fail = false;
    let recovered = with_runtime(&config, |runtime| {
        execute(
            &mut db,
            &operator("agent:recovery"),
            runtime,
            &mut mail,
            selection(),
            Input {
                password: "different-password-123",
                ..input()
            },
        )
    })
    .unwrap();
    assert_eq!(recovered.phase, Phase::AccessCompleted);
    assert_eq!(n(&db, "user"), 1);
    assert_eq!(n(&db, "account"), 1);
    assert_eq!(n(&db, "rm_workspace_bootstrap_events"), 4);
    assert_eq!(
        db.query_row("SELECT password FROM account", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        hash
    );
    let sent = mail.tokens.len();
    let before = db.total_changes();
    let again = with_runtime(&config, |runtime| {
        execute(
            &mut db,
            &operator("user:installer"),
            runtime,
            &mut mail,
            selection(),
            input(),
        )
    })
    .unwrap();
    assert_eq!(again.user_id, recovered.user_id);
    assert_eq!(mail.tokens.len(), sent);
    assert_eq!(db.total_changes(), before);
}
#[test]
fn another_identity_or_company_cannot_steal_the_singleton_reservation() {
    let root = tempfile::tempdir().unwrap();
    let mut db = fixture(root.path());
    let config = Config::new();
    let mut mail = Mail {
        fail: true,
        ..Default::default()
    };
    assert!(
        with_runtime(&config, |runtime| execute(
            &mut db,
            &operator("user:installer"),
            runtime,
            &mut mail,
            selection(),
            input()
        ))
        .is_err()
    );
    let before = db.total_changes();
    assert!(
        with_runtime(&config, |runtime| execute(
            &mut db,
            &operator("agent:recovery"),
            runtime,
            &mut mail,
            selection(),
            Input {
                email: "other@example.invalid",
                ..input()
            }
        ))
        .is_err()
    );
    let mut other = selection();
    other.slug = "b";
    assert!(
        with_runtime(&config, |runtime| execute(
            &mut db,
            &operator("agent:recovery"),
            runtime,
            &mut mail,
            other,
            input()
        ))
        .is_err()
    );
    assert_eq!(db.total_changes(), before);
    assert_eq!(n(&db, "user"), 1);
    assert_eq!(n(&db, "rm_workspace_user_access_events"), 0);
}
#[test]
fn wrong_workspace_or_archived_unregistered_selection_refuses_before_reservation() {
    let root = tempfile::tempdir().unwrap();
    let mut db = fixture(root.path());
    let config = Config::new();
    let mut mail = Mail::default();
    let before = db.total_changes();
    for mode in 0..3 {
        let mut company = selection();
        match mode {
            0 => company.workspace = "other",
            1 => company.archived = true,
            _ => company.registered = false,
        };
        assert!(
            with_runtime(&config, |runtime| execute(
                &mut db,
                &operator("user:installer"),
                runtime,
                &mut mail,
                company,
                input()
            ))
            .is_err()
        );
    }
    assert_eq!(db.total_changes(), before);
    assert_eq!(n(&db, "rm_workspace_bootstrap_events"), 0);
    assert_eq!(n(&db, "user"), 0);
    assert!(mail.tokens.is_empty());
}
#[test]
fn completed_retry_never_reactivates_a_later_disabled_owner() {
    let root = tempfile::tempdir().unwrap();
    let mut db = fixture(root.path());
    let config = Config::new();
    let mut mail = Mail::default();
    let result = with_runtime(&config, |runtime| {
        execute(
            &mut db,
            &operator("user:installer"),
            runtime,
            &mut mail,
            selection(),
            input(),
        )
    })
    .unwrap();
    db.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES(?1,'disable',NULL,'user:installer')",[result.user_id.as_ref().unwrap()]).unwrap();
    let before = db.total_changes();
    assert_eq!(
        with_runtime(&config, |runtime| execute(
            &mut db,
            &operator("agent:recovery"),
            runtime,
            &mut mail,
            selection(),
            input()
        ))
        .unwrap()
        .phase,
        Phase::AccessCompleted
    );
    assert_eq!(db.total_changes(), before);
    assert_eq!(n(&db, "rm_workspace_user_access_events"), 2);
}
