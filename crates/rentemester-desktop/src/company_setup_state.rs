//! Exact reviewed company setup and a private unresolved marker, never a replay queue.
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
    let m = fs::symlink_metadata(path)
        .map_err(|_| "Privat virksomhedsopsætningsstatus kunne ikke åbnes")?;
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
            "Virksomhedsopsætningsstatus kræver din egen private mappe0700/fil0600 uden symbolske links."
                .into(),
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
        return Err(
            "Privat virksomhedsopsætningsstatus må ikke have udvidede adgangslister.".into(),
        );
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
    let path = dir.join("pending-company-setup.json");
    if !path
        .try_exists()
        .map_err(|_| "Virksomhedsopsætningsstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Virksomhedsopsætningsstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Virksomhedsopsætningsstatus overskrider grænsen".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Virksomhedsopsætningsstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret virksomhedsopsætningsforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.len() == 7
        && object.keys().all(|key| {
            [
                "format",
                "origin",
                "company",
                "principal",
                "ledger",
                "localIdentity",
                "operation",
            ]
            .contains(&key.as_str())
        })
        && value["format"] == "rentemester-desktop-company-setup/1"
        && ["origin", "company", "principal", "ledger"]
            .iter()
            .all(|key| {
                value[*key].as_str().is_some_and(|s| {
                    !s.trim().is_empty() && s.len() <= 2048 && !s.chars().any(char::is_control)
                })
            })
        && (value["localIdentity"].is_null()
            || value["localIdentity"]
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.len() <= 2048))
        && matches!(
            value["operation"].as_str(),
            Some("profile" | "account" | "role")
        )
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig virksomhedsopsætningsstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-company-setup.json"))
        .map_err(|_| "Et virksomhedsopsætningsforsøg kræver afklaring før ny ændring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Virksomhedsopsætningsstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Virksomhedsopsætningsstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Virksomhedsopsætningsstatus ændrede sig og bevares".into());
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
            .open(dir.join(format!("company-setup-manually-resolved-{suffix}.json")))
            .map_err(|_| "Opsætningsafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Opsætningsafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-company-setup.json"))
        .map_err(|_| "Virksomhedsopsætningsstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Virksomhedsopsætningsstatus kunne ikke synkroniseres".into())
}

