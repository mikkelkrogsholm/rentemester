use rentemester_client::{ApiClient, Error, publish_original_mileage};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
fn server(status: u16, value: Value) -> (ApiClient, thread::JoinHandle<(String, Value)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        true,
        None,
    )
    .unwrap();
    let t = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(4)))
            .unwrap();
        let mut bytes = vec![];
        let end = loop {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..end]).to_string();
        let len: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap_or(0);
        while bytes.len() < end + len {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let body = if len == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[end..end + len]).unwrap()
        };
        let text = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        )
        .unwrap();
        (header, body)
    });
    (api, t)
}

const CSV: &str = "entry_no,trip_date,purpose,from_location,to_location,kilometers,vehicle,driver,rate_per_km,amount_basis,rate_basis,rate_source\n";
const JSON: &str = "{\n  \"appliedRules\": [\n    \"DK-MILEAGE-LOG-001\"\n  ],\n  \"from\": \"2026-01-01\",\n  \"to\": \"2026-01-31\",\n  \"entryCount\": 0,\n  \"totalKilometers\": 0,\n  \"totalAmountBasis\": 0,\n  \"note\": \"Mileage log only. Rentemester records the trips; tax treatment is the user/advisor's responsibility.\",\n  \"entries\": []\n}\n";
fn packet() -> Value {
    json!({"ok":true,"from":"2026-01-01","to":"2026-01-31","entryCount":0,"totalKilometers":0,"totalAmountBasis":0,"appliedRules":["DK-MILEAGE-LOG-001"],"errors":[],"artifacts":{"json":{"filename":"mileage-log.json","contentType":"application/json; charset=utf-8","bytes":JSON.as_bytes()},"csv":{"filename":"mileage-log.csv","contentType":"text/csv; charset=utf-8","bytes":CSV.as_bytes()}}})
}
#[test]
fn exact_bytes_replacement_both_validated_first_and_prepared_local_failure() {
    let out = std::env::temp_dir().join(format!("mileage-publisher-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    // Shared money wire values are f64 (0.0); source JSON artifact uses JS 0.
    let mut shared_wire = packet();
    shared_wire["totalKilometers"] = json!(0.0);
    shared_wire["totalAmountBasis"] = json!(0.0);
    let v = publish_original_mileage(&shared_wire, &out).unwrap();
    assert_eq!(v["ok"], true);
    assert!(v.get("artifacts").is_none());
    std::fs::write(out.join("unrelated"), b"keep").unwrap();
    std::fs::write(out.join("mileage-log.json"), b"old").unwrap();
    std::fs::write(out.join("mileage-log.csv"), b"old csv").unwrap();
    let mut bad = packet();
    bad["artifacts"]["csv"]["bytes"] = json!([300]);
    assert!(publish_original_mileage(&bad, &out).is_err());
    assert_eq!(std::fs::read(out.join("mileage-log.json")).unwrap(), b"old");
    publish_original_mileage(&packet(), &out).unwrap();
    assert_eq!(
        std::fs::read(out.join("mileage-log.json")).unwrap(),
        JSON.as_bytes()
    );
    assert_eq!(
        std::fs::read(out.join("mileage-log.csv")).unwrap(),
        CSV.as_bytes()
    );
    assert_eq!(std::fs::read(out.join("unrelated")).unwrap(), b"keep");
    std::fs::remove_file(out.join("mileage-log.csv")).unwrap();
    std::fs::create_dir(out.join("mileage-log.csv")).unwrap();
    let err = publish_original_mileage(&packet(), &out).unwrap_err();
    assert!(err.to_string().contains("local_export_failed"));
    assert!(err.to_string().contains("prepared server audit may exist"));
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 3);
    std::fs::remove_dir_all(out).unwrap();
}
#[tokio::test]
async fn source422_semantic_invalid_admitted_status_authority_unknown_and_no_outputdir() {
    let input=br#"{"tripDate":"","purpose":"","fromLocation":"","toLocation":"","vehicle":"","driver":"","rateBasis":"","kilometers":-1}"#;
    let refusal = json!({"ok":false,"appliedRules":["DK-MILEAGE-LOG-001"],"warnings":[],"errors":["tripDate is required","purpose is required"]});
    let (api, t) = server(422, refusal.clone());
    assert_eq!(
        api.original_mileage_log("synthetic", input, true)
            .await
            .unwrap(),
        refusal
    );
    let (h, b) = t.join().unwrap();
    assert!(h.starts_with("POST /api/companies/synthetic/original/mileage/log "));
    assert_eq!(b["kilometers"], -1);
    assert_eq!(b["confirm"], true);
    for (status, v, expected) in [
        (403, json!({}), Error::Refused { status: 403 }),
        (503, json!({}), Error::OutcomeUnknown),
        (200, json!({"ok":true}), Error::OutcomeUnknown),
    ] {
        let (api, t) = server(status, v);
        assert_eq!(
            api.original_mileage_log("synthetic", input, true)
                .await
                .unwrap_err(),
            expected
        );
        t.join().unwrap();
    }
    let (api, t) = server(200, packet());
    let v = api
        .original_mileage_export(
            "synthetic",
            br#"{"from":"2026-01-01","to":"2026-01-31"}"#,
            true,
        )
        .await
        .unwrap();
    assert_eq!(v, packet());
    let (_, b) = t.join().unwrap();
    assert!(b.get("outputDir").is_none());
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    assert!(matches!(
        api.original_mileage_log("synthetic", input, false).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.original_mileage_export(
            "synthetic",
            br#"{"from":"","to":"","outputDir":"forged"}"#,
            true
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
}
#[test]
fn meaningful_quoted_unicode_bytes_and_strict_dtos() {
    let mut packet = packet();
    let doc=JSON.replace("\"entryCount\": 0","\"entryCount\": 1").replace("\"totalKilometers\": 0","\"totalKilometers\": 2.5").replace("\"totalAmountBasis\": 0","\"totalAmountBasis\": 22.81").replace("\"entries\": []", "\"entries\": [\n    {\n      \"id\": 7,\n      \"entryNo\": \"MIL-2026-000001\",\n      \"tripDate\": \"2026-01-02\",\n      \"purpose\": \"Møde, \\\"Ø\\\"\\nnext\",\n      \"fromLocation\": \"A\",\n      \"toLocation\": \"B\",\n      \"kilometers\": 2.5,\n      \"vehicle\": \"Car\",\n      \"driver\": \"Ægir\",\n      \"ratePerKm\": 9.125,\n      \"amountBasis\": 22.81,\n      \"rateBasis\": \"Explicit\",\n      \"rateSource\": null,\n      \"notes\": \"not CSV\",\n      \"createdAt\": \"2026-01-02 12:00:00\"\n    }\n  ]");
    let csv = format!(
        "{CSV}MIL-2026-000001,2026-01-02,\"Møde, \"\"Ø\"\"\nnext\",A,B,2.5,Car,Ægir,9.125,22.81,Explicit,\n"
    );
    packet["entryCount"] = json!(1);
    packet["totalKilometers"] = json!(2.5);
    packet["totalAmountBasis"] = json!(22.81);
    packet["artifacts"]["json"]["bytes"] = json!(doc.as_bytes());
    packet["artifacts"]["csv"]["bytes"] = json!(csv.as_bytes());
    let out = std::env::temp_dir().join(format!("mileage-meaningful-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    publish_original_mileage(&packet, &out).unwrap();
    assert_eq!(
        std::fs::read(out.join("mileage-log.json")).unwrap(),
        doc.as_bytes()
    );
    assert_eq!(
        std::fs::read(out.join("mileage-log.csv")).unwrap(),
        csv.as_bytes()
    );
    for (key, value) in [
        ("filename", json!("../escape.csv")),
        ("contentType", json!("text/plain")),
        ("bytes", json!([1.5])),
    ] {
        let mut bad = packet.clone();
        bad["artifacts"]["csv"][key] = value;
        assert!(publish_original_mileage(&bad, &out).is_err());
        assert_eq!(
            std::fs::read(out.join("mileage-log.csv")).unwrap(),
            csv.as_bytes()
        );
    }
    let mut bad = packet.clone();
    bad["companyId"] = json!(7);
    assert!(publish_original_mileage(&bad, &out).is_err());
    std::fs::remove_dir_all(out).unwrap();
}
#[tokio::test]
async fn list_report_and_log_exact_receipts_reject_native_fields() {
    let list =
        json!({"ok":true,"count":0,"rows":[],"appliedRules":["DK-MILEAGE-LOG-001"],"errors":[]});
    let (api, t) = server(200, list.clone());
    assert_eq!(api.original_mileage_list("synthetic").await.unwrap(), list);
    let (h, _) = t.join().unwrap();
    assert!(h.starts_with("GET /api/companies/synthetic/original/mileage/list "));
    let report = json!({"ok":false,"from":"","to":"bad","entryCount":0,"totalKilometers":0,"totalAmountBasis":0,"entries":[],"appliedRules":["DK-MILEAGE-LOG-001"],"errors":["from must be a valid YYYY-MM-DD date","to must be a valid YYYY-MM-DD date"]});
    let (api, t) = server(422, report.clone());
    assert_eq!(
        api.original_mileage_report("synthetic", "", "bad")
            .await
            .unwrap(),
        report
    );
    let (h, _) = t.join().unwrap();
    assert!(h.contains("report?from=&to=bad"));
    let log = json!({"ok":true,"mileageEntryId":7,"entryNo":"MIL-2026-000001","amountBasis":22.81,"appliedRules":["DK-MILEAGE-LOG-001"],"warnings":["Advisory only"],"errors":[]});
    let input=br#"{"tripDate":"2026-01-02","purpose":"Trip","fromLocation":"A","toLocation":"B","kilometers":2.5,"vehicle":"Car","driver":"Driver","ratePerKm":9.125,"rateBasis":"Explicit"}"#;
    let (api, t) = server(200, log.clone());
    assert_eq!(
        api.original_mileage_log("synthetic", input, true)
            .await
            .unwrap(),
        log
    );
    t.join().unwrap();
    for (key, value) in [
        ("company", json!("synthetic")),
        ("entryId", json!(1)),
        ("amountBasis", json!("22.81")),
        ("mileageEntryId", json!(0)),
    ] {
        let mut bad = log.clone();
        bad[key] = value;
        let (api, t) = server(200, bad);
        assert_eq!(
            api.original_mileage_log("synthetic", input, true)
                .await
                .unwrap_err(),
            Error::OutcomeUnknown
        );
        t.join().unwrap();
    }
}
