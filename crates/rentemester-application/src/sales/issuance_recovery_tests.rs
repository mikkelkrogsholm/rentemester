//! Whole production issuance, disposable protected namespaces only. Run with inherited umask 0077.
use super::*;
use rentemester_storage::invoice_lifecycle::Kind;
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn recovery_fixture() -> Fixture {
    let f = fixture("bookkeeper");
    f.control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u');").unwrap();
    f
}
fn uid() -> u32 {
    String::from_utf8(Command::new("id").arg("-u").output().unwrap().stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}
fn root(f: &Fixture) -> PathBuf {
    f._dir.path().canonicalize().unwrap().join("workspace")
}
fn source(number: Option<&str>) -> Value {
    let mut p = json!({"issueDate":"2026-01-01","buyer":{"name":"Synthetic recovery buyer","address":"Synthetic 2","vatOrCvr":"12345678"},"lines":[{"description":"Synthetic recovery work","quantity":1,"unitPriceExVat":100}]});
    if let Some(n) = number {
        p["invoiceNumber"] = json!(n);
    }
    p
}
fn body(mode: &str, p: &Value) -> Value {
    let mut p = p.clone();
    if mode == "original" {
        p["invoiceType"] = json!("full");
        p["totals"] = json!({"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125});
    }
    json!({"confirm":true,"payload":p})
}
fn run(
    c: &mut Connection,
    l: &mut Connection,
    g: &mut NamespaceGuard,
    action: Action<'_>,
) -> Result<Output, String> {
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let cookie = cookies.issue_session("synthetic-token").unwrap();
    let proof = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    execute(
        c,
        l,
        g,
        Context {
            cookie: &proof,
            clock: &Fixed,
            selection: CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
            },
        },
        Request {
            action,
            today: "2026-01-01",
            now: "2026-01-01T00:00:00.000Z",
            year: Some("2026"),
        },
    )
    .map(|x| x.value)
    .map_err(|e| format!("{}: {}", e.error.code, e.error.message))
}
fn issue(
    c: &mut Connection,
    l: &mut Connection,
    g: &mut NamespaceGuard,
    mode: &str,
    p: &Value,
) -> bool {
    let input: Input = serde_json::from_value(p.clone()).unwrap();
    let b = body(mode, p);
    let action = match mode {
        "normal" => Action::Issue(&input),
        "original" => Action::Lifecycle(Kind::Issue, &b),
        "create" => Action::Lifecycle(Kind::Create, &b),
        _ => panic!("mode"),
    };
    match run(c, l, g, action) {
        Ok(Output::Json(v)) => v["ok"] != false,
        Err(e) => {
            eprintln!("issuance_recovery refused mode={mode}: {e}");
            false
        }
        _ => panic!("JSON receipt required"),
    }
}
fn open(root: &Path) -> (Connection, Connection, NamespaceGuard) {
    let g = NamespaceGuard::acquire_with_profile(
        root,
        uid(),
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    let c = Connection::open(root.join(".rentemester/workspace-control.sqlite")).unwrap();
    let l = Connection::open(root.join("a/data/ledger.sqlite")).unwrap();
    for db in [&c, &l] {
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
            .unwrap();
    }
    (c, l, g)
}
fn count(db: &Connection, table: &str) -> i64 {
    db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn inventory(root: &Path) -> Vec<(String, Vec<u8>, u64)> {
    let mut result: Vec<(String, Vec<u8>, u64)> = vec![];
    let directory = root.join("a/documents/originals");
    if !directory.exists() {
        return result;
    }
    for e in std::fs::read_dir(directory).unwrap() {
        let p = e.unwrap().path();
        if p.is_file() {
            result.push((
                p.file_name().unwrap().to_str().unwrap().into(),
                std::fs::read(&p).unwrap(),
                std::fs::metadata(p).unwrap().ino(),
            ));
        }
    }
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}
fn validate(root: &Path, c: &mut Connection, l: &mut Connection, g: &mut NamespaceGuard, n: i64) {
    assert_eq!(count(l, "documents"), 2 * n);
    assert_eq!(count(l, "journal_entries"), 0);
    assert_eq!(count(l, "issued_invoice_postings"), 0);
    assert_eq!(count(l, "email_send_log"), 0);
    assert_eq!(count(l, "peppol_submissions"), 0);
    assert_eq!(count(l, "recurring_invoice_delivery_events"), 0);
    let rows: Vec<(i64,String,String,String,String)> = l.prepare("SELECT id,stored_path,sha256_hash,original_filename,document_type FROM documents ORDER BY id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap().map(Result::unwrap).collect();
    let before = inventory(root);
    for (id, path, hash, name, kind) in rows {
        let bytes = std::fs::read(root.join("a").join(&path)).unwrap();
        assert_eq!(Prepared::new(name, bytes.clone()).unwrap().hash(), hash);
        let event = if kind == "issued_invoice" {
            "invoice_issue"
        } else {
            "invoice_render_pdf"
        };
        assert_eq!(l.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type=?1 AND entity_id=?2 AND actor='user:u'",rusqlite::params![event,id.to_string()],|r|r.get(0)).unwrap(),1);
        if kind == "issued_invoice" {
            let (number, encoded): (String, String) = l
                .query_row(
                    "SELECT invoice_no,payload_json FROM documents WHERE id=?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();
            let payload: Value = serde_json::from_str(&encoded).unwrap();
            assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), payload);
            assert_eq!(payload["invoiceNumber"].as_str(), Some(number.as_str()));
            let pdfs: Vec<(String, String)> = l
                .prepare("SELECT stored_path,payload_json FROM documents WHERE document_type='issued_invoice_pdf' AND invoice_no=?1")
                .unwrap()
                .query_map([&number], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            assert_eq!(pdfs.len(), 1, "each invoice has exactly one PDF");
            assert_eq!(serde_json::from_str::<Value>(&pdfs[0].1).unwrap(), payload);
            assert_eq!(
                std::fs::read(root.join("a").join(&pdfs[0].0)).unwrap(),
                rentemester_engine::invoice_pdf::build(&payload),
                "PDF must represent the same immutable invoice payload"
            );
            let b = json!({"confirm":true,"documentId":id});
            assert!(matches!(
                run(c, l, g, Action::Lifecycle(Kind::Render, &b)),
                Ok(Output::Json(_))
            ));
            assert!(
                matches!(run(c,l,g,Action::Pdf(id)),Ok(Output::Pdf(v)) if v.starts_with(b"%PDF-"))
            );
        }
    }
    assert_eq!(
        inventory(root),
        before,
        "read/render must preserve bytes and inodes"
    );
}
#[test]
#[ignore = "subprocess helper selected only by issuance_recovery harness"]
fn issuance_recovery_child() {
    let root = PathBuf::from(std::env::var("RM_ISSUANCE_ROOT").unwrap());
    let (mut c, mut l, mut g) = open(&root);
    assert!(issue(
        &mut c,
        &mut l,
        &mut g,
        &std::env::var("RM_ISSUANCE_MODE").unwrap(),
        &source(Some("2026-0001"))
    ));
}
#[test]
fn issuance_recovery_crash_reopen_and_explicit_retry() {
    for mode in ["normal", "original", "create"] {
        for phase in [
            "issuance_json_published",
            "issuance_pdf_published",
            "issuance_registered",
            "precommit",
            "postcommit",
        ] {
            if mode == "create" && phase != "issuance_registered" {
                continue;
            }
            let f = recovery_fixture();
            let root = root(&f);
            let marker = f._dir.path().join("checkpoint");
            let Fixture {
                _dir,
                control,
                company,
                guard,
                ..
            } = f;
            drop((control, company, guard));
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "sales::tests::issuance_recovery_tests::issuance_recovery_child",
                    "--nocapture",
                ])
                .env("RM_ISSUANCE_ROOT", &root)
                .env("RM_ISSUANCE_MODE", mode)
                .env("RM_FENCE_POINT", phase)
                .env("RM_FENCE_MARKER", &marker)
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            let start = Instant::now();
            while !marker.exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("{mode}/{phase} child exited before checkpoint: {status}");
                }
                if start.elapsed() > Duration::from_secs(20) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("checkpoint timeout {mode}/{phase}");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            child.kill().unwrap();
            let status = child.wait().unwrap();
            assert!(!status.success());
            let (mut c, mut l, mut g) = open(&root);
            let committed = phase == "postcommit";
            validate(&root, &mut c, &mut l, &mut g, i64::from(committed));
            assert_eq!(
                l.query_row::<i64, _, _>(
                    "SELECT coalesce(sum(value),0) FROM sequences WHERE kind='issued_invoice'",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
                i64::from(committed)
            );
            assert_eq!(l.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type IN ('invoice_issue','invoice_render_pdf')",[],|r|r.get(0)).unwrap(),if committed {2}else{0});
            let residual = inventory(&root);
            assert_eq!(
                residual.len(),
                if phase == "issuance_json_published" {
                    1
                } else {
                    2
                }
            );
            // Authoritative read-back precedes retry; omitted-number retry is deliberately not supported.
            assert!(matches!(
                run(&mut c, &mut l, &mut g, Action::List),
                Ok(Output::Json(_))
            ));
            assert_eq!(
                issue(&mut c, &mut l, &mut g, mode, &source(Some("2026-0001"))),
                !committed
            );
            validate(&root, &mut c, &mut l, &mut g, 1);
            assert_eq!(
                inventory(&root).len(),
                2,
                "fixed-clock retry reuses bytes; not an actual-now retry guarantee"
            );
            eprintln!(
                "issuance_recovery mode={mode} killed={phase} reaped={status} committed={committed} retained_hash_files={} readback_then_retry={} immutable_pair=verified no_posting=true",
                residual.len(),
                !committed
            );
        }
    }
}

#[test]
fn issuance_recovery_concurrent_owner_and_same_number_contenders() {
    // Concurrent callers queue against one authoritative owner, as the native server does.
    let f = recovery_fixture();
    let (send, receive) =
        std::sync::mpsc::channel::<(String, Option<String>, std::sync::mpsc::Sender<bool>)>();
    let owner = std::thread::spawn(move || {
        let mut f = f;
        for (mode, number, reply) in receive {
            let ok = issue(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                &mode,
                &source(number.as_deref()),
            );
            reply.send(ok).unwrap();
        }
        let root = root(&f);
        validate(&root, &mut f.control, &mut f.company, &mut f.guard, 7);
        assert_eq!(inventory(&root).len(), 14);
        assert_eq!(f.company.query_row::<i64,_,_>("SELECT count(DISTINCT invoice_no) FROM documents WHERE document_type='issued_invoice'",[],|r|r.get(0)).unwrap(),7);
        assert_eq!(
            f.company
                .query_row::<i64, _, _>(
                    "SELECT value FROM sequences WHERE kind='issued_invoice'",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
            7
        );
    });
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(6));
    let mut callers = vec![];
    for mode in [
        "normal", "original", "create", "normal", "original", "create",
    ] {
        let send = send.clone();
        let barrier = barrier.clone();
        callers.push(std::thread::spawn(move || {
            let (tx, rx) = std::sync::mpsc::channel();
            barrier.wait();
            send.send((mode.into(), None, tx)).unwrap();
            assert!(rx.recv().unwrap());
        }));
    }
    for c in callers {
        c.join().unwrap();
    }
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut contenders = vec![];
    for mode in ["normal", "original"] {
        let send = send.clone();
        let barrier = barrier.clone();
        contenders.push(std::thread::spawn(move || {
            let (tx, rx) = std::sync::mpsc::channel();
            barrier.wait();
            send.send((mode.into(), Some("2026-0007".into()), tx))
                .unwrap();
            rx.recv().unwrap()
        }));
    }
    let wins = contenders
        .into_iter()
        .map(|c| usize::from(c.join().unwrap()))
        .sum::<usize>();
    assert_eq!(wins, 1);
    drop(send);
    owner.join().unwrap();
    eprintln!(
        "issuance_recovery concurrent_owner normal=2 original=2 create=2 unique_scoped_numbers=6 same_number_contenders=2 winners=1 refusals=1 complete_pairs=7"
    );
}
#[test]
fn issuance_recovery_create_registration_rollback_and_corrupt_evidence() {
    let mut f = recovery_fixture();
    let root = root(&f);
    assert!(issue(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        "create",
        &source(Some("2026-0001"))
    ));
    validate(&root, &mut f.control, &mut f.company, &mut f.guard, 1);
    assert_eq!(inventory(&root).len(), 2);
    let id: i64 = f
        .company
        .query_row(
            "SELECT id FROM documents WHERE document_type='issued_invoice'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    for kind in ["issued_invoice", "issued_invoice_pdf"] {
        let path: String = f
            .company
            .query_row(
                "SELECT stored_path FROM documents WHERE document_type=?1",
                [kind],
                |r| r.get(0),
            )
            .unwrap();
        let path = root.join("a").join(path);
        let original = std::fs::read(&path).unwrap();
        std::fs::write(&path, b"synthetic corrupt evidence").unwrap();
        let before = inventory(&root);
        let b = json!({"documentId":id,"confirm":true});
        assert!(
            matches!(run(&mut f.control,&mut f.company,&mut f.guard,Action::Lifecycle(Kind::Render,&b)),Err(e) if e.starts_with("invoice_evidence_unavailable"))
        );
        assert_eq!(
            inventory(&root),
            before,
            "corrupt evidence must not be replaced"
        );
        std::fs::write(path, original).unwrap();
    }
    eprintln!(
        "issuance_recovery create full_success=true registration_rollback=covered_by_registered_kill retry_complete_pair=true corrupt_json=refused corrupt_pdf=refused no_replacement=true"
    );
}
#[test]
fn issuance_recovery_missing_and_mismatched_inputs_have_no_effects() {
    let mut f = recovery_fixture();
    let root = root(&f);
    let mut missing = source(None);
    missing.as_object_mut().unwrap().remove("buyer");
    assert!(!issue(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        "normal",
        &missing
    ));
    let mut mismatch = body("original", &source(None));
    mismatch["payload"]["totals"]["grossAmount"] = json!(999);
    let result = run(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        Action::Lifecycle(Kind::Issue, &mismatch),
    );
    assert!(match result {
        Err(_) => true,
        Ok(Output::Json(v)) => v["ok"] == false,
        _ => false,
    });
    validate(&root, &mut f.control, &mut f.company, &mut f.guard, 0);
    assert!(inventory(&root).is_empty());
    assert_eq!(
        f.company
            .query_row::<i64, _, _>(
                "SELECT coalesce(sum(value),0) FROM sequences WHERE kind='issued_invoice'",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        0
    );
    eprintln!(
        "issuance_recovery missing_buyer=refused original_mismatched_totals=refused committed_sequences=0 files=0 documents=0"
    );
}

#[test]
fn issuance_recovery_service_shared_helper_attribution() {
    let mut f = fixture("bookkeeper");
    let buyer = json_output(f.run(Action::Customers(&customer())).unwrap());
    let issued = json_output(
        f.run(Action::Issue(&input(buyer["id"].as_i64().unwrap())))
            .unwrap(),
    );
    let id = issued["documentId"].as_i64().unwrap();
    let actor = format!("service_account:{}", f.subject);
    assert_eq!(f.company.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type IN ('invoice_issue','invoice_render_pdf') AND actor=?1",[actor],|r|r.get(0)).unwrap(),2);
    assert_eq!(f.count("documents"), 2);
    assert_eq!(f.count("journal_entries"), 0);
    assert_eq!(f.count("email_send_log"), 0);
    let before = inventory(&root(&f));
    assert!(
        matches!(f.run(Action::Pdf(id)).unwrap().result,Output::Pdf(bytes) if bytes.starts_with(b"%PDF-"))
    );
    assert_eq!(inventory(&root(&f)), before);
    eprintln!(
        "issuance_recovery service shared_helper=verified service_principal_audits=2 immutable_pair=true no_posting=true no_delivery=true"
    );
}
