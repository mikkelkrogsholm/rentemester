#![cfg(target_os = "macos")]
use super::*;
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier, VerifiedCookie},
    namespace::{NamespaceGuard, NamespaceProfile},
};
use std::path::PathBuf;
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
    control_path: PathBuf,
    company_path: PathBuf,
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
        control_path,
        company_path,
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
fn value(output: Output) -> Value {
    match output {
        Output::Value(v) => v,
        Output::Admission(_) => panic!("expected result"),
    }
}
fn prepare(f: &mut Fixture, request: Request<'_>) -> Admission {
    match execute(&mut f.control, &mut f.company, context(&f.cookie), request)
        .unwrap()
        .value
    {
        Output::Admission(a) => a,
        Output::Value(_) => panic!("expected admission"),
    }
}
#[test]
fn master_data_original_reader_bookkeeper_and_provider_finish_current_revocation() {
    let mut f = fixture("reader");
    f.guard
        .ensure_connections(&mut f.control, &mut f.company)
        .unwrap();
    assert!(f.control_path.is_file() && f.company_path.is_file());
    let result = execute(
        &mut f.control,
        &mut f.company,
        context(&f.cookie),
        Request::List(Kind::Customer, false),
    )
    .unwrap();
    assert_eq!(value(result.value)["count"], 0);
    assert!(
        execute(
            &mut f.control,
            &mut f.company,
            context(&f.cookie),
            Request::PrepareVat("DK12345678")
        )
        .is_err()
    );
    f.control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u')").unwrap();
    let admission = prepare(&mut f, Request::PrepareVat("dk/12 34 56 78"));
    assert_eq!(admission.provider_identity(), Some("DK12345678"));
    let observation = json!({"countryCode":"DK","vatNumber":"12345678","normalized":"DK12345678","valid":false,"name":null,"address":null,"rawResponse":null});
    let result = execute(
        &mut f.control,
        &mut f.company,
        context(&f.cookie),
        Request::Finish(admission, &observation),
    )
    .unwrap();
    assert_eq!(value(result.value)["validation"]["valid"], false);
    let admission = prepare(&mut f, Request::PrepareVat("DE123456789"));
    f.control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')").unwrap();
    assert!(
        execute(
            &mut f.control,
            &mut f.company,
            context(&f.cookie),
            Request::Finish(admission, &observation)
        )
        .is_err()
    );
    assert!(
        execute(
            &mut f.control,
            &mut f.company,
            context(&f.cookie),
            Request::List(Kind::Customer, false)
        )
        .is_err()
    );
    assert_eq!(
        f.company
            .query_row::<i64, _, _>("SELECT count(*) FROM customers", [], |r| r.get(0))
            .unwrap(),
        0
    );
}
#[test]
fn master_data_original_provider_finish_subject_company_namespace_and_mismatch_bindings() {
    let mut f = fixture("bookkeeper");
    for field in ["subject", "workspace", "slug", "ledger"] {
        let mut admission = prepare(&mut f, Request::PrepareVat("DK12345678"));
        match field {
            "subject" => admission.subject = "other".into(),
            "workspace" => admission.workspace = "other".into(),
            "slug" => admission.slug = "other".into(),
            _ => admission.ledger = "other".into(),
        }
        let observation = json!({"normalized":"DK12345678","valid":true});
        assert!(
            execute(
                &mut f.control,
                &mut f.company,
                context(&f.cookie),
                Request::Finish(admission, &observation)
            )
            .is_err()
        );
    }
    let admission = prepare(&mut f, Request::PrepareVat("DK12345678"));
    assert!(
        execute(
            &mut f.control,
            &mut f.company,
            context(&f.cookie),
            Request::Finish(admission, &json!({"normalized":"DE123456789","valid":true}))
        )
        .is_err()
    );
    let input: Create =
        serde_json::from_value(json!({"input":{},"fromCvr":"12345678","confirm":true})).unwrap();
    let admission = prepare(
        &mut f,
        Request::PrepareCvr("12345678", Some((Kind::Customer, &input))),
    );
    let mismatch = json!({"cvrNummer":87654321,"virksomhedMetadata":{"nyesteNavn":{"navn":"Synthetic mismatch"}}});
    let result = execute(
        &mut f.control,
        &mut f.company,
        context(&f.cookie),
        Request::Finish(admission, &mismatch),
    )
    .unwrap();
    assert_eq!(value(result.value)["ok"], false);
    assert_eq!(
        f.company
            .query_row::<i64, _, _>("SELECT count(*) FROM customers", [], |r| r.get(0))
            .unwrap(),
        0
    );
    assert_eq!(
        f.company
            .query_row::<i64, _, _>("SELECT count(*) FROM cvr_lookups", [], |r| r.get(0))
            .unwrap(),
        0
    );
}
#[test]
fn master_data_original_composed_create_fill_override_and_revocation_no_cache_publication() {
    let mut f = fixture("bookkeeper");
    let input:Create=serde_json::from_value(json!({"input":{"name":" Manual customer ","defaultCurrency":" eur ","notes":" Kept "},"fromCvr":"12345678","confirm":true})).unwrap();
    let provider = json!({"cvrNummer":12345678,"virksomhedMetadata":{"nyesteNavn":{"navn":"Synthetic provider"},"nyesteBeliggenhedsadresse":{"vejnavn":"Synthetic Street","husnummerFra":1,"postnummer":1000,"postdistrikt":"Synthetic City"}}});
    let admission = prepare(
        &mut f,
        Request::PrepareCvr("12345678", Some((Kind::Customer, &input))),
    );
    assert!(f.company.is_autocommit() && f.control.is_autocommit());
    let result = execute(
        &mut f.control,
        &mut f.company,
        context(&f.cookie),
        Request::Finish(admission, &provider),
    )
    .unwrap();
    assert_eq!(value(result.value)["ok"], true);
    let rows = value(
        execute(
            &mut f.control,
            &mut f.company,
            context(&f.cookie),
            Request::List(Kind::Customer, false),
        )
        .unwrap()
        .value,
    );
    assert_eq!(rows["rows"][0]["name"], "Manual customer");
    assert_eq!(rows["rows"][0]["vatOrCvr"], "DK12345678");
    assert_eq!(rows["rows"][0]["defaultCurrency"], "EUR");
    assert_eq!(rows["rows"][0]["paymentTermsDays"], Value::Null);
    let admission = prepare(
        &mut f,
        Request::PrepareCvr("87654321", Some((Kind::Vendor, &input))),
    );
    f.control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')").unwrap();
    assert!(
        execute(
            &mut f.control,
            &mut f.company,
            context(&f.cookie),
            Request::Finish(admission, &provider)
        )
        .is_err()
    );
    assert_eq!(
        f.company
            .query_row::<i64, _, _>("SELECT count(*) FROM vendors", [], |r| r.get(0))
            .unwrap(),
        0
    );
    assert_eq!(
        f.company
            .query_row::<i64, _, _>("SELECT count(*) FROM cvr_lookups", [], |r| r.get(0))
            .unwrap(),
        1
    );
}

