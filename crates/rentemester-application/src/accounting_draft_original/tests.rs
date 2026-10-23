#![cfg(target_os = "macos")]
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
fn request(body: &[u8]) -> Request<'_> {
    Request {
        body,
        query: None,
        valid_transport: true,
    }
}
#[test]
fn accounting_draft_original_current_permissions_before_parse() {
    for (role, denied) in [
        (
            "reader",
            vec![
                Operation::Create,
                Operation::Revise,
                Operation::Submit,
                Operation::Reject,
                Operation::ApproveAndPost,
            ],
        ),
        (
            "bookkeeper",
            vec![Operation::Reject, Operation::ApproveAndPost],
        ),
    ] {
        let mut f = fixture(role);
        for op in denied {
            let e = execute(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                context(&f.cookie),
                request(b"not json"),
                op,
            )
            .err()
            .unwrap();
            assert_eq!(e.error.code, "access_denied");
        }
        let value = execute(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            context(&f.cookie),
            request(b""),
            Operation::List,
        )
        .unwrap()
        .value;
        assert_eq!(value, json!({"ok":true,"accountingDrafts":[]}));
        f.control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES('u','a','revoke','user:u')").unwrap();
        assert_eq!(
            execute(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                context(&f.cookie),
                request(b"not json"),
                Operation::Show
            )
            .err()
            .unwrap()
            .error
            .code,
            "access_denied"
        );
    }
}
#[test]
fn accounting_draft_original_strict_boundaries_and_typed_assertions() {
    let base = json!({"draftId":"synthetic","payload":{},"confirm":true});
    for actor in ["user:u", "service_account:u"] {
        let mut v = base.clone();
        v["actor"] = json!(actor);
        v["principal"] = json!(actor);
        let b = serde_json::to_vec(&v).unwrap();
        let parsed = parse(&request(&b), Operation::Create, actor).unwrap();
        assert_eq!(parsed, base);
        assert_eq!(
            parse(&request(&b), Operation::Create, "user:other")
                .unwrap_err()
                .code,
            "access_denied"
        );
    }
    for body in [
        br#"{"draftId":"synthetic","payload":{"lines":[{"x":1,"x":2}]},"confirm":true}"#.as_slice(),
        br#"{"draftId":"synthetic","payload":{},"confirm":true,"confirm":true}"#,
        br#"{"draftId":"synthetic","payload":{"createdBy":"user:u"},"confirm":true}"#,
        br#"{"draftId":"synthetic","payload":{},"confirm":true,"company":"a"}"#,
        br#"{"draftId":"synthetic","payload":{},"confirm":false}"#,
    ] {
        assert!(parse(&request(body), Operation::Create, "user:u").is_err());
    }
    for q in [
        None,
        Some(""),
        Some("draftId="),
        Some("draftId=a&draftId=b"),
        Some("draftId=a&actor=x"),
        Some("draftId=%GG"),
        Some("draftId=a=b"),
    ] {
        assert!(
            parse(
                &Request {
                    body: b"",
                    query: q,
                    valid_transport: true
                },
                Operation::Show,
                "user:u"
            )
            .is_err()
        );
    }
    assert_eq!(
        parse(
            &Request {
                body: b"",
                query: Some("draftId=synthetic"),
                valid_transport: true
            },
            Operation::Show,
            "user:u"
        )
        .unwrap(),
        json!({"draftId":"synthetic"})
    );
    for op in [Operation::Show, Operation::List] {
        assert!(parse(&request(b"{}"), op, "user:u").is_err());
    }
    assert!(
        parse(
            &Request {
                body: b"",
                query: Some(""),
                valid_transport: true
            },
            Operation::List,
            "user:u"
        )
        .is_err()
    );
    let oversized = serde_json::to_vec(
        &json!({"draftId":"synthetic","payload":{"text":"x".repeat(MAX_PAYLOAD)},"confirm":true}),
    )
    .unwrap();
    assert!(parse(&request(&oversized), Operation::Create, "user:u").is_err());
}
#[test]
fn accounting_draft_original_exact_core_business_failure_rolls_back() {
    let mut f = fixture("reviewer");
    let input =
        json!({"draftId":"synthetic-missing","expectedEventHash":"a".repeat(64),"confirm":true});
    // Native journal writers require Immediate, including expected-core fixtures.
    let expected = {
        let tx = f
            .company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let v = store::execute(
            &tx,
            Operation::ApproveAndPost,
            &input,
            "user:u",
            rentemester_engine::fiscal_year::Date::parse("2026-02-15").unwrap(),
        )
        .unwrap();
        tx.rollback().unwrap();
        v
    };
    assert_eq!(expected["ok"], false);
    let bytes = serde_json::to_vec(&input).unwrap();
    let failure = execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        request(&bytes),
        Operation::ApproveAndPost,
    )
    .err()
    .unwrap();
    assert_eq!(failure.error.code, "accounting_draft_original_business");
    assert_eq!(
        serde_json::from_str::<Value>(&failure.error.message).unwrap(),
        expected
    );
    for table in [
        "journal_lines",
        "accounting_draft_events",
        "journal_entries",
        "audit_log",
    ] {
        assert_eq!(
            f.company
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
#[test]
fn accounting_draft_original_service_current_typed_authority_precedes_parse() {
    use rentemester_runtime::service_key::ServiceKey;
    let mut f = fixture("owner");
    f.control.execute_batch("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u')").unwrap();
    let verifier = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let receipt = crate::service_lifecycle::execute(
        &mut f.control,
        &verifier,
        &Fixed,
        &f.cookie,
        &crate::service_lifecycle::Operation::Create {
            display_name: "Synthetic draft bookkeeper".into(),
            operation_id: None,
            expires_in_seconds: None,
        },
    )
    .unwrap();
    let issued: Value = serde_json::from_str(receipt.as_json()).unwrap();
    let subject = issued["serviceAccountId"].as_str().unwrap();
    let key = ServiceKey::new("w", issued["secret"].as_str().unwrap()).unwrap();
    f.control.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')", [subject]).unwrap();
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','reader','user:u')", [subject]).unwrap();
    let service = || service_company::Context {
        key: &key,
        clock: &Fixed,
        max_future_days: 45,
        selection: control_fence::CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
    };
    for op in [
        Operation::Create,
        Operation::ApproveAndPost,
        Operation::Revise,
        Operation::Submit,
    ] {
        assert_eq!(
            execute_service(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                service(),
                request(b"not json"),
                op
            )
            .err()
            .unwrap()
            .error
            .code,
            "access_denied"
        );
    }
    for op in [Operation::List] {
        let value = execute_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            service(),
            request(b""),
            op,
        )
        .unwrap()
        .result;
        assert_eq!(value, json!({"ok":true,"accountingDrafts":[]}));
    }
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')", [subject]).unwrap();
    for op in [Operation::Create, Operation::Revise, Operation::Submit] {
        assert_eq!(
            execute_service(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                service(),
                request(b"not json"),
                op
            )
            .err()
            .unwrap()
            .error
            .code,
            "invalid_input"
        );
    }
    for op in [Operation::Reject, Operation::ApproveAndPost] {
        assert_eq!(
            execute_service(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                service(),
                request(b"not json"),
                op
            )
            .err()
            .unwrap()
            .error
            .code,
            "access_denied"
        );
    }
    // A downgrade is evaluated on the next call, even with malformed input.
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','reader','user:u')", [subject]).unwrap();
    assert_eq!(
        execute_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            service(),
            request(b"not json"),
            Operation::Create
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','reviewer','user:u')", [subject]).unwrap();
    let input = serde_json::to_vec(&json!({"draftId":"synthetic", "expectedEventHash":"a".repeat(64), "confirm":true, "actor":format!("user:{subject}")})).unwrap();
    assert_eq!(
        execute_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            service(),
            request(&input),
            Operation::ApproveAndPost
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES(?1,'a','revoke','user:u')", [subject]).unwrap();
    assert_eq!(
        execute_service(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            service(),
            request(b"not json"),
            Operation::Show
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
}