use serde_json::{Map, json};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Profile,
    Account,
    Role,
}
impl Operation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::Account => "account",
            Self::Role => "role",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub profile: Value,
    pub chart: Value,
    pub principal: String,
    pub ledger: String,
}
fn required(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Serveren mangler {key}"))
}
impl Snapshot {
    pub fn parse(
        company: &str,
        profile: Value,
        chart: Value,
        me: Value,
        journal: Value,
    ) -> Result<Self, String> {
        if [&profile, &chart, &me, &journal]
            .iter()
            .any(|v| v["ok"] != true)
            || chart["accounts"]["slug"] != company
            || journal["journal"]["slug"] != company
            || !profile["company"]["id"].as_i64().is_some_and(|id| id > 0)
            || chart["accounts"]["company"] != profile["company"]
            || !chart["accounts"]["accounts"].is_array()
            || !chart["accounts"]["accountRoles"]["resolutions"].is_array()
            || !chart["accounts"]["accountRoles"]["missing"].is_array()
            || !chart["accounts"]["accountRoles"]["ambiguous"].is_array()
        {
            return Err(
                "Serveren bekræftede ikke virksomhedens profil, kontoplan og roller".into(),
            );
        }
        Ok(Self {
            profile: profile["company"].clone(),
            chart: chart["accounts"].clone(),
            principal: required(&me["user"], "id")?,
            ledger: required(&journal["journal"], "ledgerUuid")?,
        })
    }
    pub fn marker(
        &self,
        operation: Operation,
        origin: &str,
        company: &str,
        local: Option<&str>,
    ) -> Value {
        json!({"format":"rentemester-desktop-company-setup/1","origin":origin,"company":company,"principal":self.principal,"ledger":self.ledger,"localIdentity":local,"operation":operation.name()})
    }
    pub fn matches_marker(
        &self,
        record: &Value,
        origin: &str,
        company: &str,
        local: Option<&str>,
    ) -> bool {
        valid_record(record)
            && record["company"] == company
            && record["principal"] == self.principal
            && record["ledger"] == self.ledger
            && record["localIdentity"].as_str() == local
            && (record["origin"] == origin
                || local.is_some_and(|id| record["localIdentity"].as_str() == Some(id)))
    }
}
pub const PROFILE: &[(&str, &str)] = &[
    ("cvr", "CVR"),
    ("address", "Adresse"),
    ("postalCode", "Postnummer"),
    ("city", "By"),
    (
        "vatPeriodType",
        "Momsperiode: month, quarter, half-year eller none",
    ),
    ("paymentTermsDays", "Betalingsfrist i dage (0–365)"),
];
pub const ACCOUNT: &[(&str, &str)] = &[
    ("accountNo", "Kontonummer"),
    ("name", "Kontonavn"),
    (
        "type",
        "Kontotype: asset, liability, equity, income, expense eller vat",
    ),
    ("normalBalance", "Normal saldo: debit eller credit"),
    ("defaultVatCode", "Valgfri eksplicit momskode (tom = ingen)"),
];
pub const ROLE: &[(&str, &str)] = &[
    (
        "role",
        "Rolle: bank, debtors, creditors, output_vat, input_vat, reverse_charge_vat, vat_settlement eller operational_default",
    ),
    ("accountNo", "Valgt eksisterende kontonummer"),
];
pub fn fields(op: Operation) -> &'static [(&'static str, &'static str)] {
    match op {
        Operation::Profile => PROFILE,
        Operation::Account => ACCOUNT,
        Operation::Role => ROLE,
    }
}
pub fn body(
    op: Operation,
    values: &[(String, String)],
    snapshot: &Snapshot,
) -> Result<Vec<u8>, String> {
    let get = |key: &str| {
        values
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.trim())
            .unwrap_or("")
    };
    let mut out = Map::new();
    for (key, _) in fields(op) {
        let text = get(key);
        if text.len() > 1024 || text.chars().any(char::is_control) {
            return Err("Feltet er for langt eller indeholder kontroltegn".into());
        }
        let value = match (op, *key) {
            (Operation::Profile, "vatPeriodType") => match text {
                "none" => Value::Null,
                "month" | "quarter" | "half-year" => json!(text),
                _ => {
                    return Err(
                        "Vælg momsperioden eksplicit, eller skriv none for ingen registrering"
                            .into(),
                    );
                }
            },
            (Operation::Profile, "paymentTermsDays") => json!(
                text.parse::<u16>()
                    .ok()
                    .filter(|n| *n <= 365)
                    .ok_or("Betalingsfrist skal være et heltal 0–365")?
            ),
            (Operation::Account, "type")
                if !["asset", "liability", "equity", "income", "expense", "vat"]
                    .contains(&text) =>
            {
                return Err("Vælg kontotype eksplicit".into());
            }
            (Operation::Account, "normalBalance") if !["debit", "credit"].contains(&text) => {
                return Err("Vælg debit eller credit eksplicit".into());
            }
            (Operation::Role, "role")
                if ![
                    "bank",
                    "debtors",
                    "creditors",
                    "output_vat",
                    "input_vat",
                    "reverse_charge_vat",
                    "vat_settlement",
                    "operational_default",
                ]
                .contains(&text) =>
            {
                return Err("Vælg en eksisterende serverrolle".into());
            }
            (Operation::Account, "defaultVatCode") if text.is_empty() => continue,
            (Operation::Account | Operation::Role, _) if text.is_empty() => {
                return Err("Udfyld de påkrævede felter".into());
            }
            _ => json!(text),
        };
        if op != Operation::Profile
            || (snapshot.profile[*key] != value
                && !(snapshot.profile[*key].is_null() && value == ""))
        {
            out.insert((*key).into(), value);
        }
    }
    if out.is_empty() {
        return Err("Ingen ændrede profilfelter at gennemgå".into());
    }
    if op == Operation::Role {
        if !snapshot.chart["accounts"].as_array().is_some_and(|rows| {
            rows.iter()
                .any(|row| row["accountNo"] == get("accountNo") && row["active"] == true)
        }) {
            return Err("Vælg en aktiv konto fra serverens kontoplan".into());
        }
        out.insert("confirm".into(), json!(true));
    }
    serde_json::to_vec(&Value::Object(out)).map_err(|_| "Opsætningen kunne ikke fastholdes".into())
}
#[derive(Clone)]
pub struct Review {
    pub operation: Operation,
    pub bytes: Vec<u8>,
    pub before: Snapshot,
}
impl Review {
    pub fn current(&self, now: &Snapshot) -> bool {
        self.before.principal == now.principal
            && self.before.ledger == now.ledger
            && self.before.profile == now.profile
            && self.before.chart == now.chart
    }
    pub fn receipt(&self, value: &Value) -> bool {
        if value["ok"] != true {
            return false;
        }
        let Ok(body) = serde_json::from_slice::<Value>(&self.bytes) else {
            return false;
        };
        match self.operation {
            Operation::Profile => {
                value["company"].is_object() && value["company"]["id"] == self.before.profile["id"]
            }
            Operation::Account => value["account"]["accountNo"] == body["accountNo"],
            Operation::Role => {
                value["resolution"]["ok"] == true
                    && value["resolution"]["role"] == body["role"]
                    && value["resolution"]["accountNo"] == body["accountNo"]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        let profile = json!({"id":1,"name":"Synthetic Company","cvr":null,"address":null,"postalCode":null,"city":null,"vatPeriodType":null,"paymentTermsDays":0});
        Snapshot::parse("synthetic",json!({"ok":true,"company":profile}),json!({"ok":true,"accounts":{"slug":"synthetic","company":profile,"accounts":[{"accountNo":"A","name":"Synthetic account","type":"asset","normalBalance":"debit","active":true}],"accountRoles":{"status":"incomplete","missing":["bank"],"ambiguous":[],"resolutions":[{"ok":false,"role":"bank"}]}}}),json!({"ok":true,"user":{"id":"synthetic-user"}}),json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"synthetic-ledger"}})).unwrap()
    }
    fn values(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
    #[test]
    fn profile_patch_preserves_omitted_payment_and_unchanged_fields() {
        let data = snapshot();
        let fields = values(&[
            ("cvr", ""),
            ("address", "Synthetic Street"),
            ("postalCode", ""),
            ("city", ""),
            ("vatPeriodType", "none"),
            ("paymentTermsDays", "0"),
        ]);
        let bytes = body(Operation::Profile, &fields, &data).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            json!({"address":"Synthetic Street"})
        );
        let mut updated = data.clone();
        updated.profile["address"] = json!("Synthetic Street");
        assert!(body(Operation::Profile, &fields, &updated).is_err());
        for bad in ["", "annual", "guess"] {
            let mut v = fields.clone();
            v[4].1 = bad.into();
            assert!(body(Operation::Profile, &v, &data).is_err());
        }
        let mut v = fields.clone();
        v[5].1 = "366".into();
        assert!(body(Operation::Profile, &v, &data).is_err());
    }
    #[test]
    fn chosen_cadence_and_clearing_address_are_explicit() {
        let mut data = snapshot();
        data.profile["address"] = json!("Old synthetic address");
        let fields = values(&[
            ("cvr", ""),
            ("address", ""),
            ("postalCode", ""),
            ("city", ""),
            ("vatPeriodType", "quarter"),
            ("paymentTermsDays", "14"),
        ]);
        assert_eq!(
            serde_json::from_slice::<Value>(&body(Operation::Profile, &fields, &data).unwrap())
                .unwrap(),
            json!({"address":"","vatPeriodType":"quarter","paymentTermsDays":14})
        );
    }
    #[test]
    fn account_has_no_inferred_type_balance_vat_or_number() {
        let data = snapshot();
        let fields = values(&[
            ("accountNo", "chosen"),
            ("name", "Synthetic account"),
            ("type", "expense"),
            ("normalBalance", "credit"),
            ("defaultVatCode", ""),
        ]);
        let out: Value =
            serde_json::from_slice(&body(Operation::Account, &fields, &data).unwrap()).unwrap();
        assert_eq!(
            out,
            json!({"accountNo":"chosen","name":"Synthetic account","type":"expense","normalBalance":"credit"})
        );
        for index in 0..4 {
            let mut missing = fields.clone();
            missing[index].1.clear();
            assert!(body(Operation::Account, &missing, &data).is_err());
        }
        let mut explicit = fields.clone();
        explicit[4].1 = "USER_CHOSEN_CODE".into();
        // Domain authority remains the server; the view does not invent a VAT-code registry.
        assert_eq!(
            serde_json::from_slice::<Value>(&body(Operation::Account, &explicit, &data).unwrap())
                .unwrap()["defaultVatCode"],
            "USER_CHOSEN_CODE"
        );
    }
    #[test]
    fn role_requires_active_caller_chosen_account_and_explicit_body_confirmation() {
        let mut data = snapshot();
        let fields = values(&[("role", "bank"), ("accountNo", "A")]);
        assert_eq!(
            serde_json::from_slice::<Value>(&body(Operation::Role, &fields, &data).unwrap())
                .unwrap(),
            json!({"role":"bank","accountNo":"A","confirm":true})
        );
        assert!(
            body(
                Operation::Role,
                &values(&[("role", "bank"), ("accountNo", "missing")]),
                &data
            )
            .is_err()
        );
        assert!(
            body(
                Operation::Role,
                &values(&[("role", "guess"), ("accountNo", "A")]),
                &data
            )
            .is_err()
        );
        data.chart["accounts"][0]["active"] = json!(false);
        assert!(body(Operation::Role, &fields, &data).is_err());
    }
    #[test]
    fn reviewed_basis_refuses_changed_roles_profile_principal_or_ledger() {
        let before = snapshot();
        let review = Review {
            operation: Operation::Account,
            bytes: vec![],
            before: before.clone(),
        };
        assert!(review.current(&before));
        for path in ["profile", "chart", "principal", "ledger"] {
            let mut fresh = before.clone();
            match path {
                "profile" => fresh.profile["city"] = json!("Changed"),
                "chart" => fresh.chart["accountRoles"]["resolutions"] = json!([]),
                "principal" => fresh.principal = "another-user".into(),
                _ => fresh.ledger = "another-ledger".into(),
            };
            assert!(!review.current(&fresh));
        }
    }
    #[test]
    fn receipt_must_name_requested_account_and_role() {
        let data = snapshot();
        let account = Review {
            operation: Operation::Account,
            bytes: br#"{"accountNo":"A"}"#.to_vec(),
            before: data.clone(),
        };
        assert!(account.receipt(&json!({"ok":true,"account":{"accountNo":"A"}})));
        assert!(!account.receipt(&json!({"ok":true,"account":{"accountNo":"B"}})));
        let role = Review {
            operation: Operation::Role,
            bytes: br#"{"role":"bank","accountNo":"A"}"#.to_vec(),
            before: data,
        };
        assert!(
            role.receipt(
                &json!({"ok":true,"resolution":{"ok":true,"role":"bank","accountNo":"A"}})
            )
        );
        assert!(
            !role.receipt(
                &json!({"ok":true,"resolution":{"ok":true,"role":"bank","accountNo":"B"}})
            )
        );
        assert!(!role.receipt(&json!({"ok":true})));
    }
    #[test]
    fn minimal_marker_fences_rebind_and_never_stores_personal_request_fields() {
        let data = snapshot();
        let mut marker = data.marker(
            Operation::Profile,
            "https://synthetic.invalid",
            "synthetic",
            Some("opaque-local-ledger"),
        );
        assert!(valid_record(&marker));
        assert!(data.matches_marker(
            &marker,
            "https://new-port.invalid",
            "synthetic",
            Some("opaque-local-ledger")
        ));
        assert!(!data.matches_marker(
            &marker,
            "https://synthetic.invalid",
            "other",
            Some("opaque-local-ledger")
        ));
        assert!(!data.matches_marker(&marker, "https://synthetic.invalid", "synthetic", None));
        let remote = data.marker(
            Operation::Account,
            "https://synthetic.invalid",
            "synthetic",
            None,
        );
        assert!(!data.matches_marker(&remote, "https://different.invalid", "synthetic", None));
        for key in ["address", "cvr", "body", "accountNo", "request", "payment"] {
            marker[key] = json!("must not persist");
            assert!(!valid_record(&marker));
            marker.as_object_mut().unwrap().remove(key);
        }
        marker.as_object_mut().unwrap().remove("localIdentity");
        marker["unknown"] = json!(null);
        assert!(!valid_record(&marker));
    }
    #[test]
    fn durable_unknown_survives_reload_refuses_duplicate_and_archives_only_matching_scope() {
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm-setup-state-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let marker = snapshot().marker(
            Operation::Role,
            "https://synthetic.invalid",
            "synthetic",
            None,
        );
        begin_at(&dir, &marker).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(marker.clone()));
        assert!(begin_at(&dir, &marker).is_err());
        assert!(finish_at(&dir, &json!({}), true).is_err());
        assert!(load_at(&dir).unwrap().is_some());
        finish_at(&dir, &marker, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        let archives = fs::read_dir(&dir)
            .unwrap()
            .map(|r| r.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(archives.len(), 1);
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&archives[0]).unwrap()).unwrap(),
            marker
        );
        fs::remove_file(&archives[0]).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn canonical_read_rejects_company_mismatch_or_missing_actor_before_forms_exist() {
        let company = json!({"id":1,"name":"Synthetic"});
        let profile = json!({"ok":true,"company":company});
        let chart = json!({"ok":true,"accounts":{"slug":"other","company":company,"accounts":[],"accountRoles":{"missing":[],"ambiguous":[],"resolutions":[]}}});
        let me = json!({"ok":true,"user":{"id":"u"}});
        let journal = json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"ledger"}});
        assert!(
            Snapshot::parse(
                "synthetic",
                profile.clone(),
                chart.clone(),
                me,
                journal.clone()
            )
            .is_err()
        );
        let mut chart = chart;
        chart["accounts"]["slug"] = json!("synthetic");
        assert!(
            Snapshot::parse(
                "synthetic",
                profile,
                chart,
                json!({"ok":true,"user":{}}),
                journal
            )
            .is_err()
        );
    }
    #[test]
    fn corrupted_or_nonprivate_marker_blocks_without_removing_evidence() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm-setup-corrupt-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let path = dir.join("pending-company-setup.json");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        file.write_all(br#"{"format":"unknown"}"#).unwrap();
        drop(file);
        assert!(load_at(&dir).is_err());
        assert!(finish_at(&dir, &json!({}), true).is_err());
        assert!(path.exists());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load_at(&dir).is_err());
        assert!(path.exists());
        fs::remove_file(&path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
