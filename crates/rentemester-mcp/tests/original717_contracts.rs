//! Work-Issue: #717. Integrated source metadata and actual stdio family dispatch.
#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-mcp");
const NAMES: [&str; 14] = [
    "system_backup_destination_list",
    "system_backup_destination_add",
    "system_backup_destination_remove",
    "system_backup_confirm_placement",
    "system_backup_governance",
    "gdpr_discover",
    "gdpr_export",
    "gdpr_audit_log",
    "efaktura_onboarding_status",
    "efaktura_onboard",
    "efaktura_registrer",
    "efaktura_modtag",
    "efaktura_modtag_workspace",
    "efaktura_konfigurer",
];
fn run(origin: &str, name: &str, args: Value) -> Value {
    let cookie = std::env::temp_dir().join(format!(
        "rm717-contract-{}-{:?}",
        std::process::id(),
        thread::current().id()
    ));
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
        .write_all(b"session=synthetic")
        .unwrap();
    let mut child = Command::new(BIN)
        .args([
            "--server",
            origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let messages = vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":args}}),
    ];
    let mut stdin = child.stdin.take().unwrap();
    let writer = thread::spawn(move || {
        for m in messages {
            writeln!(stdin, "{m}").unwrap();
        }
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    std::fs::remove_file(cookie).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .find(|r| r["id"] == 2)
        .unwrap()
}
fn envelope(row: &Value) -> Value {
    let r = &row["result"];
    let e: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(e, r["structuredContent"]);
    assert_eq!(r["isError"], e["ok"] != true);
    e
}
fn args(name: &str) -> Value {
    let mut a = json!({"company":"synthetic","confirm":true});
    match name {
        "system_backup_destination_list"
        | "system_backup_governance"
        | "gdpr_audit_log"
        | "efaktura_onboarding_status" => {
            a.as_object_mut().unwrap().remove("confirm");
        }
        "system_backup_destination_add" => {
            a["label"] = json!("Synthetic");
            a["kind"] = json!("other");
            a["location"] = json!("synthetic:offsite");
            a["inEeaOrEu"] = json!(true);
            a["attestedBy"] = json!("synthetic@example.invalid");
        }
        "system_backup_destination_remove" => a["id"] = json!("synthetic"),
        "system_backup_confirm_placement" => {
            a["destinationId"] = json!("synthetic");
            a["backupId"] = json!("b1");
            a["archiveSha256"] = json!("a".repeat(64));
        }
        "efaktura_registrer" => {
            a["cvr"] = json!("12345678");
            a["companyName"] = json!("Synthetic");
        }
        "efaktura_modtag_workspace" => a = json!({"workspace":"synthetic","confirm":true}),
        "efaktura_konfigurer" => a["apiLicenseKey"] = json!("synthetic-private"),
        "gdpr_discover" | "gdpr_export" => a["name"] = json!("Synthetic subject"),
        _ => {}
    }
    a
}
fn success(name: &str) -> Value {
    let status = json!({"configured":true,"environment":"test","identity":null,"companyKeyConfigured":true,"inboundReady":true,"outboundReady":true,"ready":true,"blockers":[]});
    let mut c = match name {
        "system_backup_destination_list" => json!({"destinations":[],"destinationCount":0}),
        "system_backup_destination_add" => {
            json!({"destination":{"id":"synthetic","placements":[]}})
        }
        "system_backup_destination_remove" => json!({}),
        "system_backup_confirm_placement" => {
            json!({"placement":{"verified":false,"archiveSha256":"a".repeat(64)}})
        }
        "system_backup_governance" => {
            json!({"compliance":{},"lock":{},"latestBackupPlacedOffsite":false,"destinations":[]})
        }
        "gdpr_discover" => json!({"rows":[],"byTable":{}}),
        "gdpr_export" => json!({"records":[],"asOf":"2026-01-01","coverage":{"complete":false}}),
        "gdpr_audit_log" => json!({"events":[],"canonicalPayload":"[]","fingerprint":"synthetic"}),
        "efaktura_onboarding_status" => status.clone(),
        "efaktura_onboard" => json!({"companyKey":"synthetic","status":status}),
        "efaktura_registrer" => {
            json!({"companyKey":"synthetic","directionsRegistered":[],"participantType":"DK:CVR","participantId":"12345678"})
        }
        "efaktura_modtag" => {
            json!({"pagesFetched":1,"documentsListed":0,"documentsIngested":0,"documentsSkipped":0,"documentsQuarantined":0,"documents":[]})
        }
        "efaktura_konfigurer" => json!({"configPath":"config/digisense.json","environment":"test"}),
        _ => panic!("local-only workspace is tested separately"),
    };
    c["ok"] = json!(true);
    c["errors"] = json!([]);
    c["appliedRules"] = json!([]);
    c
}
fn fixture(status: u16, reply: Value) -> (String, thread::JoinHandle<String>) {
    let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut bytes = vec![];
        loop {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&bytes[..i]).to_lowercase();
                let len = h
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map(|n| n.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= i + 4 + len {
                    break;
                }
            }
        }
        let wire = String::from_utf8(bytes).unwrap();
        assert!(wire.to_lowercase().contains("x-rentemester-manifest:"));
        let text = reply.to_string();
        write!(s,"HTTP/1.1 {status} Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",text.len()).unwrap();
        drop(s);
        listener.set_nonblocking(true).unwrap();
        thread::sleep(std::time::Duration::from_millis(100));
        assert!(listener.accept().is_err(), "no automatic retry or readback");
        wire
    });
    (origin, task)
}
#[test]
fn original717_contracts_metadata_exact_and_all_source_names_unique() {
    let output = Command::new(BIN).arg("--catalogue").output().unwrap();
    assert!(output.status.success());
    let c: Value = serde_json::from_slice(&output.stdout).unwrap();
    let tools = c["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 448);
    let names: std::collections::BTreeSet<_> =
        tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), 448);
    let inventory: Value = serde_json::from_str(include_str!(
        "../../../docs/architecture/rust-target/implementation/baseline/inventory.json"
    ))
    .unwrap();
    for source in inventory["mcp"].as_array().unwrap() {
        assert!(names.contains(source["name"].as_str().unwrap()));
    }
    for name in NAMES {
        let a = tools.iter().find(|t| t["name"] == name).unwrap();
        let b = inventory["mcp"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap();
        assert_eq!(a, b, "{name}");
    }
    for (names, expected) in [
        (vec!["invoice_imported_receivables", "budget_vs_actual", "bookkeeping_batch_plan", "bookkeeping_batch_status"], json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false})),
        (vec!["invoice_write_off_bad_debt", "invoice_claim_interest", "invoice_post_interest", "invoice_post_interest_correction", "invoice_claim_compensation", "invoice_post_compensation", "native_gdpr_audit_log"], json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false})),
        (vec!["bookkeeping_batch_persist", "bookkeeping_batch_dry_run", "bookkeeping_batch_approve", "bookkeeping_batch_apply"], json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false})),
    ] {
        for name in names {
            let tool=tools.iter().find(|t|t["name"]==name).unwrap();
            assert_eq!(tool["annotations"],expected,"{name}");
        }
    }
    assert_eq!(
        tools
            .iter()
            .find(|t| t["name"] == "native_gdpr_audit_log")
            .unwrap()["annotations"]["readOnlyHint"],
        false
    );
}
#[test]
fn original717_contracts_confirmation_and_wrong_types_before_io() {
    for name in NAMES {
        let a = args(name);
        if a.get("confirm").is_none() {
            continue;
        }
        for confirm in [None, Some(false)] {
            let mut a = a.clone();
            a.as_object_mut().unwrap().remove("confirm");
            if let Some(c) = confirm {
                a["confirm"] = json!(c);
            }
            assert_eq!(
                envelope(&run("http://127.0.0.1:9", name, a))["code"],
                "CONFIRM_REQUIRED",
                "{name}"
            );
        }
        let mut bad = a.clone();
        bad["confirm"] = json!("true");
        let row = run("http://127.0.0.1:9", name, bad);
        assert!(
            row.get("error").is_some() || row["result"].get("structuredContent").is_none(),
            "{name}: {row}"
        );
    }
}
#[test]
fn original717_contracts_actual_family_transport_success_refusal_and_unknown() {
    for name in NAMES {
        if name == "efaktura_modtag_workspace" {
            continue;
        }
        for (status, core, expected) in [
            (200, success(name), None),
            (
                200,
                json!({"ok":false,"errors":["synthetic refusal"],"appliedRules":[]}),
                Some("domain"),
            ),
            (403, json!({"ok":false}), Some("server_refused")),
            (503, json!({"ok":false}), Some("uncertain")),
        ] {
            let (origin, task) = fixture(status, json!({"ok":true,"result":core}));
            let e = envelope(&run(&origin, name, args(name)));
            match expected {
                None => assert_eq!(e["ok"], true, "{name}: {e}"),
                Some("domain") => assert_eq!(e["errors"], json!(["synthetic refusal"]), "{name}"),
                Some("server_refused") => assert_eq!(e["code"], "server_refused", "{name}"),
                _ => assert_eq!(
                    e["code"],
                    if matches!(
                        name,
                        "system_backup_destination_list"
                            | "system_backup_governance"
                            | "efaktura_onboarding_status"
                    ) {
                        "server_refused"
                    } else {
                        "outcome_unknown"
                    },
                    "{name}: {e}"
                ),
            }
            let wire = task.join().unwrap();
            assert!(
                wire.lines().next().unwrap().contains(&format!(
                    "/api/companies/synthetic/original-{}",
                    name.replace('_', "-")
                )),
                "{name}: {wire}"
            );
            let body = wire.split_once("\r\n\r\n").unwrap().1;
            if !body.is_empty() {
                let mut expected = args(name);
                expected.as_object_mut().unwrap().remove("company");
                assert_eq!(serde_json::from_str::<Value>(body).unwrap(), expected);
            }
        }
    }
}

#[test]
fn original717_contracts_malformed_success_is_typed_and_workspace_not_remote_authority() {
    for name in NAMES {
        if matches!(
            name,
            "system_backup_destination_remove" | "efaktura_modtag_workspace"
        ) {
            continue;
        }
        let (origin, task) = fixture(
            200,
            json!({"ok":true,"result":{"ok":true,"errors":[],"appliedRules":[]}}),
        );
        let e = envelope(&run(&origin, name, args(name)));
        assert!(
            e["code"] == "protocol" || e["code"] == "outcome_unknown",
            "{name}: {e}"
        );
        task.join().unwrap();
    }
    assert_eq!(
        envelope(&run(
            "http://127.0.0.1:9",
            "efaktura_modtag_workspace",
            args("efaktura_modtag_workspace")
        ))["code"],
        "invalid_input"
    );
}
