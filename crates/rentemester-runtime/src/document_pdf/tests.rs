use super::*;
use std::os::unix::fs::PermissionsExt;

fn script(body: &str) -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("worker");
    std::fs::write(&path, format!("#!/usr/bin/python3\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    (root, path)
}
fn run(body: &str, deadline: Duration, rss: u64) -> Result<Vec<u8>, OperationalError> {
    let (_root, path) = script(body);
    execute(&path, b"original", Instant::now(), deadline, rss)
}
#[test]
fn actual_child_pipes_clear_environment_and_do_not_inherit_working_directory() {
    let result=run("import os,sys,json; data=sys.stdin.buffer.read(); print(json.dumps({'input':data.decode(),'env':dict(os.environ),'cwd':os.getcwd()}))",Duration::from_secs(3),MAX_RSS).unwrap();
    let value: Value = serde_json::from_slice(&result).unwrap();
    assert_eq!(value["input"], "original");
    assert_eq!(value["cwd"], "/");
    // Apple's /usr/bin/python3 shim synthesizes SDK variables itself. Verify environment
    // clearing independently with the native env binary (no interpreter/shim).
    let empty = execute(
        Path::new("/usr/bin/env"),
        b"",
        Instant::now(),
        Duration::from_secs(3),
        MAX_RSS,
    )
    .unwrap();
    assert!(empty.is_empty());
}
#[test]
fn actual_child_deadline_overflow_crash_and_memory_are_curated() {
    assert_eq!(
        run(
            "import time; time.sleep(5)",
            Duration::from_millis(80),
            MAX_RSS
        ),
        Err(OperationalError::Timeout)
    );
    assert_eq!(
        run(
            "import sys,time;sys.stdout.buffer.write(b'x'*16777217);sys.stdout.flush();time.sleep(5)",
            Duration::from_secs(3),
            MAX_RSS
        ),
        Err(OperationalError::StdoutLimit)
    );
    assert_eq!(
        run(
            "import sys,time;sys.stderr.buffer.write(b'x'*65537);sys.stderr.flush();time.sleep(5)",
            Duration::from_secs(3),
            MAX_RSS
        ),
        Err(OperationalError::StderrLimit)
    );
    assert_eq!(
        run("import sys;sys.exit(4)", Duration::from_secs(3), MAX_RSS),
        Err(OperationalError::Crashed)
    );
    assert_eq!(
        run(
            "import time;x=bytearray(96*1024*1024);time.sleep(5)",
            Duration::from_secs(3),
            48 * 1024 * 1024
        ),
        Err(OperationalError::MemoryLimit)
    );
}
#[test]
fn strict_provenance_and_actual_input_hash_are_required() {
    let pin = platform_artifact().unwrap();
    let identity = ParserIdentity {
        parser_id: "pdfium".into(),
        parser_version: format!(
            "151.0.7881.0+binding0.9.4.adapter1.{}",
            &pin.library_sha256[..16]
        ),
        contract_version: "document-pdf-text-pdfium-v1".into(),
    };
    let mut value = serde_json::json!({"contractVersion":identity.contract_version,"inputSha256":hash(b"source"),"status":"no_text_layer","errorCode":"no_text_layer","pages":[],"provenance":{"parserId":identity.parser_id,"parserVersion":identity.parser_version,"bindingVersion":"0.9.4","adapterVersion":"1","artifactSha256":pin.sha256,"librarySha256":pin.library_sha256,"layoutConvention":"pdfium-character-loose-box-bottom-left-v1","limits":{"cpuSeconds":15,"addressSpaceBytes":if cfg!(target_os="linux"){Some(536870912_u64)}else{None},"wallSeconds":15,"macMemoryEnforcementQualified":false}}});
    assert!(
        decode(
            &serde_json::to_vec(&value).unwrap(),
            b"source",
            &identity,
            &pin
        )
        .is_ok()
    );
    assert_eq!(
        decode(
            &serde_json::to_vec(&value).unwrap(),
            b"changed",
            &identity,
            &pin
        ),
        Err(OperationalError::ProtocolViolation)
    );
    value["provenance"]["librarySha256"] = Value::String("0".repeat(64));
    assert_eq!(
        decode(
            &serde_json::to_vec(&value).unwrap(),
            b"source",
            &identity,
            &pin
        ),
        Err(OperationalError::ProtocolViolation)
    );
    value["provenance"]["librarySha256"] = Value::String(pin.library_sha256.clone());
    value["callerActor"] = Value::String("untrusted".into());
    assert_eq!(
        decode(
            &serde_json::to_vec(&value).unwrap(),
            b"source",
            &identity,
            &pin
        ),
        Err(OperationalError::ProtocolViolation)
    );
}
#[test]
fn config_refuses_relative_and_missing_bundle_without_launch() {
    assert!(Config::new(PathBuf::from("relative")).is_err());
    let root = tempfile::tempdir().unwrap();
    assert!(Config::new(root.path().to_owned()).is_err());
}
#[test]
fn admitted_executable_is_rechecked_before_launch() {
    let (root, worker) = script("raise Exception('must not execute')");
    let library = root.path().join("lib");
    std::fs::write(&library, b"pinned").unwrap();
    let mut pin = platform_artifact().unwrap();
    pin.library_sha256 = hash(b"pinned");
    let config = Config {
        worker: worker.clone(),
        worker_hash: file_hash(&worker, 128 * 1024 * 1024, true).unwrap(),
        library,
        pin,
        identity: ParserIdentity {
            parser_id: "pdfium".into(),
            parser_version: "test".into(),
            contract_version: "document-pdf-text-pdfium-v1".into(),
        },
    };
    std::fs::write(worker, b"changed admitted executable").unwrap();
    assert_eq!(config.parse(b"source"), Err(OperationalError::Unavailable));
}
/// Opt-in real bundle qualification, same exact local artifact as bootstrap. No download/build.
#[test]
#[ignore = "requires explicit RM_PDF_WORKER_BUNDLE pointing at pinned synthetic bootstrap bundle"]
fn actual_pinned_worker_and_identity() {
    let bundle = std::env::var_os("RM_PDF_WORKER_BUNDLE").unwrap();
    let config = Config::new(PathBuf::from(bundle)).unwrap();
    let body = synthetic_pdf();
    let result = config.parse(&body).unwrap();
    assert_eq!(result.status, rentemester_engine::document_pdf::Status::Ok);
    assert_eq!(
        result.pages[0].text,
        "Synthetic parser evidence\nInvoice SYN-001"
    );
    assert_eq!(
        (result.pages[0].width, result.pages[0].height),
        (612.0, 792.0)
    );
    assert!(result.pages[0].layout.len() > 2);
    assert!(
        result.pages[0]
            .layout
            .iter()
            .any(|item| item.width > 0.0 && item.height > 0.0)
    );
    let parsed = config.parse(b"%PDF-1.4\nmalformed\n").unwrap();
    assert_eq!(
        parsed.status,
        rentemester_engine::document_pdf::Status::MalformedPdf
    );
    assert_eq!(parsed.input_sha256, hash(b"%PDF-1.4\nmalformed\n"));
}

fn synthetic_pdf() -> Vec<u8> {
    let stream = "BT\n/F1 18 Tf\n72 720 Td\n(Synthetic parser evidence) Tj\n0 -28 Td\n(Invoice SYN-001) Tj\nET\n";
    let objects=["<< /Type /Catalog /Pages 2 0 R >>".to_string(),"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),format!("<< /Length {} >>\nstream\n{}endstream",stream.len(),stream),"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, object).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    pdf
}
