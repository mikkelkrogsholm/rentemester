//! Current human company administration: retained facts and historical observations.
use crate::{ApiClient, Error, company_path};
use serde_json::Value;
fn hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn time(v: &Value) -> bool {
    let Some(s) = v.as_str() else { return false };
    let b = s.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || *b.last().unwrap() != b'Z'
    {
        return false;
    }
    if b.len() != 20
        && !(b.len() >= 22
            && b.len() <= 30
            && b[19] == b'.'
            && b[20..b.len() - 1].iter().all(u8::is_ascii_digit))
    {
        return false;
    }
    let n = |a, z| {
        s.get(a..z)
            .filter(|x| x.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|x| x.parse::<u32>().ok())
    };
    let (Some(y), Some(m), Some(d), Some(h), Some(min), Some(sec)) =
        (n(0, 4), n(5, 7), n(8, 10), n(11, 13), n(14, 16), n(17, 19))
    else {
        return false;
    };
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    y >= 1970 && d >= 1 && d <= days && h < 24 && min < 60 && sec < 60
}
fn strings(v: &Value) -> bool {
    v.as_array().is_some_and(|a| a.iter().all(Value::is_string))
}
fn validate(value: Value, company: &str) -> Result<Value, Error> {
    let b = &value["backup"];
    let g = &value["governance"];
    let lock = &value["lock"];
    let fail = || Error::Protocol;
    if value["ok"] != true
        || value["slug"] != company
        || !b.is_object()
        || !lock.is_object()
        || b.get("latestBackupId").is_none()
        || b.get("latestBackupAt").is_none()
        || g.get("latestBackupId").is_none()
        || b["scope"] != "local-retained-snapshots"
        || b["remoteSyncVerified"] != false
        || !time(&b["checkedAt"])
        || !strings(&b["errors"])
        || !b["backupDue"].is_boolean()
        || !lock["enforced"].is_boolean()
        || !lock["locked"].is_boolean()
        || g["format"] != "rentemester-native-backup-governance/1"
        || g["checkedAt"] != b["checkedAt"]
        || g["latestBackupId"] != b["latestBackupId"]
        || g["attestationIndependentlyVerified"] != false
        || g["recoveryVerified"] != false
        || !strings(&g["errors"])
    {
        return Err(fail());
    }
    if !(b["latestBackupId"].is_null() && b["latestBackupAt"].is_null()
        || hash(&b["latestBackupId"]) && time(&b["latestBackupAt"]))
    {
        return Err(fail());
    }
    let destinations = g["destinations"].as_array().ok_or_else(fail)?;
    let mut placed = 0u64;
    let mut remote = 0u64;
    let mut declared = 0u64;
    let mut remote_declared = false;
    let mut ids = std::collections::BTreeSet::new();
    for d in destinations {
        let id = d["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(fail)?;
        if !ids.insert(id)
            || !d["label"].is_string()
            || !d["offsiteAttested"].is_boolean()
            || d["attestationIndependentlyVerified"] != false
            || d["remoteSyncVerified"] != false
        {
            return Err(fail());
        }
        let ps = d["placements"].as_array().ok_or_else(fail)?;
        let rs = d["remoteVerifications"].as_array().ok_or_else(fail)?;
        declared += u64::from(d["offsiteAttested"] == true);
        placed += ps.len() as u64;
        remote += rs.len() as u64;
        for p in ps {
            if p["format"] != "rentemester-native-backup-placement/1"
                || p["destinationId"] != id
                || p["archiveSha256"] != b["latestBackupId"]
                || !hash(&p["archiveSha256"])
                || p["archiveSizeBytes"].as_u64().is_none_or(|n| n == 0)
                || !time(&p["backupCreatedAt"])
                || p["verified"] != true
                || p["signatureVerified"] != true
                || p["remoteSyncVerified"] != false
                || !p["operationId"].is_string()
            {
                return Err(fail());
            }
        }
        for r in rs {
            if r["format"] != "rentemester-native-remote-backup-verification/1"
                || r["destinationId"] != id
                || r["archiveSha256"] != b["latestBackupId"]
                || !hash(&r["archiveSha256"])
                || !time(&r["backupCreatedAt"])
                || !time(&r["observedAt"])
                || !time(&r["verifiedAt"])
                || r["remoteSyncVerified"] != true
                || r["signatureVerified"] != true
                || r["attestationIndependentlyVerified"] != false
                || !ps.iter().any(|p| {
                    p["operationId"] == r["placementOperationId"]
                        && p["archiveSha256"] == r["archiveSha256"]
                        && p["archiveSizeBytes"] == r["archiveSizeBytes"]
                        && p["backupCreatedAt"] == r["backupCreatedAt"]
                })
            {
                return Err(fail());
            }
        }
        remote_declared |= !rs.is_empty() && d["offsiteAttested"] == true;
    }
    for (field, n) in [
        ("destinationCount", destinations.len() as u64),
        ("declaredOffsiteDestinationCount", declared),
        ("latestBackupPlacementCount", placed),
        ("latestBackupRemoteVerificationCount", remote),
    ] {
        if g[field].as_u64() != Some(n) {
            return Err(fail());
        }
    }
    let qualified = remote > 0
        && b["errors"].as_array().is_some_and(Vec::is_empty)
        && g["errors"].as_array().is_some_and(Vec::is_empty);
    if g["latestBackupExternalCopyObserved"] != qualified
        || g["latestBackupExternalCopyAtDeclaredDestination"] != (qualified && remote_declared)
    {
        return Err(fail());
    }
    Ok(value)
}
impl ApiClient {
    /// One bodyless read. This does not contact a backup provider or test recovery.
    pub async fn backup_governance(&self, company: &str) -> Result<Value, Error> {
        let path = company_path(company, "backup-governance")?;
        if self.cookie.is_none() || self.service_key.is_some() {
            return Err(Error::InvalidInput(
                "current human MFA session cookie required",
            ));
        }
        validate(self.get(&path, &[]).await?, company)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn sample() -> Value {
        json!({"ok":true,"slug":"synthetic","backup":{"scope":"local-retained-snapshots","remoteSyncVerified":false,"checkedAt":"2026-10-21T12:00:00.000Z","latestBackupId":null,"latestBackupAt":null,"backupDue":true,"errors":[]},"lock":{"enforced":false,"locked":false},"governance":{"format":"rentemester-native-backup-governance/1","checkedAt":"2026-10-21T12:00:00.000Z","latestBackupId":null,"attestationIndependentlyVerified":false,"recoveryVerified":false,"destinationCount":0,"declaredOffsiteDestinationCount":0,"latestBackupPlacementCount":0,"latestBackupRemoteVerificationCount":0,"latestBackupExternalCopyObserved":false,"latestBackupExternalCopyAtDeclaredDestination":false,"destinations":[],"errors":[]}})
    }
    #[test]
    fn validates_exact_current_scope_counts_times_and_false_claims() {
        let good = sample();
        assert_eq!(validate(good.clone(), "synthetic").unwrap(), good);
        for (field, value) in [
            ("format", json!("wrong")),
            ("destinationCount", json!(1)),
            ("latestBackupPlacementCount", json!(-1)),
            ("latestBackupId", json!("bad")),
            ("checkedAt", json!("2026-99-99")),
            ("attestationIndependentlyVerified", json!(true)),
            ("recoveryVerified", json!(true)),
            ("latestBackupExternalCopyObserved", json!(true)),
        ] {
            let mut bad = good.clone();
            bad["governance"][field] = value;
            assert!(validate(bad, "synthetic").is_err(), "{field}")
        }
        assert!(validate(good, "other").is_err());
    }
    #[tokio::test]
    async fn rejects_service_missing_cookie_and_noncanonical_company_before_network() {
        for api in [
            ApiClient::new("http://127.0.0.1:9", true, None).unwrap(),
            ApiClient::with_service_key("http://127.0.0.1:9", true, "synthetic-service".into())
                .unwrap(),
        ] {
            assert!(matches!(
                api.backup_governance("synthetic").await,
                Err(Error::InvalidInput(_))
            ))
        }
        let api =
            ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
        for slug in ["../other", "synthetic?at=old", "synthetic%2Fother", ""] {
            assert!(matches!(
                api.backup_governance(slug).await,
                Err(Error::InvalidInput(_))
            ))
        }
    }
    #[test]
    fn matching_archive_join_is_required_and_new_archive_cannot_borrow_observation() {
        let mut v = sample();
        let digest = "a".repeat(64);
        let at = "2026-10-21T11:00:00.000Z";
        v["backup"]["latestBackupId"] = json!(digest);
        v["backup"]["latestBackupAt"] = json!(at);
        v["governance"]["latestBackupId"] = json!(digest);
        v["governance"]["destinations"] = json!([{"id":"drive-0001","label":"Synthetic","offsiteAttested":true,"attestationIndependentlyVerified":false,"remoteSyncVerified":false,"placements":[{"format":"rentemester-native-backup-placement/1","destinationId":"drive-0001","operationId":"placed-0001","archiveSha256":digest,"archiveSizeBytes":12,"backupCreatedAt":at,"verified":true,"signatureVerified":true,"remoteSyncVerified":false}],"remoteVerifications":[{"format":"rentemester-native-remote-backup-verification/1","destinationId":"drive-0001","placementOperationId":"placed-0001","archiveSha256":digest,"archiveSizeBytes":12,"backupCreatedAt":at,"observedAt":"2026-10-21T11:01:00.000Z","verifiedAt":"2026-10-21T11:01:01.000Z","remoteSyncVerified":true,"signatureVerified":true,"attestationIndependentlyVerified":false}]}]);
        for field in [
            "destinationCount",
            "declaredOffsiteDestinationCount",
            "latestBackupPlacementCount",
            "latestBackupRemoteVerificationCount",
        ] {
            v["governance"][field] = json!(1)
        }
        for field in [
            "latestBackupExternalCopyObserved",
            "latestBackupExternalCopyAtDeclaredDestination",
        ] {
            v["governance"][field] = json!(true)
        }
        assert_eq!(validate(v.clone(), "synthetic").unwrap(), v);
        let mut bad = v.clone();
        bad["governance"]["destinations"][0]["remoteVerifications"][0]["placementOperationId"] =
            json!("unrelated");
        assert!(validate(bad, "synthetic").is_err());
        let mut newer = v;
        newer["backup"]["latestBackupId"] = json!("b".repeat(64));
        newer["governance"]["latestBackupId"] = json!("b".repeat(64));
        assert!(validate(newer, "synthetic").is_err());
    }
    #[tokio::test]
    async fn exact_bodyless_cookie_get_preserves_server_value() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let expected = sample();
        let reply = expected.to_string();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            loop {
                let mut buf = [0; 4096];
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buf[..n]);
                if request.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",reply.len(),reply).unwrap();
            request
        });
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(api.backup_governance("synthetic").await.unwrap(), expected);
        let request = String::from_utf8(worker.join().unwrap()).unwrap();
        assert!(request.starts_with("GET /api/companies/synthetic/backup-governance HTTP/1.1\r\n"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("cookie: session=synthetic")
        );
        assert_eq!(request.split("\r\n\r\n").nth(1), Some(""));
    }
}
