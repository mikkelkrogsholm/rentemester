//! Work-Issue: #717. SDK family HTTP contract proofs, synthetic data only.
mod common;
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    thread,
};
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
    if name != "efaktura_konfigurer" {
        c["errors"] = json!([]);
        c["appliedRules"] = json!([]);
    }
    c
}

async fn call(api: &ApiClient, name: &str, a: Value) -> Result<Value, Error> {
    if name.starts_with("system_backup_") {
        let mut input = a;
        let company = input
            .as_object_mut()
            .unwrap()
            .remove("company")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        api.backup_views_original717(name, &company, input).await
    } else if name.starts_with("gdpr_") {
        api.original717_privacy(name, &a).await
    } else if name == "efaktura_konfigurer" {
        api.original717(name, &a).await
    } else {
        api.original717_digisense(name, &a).await
    }
}
fn fixture(status: u16, reply: Value) -> (ApiClient, thread::JoinHandle<()>) {
    let l = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
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
                    assert!(h.contains("cookie: session=synthetic"));
                    assert!(h.contains("x-rentemester-manifest:"));
                    break;
                }
            }
        }
        let body = reply.to_string();
        write!(s,"HTTP/1.1 {status} Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        drop(s);
        l.set_nonblocking(true).unwrap();
        thread::sleep(std::time::Duration::from_millis(100));
        assert!(l.accept().is_err(), "SDK must send once");
    });
    (
        ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap(),
        t,
    )
}
#[tokio::test]
async fn original717_contracts_sdk_actual_family_success_refusal_and_typed_errors() {
    for name in NAMES {
        if name == "efaktura_modtag_workspace" {
            continue;
        }
        let core = success(name);
        let (api, t) = fixture(200, json!({"ok":true,"result":core}));
        assert_eq!(call(&api, name, args(name)).await.unwrap(), core, "{name}");
        t.join().unwrap();
        let core = json!({"ok":false,"errors":["synthetic refusal"],"appliedRules":[]});
        let (api, t) = fixture(200, json!({"ok":true,"result":core}));
        assert_eq!(call(&api, name, args(name)).await.unwrap(), core, "{name}");
        t.join().unwrap();
        let (api, t) = fixture(403, json!({"ok":false}));
        assert_eq!(
            call(&api, name, args(name)).await,
            Err(Error::Refused { status: 403 }),
            "{name}"
        );
        t.join().unwrap();
        let (api, t) = fixture(503, json!({"ok":false}));
        assert_eq!(
            call(&api, name, args(name)).await,
            Err(
                if matches!(
                    name,
                    "system_backup_destination_list"
                        | "system_backup_governance"
                        | "efaktura_onboarding_status"
                ) {
                    Error::Refused { status: 503 }
                } else {
                    Error::OutcomeUnknown
                }
            ),
            "{name}"
        );
        t.join().unwrap();
        if name != "system_backup_destination_remove" {
            let (api, t) = fixture(
                200,
                json!({"ok":true,"result":{"ok":true,"errors":[],"appliedRules":[]}}),
            );
            assert_eq!(
                call(&api, name, args(name)).await,
                Err(Error::Protocol),
                "{name}"
            );
            t.join().unwrap();
        }
    }
}
#[tokio::test]
async fn original717_contracts_sdk_optional_confirmation_and_workspace_binding() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for name in NAMES {
        if matches!(name, "efaktura_modtag_workspace" | "efaktura_konfigurer") {
            continue;
        }
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
                call(&api, name, a).await.unwrap()["code"],
                "CONFIRM_REQUIRED",
                "{name}"
            );
        }
    }
    assert!(matches!(
        call(
            &api,
            "efaktura_modtag_workspace",
            args("efaktura_modtag_workspace")
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
}
