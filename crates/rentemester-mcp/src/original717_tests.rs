use super::*;
fn args(name: &str) -> Value {
    match name {
        "system_restore_backup" => {
            json!({"backupDir":"/synthetic/backup","targetCompany":"/synthetic/target"})
        }
        "system_backup_place" => {
            json!({"company":"synthetic","archivePath":"/synthetic/a.tar","destinationId":"cold"})
        }
        "system_backup_verify_remote_placement" => {
            json!({"company":"synthetic","destinationId":"cold","backupId":"b1","remoteObjectId":"object1"})
        }
        "mail_intake_ingest" => json!({"company":"synthetic","source":"/synthetic/mail.eml"}),
        "efaktura_konfigurer" => {
            json!({"company":"synthetic","apiLicenseKey":"synthetic-private-secret"})
        }
        _ => json!({"company":"synthetic"}),
    }
}
fn agreement(v: &Value) {
    assert_eq!(
        serde_json::from_str::<Value>(v["content"][0]["text"].as_str().unwrap()).unwrap(),
        v["structuredContent"]
    );
    assert_eq!(v["isError"], v["structuredContent"]["ok"] != true);
}
async fn exercise(name: &str) {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for confirmation in [None, Some(false)] {
        let mut a = args(name);
        if let Some(c) = confirmation {
            a["confirm"] = json!(c);
        }
        let v = crate::tools::call(&api, name, a).await.unwrap();
        agreement(&v);
        assert_eq!(v["structuredContent"]["code"], "CONFIRM_REQUIRED");
        assert!(!v.to_string().contains("synthetic-private-secret"));
    }
    for field in ["confirm", "actor", "_meta"] {
        let mut a = args(name);
        a[field] = json!("forged");
        let v = crate::tools::call(&api, name, a).await.unwrap();
        assert_eq!(v["isError"], true);
        assert!(v.get("structuredContent").is_none());
    }
    let v = crate::tools::call(&api, name, json!({})).await.unwrap();
    assert_eq!(v["isError"], true);
    assert!(v.get("structuredContent").is_none());
    // Actual dispatcher/SDK/transport, not only a parallel envelope formatter.
    if matches!(name, "system_restore_backup" | "mail_intake_ingest") {
        return;
    }
    for response in [
        json!({"ok":true,"result":{"ok":false,"errors":["synthetic refusal"]}}),
        json!({"ok":true,"result":{}}),
    ] {
        use std::io::{Read, Write};
        let listener = crate::original717_transport::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut b = [0; 4096];
                let n = stream.read(&mut b).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
                if let Some(p) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..p]).to_lowercase();
                    let len = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                    if bytes.len() >= p + 4 + len {
                        break;
                    }
                }
            }
            let response = response.to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
        });
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let mut a = args(name);
        a["confirm"] = json!(true);
        let v = crate::tools::call(&api, name, a).await.unwrap();
        agreement(&v);
        assert_eq!(v["isError"], true);
        worker.join().unwrap();
    }
}
macro_rules! cases {($($name:ident),*)=>{$(#[test] fn $name(){tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(exercise(stringify!($name).strip_prefix("original717_").unwrap()));})*};}
cases!(
    original717_system_backup,
    original717_system_backup_archive,
    original717_system_backup_place,
    original717_system_backup_verify_remote_placement,
    original717_system_restore_backup,
    original717_mail_intake_ingest,
    original717_imap_intake_poll,
    original717_invoice_send_email,
    original717_efaktura_konfigurer
);
#[test]
fn original717_registry_metadata_and_envelope() {
    let all = crate::tools::list();
    for tool in tools() {
        assert_eq!(
            all["tools"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|t| t["name"] == tool["name"])
                .count(),
            1
        );
        assert!(tool["title"].is_string());
        assert!(tool["outputSchema"].is_object());
        assert_eq!(
            tool["inputSchema"]["properties"]["confirm"]["type"],
            "boolean"
        );
    }
    for c in [
        json!({"ok":true,"duplicate":true,"errors":[],"appliedRules":["rule"]}),
        json!({"ok":false,"errors":["rejected"]}),
    ] {
        agreement(&wrap(c));
    }
}
#[test]
fn original717_cvr_optional_confirmation_is_domain_result() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(cvr_confirmation());
}
async fn cvr_confirmation() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for name in ["cvr_lookup", "company_sync_cvr"] {
        for confirmation in [None, Some(false)] {
            let mut a = json!({"company":"synthetic"});
            if name == "cvr_lookup" {
                a["cvr"] = json!("12345678");
            }
            if let Some(c) = confirmation {
                a["confirm"] = json!(c);
            }
            let v = crate::tools::call(&api, name, a).await.unwrap();
            agreement(&v);
            assert_eq!(v["structuredContent"]["code"], "CONFIRM_REQUIRED");
        }
        let mut a = json!({"company":"synthetic","confirm":"true"});
        if name == "cvr_lookup" {
            a["cvr"] = json!("12345678");
        }
        assert!(crate::tools::call(&api, name, a).await.is_err());
    }
}
#[test]
fn original717_audit_and_posting_selectors_reject_remote_paths() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(audit_selectors());
}
async fn audit_selectors() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for name in ["audit_verify", "audit_log_list"] {
        let v = crate::tools::call(&api, name, json!({"company":"/outside/synthetic"}))
            .await
            .unwrap();
        agreement(&v);
        assert_eq!(v["isError"], true);
        assert!(!v.to_string().contains("/outside/synthetic"));
    }
    assert!(
        api.original717_company_selector("/outside/synthetic")
            .is_err()
    );
    for c in ["../synthetic", "synthetic/other", ""] {
        assert!(api.original717_company_selector(c).is_err());
    }
}
#[test]
fn original717_wrap_core_result_cross_cutting_codes_and_false_idempotency() {
    let locked =
        wrap(json!({"ok":false,"errors":["Bookkeeping is locked"],"code":"backup_overdue"}));
    agreement(&locked);
    assert_eq!(locked["structuredContent"]["code"], "BACKUP_LOCKED");
    for (message, code) in [
        ("date falls in closed period 2026", "PERIOD_CLOSED"),
        ("invoice must be posted", "PRECONDITION_MISSING"),
        ("invoice does not exist", "NOT_FOUND"),
        ("selskabet er ikke momsregistreret", "NOT_VAT_REGISTERED"),
    ] {
        let v = wrap(json!({"ok":false,"errors":[message],"code":"untrusted_backend_code"}));
        agreement(&v);
        assert_eq!(v["structuredContent"]["code"], code);
    }
    for name in [
        "system_backup",
        "system_backup_archive",
        "system_backup_place",
        "system_backup_verify_remote_placement",
        "system_restore_backup",
        "invoice_send_email",
    ] {
        assert_eq!(
            tools().iter().find(|t| t["name"] == name).unwrap()["annotations"]["idempotentHint"],
            false
        );
    }
}
