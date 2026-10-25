//! Actual binary and current native HTTP producers, not a replacement engine.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn profile() -> Value {
    json!({"id":1,"name":"Synthetic Company","country":"DK","currency":"DKK","cvr":null,"fiscalYearStartMonth":1,"fiscalYearLabelStrategy":"end-year","address":null,"postalCode":null,"city":null,"companyForm":null,"industryCode":null,"industryText":null,"cvrStatus":null,"auditWaived":null,"cvrSyncedAt":null,"vatPeriodType":"quarter","paymentTermsDays":14,"createdAt":"2026-01-01T00:00:00.000Z","payment":null})
}
fn wire<S: Read + Write>(s: &mut S, expected: &str, status: u16, result: &Value) -> Vec<u8> {
    let mut b = vec![];
    let mut end = None;
    let mut length = 0;
    loop {
        let mut buf = [0; 4096];
        let n = s.read(&mut buf).unwrap();
        assert!(n > 0);
        b.extend_from_slice(&buf[..n]);
        if end.is_none() {
            if let Some(i) = b.windows(4).position(|w| w == b"\r\n\r\n") {
                end = Some(i + 4);
                for l in String::from_utf8_lossy(&b[..i]).lines() {
                    if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                }
            }
        }
        if end.is_some_and(|i| b.len() >= i + length) {
            break;
        }
    }
    let text = String::from_utf8_lossy(&b);
    assert!(text.starts_with(expected), "{text}");
    if expected.starts_with("GET /api/protocol ") {
        assert!(!text.to_ascii_lowercase().contains("cookie:"));
    } else {
        assert!(
            text.to_ascii_lowercase()
                .contains("cookie: session=synthetic")
        );
        assert!(
            text.to_ascii_lowercase()
                .contains("x-rentemester-manifest:")
        );
    }
    let body = result.to_string();
    write!(s,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection:close\r\n\r\n{body}",body.len()).unwrap();
    b[end.unwrap()..].to_vec()
}
fn bounded_accept(l: &TcpListener) -> Option<std::net::TcpStream> {
    let at = std::time::Instant::now();
    loop {
        match l.accept() {
            Ok((s, _)) => {
                s.set_nonblocking(false).unwrap();
                return Some(s);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if at.elapsed() > std::time::Duration::from_secs(5) {
                    return None;
                }
                thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => panic!("{e}"),
        }
    }
}
fn run_fixture(
    args: &[&str],
    steps: Vec<(&'static str, u16, Value)>,
) -> (std::process::Output, Vec<Vec<u8>>) {
    use std::os::unix::fs::OpenOptionsExt;
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let cookie = std::env::temp_dir().join(format!(
        "rm-company716-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    l.set_nonblocking(true).unwrap();
    let task = thread::spawn(move || {
        let manifest: Value =
            serde_json::from_str(include_str!("../../../protocol/http-v1.json")).unwrap();
        assert_eq!(manifest["operations"].as_array().unwrap().len(), 701);
        let mut bodies = vec![];
        for (path, status, v) in steps {
            let Some(mut s) = bounded_accept(&l) else {
                return bodies;
            };
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            wire(&mut s, "GET /api/protocol HTTP/1.1", 200, &manifest);
            let Some(mut s) = bounded_accept(&l) else {
                return bodies;
            };
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            bodies.push(wire(&mut s, path, status, &v));
        }
        bodies
    });
    let out = Command::new(BIN)
        .args(["company"])
        .args(args)
        .args([
            "--server",
            &origin,
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--allow-loopback-http",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(cookie).unwrap();
    (out, task.join().unwrap())
}
#[test]
fn five_original_keys_body_projection_and_streams() {
    let (out, _) = run_fixture(
        &["list", "--json"],
        vec![(
            "GET /api/companies HTTP/1.1",
            200,
            json!({"ok":true,"count":1,"companies":[{"slug":"synthetic","name":"Synthetic Company","role":"owner","archived":false,"createdAt":"2026-01-01T00:00:00.000Z"}]}),
        )],
    );
    assert!(out.status.success(), "{:?}", out);
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        json!({"ok":true,"message":"1 company: synthetic","workspace":null,"count":1,"companies":[{"slug":"synthetic","name":"Synthetic Company","archived":false,"createdAt":"2026-01-01T00:00:00.000Z"}]})
    );
    let (out, bodies) = run_fixture(
        &[
            "add",
            "--name",
            "Synthetic Company",
            "--fiscal-year-start-month",
            "4",
            "--json",
        ],
        vec![
            (
                "POST /api/companies HTTP/1.1",
                201,
                json!({"ok":true,"company":{"slug":"synthetic-company","name":"Synthetic Company"}}),
            ),
            (
                "GET /api/companies/synthetic-company/company HTTP/1.1",
                200,
                json!({"ok":true,"company":({let mut p=profile();p["fiscalYearStartMonth"]=json!(4);p})}),
            ),
        ],
    );
    assert!(out.status.success(), "{:?}", out);
    assert_eq!(
        serde_json::from_slice::<Value>(&bodies[0]).unwrap(),
        json!({"name":"Synthetic Company","fiscalYearStartMonth":"4"})
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["workspaceCreated"], false);
    assert_eq!(v["hasPaymentDetails"], false);
    assert!(
        String::from_utf8_lossy(&out.stderr).starts_with("ADVARSEL — ingen betalingsoplysninger:")
    );
    let (out, _) = run_fixture(
        &["profile", "--company", "synthetic", "--format", "human"],
        vec![(
            "GET /api/companies/synthetic/company HTTP/1.1",
            200,
            json!({"ok":true,"company":profile()}),
        )],
    );
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("Navn: Synthetic Company"));
    assert!(text.contains("Momsperiode: kvartal"));
    assert!(out.stderr.is_empty());
    let mut after = profile();
    after["address"] = json!("Synthetic Road");
    after["paymentTermsDays"] = json!(0);
    after["vatPeriodType"] = Value::Null;
    let (out, b) = run_fixture(
        &[
            "set-profile",
            "--company",
            "synthetic",
            "--address",
            "Synthetic Road",
            "--payment-terms",
            "0",
            "--no-vat",
            "--json",
        ],
        vec![
            (
                "GET /api/companies/synthetic/company HTTP/1.1",
                200,
                json!({"ok":true,"company":profile()}),
            ),
            (
                "PATCH /api/companies/synthetic/company HTTP/1.1",
                200,
                json!({"ok":true,"company":after}),
            ),
        ],
    );
    assert!(out.status.success(), "{:?}", out);
    assert_eq!(
        serde_json::from_slice::<Value>(&b[1]).unwrap(),
        json!({"address":"Synthetic Road","paymentTermsDays":0,"vatPeriodType":null})
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["updatedFields"],
        json!(["address", "paymentTermsDays", "vatPeriodType"])
    );
    assert_eq!(v["vatRegistered"], false);
    let company = json!({"cvr":"12345678","name":"Synthetic Company","address":null,"postalCode":null,"city":null,"municipalityCode":null,"companyFormCode":null,"companyFormShort":null,"companyFormLong":null,"status":null,"industryCode":null,"industryText":null,"email":null,"phone":null,"website":null,"startDate":null,"fiscalYearStart":null,"fiscalYearEnd":null,"fiscalYearStartMonth":4,"auditWaived":null,"shareCapital":null,"shareCapitalCurrency":null,"employees":null,"advertisingProtected":false,"management":[]});
    let result = json!({"ok":true,"cvr":"12345678","company":company,"cached":true,"updatedFields":[],"errors":[],"fiscalYearStartMonth":{"current":1,"cvr":4,"matches":false}});
    let (out, b) = run_fixture(
        &["sync-cvr", "--company", "synthetic", "--json"],
        vec![(
            "POST /api/companies/synthetic/sync-cvr HTTP/1.1",
            200,
            json!({"ok":true,"sync":result}),
        )],
    );
    assert!(out.status.success());
    assert_eq!(b[0], b"{}");
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        result
    );
}
#[test]
fn refusal_authority_and_unknown_one_post_no_retry() {
    for reply in [
        json!({"ok":true,"company":{"slug":"different","name":"Wrong"}}),
        Value::Null,
    ] {
        let (out, b) = run_fixture(
            &["add", "--name", "Synthetic Company", "--json"],
            vec![("POST /api/companies HTTP/1.1", 201, reply)],
        );
        assert_eq!(out.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&out.stdout).contains("outcome_unknown"));
        assert_eq!(b.len(), 1);
    }
    let (out, b) = run_fixture(
        &["sync-cvr", "--company", "synthetic", "--json"],
        vec![(
            "POST /api/companies/synthetic/sync-cvr HTTP/1.1",
            403,
            json!({"ok":false,"code":"access_denied"}),
        )],
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(b.len(), 1);
    assert!(String::from_utf8_lossy(&out.stdout).contains("server_refused"));
    let (out, b) = run_fixture(
        &[
            "set-profile",
            "--company",
            "synthetic",
            "--actor",
            "user:forged",
            "--address",
            "New",
            "--json",
        ],
        vec![(
            "GET /api/me HTTP/1.1",
            200,
            json!({"ok":true,"user":{"id":"current"}}),
        )],
    );
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(b.len(), 1);
    assert!(String::from_utf8_lossy(&out.stdout).contains("actor differs"));
    let (out, _) = run_fixture(
        &["sync-cvr", "--company", "synthetic", "--json"],
        vec![(
            "POST /api/companies/synthetic/sync-cvr HTTP/1.1",
            200,
            json!({"ok":true,"sync":{"ok":false,"errors":["Missing CVR credentials"],"updatedFields":[]}}),
        )],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("Missing CVR credentials"));
}
#[test]
fn parse_and_missing_runtime_are_no_effect_and_example_is_offline() {
    for args in [
        vec!["add", "--name", "Synthetic", "--confirm", "yes"],
        vec!["profile"],
        vec!["list", "--workspace", "/nonexistent"],
        vec!["set-profile", "--company", "synthetic", "--no-vat=true"],
    ] {
        let o = Command::new(BIN)
            .arg("company")
            .args(args)
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(2));
        assert!(o.stdout.is_empty());
    }
    for flag in ["--help", "--example"] {
        let o = Command::new(BIN)
            .args([
                "company",
                "set-profile",
                flag,
                "--local-profile",
                "/never-opened",
            ])
            .output()
            .unwrap();
        assert!(o.status.success());
        assert!(String::from_utf8_lossy(&o.stdout).contains("--bank-account"));
    }
}
#[test]
fn verified_unix_owner_workspace_and_passive_protocol_failure() {
    use std::os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        net::UnixListener,
    };
    let path = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("c716-{}", std::process::id()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .unwrap();
    let workspace = path.join("workspace");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&workspace)
        .unwrap();
    let l = UnixListener::bind(path.join("r.sock")).unwrap();
    std::fs::set_permissions(path.join("r.sock"), std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(path.join("r.json"),json!({"format":"rentemester-local-runtime/1","origin":"http://127.0.0.1:33449","socket":"r.sock","workspaceRoot":workspace,"workspaceId":"synthetic"}).to_string()).unwrap();
    std::fs::set_permissions(path.join("r.json"), std::fs::Permissions::from_mode(0o600)).unwrap();
    let cookie = path.join("cookie");
    std::fs::write(&cookie, "session=synthetic").unwrap();
    std::fs::set_permissions(&cookie, std::fs::Permissions::from_mode(0o600)).unwrap();
    let task = thread::spawn(move || {
        let manifest: Value =
            serde_json::from_str(include_str!("../../../protocol/http-v1.json")).unwrap();
        let (mut s, _) = l.accept().unwrap();
        wire(&mut s, "GET /api/protocol HTTP/1.1", 200, &manifest);
        let (mut s, _) = l.accept().unwrap();
        wire(
            &mut s,
            "GET /api/companies HTTP/1.1",
            200,
            &json!({"ok":true,"companies":[],"count":0}),
        );
        let (mut s, _) = l.accept().unwrap();
        wire(
            &mut s,
            "GET /api/protocol HTTP/1.1",
            200,
            &json!({"format":"bad"}),
        );
    });
    let call = |args: &[&str]| {
        Command::new(BIN)
            .arg("company")
            .args(args)
            .args([
                "--local-profile",
                path.to_str().unwrap(),
                "--cookie-file",
                cookie.to_str().unwrap(),
                "--json",
            ])
            .output()
            .unwrap()
    };
    let o = call(&["list", "--workspace", workspace.to_str().unwrap()]);
    assert!(o.status.success(), "{:?}", o);
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap(),
        json!({"ok":true,"message":"Workspace has no companies yet","workspace":null,"count":0,"companies":[]})
    );
    let o = call(&["add", "--name", "Synthetic"]);
    assert_eq!(o.status.code(), Some(1));
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("protocol"));
    assert!(!text.contains("outcome_unknown"));
    task.join().unwrap();
    let o = call(&["list", "--workspace", "/wrong"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stdout).contains("binding refused"));
    std::fs::set_permissions(path.join("r.json"), std::fs::Permissions::from_mode(0o644)).unwrap();
    let o = call(&["list"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stdout).contains("binding refused"));
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn full_partial_patch_and_mismatched_receipt_are_not_silently_accepted() {
    let mut after = profile();
    after["cvr"] = json!("DK12345678");
    after["address"] = json!("Street");
    after["postalCode"] = json!("1234");
    after["city"] = json!("City");
    after["paymentTermsDays"] = json!(365);
    after["vatPeriodType"] = json!("month");
    after["payment"] = json!({"bankName":"Synthetic Bank","registrationNo":"1234","accountNo":"1234567890","iban":"DK0012345678901234"});
    let (out, b) = run_fixture(
        &[
            "set-profile",
            "--company",
            "synthetic",
            "--cvr",
            "12345678",
            "--address",
            "Street",
            "--postal-code",
            "1234",
            "--city",
            "City",
            "--payment-terms",
            "365",
            "--vat-period",
            "month",
            "--bank-name",
            "Synthetic Bank",
            "--bank-reg",
            "1234",
            "--bank-account",
            "1234567890",
            "--iban",
            "DK0012345678901234",
            "--json",
        ],
        vec![
            (
                "GET /api/companies/synthetic/company HTTP/1.1",
                200,
                json!({"ok":true,"company":profile()}),
            ),
            (
                "PATCH /api/companies/synthetic/company HTTP/1.1",
                200,
                json!({"ok":true,"company":after}),
            ),
        ],
    );
    assert!(out.status.success(), "{:?}", out);
    assert_eq!(
        serde_json::from_slice::<Value>(&b[1]).unwrap(),
        json!({"cvr":"12345678","address":"Street","postalCode":"1234","city":"City","paymentTermsDays":365,"vatPeriodType":"month","payment":{"bankName":"Synthetic Bank","registrationNo":"1234","accountNo":"1234567890","iban":"DK0012345678901234"}})
    );
    let (out, b) = run_fixture(
        &[
            "set-profile",
            "--company",
            "synthetic",
            "--address",
            "Expected",
            "--json",
        ],
        vec![
            (
                "GET /api/companies/synthetic/company HTTP/1.1",
                200,
                json!({"ok":true,"company":profile()}),
            ),
            (
                "PATCH /api/companies/synthetic/company HTTP/1.1",
                200,
                json!({"ok":true,"company":profile()}),
            ),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("outcome_unknown"));
    assert_eq!(b.len(), 2);
}

#[test]
fn source_list_wrapper_blank_slug_and_empty_payment_are_truthful() {
    let (out, _) = run_fixture(
        &["list", "--json"],
        vec![(
            "GET /api/companies HTTP/1.1",
            200,
            json!({"ok":true,"count":1,"companies":[{"slug":"archived","name":"Archived Synthetic","role":"owner","archived":true}]}),
        )],
    );
    assert!(out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        json!({"ok":true,"message":"1 company: archived","workspace":null,"count":1,"companies":[{"slug":"archived","name":"Archived Synthetic","archived":true}]})
    );
    for slug in ["", "   ", " synthetic-company "] {
        let (out, bodies) = run_fixture(
            &[
                "add",
                "--name",
                "Synthetic Company",
                "--slug",
                slug,
                "--json",
            ],
            vec![
                (
                    "POST /api/companies HTTP/1.1",
                    201,
                    json!({"ok":true,"company":{"slug":"synthetic-company","name":"Synthetic Company"}}),
                ),
                (
                    "GET /api/companies/synthetic-company/company HTTP/1.1",
                    200,
                    json!({"ok":true,"company":profile()}),
                ),
            ],
        );
        assert!(out.status.success(), "slug={slug:?} {out:?}");
        assert_eq!(bodies.len(), 2);
        let body: Value = serde_json::from_slice(&bodies[0]).unwrap();
        if slug.trim().is_empty() {
            assert!(body.get("slug").is_none());
        } else {
            assert_eq!(body["slug"], "synthetic-company");
        }
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap()["slug"],
            "synthetic-company"
        );
    }
    let (out, bodies) = run_fixture(
        &[
            "set-profile",
            "--company",
            "synthetic",
            "--bank-name",
            "",
            "--json",
        ],
        vec![
            (
                "GET /api/companies/synthetic/company HTTP/1.1",
                200,
                json!({"ok":true,"company":profile()}),
            ),
            (
                "PATCH /api/companies/synthetic/company HTTP/1.1",
                200,
                json!({"ok":true,"company":profile()}),
            ),
        ],
    );
    assert!(out.status.success(), "{out:?}");
    assert_eq!(bodies.len(), 2);
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["updatedFields"],
        json!([])
    );
    let (out, _) = run_fixture(
        &["list", "--format", "human"],
        vec![(
            "GET /api/companies HTTP/1.1",
            200,
            json!({"ok":true,"count":1,"companies":[{"slug":"synthetic","name":"Synthetic Company","role":"owner","archived":false,"createdAt":"2026-01-01T00:00:00.000Z"}]}),
        )],
    );
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("  Antal: 1\n"));
}

