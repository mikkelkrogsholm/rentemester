//! Actual CLI + current protocol fixtures; no company databases or live effects.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn disposition() -> Value {
    json!({"dispositionId":"synthetic-id","type":"loan","economicDate":"2026-10-20","amount":75,"currency":"DKK","partyIds":["synthetic-party"],"evidenceRecordIds":["synthetic-record"],"left":{"companySlug":"left","role":"lender","expectedSide":"receivable"},"right":{"companySlug":"right","role":"borrower","expectedSide":"payable"}})
}
fn disposition_hash() -> String {
    rentemester_engine::sha256_bytes(
        rentemester_engine::imported_receivables::canonical(&disposition())
            .unwrap()
            .as_bytes(),
    )
}
fn args(op: &str) -> Value {
    if matches!(op, "plan" | "propose") {
        let mut a = json!({"company":"left","disposition":disposition()});
        if op == "propose" {
            a["confirm"] = json!(true);
        }
        return a;
    }
    let mut a = json!({"company":"left","dispositionId":"synthetic-id"});
    if op == "status" {
        return a;
    }
    a["payloadHash"] = json!(disposition_hash());
    a["confirm"] = json!(true);
    match op {
        "link" => {
            a["side"] = json!("left");
            a["journalEntryId"] = json!(3);
            a["ledgerHeadHash"] = json!("b".repeat(64));
        }
        "settle" => a["settlementEvidenceRecordIds"] = json!(["synthetic-settlement"]),
        "reopen" => a["reason"] = json!("Reviewed reopen"),
        "supersede" => {
            a["reason"] = json!("Reviewed replacement");
            a["replacementDispositionId"] = json!("synthetic-replacement");
        }
        _ => {}
    }
    a
}
fn event(kind: &str, p: Value) -> Value {
    json!({"event_type":kind,"actor":"user:synthetic","principal_kind":"user","principal_id":if kind=="proposed"{"author"}else{"reviewer"},"payload_hash":rentemester_engine::sha256_bytes(rentemester_engine::imported_receivables::canonical(&p).unwrap().as_bytes()),"canonical_payload":p.to_string(),"created_at":"2026-10-20T12:00:00.000Z"})
}
fn receipt(op: &str) -> Value {
    if op == "plan" {
        return json!({"ok":true,"mode":"dry-run","disposition":disposition(),"payloadHash":disposition_hash(),"ledgerEffects":[],"proposedSides":["left","right"],"warnings":["Each legal company must post separately through its normal policy, document, VAT and period controls."],"journalCandidates":{"left":[],"right":[]}});
    }
    let mut ev = vec![event("proposed", disposition())];
    if op != "propose" {
        ev.push(event("approved", json!({"payloadHash":disposition_hash()})));
    }
    let mut life = vec![];
    let mut links = vec![];
    if matches!(op, "link" | "settle" | "reopen") {
        for side in if op == "link" {
            vec!["left"]
        } else {
            vec!["left", "right"]
        } {
            links.push(json!({"side":side,"company_slug":side,"journal_entry_id":3,"journal_entry_no":"J3","journal_entry_hash":"c".repeat(64),"ledger_head_hash":"b".repeat(64),"linked_at":"2026-10-20T12:00:00.000Z","actor":"user:synthetic","principal_kind":"user","principal_id":"reviewer"}));
            life.push(event(
                "exception",
                json!({"kind":"one_sided_posting","side":side,"payloadHash":disposition_hash()}),
            ));
        }
    }
    if links.len() == 2 {
        life.push(event(
            "exception_resolved",
            json!({"kind":"one_sided_posting","payloadHash":disposition_hash()}),
        ));
    }
    let a = args(op);
    let mut p = json!({"payloadHash":disposition_hash()});
    for k in [
        "settlementEvidenceRecordIds",
        "reason",
        "replacementDispositionId",
    ] {
        if let Some(v) = a.get(k) {
            p[k] = v.clone();
        }
    }
    match op {
        "settle" => life.push(event("settled", p)),
        "reopen" => {
            life.push(event("settled",json!({"payloadHash":disposition_hash(),"settlementEvidenceRecordIds":["synthetic-settlement"]})));
            life.push(event("reopened", p));
        }
        "supersede" => life.push(event("superseded", p)),
        _ => {}
    }
    let status = match op {
        "propose" => "proposed",
        "link" => "partly_posted",
        "settle" => "settled",
        "supersede" => "superseded",
        "reopen" => "posted",
        _ => "approved",
    };
    let mut v = json!({"ok":true,"disposition":disposition(),"payloadHash":disposition_hash(),"status":status,"events":ev,"lifecycleEvents":life,"links":links});
    if op == "status" {
        v["exceptions"] = json!([]);
        v["balanced"] = json!(false);
        v["scope"] = json!("intercompany-disposition-status");
        v["journalCandidates"] = json!({"left":[],"right":[]});
    }
    v
}

