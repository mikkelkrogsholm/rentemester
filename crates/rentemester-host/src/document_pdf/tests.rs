//! Synthetic unit worker outcomes on a real private namespace; actual worker
//! and HTTP/MFA acceptance are qualified independently by the root.
use super::*;
use rentemester_engine::document_pdf::{self as pdf, OperationalError, ParseResult};
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
use serde_json::{Value, json};
struct Fixed(i64);
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
const NOW: i64 = 1767225600000;
fn setup() -> (
    tempfile::TempDir,
    Owner,
    CookieVerifier,
    VerifiedCookie,
    u32,
) {
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
    let mut owner = Owner::create_with_chart_profile(
        &root,
        uid,
        "w",
        "a",
        "Synthetic A",
        "2026-01-01T00:00:00.000Z",
        &[],
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','owner-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let cookie = cookies.issue_session("owner-token").unwrap();
    let proof = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    let input = serde_json::from_value(json!({"slug":"b","name":"Synthetic B"})).unwrap();
    assert!(
        owner
            .create_company(&cookies, &Fixed(NOW), &proof, &input)
            .unwrap()
            .outcome
            .is_ok()
    );
    owner.select_company("a").unwrap();
    (dir, owner, cookies, proof, uid)
}
fn identity() -> ParserIdentity {
    ParserIdentity {
        parser_id: "pdfium".into(),
        parser_version: "synthetic-adapter-1".into(),
        contract_version: pdf::NATIVE_CONTRACT.into(),
    }
}
fn register(owner: &mut Owner, proof: &VerifiedCookie) -> i64 {
    let bytes = rentemester_runtime::document::Prepared::new(
        "synthetic.pdf".into(),
        b"%PDF-1.4\nSynthetic original PDF unit fixture\n".to_vec(),
    )
    .unwrap();
    let metadata = serde_json::from_value(
        json!({"source":"synthetic-pdf","documentType":"cash_register_receipt"}),
    )
    .unwrap();
    owner
        .ingest_document(
            proof,
            &Fixed(NOW),
            rentemester_application::documents::Ingest {
                file: &bytes,
                metadata: &metadata,
                today: "2026-01-01",
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .value["document"]["id"]
        .as_i64()
        .unwrap()
}
fn prepare(owner: &mut Owner, proof: &VerifiedCookie, id: i64) -> Admission {
    let attempt = owner
        .prepare_document_pdf(proof, &Fixed(NOW), id, identity())
        .unwrap();
    assert!(!attempt.retired);
    attempt.outcome.unwrap().value
}
fn output(admission: &Admission) -> Completion {
    let original = rentemester_runtime::document::Prepared::new(
        "synthetic.pdf".into(),
        admission.worker_input().unwrap().to_vec(),
    )
    .unwrap();
    let source = original.hash();
    let result: ParseResult = serde_json::from_value(json!({"contractVersion":pdf::NATIVE_CONTRACT,"inputSha256":source,"status":"ok","errorCode":null,"pages":[{"pageNumber":1,"width":612,"height":792,"rotation":0,"text":"Synthetic Æ Ω 12.34","layout":[{"text":"Synthetic Æ Ω 12.34","x":20,"y":700,"width":120,"height":12,"font":"Synthetic embedded font"}]}]})).unwrap();
    Completion {
        identity: identity(),
        outcome: Ok(result),
    }
}
fn finish(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    admission: Admission,
    result: Option<Completion>,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    owner
        .finish_document_pdf(proof, &Fixed(NOW), admission, result)
        .unwrap()
        .outcome
        .map(|value| value.value)
}
fn read(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    action: Read,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    owner
        .document_pdf(proof, &Fixed(NOW), action)
        .unwrap()
        .outcome
        .map(|value| value.value)
}
fn count(owner: &Owner, table: &str) -> i64 {
    owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
fn original(owner: &Owner, id: i64) -> std::path::PathBuf {
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let path: String = db
        .query_row(
            "SELECT stored_path FROM documents WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    owner.root.join("a").join(path)
}
#[test]
fn parsed_read_cache_and_pending_are_source_bound_and_reads_have_no_side_effects() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let pending = read(
        &mut owner,
        &proof,
        Read::Pending {
            identity: identity(),
            cursor: 0,
            limit: 10,
        },
    )
    .unwrap();
    assert_eq!(pending["documentIds"], json!([id]));
    let admission = prepare(&mut owner, &proof, id);
    let completion = output(&admission);
    let first = finish(&mut owner, &proof, admission, Some(completion)).unwrap();
    assert_eq!(first["parse"]["cached"], false);
    assert_eq!(count(&owner, "document_pdf_parse_results"), 1);
    assert_eq!(count(&owner, "document_pdf_parse_pages"), 1);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 1);
    let status = read(&mut owner, &proof, Read::Status(id)).unwrap();
    assert_eq!(status["parse"]["status"], "ok");
    assert_eq!(status["parse"]["parserId"], "pdfium");
    let text = read(
        &mut owner,
        &proof,
        Read::Text {
            document_id: id,
            offset: 0,
            limit: 1,
        },
    )
    .unwrap();
    assert_eq!(text["pages"][0]["text"], "Synthetic Æ Ω 12.34");
    assert!(text["pages"][0].get("layout").is_none());
    assert!(text["pages"][0]["layoutHash"].as_str().unwrap().len() == 64);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 1);
    let cached = prepare(&mut owner, &proof, id);
    assert!(cached.worker_input().is_none());
    let result = finish(&mut owner, &proof, cached, None).unwrap();
    assert_eq!(result["parse"]["cached"], true);
    assert_eq!(result["parse"]["resultHash"], first["parse"]["resultHash"]);
    assert_eq!(count(&owner, "document_pdf_parse_results"), 1);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 2);
    assert_eq!(
        read(
            &mut owner,
            &proof,
            Read::Pending {
                identity: identity(),
                cursor: 0,
                limit: 10
            }
        )
        .unwrap()["documentIds"],
        json!([])
    );
    assert!(
        read(
            &mut owner,
            &proof,
            Read::Text {
                document_id: id,
                offset: 0,
                limit: 11
            }
        )
        .is_err()
    );
    assert_eq!(count(&owner, "journal_entries"), 0);
}
#[test]
fn owner_mutex_and_sql_are_released_during_worker_and_current_revoke_blocks_final() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let shared = std::sync::Mutex::new(owner);
    let admission = {
        let mut owner = shared.lock().unwrap();
        prepare(&mut owner, &proof, id)
    };
    let completion = output(&admission);
    {
        let owner = shared.try_lock().unwrap();
        for path in [
            owner.root.join(".rentemester/workspace-control.sqlite"),
            owner.root.join("a/data/ledger.sqlite"),
        ] {
            let db = Connection::open(path).unwrap();
            db.busy_timeout(std::time::Duration::ZERO).unwrap();
            db.execute_batch("BEGIN IMMEDIATE;ROLLBACK").unwrap();
        }
        owner.held.0.as_ref().unwrap().control.as_ref().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:synthetic-revoker');").unwrap();
    }
    let mut owner = shared.lock().unwrap();
    assert_eq!(
        finish(&mut owner, &proof, admission, Some(completion))
            .err()
            .unwrap()
            .error
            .code,
        "access_denied"
    );
    assert_eq!(count(&owner, "document_pdf_parse_results"), 0);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 0);
    assert!(read(&mut owner, &proof, Read::Status(id)).is_err());
}
#[test]
fn worker_failure_and_source_failure_commit_only_attempts_and_terminal_never_duplicates() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let admission = prepare(&mut owner, &proof, id);
    let result = finish(
        &mut owner,
        &proof,
        admission,
        Some(Completion {
            identity: identity(),
            outcome: Err(OperationalError::Timeout),
        }),
    )
    .unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(result["errorCode"], "timeout");
    assert_eq!(
        read(&mut owner, &proof, Read::Status(id)).unwrap()["parse"],
        Value::Null
    );
    assert_eq!(count(&owner, "document_pdf_parse_results"), 0);
    std::fs::remove_file(original(&owner, id)).unwrap();
    let admission = prepare(&mut owner, &proof, id);
    assert!(admission.worker_input().is_none());
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 2);
    let result = finish(&mut owner, &proof, admission, None).unwrap();
    assert_eq!(result["errorCode"], "source_unavailable");
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 2);
    assert!(read(&mut owner, &proof, Read::Status(id)).is_err());
}
#[test]
fn source_changed_after_worker_is_tamper_and_cannot_publish_or_hide_in_read() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let admission = prepare(&mut owner, &proof, id);
    let completion = output(&admission);
    std::fs::write(original(&owner, id), b"%PDF-1.4\nChanged physical original").unwrap();
    assert_eq!(
        finish(&mut owner, &proof, admission, Some(completion))
            .err()
            .unwrap()
            .error
            .code,
        "PDF_EVIDENCE_TAMPERED"
    );
    assert_eq!(count(&owner, "document_pdf_parse_results"), 0);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 0);
    assert_eq!(
        read(&mut owner, &proof, Read::Status(id))
            .err()
            .unwrap()
            .error
            .code,
        "PDF_EVIDENCE_TAMPERED"
    );
}
#[test]
fn malformed_new_worker_payload_or_identity_is_retained_protocol_failure_not_result() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let admission = prepare(&mut owner, &proof, id);
    let mut completion = output(&admission);
    completion.identity.parser_version = "different-worker".into();
    assert_eq!(
        finish(&mut owner, &proof, admission, Some(completion)).unwrap()["errorCode"],
        "protocol_violation"
    );
    let admission = prepare(&mut owner, &proof, id);
    let mut completion = output(&admission);
    completion.outcome.as_mut().unwrap().input_sha256 = "f".repeat(64);
    assert_eq!(
        finish(&mut owner, &proof, admission, Some(completion)).unwrap()["errorCode"],
        "protocol_violation"
    );
    assert_eq!(count(&owner, "document_pdf_parse_results"), 0);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 2);
}
#[test]
fn concurrent_canonical_winner_is_reused_without_duplicate_results() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let left = prepare(&mut owner, &proof, id);
    let right = prepare(&mut owner, &proof, id);
    let l = output(&left);
    let r = output(&right);
    let first = finish(&mut owner, &proof, left, Some(l)).unwrap();
    let second = finish(&mut owner, &proof, right, Some(r)).unwrap();
    assert_eq!(second["parse"]["cached"], true);
    assert_eq!(first["parse"]["resultHash"], second["parse"]["resultHash"]);
    assert_eq!(count(&owner, "document_pdf_parse_results"), 1);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 2);
}
#[test]
fn result_pages_attempt_and_audit_roll_back_together_on_storage_failure() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let admission = prepare(&mut owner, &proof, id);
    let completion = output(&admission);
    let before = count(&owner, "audit_log");
    owner.held.0.as_ref().unwrap().company.as_ref().unwrap().execute_batch("CREATE TEMP TRIGGER synthetic_pdf_audit_failure BEFORE INSERT ON audit_log WHEN NEW.event_type='document_pdf_parse_result' BEGIN SELECT RAISE(ABORT,'synthetic PDF audit fault'); END;").unwrap();
    assert!(finish(&mut owner, &proof, admission, Some(completion)).is_err());
    assert_eq!(count(&owner, "document_pdf_parse_results"), 0);
    assert_eq!(count(&owner, "document_pdf_parse_pages"), 0);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 0);
    assert_eq!(count(&owner, "audit_log"), before);
    assert_eq!(count(&owner, "documents"), 1);
}
#[test]
fn another_selected_ledger_or_authenticated_subject_cannot_adopt_worker_output() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let admission = prepare(&mut owner, &proof, id);
    let completion = output(&admission);
    owner.select_company("b").unwrap();
    assert_eq!(
        finish(&mut owner, &proof, admission, Some(completion))
            .err()
            .unwrap()
            .error
            .code,
        "document_pdf_admission_changed"
    );
    owner.select_company("a").unwrap();
    let admission = prepare(&mut owner, &proof, id);
    let completion = output(&admission);
    owner.held.0.as_ref().unwrap().control.as_ref().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('v','Synthetic Other','v@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('fv','synthetic','[]','v',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('sv','v','other-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('v','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('v','a','grant','owner','user:u');").unwrap();
    let cookie = cookies.issue_session("other-token").unwrap();
    let other = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    assert_eq!(
        finish(&mut owner, &other, admission, Some(completion))
            .err()
            .unwrap()
            .error
            .code,
        "document_pdf_admission_changed"
    );
    assert_eq!(count(&owner, "document_pdf_parse_results"), 0);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 0);
}

