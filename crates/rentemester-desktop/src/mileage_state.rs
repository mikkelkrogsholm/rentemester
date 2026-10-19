//! One private unresolved mileage attempt; no trip payload persistence or replay.
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
fn directory() -> PathBuf {
    crate::journal_attempt::directory()
}
fn private(path: &Path, directory: bool) -> Result<(), String> {
    let m = fs::symlink_metadata(path).map_err(|_| "Privat kørselsstatus kunne ikke åbnes")?;
    let uid = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(|_| "Brugeridentitet kunne ikke læses")?;
    if m.file_type().is_symlink()
        || m.is_dir() != directory
        || (!directory && !m.is_file())
        || m.mode() & 0o077 != 0
        || String::from_utf8_lossy(&uid.stdout)
            .trim()
            .parse::<u32>()
            .ok()
            != Some(m.uid())
    {
        return Err(
            "Kørselsstatus kræver din egen private mappe0700/fil0600 uden symbolske links.".into(),
        );
    }
    let acl = std::process::Command::new("/bin/ls")
        .arg("-lde")
        .arg(path)
        .output()
        .map_err(|_| "Privat adgangskontrol kunne ikke undersøges")?;
    if !acl.status.success()
        || String::from_utf8_lossy(&acl.stdout)
            .lines()
            .skip(1)
            .any(|line| !line.trim().is_empty())
    {
        return Err("Privat kørselsstatus må ikke have udvidede adgangslister.".into());
    }
    Ok(())
}
fn ensure() -> Result<PathBuf, String> {
    let dir = directory();
    if !dir.is_absolute() {
        return Err("Vælg en absolut privat statusmappe".into());
    }
    if !dir.exists() {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .map_err(|_| "Privat statusmappe kunne ikke oprettes")?;
    }
    private(&dir, true)?;
    if dir
        .canonicalize()
        .map_err(|_| "Privat statusmappe kunne ikke åbnes")?
        != dir
    {
        return Err("Statusmappen skal være en kanonisk sti".into());
    }
    Ok(dir)
}
pub fn load() -> Result<Option<Value>, String> {
    load_at(&directory())
}
fn load_at(dir: &Path) -> Result<Option<Value>, String> {
    let path = dir.join("pending-mileage.json");
    if !path
        .try_exists()
        .map_err(|_| "Kørselsstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Kørselsstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Kørselsstatus overskrider grænsen".into());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Kørselsstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret kørselsforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

pub fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-mileage/1"
        && ["origin", "company", "principal", "ledger", "year"]
            .iter()
            .all(|k| {
                value[k]
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s.len() <= 4096)
            })
        && value["baselineMaxId"].as_i64().is_some_and(|id| id >= 0)
        && value
            .get("localIdentity")
            .is_some_and(|v| v.is_null() || v.is_string())
        && value.as_object().is_some_and(|o| {
            o.keys().all(|k| {
                [
                    "format",
                    "origin",
                    "company",
                    "principal",
                    "ledger",
                    "year",
                    "localIdentity",
                    "baselineMaxId",
                ]
                .contains(&k.as_str())
            })
        })
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig kørselsstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-mileage.json"))
        .map_err(|_| "Et kørselsforsøg kræver afklaring før ny registrering")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Kørselsstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Kørselsstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Kørselsstatus ændrede sig og bevares".into());
    }
    if manual {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "Systemtid kunne ikke læses")?
            .as_nanos();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join(format!("mileage-manually-resolved-{suffix}.json")))
            .map_err(|_| "Kørselsafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Kørselsafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-mileage.json"))
        .map_err(|_| "Kørselsstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Kørselsstatus kunne ikke synkroniseres".into())
}

