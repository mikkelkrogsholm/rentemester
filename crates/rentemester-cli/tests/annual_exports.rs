use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Output},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
const XHTML: &str = "<html>synthetic</html>";
const HASH: &str = "5be8790e8abd97a38ff67eefebc2bc3321a1d72d60d8139aa4c27a28284f2d03";
fn directory(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("rm-annual135-{}-{name}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path.join("cookie"))
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    path
}
fn server(value: Value) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = vec![];
        while !request.ends_with(b"\r\n\r\n") {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&b[..n]);
        }
        let body = value.to_string();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        String::from_utf8(request).unwrap()
    });
    (origin, task)
}
fn call(kind: &str, base: &str, dir: &Path, extra: &[&str]) -> Output {
    Command::new(BIN)
        .args([
            "report",
            kind,
            "--server",
            base,
            "--cookie-file",
            dir.join("cookie").to_str().unwrap(),
            "--company",
            "synthetic",
            "--from",
            "2020-01-01",
            "--to",
            "2020-12-31",
            "--allow-loopback-http",
        ])
        .args(extra)
        .output()
        .unwrap()
}
fn annual(ok: bool) -> Value {
    json!({"ok":ok,"errors":if ok{json!([])}else{json!(["Regnskabsåret er ikke låst"] )},"appliedRules":["DK-ANNUAL-REPORT-CLASS-B-001"],"fiscalYearStart":"2020-01-01","fiscalYearEnd":"2020-12-31","company":{"name":"Synthetic Company","cvr":"87654321"},"aretsResultat":25.05,"profitAndLoss":{"income":{"lines":[],"total":25.05},"expense":{"lines":[],"total":0},"totalIncome":25.05,"totalExpense":0},"balanceSheet":{"assets":{"lines":[],"total":25.05},"liabilities":{"lines":[],"total":0},"equity":{"lines":[],"total":25.05},"periodResult":25.05,"totalAssets":25.05,"totalLiabilitiesAndEquity":25.05,"balanced":true},"notes":[{"id":"accounting-policies","title":"Anvendt regnskabspraksis","body":"Synthetic skeleton","placeholder":true}],"ledelsespategning":{"text":"Synthetic management review","placeholder":true},"disclaimer":"Preparation only; owner reviews and files."})
}
fn ixbrl(ok: bool) -> Value {
    json!({"ok":true,"annualExport":{"ok":ok,"errors":if ok{json!([])}else{json!(["annual prerequisites failed"])},"annualReport":annual(ok),"xhtml":if ok{XHTML}else{""},"sha256":if ok{HASH}else{""},"taxonomy":{"name":"bounded","version":"0.2.0"},"appliedRules":["DK-ANNUAL-REPORT-IXBRL-002"]}})
}
#[test]
fn taxonomy_is_pure_local_and_usage_errors_keep_empty_stdout() {
    let out = Command::new(BIN)
        .args(["report", "annual", "--ixbrl-taxonomy", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["version"], "0.2.0");
    assert_eq!(value["elementCount"], 20);
    assert_eq!(value["elements"].as_array().unwrap().len(), 20);
    assert_eq!(value["namespace"], "urn:rentemester:dk:arsrapport:v1");
    for args in [
        vec!["report", "tax"],
        vec!["report", "tax", "--ixbrl-out", "x"],
        vec![
            "report",
            "annual",
            "--ixbrl-taxonomy",
            "--company",
            "/private/not-opened",
        ],
    ] {
        let out = Command::new(BIN).args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
    }
    let out = Command::new(BIN)
        .args(["report", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().contains("annualTax"));
}
#[test]
fn annual_without_artifact_preserves_flat_report_and_human_sections() {
    let dir = directory("annual");
    for json_mode in [true, false] {
        let expected = annual(true);
        let (origin, t) = server(json!({"ok":true,"annualReport":{"report":expected}}));
        let out = call(
            "annual",
            &origin,
            &dir,
            if json_mode { &["--json"] } else { &[] },
        );
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        if json_mode {
            assert_eq!(
                serde_json::from_slice::<Value>(&out.stdout).unwrap(),
                expected
            );
            assert!(out.stdout.ends_with(b"\n"));
        } else {
            let text = String::from_utf8(out.stdout).unwrap();
            assert!(text.contains("Årsrapport (regnskabsklasse B)"));
            assert!(text.contains("25,05 kr."));
            assert!(text.contains("Ledelsespåtegning:"));
            assert!(text.contains("Anvendt regnskabspraksis (skabelon — udfyldes)"));
        }
        assert!(t.join().unwrap().starts_with("GET /api/companies/synthetic/annual-report?fiscalYearStart=2020-01-01&fiscalYearEnd=2020-12-31 HTTP/1.1"));
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn one_snapshot_xhtml_is_hash_verified_and_existing_output_never_overwritten() {
    let dir = directory("file");
    let path = dir.join("annual.xhtml");
    for attempt in [0, 1, 2] {
        let mut response = ixbrl(true);
        if attempt == 2 {
            response["annualExport"]["xhtml"] = json!("tampered");
        }
        let (origin, t) = server(response);
        let out = call(
            "annual",
            &origin,
            &dir,
            &["--json", "--ixbrl-out", path.to_str().unwrap()],
        );
        if attempt == 0 {
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let value: Value = serde_json::from_slice(&out.stdout).unwrap();
            assert_eq!(value["company"], annual(true)["company"]);
            assert_eq!(value["ixbrl"]["path"], path.to_str().unwrap());
            assert_eq!(value["ixbrl"]["sha256"], HASH);
            assert_eq!(value["ixbrl"]["taxonomy"]["version"], "0.2.0");
            assert!(value.get("xhtml").is_none());
        } else {
            assert_eq!(out.status.code(), Some(1));
            assert!(out.stdout.is_empty());
            assert!(!out.stderr.is_empty());
        }
        assert_eq!(std::fs::read(&path).unwrap(), XHTML.as_bytes());
        let request = t.join().unwrap();
        assert!(request.starts_with("GET /api/companies/synthetic/annual-exports?"));
        assert!(request.contains("kind=ixbrl HTTP/1.1"));
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn annual_blockers_write_no_file_and_tax_preserves_json_and_human_null_review() {
    let dir = directory("blocked");
    let path = dir.join("blocked.xhtml");
    let (origin, t) = server(ixbrl(false));
    let out = call(
        "annual",
        &origin,
        &dir,
        &["--json", "--ixbrl-out", path.to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(1));
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["ixbrl"]["ok"], false);
    assert_eq!(value["ixbrl"]["path"], Value::Null);
    assert!(!path.exists());
    t.join().unwrap();
    for json_mode in [true, false] {
        let tax = json!({"ok":true,"errors":[],"appliedRules":["DK-TAX-RETURN-CORP-001"],"fiscalYearStart":"2020-01-01","fiscalYearEnd":"2020-12-31","bookkeptResult":-25.05,"adjustments":[],"totalAdjustments":0,"taxableIncome":-25.05,"corporateTax":null,"needsReview":[{"kind":"company_form_out_of_scope","label":"Unknown company form","requiredAction":"Ask advisor"}],"disclaimer":"Preparation only"});
        let (origin, t) = server(json!({"ok":true,"annualExport":tax}));
        let out = call(
            "tax",
            &origin,
            &dir,
            if json_mode { &["--json"] } else { &[] },
        );
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        if json_mode {
            assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), tax);
        } else {
            let text = String::from_utf8(out.stdout).unwrap();
            assert!(text.contains("Skattemæssig indkomst (oplysningsskema)"));
            assert!(text.contains("ikke beregnet (se needs-review)"));
            assert!(text.contains("Ask advisor"));
        }
        t.join().unwrap();
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn syntactically_supplied_invalid_bounds_are_business1_without_fabricated_figures() {
    for (kind, from, to) in [
        ("annual", "2020-02-30", "2020-12-31"),
        ("tax", "2021-01-01", "2020-12-31"),
    ] {
        let out = Command::new(BIN)
            .args([
                "report",
                kind,
                "--server",
                "http://127.0.0.1:9",
                "--cookie-file",
                "not-opened",
                "--company",
                "synthetic",
                "--from",
                from,
                "--to",
                to,
                "--json",
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1));
        assert!(out.stderr.is_empty());
        let value: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(value["ok"], false);
        assert!(!value["errors"].as_array().unwrap().is_empty());
        for field in [
            "bookkeptResult",
            "taxableIncome",
            "corporateTax",
            "profitAndLoss",
            "balanceSheet",
        ] {
            assert!(value.get(field).is_none());
        }
    }
}