// Source-bound bridge, deliberately independent of any runtime JS/provider.
const ORIGINAL_BENCHMARK_SHA: &str =
    "d418adf201b5b237a3934778c5fac60f7303f5b683c6a1d3c16380517ab583b7";
const ORIGINAL_GENERATOR_SHA: &str =
    "368a0a48b59d97c951347676a18906df4b0a3ceaa50f09d7947a474b7aefe0ca";
const ORIGINAL_INPUT_SHA: [&str; 9] = [
    "23bb51253c61e1b77138eaa462bd7fac197623cef8cad233ecfaa4f11be0525a",
    "1c3b72c640c8fd05fab30097ebac93740d9ff447c37d07762c50a77152fdaa3e",
    "7dc9b0817eafb7e56147ef06bce969df63199258c1e49dfdb56315cbec28a057",
    "c16032c9cef4e7b2e71bbdddd3878cbc748e0fdd3c003149dcfe601507e0146f",
    "9da71021b8402c84e5c63f918ae898baf89b2ea9c30dc235740b17b210cec0f5",
    "0f736e85c95aadfba6fc7e19d07bb5eea4ffa3ae2655d16b4bc3aa3f6a017df0",
    "9bfad7a126f0dd483eb5e66fc05ccfafb64735188c76b349e3f76b4a5802c508",
    "d2c5679ed7960eb9d8b3e27e3b941c415dc70f2f5c279b7fa5d984cbf7297884",
    "6726115684c9e0b3bcb0d8c129d040fbeb993e451566e45a45d0cbdebb34329d",
];
fn original_manifest() -> Value {
    json!({"schema":"rentemester-original-pdf-benchmark-v1", "sources":[
        {"path":"scripts/benchmark-pdf-parser.ts","sha256":ORIGINAL_BENCHMARK_SHA,"byteLength":7633},
        {"path":"tests/fixtures/pdf-parser/synthetic-text-pdf.ts","sha256":ORIGINAL_GENERATOR_SHA,"byteLength":1516}
    ],"expectedText":"Synthetic parser evidence\nInvoice SYN-001","width":612,"height":792,
    "inputs":ORIGINAL_INPUT_SHA.iter().enumerate().map(|(i, sha)|json!({"ordinal":i+1,"file":format!("benchmark-{}.pdf",i+1),"sha256":sha,"byteLength":if i==3 {40}else{650},"status":if i==3 {"malformed_pdf"}else{"ok"}})).collect::<Vec<_>>()})
}
fn verified_original_corpus(
    root: &std::path::Path,
) -> Vec<rentemester_runtime::document::Prepared> {
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest,
        original_manifest(),
        "unknown schema, changed source or corpus refused"
    );
    (0..9)
        .map(|i| {
            let file = format!("benchmark-{}.pdf", i + 1);
            let bytes = std::fs::read(root.join(&file)).unwrap();
            assert_eq!(bytes.len(), if i == 3 { 40 } else { 650 });
            let prepared = rentemester_runtime::document::Prepared::new(file, bytes).unwrap();
            assert_eq!(prepared.hash(), ORIGINAL_INPUT_SHA[i], "corpus tamper");
            prepared
        })
        .collect()
}
#[test]
fn original_benchmark_source_pins_and_unknown_manifest_control() {
    for (bytes, sha) in [
        (
            include_bytes!("../../../../scripts/benchmark-pdf-parser.ts").as_slice(),
            ORIGINAL_BENCHMARK_SHA,
        ),
        (
            include_bytes!("../../../../tests/fixtures/pdf-parser/synthetic-text-pdf.ts")
                .as_slice(),
            ORIGINAL_GENERATOR_SHA,
        ),
    ] {
        assert_eq!(
            rentemester_runtime::document::Prepared::new("source.txt".into(), bytes.to_vec())
                .unwrap()
                .hash(),
            sha
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let mut unknown = original_manifest();
    unknown["extra"] = json!(true);
    std::fs::write(
        dir.path().join("manifest.json"),
        serde_json::to_vec(&unknown).unwrap(),
    )
    .unwrap();
    assert!(std::panic::catch_unwind(|| verified_original_corpus(dir.path())).is_err());
    std::fs::write(
        dir.path().join("manifest.json"),
        serde_json::to_vec(&original_manifest()).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.path().join("benchmark-1.pdf"), vec![b'x'; 650]).unwrap();
    assert!(std::panic::catch_unwind(|| verified_original_corpus(dir.path())).is_err());
}

/// Opt-in actual native qualification. Four-way test harness, NOT HTTP batch proof.
#[test]
#[ignore = "requires explicit RM_PDF_WORKER_BUNDLE, RM_PDF_BENCHMARK_CORPUS and new RM_PDF_BENCHMARK_REPORT"]
fn actual_original_nine_pinned_bundle_host_benchmark() {
    use std::{io::Write, path::PathBuf, time::Instant};
    let bundle =
        PathBuf::from(std::env::var_os("RM_PDF_WORKER_BUNDLE").expect("explicit bundle required"));
    let corpus = PathBuf::from(
        std::env::var_os("RM_PDF_BENCHMARK_CORPUS").expect("explicit corpus required"),
    );
    let report = PathBuf::from(
        std::env::var_os("RM_PDF_BENCHMARK_REPORT").expect("explicit new report required"),
    );
    assert!(bundle.is_absolute() && corpus.is_absolute() && report.is_absolute());
    let mut report_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(report)
        .unwrap();
    // Leave an honest incomplete receipt if qualification aborts; never overwrite earlier evidence.
    report_file
        .write_all(b"{\"qualification\":\"incomplete\"}\n")
        .unwrap();
    let config = rentemester_runtime::document_pdf::Config::new(bundle).unwrap();
    let ident = config.identity().clone();
    let files = verified_original_corpus(&corpus);
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let metadata = serde_json::from_value(
        json!({"source":"synthetic-pdf","documentType":"cash_register_receipt"}),
    )
    .unwrap();
    let ids: Vec<i64> = files
        .iter()
        .map(|file| {
            owner
                .ingest_document(
                    &proof,
                    &Fixed(NOW),
                    rentemester_application::documents::Ingest {
                        file,
                        metadata: &metadata,
                        today: "2026-01-01",
                    },
                )
                .unwrap()
                .outcome
                .unwrap()
                .value["document"]["id"]
                .as_i64()
                .unwrap()
        })
        .collect();
    assert_eq!(ids, (1..=9).collect::<Vec<_>>());
    let mut resumed = Vec::new();
    let mut cursors = Vec::new();
    for cursor in [0, 4, 8] {
        let page = read(
            &mut owner,
            &proof,
            Read::Pending {
                identity: ident.clone(),
                cursor,
                limit: 4,
            },
        )
        .unwrap();
        resumed.extend(
            page["documentIds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_i64().unwrap()),
        );
        cursors.push(page["nextCursor"].clone());
    }
    assert_eq!(resumed, ids);
    assert_eq!(cursors, json!([4, 8, null]).as_array().unwrap().clone());
    let cold_start = Instant::now();
    let mut cold = Vec::new();
    let mut native_results = Vec::new();
    for chunk in ids.chunks(4) {
        let admissions: Vec<_> = chunk
            .iter()
            .map(|id| {
                owner
                    .prepare_document_pdf(&proof, &Fixed(NOW), *id, ident.clone())
                    .unwrap()
                    .outcome
                    .unwrap()
                    .value
            })
            .collect();
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = admissions
                .iter()
                .map(|a| {
                    let config = &config;
                    scope.spawn(move || {
                        config
                            .parse(a.worker_input().expect("cold source required"))
                            .unwrap()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        });
        for ((id, admission), result) in chunk.iter().zip(admissions).zip(results) {
            let i = (*id - 1) as usize;
            assert_eq!(result.input_sha256, ORIGINAL_INPUT_SHA[i]);
            assert_eq!(
                serde_json::to_value(result.status).unwrap(),
                original_manifest()["inputs"][i]["status"]
            );
            if i == 3 {
                assert!(result.pages.is_empty());
            } else {
                assert_eq!(result.pages.len(), 1);
                assert_eq!(
                    result.pages[0].text,
                    "Synthetic parser evidence\nInvoice SYN-001"
                );
                assert_eq!(
                    (result.pages[0].width, result.pages[0].height),
                    (612.0, 792.0)
                );
            }
            native_results.push(serde_json::to_value(&result).unwrap());
            let value = finish(
                &mut owner,
                &proof,
                admission,
                Some(Completion {
                    identity: ident.clone(),
                    outcome: Ok(result),
                }),
            )
            .unwrap();
            assert_eq!(value["parse"]["cached"], false);
            assert_eq!(value["parse"]["documentId"], *id);
            cold.push(value);
        }
    }
    let cold_ms = cold_start.elapsed().as_secs_f64() * 1000.0;
    let warm_start = Instant::now();
    let mut warm = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        let admission = owner
            .prepare_document_pdf(&proof, &Fixed(NOW), *id, ident.clone())
            .unwrap()
            .outcome
            .unwrap()
            .value;
        assert!(admission.worker_input().is_none(), "warm must not launch");
        let value = finish(&mut owner, &proof, admission, None).unwrap();
        assert_eq!(value["parse"]["cached"], true);
        assert_eq!(value["parse"]["resultHash"], cold[i]["parse"]["resultHash"]);
        let text = read(
            &mut owner,
            &proof,
            Read::Text {
                document_id: *id,
                offset: 0,
                limit: 1,
            },
        )
        .unwrap();
        assert_eq!(text["parse"]["sourceSha256"], ORIGINAL_INPUT_SHA[i]);
        assert_eq!(text["parse"]["parserId"], ident.parser_id);
        assert_eq!(text["parse"]["parserVersion"], ident.parser_version);
        assert_eq!(text["parse"]["contractVersion"], ident.contract_version);
        if i != 3 {
            assert_eq!(
                text["pages"][0]["text"],
                native_results[i]["pages"][0]["text"]
            );
            assert_eq!(text["pages"][0]["width"], 612.0);
            assert_eq!(text["pages"][0]["height"], 792.0);
        }
        warm.push(json!({"result":value,"read":text}));
    }
    let warm_ms = warm_start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(count(&owner, "journal_entries"), 0);
    assert_eq!(count(&owner, "documents"), 9);
    assert_eq!(count(&owner, "document_pdf_parse_results"), 9);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 18);
    let pending = read(
        &mut owner,
        &proof,
        Read::Pending {
            identity: ident.clone(),
            cursor: 0,
            limit: 10,
        },
    )
    .unwrap();
    assert_eq!(pending["documentIds"], json!([]));
    let receipt = json!({"qualification":"completed","scope":"actual Config supervisor + private synthetic host prepare/finish/read; four-way test harness is not HTTP batch proof","manifest":original_manifest(),"identity":{"parserId":ident.parser_id,"parserVersion":ident.parser_version,"contractVersion":ident.contract_version},"resumeCursors":cursors,"orderedDocumentIds":ids,"cold":cold,"nativeResults":native_results,"warm":warm,"pendingAfter":pending,"observations":{"successfulNativeSupervisorCalls":native_results.len(),"warmSupervisorCalls":0,"coldMs":cold_ms,"warmIncludingReadsMs":warm_ms,"successfulDocumentsPerSecond":8.0/(cold_ms/1000.0),"pagesPerSecond":8.0/(cold_ms/1000.0)},"limitations":["No child-launch or max-active telemetry is exposed by current Config; successful parse calls each execute one child in this implementation. Harness width is not measured active children or HTTP batch concurrency.","Ordinary synthetic corpus does not qualify hostile peak memory. Mac RSS sampled at 10ms can overshoot 512MiB; not an exact hard cap. Linux worker AS cap is 512MiB.","PDFium loose character boxes differ semantically from historical PDFjs text-item layout; historical identity and bytes are not relabeled."]});
    report_file.set_len(0).unwrap();
    use std::io::{Seek, SeekFrom};
    report_file.seek(SeekFrom::Start(0)).unwrap();
    serde_json::to_writer_pretty(&mut report_file, &receipt).unwrap();
    report_file.write_all(b"\n").unwrap();
    report_file.sync_all().unwrap();
}
