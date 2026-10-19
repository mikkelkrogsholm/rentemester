//! One private unresolved consolidation attempt; no payload persistence or replay.
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
    let m =
        fs::symlink_metadata(path).map_err(|_| "Privat konsolideringsstatus kunne ikke åbnes")?;
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
            "Konsolideringsstatus kræver din egen private mappe0700/fil0600 uden symbolske links."
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
        return Err("Privat konsolideringsstatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-consolidation.json");
    if !path
        .try_exists()
        .map_err(|_| "Konsolideringsstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Konsolideringsstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Konsolideringsstatus overskrider grænsen".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Konsolideringsstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret konsolideringsforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-consolidation/1"
        && value.as_object().is_some_and(|m| {
            m.len() == 9
                && m.keys().all(|k| {
                    [
                        "format",
                        "origin",
                        "company",
                        "principal",
                        "ledger",
                        "localIdentity",
                        "operation",
                        "profileId",
                        "profileHash",
                    ]
                    .contains(&k.as_str())
                })
        })
        && ["origin", "company", "principal", "ledger"]
            .iter()
            .all(|k| {
                value[*k]
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s.len() < 1024)
            })
        && matches!(
            value["operation"].as_str(),
            Some("propose" | "approve" | "revoke")
        )
        && ["localIdentity", "profileId", "profileHash"]
            .iter()
            .all(|k| value[*k].is_null() || value[*k].as_str().is_some_and(|s| s.len() < 1024))
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig konsolideringsstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-consolidation.json"))
        .map_err(|_| "Et konsolideringsforsøg kræver afklaring før ny bogføring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Konsolideringsstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Konsolideringsstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Konsolideringsstatus ændrede sig og bevares".into());
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
            .open(dir.join(format!("consolidation-manually-resolved-{suffix}.json")))
            .map_err(|_| "Konsolideringsafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Konsolideringsafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-consolidation.json"))
        .map_err(|_| "Konsolideringsstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Konsolideringsstatus kunne ikke synkroniseres".into())
}