#[test]
fn creation_identity_terms_rename_and_exact_profile_readback() {
    let mut created = profile();
    created["cvr"] = json!("DK12345678");
    created["address"] = json!("Synthetic Road");
    created["postalCode"] = json!("1234");
    created["city"] = Value::Null;
    created["paymentTermsDays"] = json!(0);
    created["fiscalYearStartMonth"] = json!(4);
    created["fiscalYearLabelStrategy"] = json!("span");
    let (out, bodies) = run_fixture(
        &[
            "add",
            "--name",
            " Synthetic Company ",
            "--slug",
            " ",
            "--cvr",
            " dk12345678 ",
            "--address",
            " Synthetic Road ",
            "--postal-code",
            " 1234 ",
            "--city",
            " ",
            "--payment-terms",
            "0",
            "--fiscal-year-start-month",
            "4",
            "--fiscal-year-label-strategy",
            "span",
            "--json",
        ],
        vec![
            (
                "POST /api/companies HTTP/1.1",
                201,
                json!({"ok":true,"company":{"slug":"synthetic-company","name":"Synthetic Company"}}),
            ),
            (
                "GET /api/companies/synthetic-company/company HTTP/1.1",
                200,
                json!({"ok":true,"company":created}),
            ),
        ],
    );
    assert!(out.status.success(), "{out:?}");
    assert_eq!(bodies.len(), 2);
    let body: Value = serde_json::from_slice(&bodies[0]).unwrap();
    assert_eq!(
        body,
        json!({"name":" Synthetic Company ","cvr":" dk12345678 ","address":" Synthetic Road ","postalCode":" 1234 ","city":" ","paymentTermsDays":0,"fiscalYearStartMonth":"4","fiscalYearLabelStrategy":"span"})
    );
    let mut renamed = created.clone();
    renamed["name"] = json!("Renamed Synthetic");
    renamed["vatPeriodType"] = Value::Null;
    let (out, bodies) = run_fixture(
        &[
            "set-profile",
            "--company",
            "synthetic-company",
            "--name",
            " Renamed Synthetic ",
            "--no-vat",
            "--json",
        ],
        vec![
            (
                "GET /api/companies/synthetic-company/company HTTP/1.1",
                200,
                json!({"ok":true,"company":created}),
            ),
            (
                "PATCH /api/companies/synthetic-company/company HTTP/1.1",
                200,
                json!({"ok":true,"company":renamed}),
            ),
        ],
    );
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        serde_json::from_slice::<Value>(&bodies[1]).unwrap(),
        json!({"name":" Renamed Synthetic ","vatPeriodType":null})
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["updatedFields"], json!(["name", "vatPeriodType"]));
    assert_eq!(v["settings"]["paymentTermsDays"], 0);
    let (out, bodies) = run_fixture(
        &["profile", "--company", "synthetic-company", "--json"],
        vec![(
            "GET /api/companies/synthetic-company/company HTTP/1.1",
            200,
            json!({"ok":true,"company":renamed}),
        )],
    );
    assert!(out.status.success(), "{out:?}");
    assert_eq!(bodies.len(), 1);
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        json!({"ok":true,"profile":{"name":"Renamed Synthetic","cvr":"DK12345678","address":"Synthetic Road","postalCode":"1234","city":null,"paymentTermsDays":0,"vatPeriodType":null,"vatRegistered":false,"vatPeriodLabel":"ikke momsregistreret"}})
    );
}