/// Entire server JSON remains available; display does not reconstruct amounts or fiscal periods.
#[derive(Clone)]
pub struct MileageReport {
    pub root: Value,
    pub principal: String,
    pub ledger: String,
}
impl MileageReport {
    pub fn parse(
        root: Value,
        me: Value,
        journal: Value,
        company: &str,
        year: &str,
    ) -> Result<Self, String> {
        let r = &root["mileage"];
        if year.is_empty()
            || root["ok"] != true
            || r["slug"] != company
            || r["selectedYear"] != year
            || me["ok"] != true
            || journal["ok"] != true
            || journal["journal"]["slug"] != company
            || journal["journal"]["selectedYear"] != year
        {
            return Err(
                "Serveren bekræftede ikke den valgte virksomhed og det eksplicitte regnskabsår"
                    .into(),
            );
        }
        let principal = me["user"]["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Aktuel bruger mangler")?
            .to_owned();
        let ledger = journal["journal"]["ledgerUuid"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Ledgeridentitet mangler")?
            .to_owned();
        if r["entries"].as_array().is_none()
            || r["months"].as_array().is_none()
            || r["fiscalYears"].as_array().is_none()
            || !["periodStart", "periodEnd"]
                .iter()
                .all(|k| r[k].as_str().is_some_and(|s| s.len() == 10))
            || !["totalKilometers", "totalAmountBasis", "tripCount"]
                .iter()
                .all(|k| r[k].is_number())
        {
            return Err("Serverrapporten mangler periode, originale kørsler eller totaler".into());
        }
        if r["months"].as_array().unwrap().iter().any(|m| {
            !m["label"].is_string()
                || !["tripCount", "kilometers", "amountBasis"]
                    .iter()
                    .all(|k| m[k].is_number())
        }) || r["fiscalYears"]
            .as_array()
            .unwrap()
            .iter()
            .any(|y| !y["label"].is_string())
        {
            return Err(
                "Serverens månedsopgørelse eller regnskabsår mangler kanoniske felter".into(),
            );
        }
        let mut ids = std::collections::BTreeSet::new();
        for e in r["entries"].as_array().unwrap() {
            let id = e["id"]
                .as_i64()
                .filter(|n| *n > 0)
                .ok_or("Kørsels-id mangler")?;
            if !ids.insert(id)
                || ![
                    "entryNo",
                    "tripDate",
                    "purpose",
                    "fromLocation",
                    "toLocation",
                    "vehicle",
                    "driver",
                    "rateBasis",
                    "createdAt",
                ]
                .iter()
                .all(|k| e[k].is_string())
                || !["kilometers", "ratePerKm", "amountBasis"]
                    .iter()
                    .all(|k| e[k].is_number())
            {
                return Err("Kørslens kanoniske identitet eller beløb mangler".into());
            }
        }
        Ok(Self {
            root,
            principal,
            ledger,
        })
    }
    pub fn data(&self) -> &Value {
        &self.root["mileage"]
    }
    pub fn marker(&self, origin: &str, company: &str, local: Option<&str>) -> Value {
        serde_json::json!({"format":"rentemester-desktop-mileage/1","origin":origin,"company":company,"principal":self.principal,"ledger":self.ledger,"year":self.data()["selectedYear"],"localIdentity":local,"baselineMaxId":self.data()["entries"].as_array().into_iter().flatten().filter_map(|e|e["id"].as_i64()).max().unwrap_or(0)})
    }
    pub fn same_scope(
        &self,
        marker: &Value,
        origin: &str,
        company: &str,
        local: Option<&str>,
    ) -> bool {
        let current = self.marker(origin, company, local);
        valid_record(marker)
            // A managed local server can receive a new loopback port after restart.
            // Its trusted profile, principal and ledger must still match exactly.
            && (marker["origin"] == current["origin"] || local.is_some())
            && [
                "company",
                "principal",
                "ledger",
                "year",
                "localIdentity",
            ]
            .iter()
            .all(|k| marker[k] == current[k])
    }
}
#[derive(Clone)]
pub struct RegisterMileage {
    pub body: Vec<u8>,
}
impl RegisterMileage {
    pub fn review(values: &[(String, String)], report: &MileageReport) -> Result<Self, String> {
        let mut out = serde_json::Map::new();
        for key in [
            "tripDate",
            "purpose",
            "fromLocation",
            "toLocation",
            "kilometers",
            "vehicle",
            "driver",
            "ratePerKm",
            "rateBasis",
            "rateSource",
            "notes",
        ] {
            let raw = values
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.trim())
                .unwrap_or("");
            if raw.len() > 4096 {
                return Err("Et kørselsfelt overskrider4096 bytes".into());
            }
            if raw.is_empty() {
                if ["rateSource", "notes"].contains(&key) {
                    continue;
                }
                return Err(
                    "Udfyld dato, formål, rute, km, køretøj, chauffør, sats og satsgrundlag".into(),
                );
            }
            let value = if ["kilometers", "ratePerKm"].contains(&key) {
                Value::Number(raw.parse::<serde_json::Number>().map_err(
                    |_| "Km og sats skal være tal med decimalpunktum; serveren beregner beløbet",
                )?)
            } else {
                Value::String(raw.into())
            };
            out.insert(key.into(), value);
        }
        let date = out["tripDate"].as_str().unwrap();
        if date.len() != 10
            || !date.bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'-'
                } else {
                    b.is_ascii_digit()
                }
            })
            || date < report.data()["periodStart"].as_str().unwrap()
            || date > report.data()["periodEnd"].as_str().unwrap()
        {
            return Err(
                "Vælg en dato i det indlæste regnskabsår; serveren validerer kalenderdatoen".into(),
            );
        }
        out.insert("confirm".into(), Value::Bool(true));
        let body =
            serde_json::to_vec(&out).map_err(|_| "Kørselsanmodningen kunne ikke fastholdes")?;
        if body.len() > 32 * 1024 {
            return Err("Kørselsanmodningen overskrider serverens32KiB".into());
        }
        Ok(Self { body })
    }
}
pub fn receipt(value: &Value) -> Result<Value, String> {
    let r = &value["mileage"];
    if value["ok"] != true
        || !r["mileageEntryId"].as_i64().is_some_and(|id| id > 0)
        || !r["entryNo"].is_string()
        || !r["amountBasis"].is_number()
        || r["warnings"]
            .as_array()
            .is_none_or(|a| a.iter().any(|v| !v.is_string()))
        || r["appliedRules"]
            .as_array()
            .is_none_or(|a| a.iter().any(|v| !v.is_string()))
    {
        return Err(
            "Serverkvitteringen er ufuldstændig; registreringen kan være gennemført".into(),
        );
    }
    Ok(r.clone())
}
pub fn readback<'a>(report: &'a MileageReport, receipt: &Value) -> Option<&'a Value> {
    report.data()["entries"].as_array()?.iter().find(|e| {
        e["id"] == receipt["mileageEntryId"]
            && e["entryNo"] == receipt["entryNo"]
            && e["amountBasis"] == receipt["amountBasis"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn report() -> MileageReport {
        MileageReport::parse(json!({"ok":true,"mileage":{"slug":"synthetic","selectedYear":"2026/27","periodStart":"2026-07-01","periodEnd":"2027-06-30","entries":[],"months":[],"fiscalYears":[],"tripCount":0,"totalKilometers":0,"totalAmountBasis":0,"futureEvidence":{"preserved":true}}}),json!({"ok":true,"user":{"id":"synthetic-owner"}}),json!({"ok":true,"journal":{"slug":"synthetic","selectedYear":"2026/27","ledgerUuid":"synthetic-ledger"}}),"synthetic","2026/27").unwrap()
    }
    fn form() -> Vec<(String, String)> {
        [
            ("tripDate", "2026-07-01"),
            ("purpose", "Synthetic visit"),
            ("fromLocation", "A"),
            ("toLocation", "B"),
            ("kilometers", "12.345"),
            ("vehicle", "Synthetic car"),
            ("driver", "Synthetic driver"),
            ("ratePerKm", "3.791"),
            ("rateBasis", "Explicit synthetic basis"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect()
    }
    #[test]
    fn exact_chosen_precision_without_client_amount_rate_default_or_keys() {
        let r = report();
        let review = RegisterMileage::review(&form(), &r).unwrap();
        let v: Value = serde_json::from_slice(&review.body).unwrap();
        assert_eq!(v["kilometers"].to_string(), "12.345");
        assert_eq!(v["ratePerKm"].to_string(), "3.791");
        assert!(v.get("amountBasis").is_none());
        assert!(v.get("idempotencyKey").is_none());
        assert_eq!(v["confirm"], true);
        assert_eq!(r.data()["futureEvidence"]["preserved"], true);
        let mut f = form();
        f.retain(|(k, _)| k != "rateBasis");
        assert!(RegisterMileage::review(&f, &r).is_err());
    }
    #[test]
    fn shifted_year_and_essential_form_fields_are_explicit() {
        let r = report();
        for (key, value) in [
            ("tripDate", "2026-06-30"),
            ("kilometers", "12,35"),
            ("ratePerKm", "NaN"),
            ("driver", ""),
        ] {
            let mut f = form();
            f.iter_mut().find(|(k, _)| k == key).unwrap().1 = value.into();
            assert!(RegisterMileage::review(&f, &r).is_err());
        }
    }
    #[test]
    fn unknown_scope_blocks_other_origin_company_principal_ledger_year_or_local_profile() {
        let r = report();
        let marker = r.marker(
            "https://synthetic.invalid",
            "synthetic",
            Some("private-profile"),
        );
        assert!(valid_record(&marker));
        assert!(r.same_scope(
            &marker,
            "https://synthetic.invalid",
            "synthetic",
            Some("private-profile")
        ));
        for key in [
            "company",
            "principal",
            "ledger",
            "year",
            "localIdentity",
        ] {
            let mut changed = marker.clone();
            changed[key] = json!("other");
            assert!(!r.same_scope(
                &changed,
                "https://synthetic.invalid",
                "synthetic",
                Some("private-profile")
            ));
        }
        let mut unsafe_marker = marker;
        unsafe_marker["requestBody"] = json!("not persisted");
        assert!(!valid_record(&unsafe_marker));
    }
    #[test]
    fn managed_local_restart_rebinds_port_without_rebinding_identity() {
        let report = report();
        let marker = report.marker("http://127.0.0.1:40001", "synthetic", Some("private-profile"));
        assert!(report.same_scope(&marker, "http://127.0.0.1:40002", "synthetic", Some("private-profile")));
        assert!(!report.same_scope(&marker, "http://127.0.0.1:40002", "synthetic", Some("other-profile")));
        assert!(!report.same_scope(&marker, "http://127.0.0.1:40002", "synthetic", None));
        let remote = report.marker("https://first.invalid", "synthetic", None);
        assert!(!report.same_scope(&remote, "https://other.invalid", "synthetic", None));
        let mut wrong_ledger = marker;
        wrong_ledger["ledger"] = json!("different-ledger");
        assert!(!report.same_scope(&wrong_ledger, "http://127.0.0.1:40002", "synthetic", Some("private-profile")));
    }
    #[test]
    fn successful_receipt_requires_actual_canonical_entry_and_original_server_advisory() {
        let value = json!({"ok":true,"mileage":{"mileageEntryId":7,"entryNo":"MIL-SYNTHETIC-7","amountBasis":46.82,"warnings":["Original native advisory"],"appliedRules":["native-rule"]}});
        let got = receipt(&value).unwrap();
        let mut r = report();
        assert!(readback(&r, &got).is_none());
        r.root["mileage"]["entries"] =
            json!([{"id":7,"entryNo":"MIL-SYNTHETIC-7","amountBasis":46.82}]);
        assert!(readback(&r, &got).is_some());
        assert_eq!(got["warnings"][0], "Original native advisory");
        r.root["mileage"]["entries"][0]["amountBasis"] = json!(46.81);
        assert!(readback(&r, &got).is_none());
    }
    #[test]
    fn durable_marker_no_overwrite_and_explicit_manual_resolution() {
        let dir = std::env::temp_dir().join(format!(
            "rm-mileage-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let m = report().marker("https://synthetic.invalid", "synthetic", None);
        begin_at(&dir, &m).unwrap();
        assert!(begin_at(&dir, &m).is_err());
        assert_eq!(load_at(&dir).unwrap(), Some(m.clone()));
        finish_at(&dir, &m, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        fs::remove_dir_all(&dir).unwrap();
    }
}