#[test]
fn master_data_original_vat_requires_fresh_mfa_at_prepare_and_finish() {
    struct Older;
    impl Clock for Older {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1767225600000 + 2 * 60 * 60 * 1000)
        }
    }
    let mut f = fixture("bookkeeper");
    let admission = match execute(
        &mut f.control,
        &mut f.company,
        context(&f.cookie),
        Request::PrepareVat("DK12345678"),
    )
    .unwrap()
    .value
    {
        Output::Admission(a) => a,
        _ => panic!("admission expected"),
    };
    let mut old = context(&f.cookie);
    old.clock = &Older;
    assert!(
        execute(
            &mut f.control,
            &mut f.company,
            old,
            Request::List(Kind::Customer, false)
        )
        .is_ok()
    );
    let mut old = context(&f.cookie);
    old.clock = &Older;
    assert!(
        execute(
            &mut f.control,
            &mut f.company,
            old,
            Request::PrepareVat("DK12345678")
        )
        .is_err()
    );
    let mut old = context(&f.cookie);
    old.clock = &Older;
    assert!(execute(&mut f.control,&mut f.company,old,Request::Finish(admission,&json!({"countryCode":"DK","vatNumber":"12345678","normalized":"DK12345678","valid":true,"name":null,"address":null,"rawResponse":null}))).is_err());
    assert_eq!(
        f.company
            .query_row::<i64, _, _>("SELECT count(*) FROM vies_validations", [], |r| r.get(0))
            .unwrap(),
        0
    );
}