#[test]
fn supplied_creation_fields_and_rename_cannot_have_wrong_or_lost_receipts() {
    for (flag, value) in [
        ("--address", "Expected"),
        ("--postal-code", "1234"),
        ("--city", "Expected"),
        ("--payment-terms", "0"),
        ("--cvr", "12345678"),
        ("--fiscal-year-start-month", "4"),
        ("--fiscal-year-label-strategy", "span"),
        ("--bank-name", "Expected Bank"),
    ] {
        let (out, bodies) = run_fixture(
            &["add", "--name", "Synthetic Company", flag, value, "--json"],
            vec![
                (
                    "POST /api/companies HTTP/1.1",
                    201,
                    json!({"ok":true,"company":{"slug":"synthetic-company","name":"Synthetic Company"}}),
                ),
                (
                    "GET /api/companies/synthetic-company/company HTTP/1.1",
                    200,
                    json!({"ok":true,"company":profile()}),
                ),
            ],
        );
        assert_eq!(out.status.code(), Some(1), "{flag}: {out:?}");
        assert!(String::from_utf8_lossy(&out.stdout).contains("outcome_unknown"));
        assert_eq!(bodies.len(), 2); // one POST, one read, no compensating write
    }
    for receipt in [json!({"ok":true,"company":profile()}), Value::Null] {
        let (out, bodies) = run_fixture(
            &[
                "set-profile",
                "--company",
                "synthetic",
                "--name",
                "Renamed",
                "--json",
            ],
            vec![
                (
                    "GET /api/companies/synthetic/company HTTP/1.1",
                    200,
                    json!({"ok":true,"company":profile()}),
                ),
                (
                    "PATCH /api/companies/synthetic/company HTTP/1.1",
                    200,
                    receipt,
                ),
            ],
        );
        assert_eq!(out.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&out.stdout).contains("outcome_unknown"));
        assert_eq!(bodies.len(), 2);
    }
    let (out, bodies) = run_fixture(
        &[
            "add",
            "--name",
            "Synthetic Company",
            "--address",
            "Expected",
            "--json",
        ],
        vec![
            (
                "POST /api/companies HTTP/1.1",
                201,
                json!({"ok":true,"company":{"slug":"synthetic-company","name":"Synthetic Company"}}),
            ),
            (
                "GET /api/companies/synthetic-company/company HTTP/1.1",
                503,
                json!({"ok":false}),
            ),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("outcome_unknown"));
    assert_eq!(bodies.len(), 2);
}
