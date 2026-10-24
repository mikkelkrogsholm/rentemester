//! Actual common-server authority, rollback and retained-original acceptance for original imports.
use rentemester_application::{
    control_fence::CompanySelection, dinero_import, documents::Context, opening_balance,
};
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier, VerifiedCookie},
    document::Prepared,
    namespace::{NamespaceGuard, NamespaceProfile},
};
use rentemester_storage::{dinero_import as store, synthetic_import};
use rusqlite::Connection;
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1_767_225_600_000)
    }
}
fn context(cookie: &VerifiedCookie) -> Context<'_> {
    Context {
        cookie,
        clock: &Fixed,
        selection: CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    control: Connection,
    company: Connection,
    guard: NamespaceGuard,
    cookie: VerifiedCookie,
}
fn fixture() -> Fixture {
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
        "Synthetic import",
        "2026-01-01T00:00:00.000Z",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    let mut control = Connection::open(root.join(".rentemester/workspace-control.sqlite")).unwrap();
    let mut company = Connection::open(root.join("a/data/ledger.sqlite")).unwrap();
    for db in [&control, &company] {
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
            .unwrap();
    }
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic import",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
    let verifier = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let issued = verifier.issue_session("synthetic-token").unwrap();
    let cookie = verifier
        .verify(Some(issued.request_header()), None)
        .unwrap();
    Fixture {
        _dir: dir,
        root,
        control,
        company,
        guard,
        cookie,
    }
}
fn snapshot(db: &Connection) -> BTreeMap<String, Vec<String>> {
    let tables: Vec<String> = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    tables
        .into_iter()
        .map(|table| {
            let mut stmt = db
                .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
                .unwrap();
            let width = stmt.column_count();
            let mut rows: Vec<String> = stmt
                .query_map([], |r| {
                    Ok((0..width)
                        .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                        .collect::<Vec<_>>()
                        .join("|"))
                })
                .unwrap()
                .map(Result::unwrap)
                .collect();
            rows.sort();
            (table, rows)
        })
        .collect()
}
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for e in std::fs::read_dir(path).unwrap() {
            let p = e.unwrap().path();
            let relative = p.strip_prefix(root).unwrap();
            if relative.starts_with("data") {
                continue;
            }
            if p.is_dir() {
                visit(root, &p, out);
            } else {
                out.insert(relative.into(), std::fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}
fn synthetic() -> synthetic_import::Input {
    synthetic_import::Input {source_system:"synthetic-csv".into(),content:"# cutOverDate: 2025-01-01\nsection,accountNo,name,debit,credit\naccount,A,Source asset,,\naccount,E,Source equity,,\nopening,A,,1234,\nopening,E,,,1234\n".into()}
}
#[test]
fn original_import94_synthetic_dryrun_current_authority_full_rollback_then_post_once() {
    let mut f = fixture();
    f.company.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('A','Old asset','asset','debit'),('E','Old equity','equity','credit')").unwrap();
    let input = synthetic();
    let before = snapshot(&f.company);
    let result = opening_balance::execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        opening_balance::Action::OriginalRun(&input, true),
    )
    .unwrap()
    .value;
    assert_eq!(result["ok"], true);
    assert_eq!(result["dryRun"], true);
    assert_eq!(result["chart"]["existing"], json!(["A", "E"]));
    assert_eq!(result["chart"]["updated"], json!(["A", "E"]));
    assert_eq!(result["chart"]["differences"].as_array().unwrap().len(), 2);
    assert!(result["entryHash"].as_str().is_some_and(|s| s.len() == 64));
    assert_eq!(
        snapshot(&f.company),
        before,
        "all company rows including account edits, audit, sequence and receipts roll back"
    );
    f.control.execute_batch("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','disable',NULL,'user:u')").unwrap();
    let denied = opening_balance::execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        opening_balance::Action::OriginalRun(&input, false),
    )
    .unwrap_err();
    assert_eq!(denied.error.code, "invalid_credentials");
    assert_eq!(snapshot(&f.company), before);
    f.control.execute_batch("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u')").unwrap();
    let result = opening_balance::execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        opening_balance::Action::OriginalRun(&input, false),
    )
    .unwrap()
    .value;
    assert_eq!(result["ok"], true);
    assert!(result.get("dryRun").is_none());
    assert!(
        rentemester_storage::opening_balance::validate_history(&f.company)
            .unwrap()
            .is_some()
    );
    let accepted = snapshot(&f.company);
    let repeated = opening_balance::execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        opening_balance::Action::OriginalRun(&input, false),
    )
    .unwrap()
    .value;
    assert_eq!(repeated["ok"], false);
    assert_eq!(repeated["openingBalanceLineCount"], 2);
    assert_eq!(repeated["cutOverDate"], "2025-01-01");
    assert_eq!(
        snapshot(&f.company),
        accepted,
        "original repeat refuses rather than inventing a keyed replay"
    );
}
#[test]
fn original_import94_dinero_zip_dryrun_cleans_owned_files_preserves_preexisting_and_accepts_provenance()
 {
    let mut f = fixture();
    let raw =
        include_bytes!("../../rentemester-storage/src/dinero_import/fixtures/source126-stored.zip");
    let source = Prepared::dinero_source("synthetic.zip".into(), raw.to_vec()).unwrap();
    let before = snapshot(&f.company);
    let prior_files = files(&f.root.join("a"));
    let dry = dinero_import::execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        dinero_import::Action::OriginalRun(&source, true),
    )
    .unwrap()
    .value;
    let dinero_import::Output::Json(dry) = dry else {
        panic!("JSON required")
    };
    assert_eq!(dry["ok"], true);
    assert_eq!(dry["dryRun"], true);
    assert!(dry.get("bilag").is_none());
    assert_eq!(dry["historicalEntriesPosted"].as_array().unwrap().len(), 3);
    assert_eq!(dry["company"]["updatedFields"], json!(["cvr"]));
    assert_eq!(dry["archiveIntegrity"]["archiveSha256"], source.hash());
    assert_eq!(snapshot(&f.company), before);
    assert_eq!(
        files(&f.root.join("a")),
        prior_files,
        "every newly published original is removed after dry run"
    );
    f.guard.publish_document_tracked(&source).unwrap();
    let existing = files(&f.root.join("a"));
    let repeat = dinero_import::execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        dinero_import::Action::OriginalRun(&source, true),
    )
    .unwrap()
    .value;
    let dinero_import::Output::Json(repeat) = repeat else {
        panic!("JSON required")
    };
    assert_eq!(repeat["ok"], true);
    assert_eq!(snapshot(&f.company), before);
    assert_eq!(
        files(&f.root.join("a")),
        existing,
        "preexisting content is never owned by rollback"
    );
    f.control.execute_batch("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','disable',NULL,'user:u')").unwrap();
    assert!(
        dinero_import::execute(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            context(&f.cookie),
            dinero_import::Action::OriginalRun(&source, false)
        )
        .is_err()
    );
    assert_eq!(snapshot(&f.company), before);
    assert_eq!(files(&f.root.join("a")), existing);
    f.control.execute_batch("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u')").unwrap();
    let accepted = dinero_import::execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        dinero_import::Action::OriginalRun(&source, false),
    )
    .unwrap()
    .value;
    let dinero_import::Output::Json(accepted) = accepted else {
        panic!("JSON required")
    };
    assert_eq!(accepted["ok"], true);
    assert!(accepted.get("dryRun").is_none());
    assert_eq!(
        accepted["bilag"],
        json!({"linkedCount":2,"unmatchedCount":0,"duplicateCount":0,"unbookedCount":1})
    );
    assert!(
        accepted["appliedRules"]
            .as_array()
            .unwrap()
            .contains(&json!("DK-IMPORT-POSTINGS-001"))
    );
    assert_eq!(store::originals(&f.company).unwrap().len(), 1);
    dinero_import::verify_physical(&f.company, &mut f.guard).unwrap();
    assert!(
        rentemester_storage::verify_journal_hashes(&f.root.join("a"))
            .unwrap()
            .ok
    );
    // Reuse the admitted company and verified source files, with no new fixture.
    // Chart-only migration has no cut-over date, including its repeat refusal.
    let mut decoded = store::decode_source(raw).unwrap();
    decoded.files.retain(|file| {
        file.path == "Firmaoplysninger.csv" || file.path.ends_with("/Kontoplan.csv")
    });
    let package = rentemester_engine::dinero_import::containers::package(&decoded.files).unwrap();
    let chart = Prepared::new("dinero-directory-source.json".into(), package).unwrap();
    let chart_result = dinero_import::execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        dinero_import::Action::OriginalRun(&chart, false),
    )
    .unwrap()
    .value;
    let dinero_import::Output::Json(chart_result) = chart_result else {
        panic!("JSON required")
    };
    assert_eq!(chart_result["ok"], true);
    assert!(chart_result.get("cutOverDate").is_none());
    let rows = snapshot(&f.company);
    let existing = files(&f.root.join("a"));
    let repeat = dinero_import::execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        dinero_import::Action::OriginalRun(&chart, false),
    )
    .unwrap()
    .value;
    let dinero_import::Output::Json(repeat) = repeat else {
        panic!("JSON required")
    };
    assert_eq!(repeat["ok"], false);
    assert!(repeat.get("cutOverDate").is_none());
    assert_eq!(repeat["openingBalanceLineCount"], 0);
    assert_eq!(snapshot(&f.company), rows);
    assert_eq!(files(&f.root.join("a")), existing);
}