use rentemester_client::ConsolidationMutation;
use serde_json::json;
use std::sync::Arc;
#[derive(Clone, Default)]
pub struct Selection {
    pub from: String,
    pub as_of: String,
    pub id: Option<String>,
}
#[derive(Clone, PartialEq)]
pub struct Snapshot {
    pub company: String,
    pub principal: String,
    pub ledger: String,
    pub workspace_role: String,
    pub companies: Vec<Value>,
    pub from: String,
    pub as_of: String,
    pub current: Value,
    pub available: Value,
    pub selected: Option<Value>,
    pub report: Option<Value>,
    pub report_error: Option<String>,
}
fn text(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Serveren mangler {key}"))
}
fn hash(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
pub fn date(s: &str) -> bool {
    s.len() == 10
        && s.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
}
fn profile_valid(v: &Value) -> bool {
    v["profile"].is_object()
        && v["profile"]["version"] == 1
        && text(&v["profile"], "id").is_ok()
        && hash(&v["profileHash"])
        && text(v, "actor").is_ok()
        && matches!(
            v["status"].as_str(),
            Some("proposed" | "approved" | "revoked")
        )
}
fn report_valid(v: &Value, s: &Selection) -> bool {
    if v["ok"] != true
        || v["scope"] != "consolidated-report"
        || v["period"]["from"] != s.from
        || v["period"]["to"] != s.as_of
        || !matches!(v["status"].as_str(), Some("ready" | "blocked"))
        || !v["blockers"].is_array()
        || !v["sourceSnapshots"].is_array()
    {
        return false;
    }
    if !v["profileId"].is_null() && v["profileId"].as_str() != s.id.as_deref() {
        return false;
    }
    if !v["rawCompanySums"].as_array().is_some_and(|a| {
        a.iter().all(|r| {
            r["total"].is_string()
                && r["byCompany"]
                    .as_array()
                    .is_some_and(|a| a.iter().all(|r| r["amount"].is_string()))
        })
    }) {
        return false;
    }
    if !v["appliedEliminations"].as_array().is_some_and(|a| {
        a.iter().all(|r| {
            r["adjustments"]
                .as_array()
                .is_some_and(|a| a.iter().all(|r| r["amount"].is_string()))
        })
    }) {
        return false;
    }
    let figures = v["consolidatedFigures"].as_array();
    if v["status"] == "blocked" {
        return v["consolidatedFigures"].is_null();
    }
    figures.is_some_and(|a| {
        !a.is_empty()
            && a.iter().all(|r| {
                [
                    "rawCompanySum",
                    "eliminationAdjustment",
                    "consolidatedAmount",
                ]
                .iter()
                .all(|k| r[*k].is_string())
            })
    }) && v["blockers"].as_array().is_some_and(Vec::is_empty)
}
impl Snapshot {
    pub fn parse(
        company: &str,
        s: &Selection,
        current: Value,
        available: Value,
        report: Option<Value>,
        report_error: Option<String>,
        me: Value,
        journal: Value,
    ) -> Result<Self, String> {
        if !date(&s.from)
            || !date(&s.as_of)
            || current["ok"] != true
            || current["scope"] != "native-consolidation"
            || current["asOf"] != s.as_of
            || !current["profiles"]
                .as_array()
                .is_some_and(|a| a.iter().all(profile_valid))
            || !current["availableProfiles"].is_array()
            || available["ok"] != true
            || available["scope"] != "consolidation-report-profiles"
            || available["asOf"] != s.as_of
            || !available["profiles"].is_array()
            || report.as_ref().is_some_and(|v| !report_valid(v, s))
            || report.is_some() && report_error.is_some()
            || me["ok"] != true
            || journal["ok"] != true
            || journal["journal"]["slug"] != company
        {
            return Err("Serveren bekræftede ikke præcis profil/dato, kanoniske decimalstrenge/kildebeviser, bruger og ledger".into());
        }
        let selected =
            s.id.as_ref()
                .and_then(|id| {
                    current["profiles"]
                        .as_array()
                        .and_then(|a| a.iter().find(|r| r["profile"]["id"] == *id))
                })
                .cloned();
        if selected.as_ref().is_some_and(|s| s["status"] == "approved")
            && report.is_none()
            && report_error.is_none()
        {
            return Err("Godkendt profil mangler faktisk rapport eller læsefejl".into());
        }
        if let Some(report) = &report {
            if selected.as_ref().is_none_or(|s| {
                s["status"] != "approved"
                    || (!report["profileHash"].is_null()
                        && report["profileHash"] != s["profileHash"])
            }) {
                return Err(
                    "Rapporten matcher ikke den valgte aktuelt godkendte profil/hash".into(),
                );
            }
        }
        Ok(Self {
            company: company.into(),
            principal: text(&me["user"], "id")?,
            ledger: text(&journal["journal"], "ledgerUuid")?,
            workspace_role: text(&me, "workspaceRole")?,
            companies: me["companies"]
                .as_array()
                .ok_or("Aktuelle virksomhedsroller mangler")?
                .clone(),
            from: s.from.clone(),
            as_of: s.as_of.clone(),
            current,
            available,
            selected,
            report,
            report_error,
        })
    }
    fn owner(&self, slug: &str) -> bool {
        self.companies
            .iter()
            .any(|c| c["slug"] == slug && c["role"] == "owner" && c["archived"] != true)
    }
    pub fn self_approval(&self) -> bool {
        self.selected
            .as_ref()
            .is_some_and(|s| s["actor"] == format!("user:{}", self.principal))
    }
    fn can_write(&self, profile: &Value) -> bool {
        self.workspace_role == "workspace_owner"
            && self.owner(&self.company)
            && self
                .companies
                .iter()
                .filter(|c| c["archived"] != true)
                .all(|c| c["role"] == "owner")
            && profile["accountMappings"].as_array().is_some_and(|a| {
                !a.is_empty()
                    && a.iter()
                        .all(|r| r["companySlug"].as_str().is_some_and(|s| self.owner(s)))
            })
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Propose,
    Approve,
    Revoke,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Propose => "propose",
            Self::Approve => "approve",
            Self::Revoke => "revoke",
        }
    }
    pub fn sdk(self) -> ConsolidationMutation {
        match self {
            Self::Propose => ConsolidationMutation::Propose,
            Self::Approve => ConsolidationMutation::Approve,
            Self::Revoke => ConsolidationMutation::Revoke,
        }
    }
}
#[derive(Clone)]
pub struct Review {
    pub action: Action,
    pub snapshot: Snapshot,
    pub body: Arc<Vec<u8>>,
    pub display: Value,
}
impl Review {
    pub fn prepare(data: &Snapshot, action: Action, raw: &str) -> Result<Self, String> {
        let mut body = if action == Action::Propose {
            if raw.len() > rentemester_client::MAX_CONSOLIDATION_PROPOSE_BODY {
                return Err("Profilen overskrider native1MiB-anmodningsgrænsen".into());
            }
            let profile: Value = serde_json::from_str(raw)
                .map_err(|e| format!("Angiv komplet typed V1-profil JSON: {e}"))?;
            if !profile.is_object()
                || profile.get("confirm").is_some()
                || profile["version"] != 1
                || text(&profile, "id").is_err()
                || !profile["reportingLines"].is_array()
                || !data.can_write(&profile)
            {
                return Err("Angiv komplet V1-profil med eksplicitte rapportlinjer/kontomappings og aktuelle ejerrettigheder. Skrivning kræver WorkspaceOwner og CompanyOwner til alle konfigurerede virksomheder; serveren genkontrollerer hele kildekonfigurationen.".into());
            }
            if serde_json::to_vec(&profile)
                .map_err(|_| "Profilen kunne ikke fastholdes")?
                .len()
                > 524288
            {
                return Err("Den kanoniske kildeprofil må højst fylde524288bytes; serveren fastsætter den endelige kanoniske form.".into());
            }
            json!({"profile":profile})
        } else {
            let selected = data
                .selected
                .as_ref()
                .ok_or("Vælg en aktuelt synlig kanonisk profil først")?;
            if !data.can_write(&selected["profile"]) {
                return Err("Aktuel WorkspaceOwner og CompanyOwner til alle konfigurerede virksomheder kræves; ingen adgang udledes.".into());
            }
            if action == Action::Approve {
                if selected["status"] != "proposed" {
                    return Err("Godkendelse kræver præcis en aktuelt foreslået profil".into());
                }
                if data.self_approval() {
                    return Err("En ANDEN autentificeret reviewer skal gennemgå og godkende præcis profil/hash. Egen godkendelse afvises; session ændres ikke automatisk.".into());
                }
                json!({"profileId":selected["profile"]["id"],"profileHash":selected["profileHash"]})
            } else {
                if selected["status"] != "approved" {
                    return Err("Tilbagekaldelse kræver en aktuelt godkendt profil".into());
                }
                json!({"profileId":selected["profile"]["id"]})
            }
        };
        body["confirm"] = json!(true);
        let bytes = serde_json::to_vec(&body).map_err(|_| "Anmodningen kunne ikke fastholdes")?;
        if bytes.len() > action.sdk().max_body() {
            return Err("Anmodningen overskrider nativegrænsen1MiB/32KiB".into());
        }
        Ok(Self {
            action,
            snapshot: data.clone(),
            body: Arc::new(bytes),
            display: body,
        })
    }
    pub fn current(&self, data: &Snapshot) -> bool {
        self.snapshot == *data
    }
    pub fn marker(&self, origin: &str, local: Option<&str>) -> Value {
        json!({"format":"rentemester-desktop-consolidation/1","origin":origin,"company":self.snapshot.company,"principal":self.snapshot.principal,"ledger":self.snapshot.ledger,"localIdentity":local,"operation":self.action.name(),"profileId":if self.action==Action::Propose{self.display["profile"]["id"].clone()}else{self.display["profileId"].clone()},"profileHash":if self.action==Action::Propose{Value::Null}else{self.snapshot.selected.as_ref().map(|s|s["profileHash"].clone()).unwrap_or(Value::Null)}})
    }
    pub fn receipt(&self, v: &Value) -> bool {
        let id = if self.action == Action::Propose {
            &self.display["profile"]["id"]
        } else {
            &self.display["profileId"]
        };
        v["ok"] == true
            && v["profileId"] == *id
            && v["status"]
                == match self.action {
                    Action::Propose => "proposed",
                    Action::Approve => "approved",
                    Action::Revoke => "revoked",
                }
            && match self.action {
                Action::Propose => hash(&v["profileHash"]),
                Action::Approve => v["profileHash"] == self.display["profileHash"],
                Action::Revoke => true,
            }
    }
    pub fn readback(&self, receipt: &Value, data: &Snapshot) -> bool {
        self.receipt(receipt)
            && data.current["profiles"].as_array().is_some_and(|a| {
                a.iter().any(|s| {
                    s["profile"]["id"] == receipt["profileId"]
                        && s["status"] == receipt["status"]
                        && s["profileHash"]
                            == if self.action == Action::Revoke {
                                self.snapshot
                                    .selected
                                    .as_ref()
                                    .map(|s| s["profileHash"].clone())
                                    .unwrap_or(Value::Null)
                            } else {
                                receipt["profileHash"].clone()
                            }
                })
            })
    }
}
pub fn scope(record: &Value, origin: &str, data: &Snapshot, local: Option<&str>) -> bool {
    record["company"] == data.company
        && record["principal"] == data.principal
        && record["ledger"] == data.ledger
        && record["localIdentity"].as_str() == local
        && (record["origin"] == origin || local.is_some())
}
pub fn lines(v: &Value, prefix: &str) -> Vec<String> {
    match v {
        Value::Object(m) => m
            .iter()
            .flat_map(|(k, v)| lines(v, &format!("{prefix}.{k}")))
            .collect(),
        Value::Array(a) if !a.is_empty() => a
            .iter()
            .enumerate()
            .flat_map(|(i, v)| lines(v, &format!("{prefix}[{i}]")))
            .collect(),
        _ => vec![format!("{prefix}: {v}")],
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> Value {
        json!({"version":1,"id":"synthetic-profile","groupId":"synthetic-group","currency":"DKK","validFrom":"2000-01-01","reportingLines":[{"id":"assets","label":"Aktiver","section":"asset","displayOrder":0},{"id":"liabilities","label":"Gæld","section":"liability","displayOrder":1},{"id":"equity","label":"Egenkapital","section":"equity","displayOrder":2},{"id":"result","label":"Resultat","section":"equity","role":"current-result","displayOrder":3},{"id":"income","label":"Indtægt","section":"income","displayOrder":4},{"id":"expense","label":"Udgift","section":"expense","displayOrder":5}],"accountMappings":[{"id":"left-map","companySlug":"left","accountNo":"source-asset","reportingLineId":"assets","validFrom":"2000-01-01"},{"id":"right-map","companySlug":"right","accountNo":"source-income","reportingLineId":"income","validFrom":"2000-01-01"}],"evidenceRefs":["reviewed-reporting-source"]})
    }
    fn selection() -> Selection {
        Selection {
            from: "2026-01-01".into(),
            as_of: "2026-06-01".into(),
            id: Some("synthetic-profile".into()),
        }
    }
    fn me(principal: &str) -> Value {
        json!({"ok":true,"user":{"id":principal},"workspaceRole":"workspace_owner","companies":[{"slug":"left","role":"owner"},{"slug":"right","role":"owner"}]})
    }
    fn journal() -> Value {
        json!({"ok":true,"journal":{"slug":"left","ledgerUuid":"immutable-ledger"}})
    }
    fn report() -> Value {
        json!({"ok":true,"scope":"consolidated-report","profileId":"synthetic-profile","profileHash":"a".repeat(64),"groupId":"synthetic-group","period":{"from":"2026-01-01","to":"2026-06-01"},"currency":"DKK","status":"ready","blockers":[],"rawCompanySums":[{"lineId":"assets","total":"24.68","byCompany":[{"companySlug":"left","amount":"12.34","sourceHashes":["actual-original-source"]},{"companySlug":"right","amount":"12.34","sourceHashes":["other-source"]}]}],"appliedEliminations":[{"eliminationId":"actual-reviewed-v2","payloadHash":"proof","adjustments":[{"lineId":"assets","amount":"-12.34"}]}],"consolidatedFigures":[{"lineId":"assets","label":"Aktiver","section":"asset","rawCompanySum":"24.68","eliminationAdjustment":"-12.34","consolidatedAmount":"12.34"},{"lineId":"result","label":"Resultat","section":"equity","rawCompanySum":"12.34","eliminationAdjustment":"0.00","consolidatedAmount":"12.34"}],"sourceSnapshots":[{"companySlug":"left","ledgerHeadHash":"head","entryCount":2,"integrityScope":"native_period_sources_and_physical_originals"}],"sourceVersion":{"version":2,"retained":true}})
    }
    fn data(principal: &str, status: &str) -> Snapshot {
        let current = json!({"ok":true,"scope":"native-consolidation","asOf":"2026-06-01","profiles":[{"profile":profile(),"profileHash":"a".repeat(64),"status":status,"actor":"user:author"}],"availableProfiles":[]});
        let available = json!({"ok":true,"scope":"consolidation-report-profiles","asOf":"2026-06-01","profiles":[]});
        Snapshot::parse(
            "left",
            &selection(),
            current,
            available,
            if status == "approved" {
                Some(report())
            } else {
                None
            },
            None,
            me(principal),
            journal(),
        )
        .unwrap()
    }
    #[test]
    fn exact_profile_json_freezes_source_without_actor_default_key_or_math() {
        let d = data("author", "revoked");
        let p = profile();
        let review = Review::prepare(&d, Action::Propose, &p.to_string()).unwrap();
        assert_eq!(review.display, json!({"profile":p,"confirm":true}));
        assert_eq!(
            serde_json::from_slice::<Value>(&review.body).unwrap(),
            review.display
        );
        assert!(review.current(&d));
        for k in ["actor", "idempotencyKey", "from", "asOf"] {
            assert!(review.display.get(k).is_none());
        }
        for bad in [
            "null".to_owned(),
            "{}".into(),
            "{".into(),
            profile()
                .to_string()
                .replace("\"version\":1", "\"version\":2"),
            " ".repeat(rentemester_client::MAX_CONSOLIDATION_PROPOSE_BODY + 1),
        ] {
            assert!(Review::prepare(&d, Action::Propose, &bad).is_err());
        }
    }
    #[test]
    fn independent_approval_binds_exact_selected_proposed_hash() {
        assert!(Review::prepare(&data("author", "proposed"), Action::Approve, "").is_err());
        let d = data("reviewer", "proposed");
        let r = Review::prepare(&d, Action::Approve, "").unwrap();
        assert_eq!(
            r.display,
            json!({"profileId":"synthetic-profile","profileHash":"a".repeat(64),"confirm":true})
        );
        assert!(Review::prepare(&data("reviewer", "approved"), Action::Approve, "").is_err());
        let mut missing = d;
        missing.selected = None;
        assert!(Review::prepare(&missing, Action::Approve, "").is_err());
    }
    #[test]
    fn separate_revoke_uses_exact_source_contract_without_invented_hash_key() {
        assert!(Review::prepare(&data("reviewer", "proposed"), Action::Revoke, "").is_err());
        let d = data("reviewer", "approved");
        let r = Review::prepare(&d, Action::Revoke, "").unwrap();
        assert_eq!(
            r.display,
            json!({"profileId":"synthetic-profile","confirm":true})
        );
        assert!(r.display.get("profileHash").is_none());
        let receipt = json!({"ok":true,"profileId":"synthetic-profile","status":"revoked"});
        assert!(r.receipt(&receipt));
        assert!(r.readback(&receipt, &data("reviewer", "revoked")));
    }
    #[test]
    fn current_workspace_and_all_company_source_authority_are_required() {
        let source = data("reviewer", "proposed");
        for change in [0, 1, 2, 3] {
            let mut d = source.clone();
            match change {
                0 => d.workspace_role = "member".into(),
                1 => d.companies[1]["role"] = json!("bookkeeper"),
                2 => d.companies[0]["archived"] = json!(true),
                _ => d
                    .companies
                    .push(json!({"slug":"third","role":"accountant"})),
            }
            assert!(Review::prepare(&d, Action::Approve, "").is_err());
            assert!(Review::prepare(&d, Action::Propose, &profile().to_string()).is_err());
        }
        let mut p = profile();
        p["accountMappings"][0]["companySlug"] = json!("hidden");
        assert!(Review::prepare(&source, Action::Propose, &p.to_string()).is_err());
    }
    #[test]
    fn full_decimal_table_current_result_elimination_and_source_proofs_remain_verbatim() {
        let d = data("reviewer", "approved");
        assert_eq!(d.report.as_ref().unwrap(), &report());
        let lines = lines(d.report.as_ref().unwrap(), "report").join("\n");
        for text in [
            "24.68",
            "-12.34",
            "current",
            "sourceVersion",
            "ledgerHeadHash",
        ] {
            if text != "current" {
                assert!(lines.contains(text));
            }
        }
        let mut bad = report();
        bad["consolidatedFigures"][0]["consolidatedAmount"] = json!(12.34);
        assert!(!report_valid(&bad, &selection()));
        let mut bad = report();
        bad["rawCompanySums"][0]["byCompany"][0]["amount"] = json!(12.34);
        assert!(!report_valid(&bad, &selection()));
        let mut bad = report();
        bad["appliedEliminations"][0]["adjustments"][0]["amount"] = json!(-12.34);
        assert!(!report_valid(&bad, &selection()));
    }
    #[test]
    fn blocked_redacted_or_unavailable_report_never_becomes_zero_or_ready() {
        let d = data("reviewer", "approved");
        let blocked = json!({"ok":true,"scope":"consolidated-report","period":{"from":"2026-01-01","to":"2026-06-01"},"status":"blocked","blockers":["all active group companies must be visible"],"rawCompanySums":[],"appliedEliminations":[],"consolidatedFigures":null,"sourceSnapshots":[]});
        let parsed = Snapshot::parse(
            "left",
            &selection(),
            d.current.clone(),
            d.available.clone(),
            Some(blocked.clone()),
            None,
            me("reviewer"),
            journal(),
        )
        .unwrap();
        assert_eq!(parsed.report, Some(blocked));
        let unavailable = Snapshot::parse(
            "left",
            &selection(),
            d.current.clone(),
            d.available.clone(),
            None,
            Some("HTTP403".into()),
            me("reviewer"),
            journal(),
        )
        .unwrap();
        assert!(unavailable.report.is_none());
        assert_eq!(unavailable.report_error.as_deref(), Some("HTTP403"));
        assert!(
            Snapshot::parse(
                "left",
                &selection(),
                d.current,
                d.available,
                None,
                None,
                me("reviewer"),
                journal()
            )
            .is_err()
        );
    }
    #[test]
    fn frozen_scope_rejects_role_hash_date_source_changes_and_wrong_receipts() {
        let d = data("reviewer", "approved");
        let r = Review::prepare(&d, Action::Revoke, "").unwrap();
        for change in 0..7 {
            let mut c = d.clone();
            match change {
                0 => c.principal = "other".into(),
                1 => c.ledger = "other".into(),
                2 => c.companies[0]["role"] = json!("bookkeeper"),
                3 => c.current["profiles"][0]["profileHash"] = json!("b".repeat(64)),
                4 => c.from = "2025-01-01".into(),
                5 => {
                    c.report.as_mut().unwrap()["sourceSnapshots"][0]["ledgerHeadHash"] =
                        json!("new")
                }
                _ => {
                    c.report.as_mut().unwrap()["appliedEliminations"][0]["payloadHash"] =
                        json!("changed")
                }
            }
            assert!(!r.current(&c));
        }
        let local = r.marker("http://127.0.0.1:1", Some("trusted-config"));
        assert!(scope(
            &local,
            "http://127.0.0.1:2",
            &d,
            Some("trusted-config")
        ));
        assert!(!scope(
            &local,
            "http://127.0.0.1:2",
            &d,
            Some("other-config")
        ));
        let remote = r.marker("https://one.invalid", None);
        assert!(!scope(&remote, "https://two.invalid", &d, None));
        let r = Review::prepare(&data("reviewer", "proposed"), Action::Approve, "").unwrap();
        let receipt = json!({"ok":true,"profileId":"synthetic-profile","profileHash":"a".repeat(64),"status":"approved"});
        assert!(r.readback(&receipt, &d));
        for k in ["profileId", "profileHash", "status"] {
            let mut wrong = receipt.clone();
            wrong[k] = json!("wrong");
            assert!(!r.receipt(&wrong));
        }
        let mut wrong = report();
        wrong["profileHash"] = json!("b".repeat(64));
        assert!(
            Snapshot::parse(
                "left",
                &selection(),
                d.current,
                d.available,
                Some(wrong),
                None,
                me("reviewer"),
                journal()
            )
            .is_err()
        );
    }
    #[test]
    fn private_unknown_marker_is_payload_free_cannot_be_replaced_and_requires_exact_manual_resolution()
     {
        let mut random = [0u8; 16];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random).unwrap();
        let suffix = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("native-consolidation-{suffix}"));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let r = Review::prepare(
            &data("author", "revoked"),
            Action::Propose,
            &profile().to_string(),
        )
        .unwrap();
        let marker = r.marker("https://synthetic.invalid", None);
        assert!(valid_record(&marker));
        assert!(marker["profileHash"].is_null());
        for k in [
            "body",
            "profile",
            "reportingLines",
            "accountMappings",
            "amount",
            "evidenceRefs",
            "from",
            "asOf",
        ] {
            assert!(marker.get(k).is_none());
        }
        let mut wrong = marker.clone();
        wrong["body"] = json!("private");
        assert!(!valid_record(&wrong));
        begin_at(&dir, &marker).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(marker.clone()));
        assert!(begin_at(&dir, &marker).is_err());
        let mut wrong = marker.clone();
        wrong["profileId"] = json!("other");
        assert!(finish_at(&dir, &wrong, false).is_err());
        assert_eq!(load_at(&dir).unwrap(), Some(marker.clone()));
        finish_at(&dir, &marker, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