fn manifest() -> Value {
    json!({"version":1,"groups":[{"id":"g","name":"Synthetic","memberships":[{"id":"left","companySlug":"left","validFrom":"2026-01-01"},{"id":"right","companySlug":"right","validFrom":"2026-01-01"}],"ownership":[]}]})
}
fn mapping() -> Value {
    json!({"id":"mapping","groupId":"g","leftCompanySlug":"left","rightCompanySlug":"right","leftAccountNos":["1400"],"rightAccountNos":["2400"],"leftPosition":"receivable","rightPosition":"payable","evidenceRefs":["synthetic"],"validFrom":"2026-01-01"})
}
fn mh() -> String {
    let m: rentemester_engine::intercompany::Manifest = serde_json::from_value(manifest()).unwrap();
    rentemester_engine::sha256_bytes(m.canonical().unwrap().as_bytes())
}
fn current() -> Value {
    let m: rentemester_engine::intercompany::Mapping = serde_json::from_value(mapping()).unwrap();
    json!({"ok":true,"manifest":manifest(),"manifestHash":mh(),"mappings":[{"mapping":mapping(),"mappingHash":rentemester_engine::sha256_bytes(m.canonical().unwrap().as_bytes()),"status":"approved","actor":"user:reviewer"}],"consolidationAvailable":true})
}
fn fixture_file(name: &str, v: &Value) -> PathBuf {
    let p = std::env::temp_dir().join(format!("rm-group716-{}-{name}", std::process::id()));
    std::fs::write(&p, v.to_string()).unwrap();
    p
}
fn invoke(
    op: &str,
    flags: Vec<String>,
    response: Value,
    lost: bool,
) -> (std::process::Output, Vec<String>) {
    let cookie =
        std::env::temp_dir().join(format!("rm-group716-{}-{op}.cookie", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic-private\n")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let t = thread::spawn(move || {
        let start = Instant::now();
        let mut last = None;
        let mut wires = vec![];
        loop {
            if last.is_some_and(|t: Instant| t.elapsed() > Duration::from_millis(180)) {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(8),
                "no business request"
            );
            let mut s = match listener.accept() {
                Ok((s, _)) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut b = vec![];
            while !b.ends_with(b"\r\n\r\n") {
                let mut c = [0];
                s.read_exact(&mut c).unwrap();
                b.push(c[0]);
            }
            let h = String::from_utf8(b.clone()).unwrap();
            let n = h
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut body = vec![0; n];
            s.read_exact(&mut body).unwrap();
            b.extend(body);
            let wire = String::from_utf8(b).unwrap();
            let protocol = wire.starts_with("GET /api/protocol ");
            let catalogue = wire.starts_with("GET /api/companies ");
            let context = wire.contains("/intercompany HTTP");
            let data = if protocol {
                assert!(!wire.to_lowercase().contains("cookie:"));
                include_str!("../../../protocol/http-v1.json").to_owned()
            } else if catalogue {
                json!({"ok":true,"companies":[{"slug":"left","archived":false},{"slug":"right","archived":false}],"count":2}).to_string()
            } else if context {
                current().to_string()
            } else if wire.starts_with("GET /api/me ") {
                json!({"ok":true,"user":{"id":"synthetic"}}).to_string()
            } else {
                response.to_string()
            };
            let post = wire.starts_with("POST ");
            if !protocol {
                assert!(wire.contains("session=synthetic-private"));
                wires.push(wire);
                last = Some(Instant::now());
            }
            if !(lost && post) {
                write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}",data.len()).unwrap();
            }
        }
        wires
    });
    let out = Command::new(BIN)
        .args([
            "group",
            op,
            "--company",
            "left",
            "--server",
            &origin,
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--allow-loopback-http",
            "--json",
        ])
        .args(flags)
        .output()
        .unwrap();
    let wires = t.join().unwrap();
    std::fs::remove_file(cookie).unwrap();
    (out, wires)
}
fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}
#[test]
fn all_eight_disposition_keys_flat_receipts_and_one_intent() {
    for op in [
        "plan",
        "propose",
        "approve",
        "link",
        "status",
        "settle",
        "reopen",
        "supersede",
    ] {
        let a = args(op);
        let mut flags = vec![];
        let mut input = None;
        if matches!(op, "plan" | "propose") {
            let p = fixture_file(op, &disposition());
            flags.extend(["--disposition".into(), p.to_str().unwrap().into()]);
            input = Some(p);
        } else {
            flags.extend(["--disposition-id".into(), "synthetic-id".into()]);
        }
        if !matches!(op, "plan" | "status") {
            flags.extend([
                "--confirm".into(),
                "yes".into(),
                "--actor".into(),
                "user:synthetic".into(),
            ]);
            if op != "propose" {
                flags.extend(["--payload-hash".into(), disposition_hash()]);
            }
        }
        for (k, flag) in [
            ("side", "--side"),
            ("ledgerHeadHash", "--ledger-head-hash"),
            ("reason", "--reason"),
            ("replacementDispositionId", "--replacement-disposition-id"),
        ] {
            if let Some(v) = a[k].as_str() {
                flags.extend([flag.into(), v.into()]);
            }
        }
        if op == "link" {
            flags.extend(strings(&["--journal-entry-id", "3"]));
        }
        if op == "settle" {
            flags.extend([
                "--settlement-evidence-json".into(),
                a["settlementEvidenceRecordIds"].to_string(),
            ]);
        }
        let command = if op == "status" {
            "disposition-status".into()
        } else {
            format!("{op}-disposition")
        };
        let (out, wires) = invoke(&command, flags, receipt(op), false);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{op}: {} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["disposition"], disposition());
        assert!(v.get("journalCandidates").is_none());
        assert!(v.get("data").is_none());
        if op != "plan" {
            assert_eq!(v["events"][0].as_object().unwrap().len(), 7);
        }
        if op == "link" {
            assert_eq!(v["links"][0].as_object().unwrap().len(), 10);
        }
        assert_eq!(
            wires.iter().filter(|s| s.starts_with("POST ")).count(),
            usize::from(op != "status")
        );
        for wire in wires.iter().filter(|s| s.starts_with("POST ")) {
            let body: Value = serde_json::from_str(wire.split("\r\n\r\n").nth(1).unwrap()).unwrap();
            assert!(body.get("actor").is_none());
            assert!(body.get("principal").is_none());
            assert_eq!(
                body["confirm"],
                if op == "plan" {
                    Value::Null
                } else {
                    json!(true)
                }
            );
        }
        if let Some(p) = input {
            std::fs::remove_file(p).unwrap();
        }
    }
}
#[test]
fn malformed_or_lost_write_is_unknown_once_and_read_is_not_unknown() {
    for lost in [false, true] {
        let (out, wires) = invoke(
            "approve-disposition",
            strings(&[
                "--disposition-id",
                "synthetic-id",
                "--payload-hash",
                &disposition_hash(),
                "--confirm",
                "yes",
            ]),
            json!({"ok":true}),
            lost,
        );
        assert_eq!(out.status.code(), Some(1));
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["code"], "outcome_unknown");
        assert_eq!(wires.iter().filter(|s| s.starts_with("POST ")).count(), 1);
    }
    let (out, _) = invoke(
        "disposition-status",
        strings(&["--disposition-id", "synthetic-id"]),
        json!({"ok":true}),
        false,
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["code"], "protocol");
}
#[test]
fn source_discovery_and_pure_preflight_before_files() {
    let out = Command::new(BIN)
        .args(["group", "--help"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["commands"].as_array().unwrap().len(), 27);
    for flags in [
        vec!["--amount", "1"],
        vec!["--confirm", "no"],
        vec!["--workspace", "/not-opened"],
        vec![
            "--disposition",
            "/not-opened",
            "--disposition",
            "/also-not-opened",
        ],
    ] {
        let out = Command::new(BIN)
            .args(["group", "propose-disposition"])
            .args(flags)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert!(!out.stderr.is_empty());
    }
}
fn profile() -> Value {
    json!({"version":1,"id":"profile","groupId":"g","currency":"DKK","reportingLines":[{"id":"asset","label":"Asset","section":"asset","displayOrder":1},{"id":"liability","label":"Liability","section":"liability","displayOrder":2},{"id":"equity","label":"Result","section":"equity","displayOrder":3,"role":"current-result"},{"id":"income","label":"Income","section":"income","displayOrder":4},{"id":"expense","label":"Expense","section":"expense","displayOrder":5}],"accountMappings":[{"id":"account","companySlug":"left","accountNo":"1400","reportingLineId":"asset","validFrom":"2026-01-01"}],"evidenceRefs":["synthetic"],"validFrom":"2026-01-01"})
}
#[test]
fn validators_manifest_mapping_profile_are_readonly_and_lifecycles_exact() {
    let m: rentemester_engine::intercompany::Mapping = serde_json::from_value(mapping()).unwrap();
    let mapping_hash = rentemester_engine::sha256_bytes(m.canonical().unwrap().as_bytes());
    let p: rentemester_engine::consolidation::Profile = serde_json::from_value(profile()).unwrap();
    let profile_hash = rentemester_engine::sha256_bytes(p.canonical().unwrap().as_bytes());
    for kind in ["manifest", "mapping", "profile"] {
        let input = match kind {
            "manifest" => manifest(),
            "mapping" => mapping(),
            _ => profile(),
        };
        let path = fixture_file(&format!("validate-{kind}"), &input);
        let (out, wires) = invoke(
            &format!("validate-{kind}"),
            vec![format!("--{kind}"), path.to_str().unwrap().into()],
            json!({}),
            false,
        );
        assert_eq!(
            out.status.code(),
            Some(0),
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(wires.iter().all(|s| s.starts_with("GET ")));
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        if kind == "manifest" {
            assert_eq!(v["groupCount"], 1);
        } else {
            assert_eq!(v[kind], input);
        }
        std::fs::remove_file(path).unwrap();
    }
    for kind in ["manifest", "mapping", "profile"] {
        let operations = if kind == "manifest" {
            vec!["apply"]
        } else {
            vec!["propose", "approve", "revoke"]
        };
        for op in operations {
            let mut flags = strings(&["--confirm", "yes"]);
            let mut input = None;
            if matches!(op, "apply" | "propose") {
                let value = match kind {
                    "manifest" => manifest(),
                    "mapping" => mapping(),
                    _ => profile(),
                };
                let path = fixture_file(&format!("{op}-{kind}"), &value);
                flags.extend([format!("--{kind}"), path.to_str().unwrap().into()]);
                input = Some(path);
                if kind == "manifest" {
                    flags.extend(strings(&["--policy-company", "left"]));
                }
            } else {
                flags.extend([
                    format!("--{kind}-id"),
                    if kind == "mapping" {
                        "mapping".into()
                    } else {
                        "profile".into()
                    },
                ]);
                if op == "approve" {
                    flags.extend([
                        format!("--{kind}-hash"),
                        if kind == "mapping" {
                            mapping_hash.clone()
                        } else {
                            profile_hash.clone()
                        },
                    ]);
                }
            }
            let response = if kind == "manifest" {
                json!({"ok":true,"status":"applied","manifestHash":mh()})
            } else {
                let status = match op {
                    "propose" => "proposed",
                    "approve" => "approved",
                    _ => "revoked",
                };
                let mut v = json!({"ok":true,"status":status});
                v[format!("{kind}Id")] = json!(kind);
                if kind == "mapping" || op != "revoke" {
                    v[format!("{kind}Hash")] = json!(if kind == "mapping" {
                        &mapping_hash
                    } else {
                        &profile_hash
                    });
                }
                v
            };
            let (out, wires) = invoke(&format!("{op}-{kind}"), flags, response, false);
            assert_eq!(
                out.status.code(),
                Some(0),
                "{op}-{kind}: {} {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(wires.iter().filter(|s| s.starts_with("POST ")).count(), 1);
            if let Some(p) = input {
                std::fs::remove_file(p).unwrap();
            }
        }
    }
}
#[test]
fn elimination_lifecycle_strict_explicit_sourcehash_and_reconcile_projection() {
    for (op, status) in [
        ("propose", "proposed"),
        ("approve", "approved"),
        ("reject", "rejected"),
        ("apply", "applied"),
        ("reverse", "reversed"),
    ] {
        let mut flags = strings(&["--confirm", "yes"]);
        let mut input = None;
        if op == "propose" {
            let path = fixture_file(
                "elimination",
                &json!({"id":"elimination","mappingId":"mapping","asOf":"2026-01-01","evidenceRefs":["synthetic"],"expectedPayloadHash":"a".repeat(64)}),
            );
            flags.extend(["--elimination".into(), path.to_str().unwrap().into()]);
            input = Some(path);
        } else {
            flags.extend(strings(&[
                "--elimination-id",
                "elimination",
                "--payload-hash",
                &"a".repeat(64),
            ]));
        }
        let (out, wires) = invoke(
            &format!("{op}-elimination"),
            flags,
            json!({"ok":true,"eliminationId":"elimination","payloadHash":"a".repeat(64),"eventHash":"b".repeat(64),"status":status}),
            false,
        );
        assert_eq!(
            out.status.code(),
            Some(0),
            "{op}: {} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(wires.iter().filter(|s| s.starts_with("POST ")).count(), 1);
        if let Some(p) = input {
            std::fs::remove_file(p).unwrap();
        }
    }
    for op in ["reconcile", "eliminations"] {
        let response = if op == "reconcile" {
            json!({"ok":true,"scope":"intercompany-reconciliation","asOf":"2026-01-01","rows":[]})
        } else {
            json!({"ok":true,"scope":"consolidation-eliminations","asOf":"2026-01-01","rows":[]})
        };
        let (out, wires) = invoke(op, strings(&["--as-of", "2026-01-01"]), response, false);
        assert_eq!(out.status.code(), Some(0));
        assert!(wires.iter().all(|s| s.starts_with("GET ")));
    }
}

// Production producer: engine/consolidation.rs::calculate returns arrays and
// Money decimal strings; blocked reports keep source evidence and null figures.
fn native_consolidated_report() -> Value {
    use rentemester_engine::consolidation::{self, Ledger, Profile};
    let profile: Profile = serde_json::from_str(include_str!(
        "../../rentemester-engine/src/consolidation/profile-source-canonical.json"
    ))
    .unwrap();
    let ledgers: Vec<Ledger> = ["a","b"].into_iter().map(|slug| serde_json::from_value(json!({
        "companySlug":slug,"currency":"DKK","ledgerHeadHash":"d".repeat(64),"entryCount":1,
        "accounts":[{"accountNo":"1000","accountType":"asset","active":true},{"accountNo":"3000","accountType":"income","active":true}],
        "pnl":[{"accountNo":"3000","accountType":"income","amount":"-12.34","sourceHash":"b".repeat(64)}],
        "cumulativeResult":[{"accountNo":"3000","accountType":"income","amount":"-12.34","sourceHash":"b".repeat(64)}],
        "balance":[{"accountNo":"1000","accountType":"asset","amount":"12.34","sourceHash":"b".repeat(64)}]
    })).unwrap()).collect();
    let mut report = consolidation::calculate(
        &profile,
        &"a".repeat(64),
        "2026-01-01",
        "2026-10-20",
        &ledgers,
        &[],
        &[],
        vec![],
        |s| rentemester_engine::sha256_bytes(s.as_bytes()),
    )
    .unwrap();
    assert_eq!(report["status"], "ready");
    // The HTTP producer adds ok after the common engine returns.
    report["ok"] = json!(true);
    report
}
#[test]
fn consolidated_report_native_arrays_checked_numbers_and_blocked_source_read() {
    let flags = || {
        strings(&[
            "--profile-id",
            "synthetic-profile",
            "--from",
            "2026-01-01",
            "--as-of",
            "2026-10-20",
        ])
    };
    let native = native_consolidated_report();
    let mut expected = native.clone();
    for row in expected["rawCompanySums"].as_array_mut().unwrap() {
        row["total"] = json!(24.68);
        for company in row["byCompany"].as_array_mut().unwrap() {
            company["amount"] = json!(12.34);
        }
    }
    for row in expected["consolidatedFigures"].as_array_mut().unwrap() {
        let amount = if matches!(row["lineId"].as_str(), Some("assets" | "result" | "sales")) {
            json!(24.68)
        } else {
            json!(0)
        };
        row["rawCompanySum"] = amount.clone();
        row["eliminationAdjustment"] = json!(0);
        row["consolidatedAmount"] = amount;
    }
    let (out, wires) = invoke("consolidated-report", flags(), native.clone(), false);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(out.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        expected
    );
    assert_eq!(wires.len(), 1);
    assert!(wires[0].starts_with("GET /api/group-consolidated-report?"));
    // The producer returns a truthful blocked read, not a failed command or partial figures.
    let blocked = json!({"ok":true,"scope":"consolidated-report","period":{"from":"2026-01-01","to":"2026-10-20"},"status":"blocked","blockers":["all active group companies must be visible"],"rawCompanySums":[],"appliedEliminations":[],"consolidatedFigures":null,"sourceSnapshots":[]});
    let (out, wires) = invoke("consolidated-report", flags(), blocked.clone(), false);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        blocked
    );
    assert_eq!(wires.len(), 1);
    for bad in 0..6 {
        let mut v = native.clone();
        match bad {
            0 => v["consolidatedFigures"] = json!({}),
            1 => v["status"] = json!("invented"),
            2 => {
                v["status"] = json!("blocked");
                v["blockers"] = json!(["blocked"]);
            }
            3 => v["rawCompanySums"][0]["total"] = json!("90071992547409.91"),
            4 => v["blockers"] = json!(["blocked"]),
            _ => v["consolidatedFigures"][0]["consolidatedAmount"] = json!("NaN"),
        }
        let (out, wires) = invoke("consolidated-report", flags(), v, false);
        assert_eq!(out.status.code(), Some(1), "case{bad}");
        assert!(out.stderr.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap()["ok"],
            false
        );
        assert_eq!(wires.len(), 1);
    }
}
