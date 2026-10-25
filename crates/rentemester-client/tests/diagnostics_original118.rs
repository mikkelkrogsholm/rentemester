mod common;
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};

fn retention() -> Value {
    json!({"ok":true,"asOf":"2026-10-25","appliedRules":["DK-BOOKKEEPING-RETENTION-001"],"rows":[{"table":"documents","total":2,"expired":1,"nextExpiry":"2030-01-01","oldestExpired":"2020-01-01"},{"table":"journal_entries","total":0,"expired":0,"nextExpiry":null,"oldestExpired":null},{"table":"bank_transactions","total":0,"expired":0,"nextExpiry":null,"oldestExpired":null}],"errors":[]})
}
fn health(ok: bool) -> Value {
    json!({"ok":ok,"checks":[{"name":"company_root","ok":true},{"name":"data_dir","ok":true},{"name":"ledger","ok":true},{"name":"documents","ok":true},{"name":"config","ok":true}],"missing":if ok {json!([])} else {json!(["schema"])},"schema_outdated":false,"schema":{"status":if ok {"current"} else {"corrupt"},"currentVersion":1,"requiredVersion":1,"pending":[]},"errors":if ok {json!([])} else {json!(["schema_corrupt: current=1 required=1"])}})
}
async fn invoke(
    retention: bool,
    date: Option<&str>,
    status: u16,
    body: Value,
    service: bool,
) -> (Result<Value, Error>, String) {
    let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut wire = Vec::new();
        while !wire.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            s.read_exact(&mut byte).unwrap();
            wire.push(byte[0]);
        }
        let body = body.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Reply\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        drop(s);
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(50));
        assert!(listener.accept().is_err(), "diagnostics must not retry");
        String::from_utf8(wire).unwrap()
    });
    let api = if service {
        ApiClient::with_service_key(&origin, true, "synthetic-service-key".into()).unwrap()
    } else {
        ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap()
    };
    let result = if retention {
        api.retention_status_original118("synthetic", date).await
    } else {
        api.system_healthcheck_original118("synthetic").await
    };
    (result, task.join().unwrap())
}
#[tokio::test]
async fn original_diagnostics118_retention_dates_and_business_false_are_server_owned() {
    for date in [
        None,
        Some("2026-10-25"),
        Some(""),
        Some("2026-02-30"),
        Some("x&actor=forged"),
    ] {
        let mut core = retention();
        if let Some(date) = date {
            core["asOf"] = json!(date);
            if date != "2026-10-25" {
                core["ok"] = json!(false);
                core["rows"] = json!([]);
                core["errors"] = json!(["asOf must be YYYY-MM-DD"]);
            }
        }
        let (result, wire) = invoke(
            true,
            date,
            200,
            json!({"ok":true,"retentionStatus":core}),
            false,
        )
        .await;
        assert_eq!(result.unwrap(), core);
        let target = wire
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        let url = reqwest::Url::parse(&format!("http://localhost{target}")).unwrap();
        assert_eq!(url.path(), "/api/companies/synthetic/retention-status");
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            date.map(|d| vec![("asOf".into(), d.into())])
                .unwrap_or_default()
        );
        assert!(wire.to_lowercase().contains("cookie: session=synthetic"));
    }
}
#[tokio::test]
async fn original_diagnostics118_health_false_keeps_data_and_service_credentials() {
    for ok in [true, false] {
        let core = health(ok);
        let (result, wire) = invoke(
            false,
            None,
            200,
            json!({"ok":true,"healthcheck":core}),
            true,
        )
        .await;
        assert_eq!(result.unwrap(), core);
        assert!(wire.starts_with("GET /api/companies/synthetic/system-healthcheck HTTP/1.1"));
        assert!(!wire.to_lowercase().contains("cookie:"));
        assert!(wire.contains("synthetic-service-key"));
    }
}
#[tokio::test]
async fn original_diagnostics118_malformed_envelopes_and_current_refusals_never_default() {
    for is_retention in [true, false] {
        let field = if is_retention {
            "retentionStatus"
        } else {
            "healthcheck"
        };
        for body in [
            json!({}),
            json!({"ok":true}),
            json!({"ok":false}),
            json!({"ok":true,(field):[]}),
            json!({"ok":true,(field):{"ok":true}}),
        ] {
            assert!(matches!(
                invoke(is_retention, None, 200, body, false).await.0,
                Err(Error::Protocol)
            ));
        }
        for status in [401, 403, 409, 429, 503] {
            assert!(
                matches!(invoke(is_retention,None,status,json!({"ok":false}),false).await.0,Err(Error::Refused{status:s}) if s==status)
            );
        }
    }
    let mut broken = retention();
    broken["rows"][0]["total"] = json!("2");
    assert!(matches!(
        invoke(
            true,
            None,
            200,
            json!({"ok":true,"retentionStatus":broken}),
            false
        )
        .await
        .0,
        Err(Error::Protocol)
    ));
    let mut broken = health(false);
    broken["errors"] = json!([]);
    assert!(matches!(
        invoke(
            false,
            None,
            200,
            json!({"ok":true,"healthcheck":broken}),
            false
        )
        .await
        .0,
        Err(Error::Protocol)
    ));
}
#[tokio::test]
async fn original_diagnostics118_truncated_or_contradictory_success_is_not_healthy() {
    for retention_case in [true, false] {
        let original = if retention_case { retention() } else { health(true) };
        let field = if retention_case { "retentionStatus" } else { "healthcheck" };
        let mut bad = vec![];
        if retention_case {
            let mut truncated = original.clone();
            truncated["rows"].as_array_mut().unwrap().pop();
            bad.push(truncated);
            let mut duplicate = original.clone();
            duplicate["rows"][1]["table"] = json!("documents");
            bad.push(duplicate);
        } else {
            let mut truncated = original.clone();
            truncated["checks"].as_array_mut().unwrap().pop();
            bad.push(truncated);
            let mut failed = original.clone();
            failed["checks"][2]["ok"] = json!(false);
            bad.push(failed);
            let mut missing = original.clone();
            missing["missing"] = json!(["schema"]);
            bad.push(missing);
            let mut schema = original.clone();
            schema.as_object_mut().unwrap().remove("schema");
            bad.push(schema);
        }
        for core in bad {
            assert!(matches!(invoke(retention_case, None, 200, json!({"ok":true,(field):core}), false).await.0, Err(Error::Protocol)));
        }
    }
    let mut corrupt = health(false);
    corrupt["schema"]["currentVersion"] = json!(-1);
    assert_eq!(invoke(false, None, 200, json!({"ok":true,"healthcheck":corrupt}), false).await.0.unwrap(), corrupt);
}
#[tokio::test]
async fn original_diagnostics118_unverified_paths_and_traversal_refuse_before_network() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let api = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        true,
        Some("session=synthetic".into()),
    )
    .unwrap();
    for company in [
        "",
        "../synthetic",
        "synthetic/nested",
        "/unverified/synthetic",
        "synthetic?actor=x",
        ".",
        "synthetic/../other",
    ] {
        assert!(matches!(
            api.retention_status_original118(company, None).await,
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            api.system_healthcheck_original118(company).await,
            Err(Error::InvalidInput(_))
        ));
    }
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn original_diagnostics118_verified_local_selectors_use_unix_and_reverify_root() {
    use rentemester_client::LocalRuntime;
    use std::{
        fs,
        os::unix::{
            fs::{DirBuilderExt, PermissionsExt},
            net::UnixListener,
        },
    };
    let root = fs::canonicalize(std::env::var_os("HOME").unwrap())
        .unwrap()
        .join(format!(".rm118-diag-{}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(root.join("workspace"))
        .unwrap();
    let listener = UnixListener::bind(root.join("r.sock")).unwrap();
    fs::set_permissions(root.join("r.sock"), fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(root.join("r.json"),json!({"format":"rentemester-local-runtime/1","origin":"http://127.0.0.1:9","socket":"r.sock","workspaceRoot":root.join("workspace"),"workspaceId":"synthetic-workspace"}).to_string()).unwrap();
    fs::set_permissions(root.join("r.json"), fs::Permissions::from_mode(0o600)).unwrap();
    let local = LocalRuntime::load(&root).unwrap();
    let api = ApiClient::new_local(&local, Some("session=synthetic".into())).unwrap();
    for suffix in [
        "workspace/../synthetic",
        "workspace/nested/synthetic",
        "elsewhere/synthetic",
        "workspace//synthetic",
        "workspace/synthetic/",
    ] {
        assert!(matches!(
            api.system_healthcheck_original118(root.join(suffix).to_str().unwrap())
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err());
    listener.set_nonblocking(false).unwrap();
    let expected = health(false);
    let reply = json!({"ok":true,"healthcheck":expected}).to_string();
    let task = thread::spawn(move || {
        let (mut stream, _) = common::unix_accept(&listener).unwrap();
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            stream.read_exact(&mut b).unwrap();
            bytes.push(b[0]);
        }
        assert!(
            String::from_utf8(bytes)
                .unwrap()
                .starts_with("GET /api/companies/synthetic/system-healthcheck HTTP/1.1")
        );
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
    });
    // The secondary company need not exist locally: only the server inspects it.
    assert_eq!(
        api.system_healthcheck_original118(root.join("workspace/synthetic").to_str().unwrap())
            .await
            .unwrap(),
        expected
    );
    task.join().unwrap();
    fs::set_permissions(root.join("workspace"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        api.retention_status_original118(root.join("workspace/synthetic").to_str().unwrap(), None)
            .await,
        Err(Error::InvalidInput(_))
    ));
    fs::remove_dir_all(root).unwrap();
}
