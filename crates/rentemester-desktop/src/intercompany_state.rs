//! One private unresolved intercompany attempt; no payload persistence or replay.
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
        fs::symlink_metadata(path).map_err(|_| "Privat mellemregningsstatus kunne ikke åbnes")?;
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
            "Mellemregningsstatus kræver din egen private mappe0700/fil0600 uden symbolske links."
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
        return Err("Privat mellemregningsstatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-intercompany.json");
    if !path
        .try_exists()
        .map_err(|_| "Mellemregningsstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Mellemregningsstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Mellemregningsstatus overskrider grænsen".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Mellemregningsstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret mellemregningsforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-intercompany/1"
        && value.as_object().is_some_and(|m| {
            m.len() == 10
                && m.keys().all(|k| {
                    [
                        "format",
                        "origin",
                        "company",
                        "principal",
                        "ledger",
                        "localIdentity",
                        "operation",
                        "mappingId",
                        "mappingHash",
                        "manifestHash",
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
            Some("manifest" | "propose" | "approve" | "revoke")
        )
        && ["localIdentity", "mappingId", "mappingHash", "manifestHash"]
            .iter()
            .all(|k| value[*k].is_null() || value[*k].as_str().is_some_and(|s| s.len() < 1024))
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig mellemregningsstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-intercompany.json"))
        .map_err(|_| "Et mellemregningsforsøg kræver afklaring før ny bogføring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Mellemregningsstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Mellemregningsstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Mellemregningsstatus ændrede sig og bevares".into());
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
            .open(dir.join(format!("intercompany-manually-resolved-{suffix}.json")))
            .map_err(|_| "Mellemregningsafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Mellemregningsafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-intercompany.json"))
        .map_err(|_| "Mellemregningsstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Mellemregningsstatus kunne ikke synkroniseres".into())
}

use rentemester_client::IntercompanyMutation;
use serde_json::json;
use std::sync::Arc;
#[derive(Clone, Default)]
pub struct Selection {
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
    pub current: Value,
    pub report: Value,
    pub selected: Option<Value>,
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
fn mapping_valid(m: &Value) -> bool {
    m["mapping"].is_object()
        && ["id", "groupId", "leftCompanySlug", "rightCompanySlug"]
            .iter()
            .all(|k| text(&m["mapping"], k).is_ok())
        && hash(&m["mappingHash"])
        && matches!(
            m["status"].as_str(),
            Some("proposed" | "approved" | "revoked")
        )
        && text(m, "actor").is_ok()
}
fn report_valid(report: &Value, date: &str) -> bool {
    report["ok"] == true
        && report["scope"] == "intercompany-reconciliation"
        && report["asOf"] == date
        && report["rows"].as_array().is_some_and(|rows| {
            rows.iter().all(|row| {
                if !row["difference"].is_null() && !row["difference"].is_string() {
                    return false;
                }
                ["left", "right"].iter().all(|k| {
                    let side = &row[*k];
                    if side.is_null() {
                        return true;
                    }
                    if !side.is_object()
                        || (!side["balance"].is_null() && !side["balance"].is_string())
                    {
                        return false;
                    }
                    side["sourceRefs"].is_null()
                        || side["sourceRefs"]
                            .as_array()
                            .is_some_and(|a| a.iter().all(|r| r["amount"].is_string()))
                })
            })
        })
}
impl Snapshot {
    pub fn parse(
        company: &str,
        selection: &Selection,
        current: Value,
        report: Value,
        me: Value,
        journal: Value,
    ) -> Result<Self, String> {
        if current["ok"] != true
            || current["consolidationAvailable"] != false
            || current["mappings"]
                .as_array()
                .is_none_or(|a| a.iter().any(|m| !mapping_valid(m)))
            || !((current["manifest"].is_null() && current["manifestHash"].is_null())
                || (current["manifest"].is_object()
                    && current["manifest"]["version"] == 1
                    && current["manifest"]["groups"].is_array()
                    && hash(&current["manifestHash"])))
            || !report_valid(&report, &selection.as_of)
            || me["ok"] != true
            || journal["ok"] != true
            || journal["journal"]["slug"] != company
        {
            return Err("Serveren bekræftede ikke manifest/mapping, præcis pr-dato-afstemning med decimalstrenge, bruger og ledger".into());
        }
        let companies = me["companies"]
            .as_array()
            .ok_or("Aktuelle synlige virksomhedsroller mangler")?
            .clone();
        let selected = selection
            .id
            .as_ref()
            .and_then(|id| {
                current["mappings"]
                    .as_array()
                    .and_then(|a| a.iter().find(|m| m["mapping"]["id"] == *id))
            })
            .cloned();
        Ok(Self {
            company: company.into(),
            principal: text(&me["user"], "id")?,
            ledger: text(&journal["journal"], "ledgerUuid")?,
            workspace_role: text(&me, "workspaceRole")?,
            companies,
            current,
            report,
            selected,
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
            .is_some_and(|m| m["actor"] == format!("user:{}", self.principal))
    }
    fn refs_owned<'a>(&self, mut refs: impl Iterator<Item = &'a str>) -> bool {
        refs.all(|s| self.owner(s))
    }
}
fn memberships(manifest: &Value) -> Result<Vec<&str>, String> {
    let groups = manifest["groups"]
        .as_array()
        .ok_or("Angiv eksplicit V1 groups-array")?;
    let mut refs = Vec::new();
    for group in groups {
        for member in group["memberships"]
            .as_array()
            .ok_or("Hver gruppe skal have eksplicit memberships-array")?
        {
            refs.push(
                member["companySlug"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("Angiv faktisk companySlug for hvert medlemskab")?,
            );
        }
    }
    Ok(refs)
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Manifest,
    Propose,
    Approve,
    Revoke,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Propose => "propose",
            Self::Approve => "approve",
            Self::Revoke => "revoke",
        }
    }
    pub fn sdk(self) -> IntercompanyMutation {
        match self {
            Self::Manifest => IntercompanyMutation::Manifest,
            Self::Propose => IntercompanyMutation::Propose,
            Self::Approve => IntercompanyMutation::Approve,
            Self::Revoke => IntercompanyMutation::Revoke,
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
        if data.workspace_role != "workspace_owner" || !data.owner(&data.company) {
            return Err("Skrivning kræver aktuel WorkspaceOwner og CompanyOwner i ankervirksomheden samt alle relevante virksomheder. Serveren genkontrollerer retten ved afsendelse.".into());
        }
        let mut body = if matches!(action, Action::Manifest | Action::Propose) {
            let value: Value = serde_json::from_str(raw)
                .map_err(|e| format!("Angiv komplet typed JSON-objekt fra kilden: {e}"))?;
            if !value.is_object() || value.get("confirm").is_some() {
                return Err("Angiv komplet kildeobjekt uden confirm. Den separate reviewbekræftelse gælder præcis den viste anmodning.".into());
            }
            value
        } else {
            let selected = data.selected.as_ref().ok_or(
                "Vælg en konkret aktuelt synlig mapping og gennemgå hele kilde/proof først",
            )?;
            if action == Action::Approve {
                if selected["status"] != "proposed" {
                    return Err("Godkendelse kræver præcis en aktuelt foreslået mapping".into());
                }
                if data.self_approval() {
                    return Err("Forslagsstilleren må ikke godkende egen mapping. En ANDEN autentificeret WorkspaceOwner med aktuelle CompanyOwner-rettigheder i begge endpoints skal gennemgå præcis denne mapping/hash. Der skiftes ingen session automatisk.".into());
                }
            } else if selected["status"] != "approved" {
                return Err("Tilbagekaldelse kræver præcis en aktuelt godkendt mapping".into());
            }
            if !data.refs_owned(
                ["leftCompanySlug", "rightCompanySlug"]
                    .iter()
                    .filter_map(|k| selected["mapping"][*k].as_str()),
            ) {
                return Err(
                    "Aktuel CompanyOwner-adgang kræves til BEGGE mappingens virksomheder".into(),
                );
            }
            json!({"mappingId":selected["mapping"]["id"],"mappingHash":selected["mappingHash"]})
        };
        match action {
            Action::Manifest => {
                let mut refs = memberships(&body)?;
                if !data.current["manifest"].is_null() {
                    refs.extend(memberships(&data.current["manifest"])?);
                }
                if !data.refs_owned(refs.into_iter()) {
                    return Err("Manifestændring kræver aktuelle ejerrettigheder til ALLE både tidligere og nye virksomhedsreferencer. Ingen eksisterende skjult konfiguration adopteres.".into());
                }
            }
            Action::Propose => {
                if data.current["manifest"].is_null() {
                    return Err(
                        "Mapping kræver et aktuelt konfigureret og fuldt synligt V1-manifest"
                            .into(),
                    );
                }
                let refs = ["leftCompanySlug", "rightCompanySlug"]
                    .iter()
                    .map(|k| {
                        body[*k]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .ok_or("Angiv begge faktiske companySlug-endpoints")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if !data.refs_owned(refs.into_iter()) {
                    return Err(
                        "Mapping kræver aktuelle CompanyOwner-rettigheder til begge endpoints"
                            .into(),
                    );
                }
            }
            _ => {}
        }
        body["confirm"] = json!(true);
        let bytes = serde_json::to_vec(&body).map_err(|_| "Anmodningen kunne ikke fastholdes")?;
        if bytes.len() > rentemester_client::MAX_INTERCOMPANY_BODY {
            return Err("Anmodningen overstiger256KiB".into());
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
        json!({"format":"rentemester-desktop-intercompany/1","origin":origin,"company":self.snapshot.company,"principal":self.snapshot.principal,"ledger":self.snapshot.ledger,"localIdentity":local,"operation":self.action.name(),"mappingId":match self.action{Action::Manifest=>Value::Null,Action::Propose=>self.display["id"].clone(),_=>self.display["mappingId"].clone()},"mappingHash":if matches!(self.action,Action::Approve|Action::Revoke){self.display["mappingHash"].clone()}else{Value::Null},"manifestHash":self.snapshot.current["manifestHash"]})
    }
    pub fn receipt(&self, value: &Value) -> bool {
        if value["ok"] != true {
            return false;
        }
        match self.action {
            Action::Manifest => {
                hash(&value["manifestHash"])
                    && matches!(value["status"].as_str(), Some("applied" | "unchanged"))
            }
            Action::Propose => {
                value["mappingId"] == self.display["id"]
                    && hash(&value["mappingHash"])
                    && value["status"] == "proposed"
            }
            Action::Approve | Action::Revoke => {
                value["mappingId"] == self.display["mappingId"]
                    && value["mappingHash"] == self.display["mappingHash"]
                    && value["status"]
                        == if self.action == Action::Approve {
                            "approved"
                        } else {
                            "revoked"
                        }
            }
        }
    }
    pub fn readback(&self, receipt: &Value, data: &Snapshot) -> bool {
        if !self.receipt(receipt) {
            return false;
        }
        if self.action == Action::Manifest {
            return data.current["manifestHash"] == receipt["manifestHash"];
        }
        data.current["mappings"].as_array().is_some_and(|a| {
            a.iter().any(|m| {
                m["mapping"]["id"] == receipt["mappingId"]
                    && m["mappingHash"] == receipt["mappingHash"]
                    && m["status"] == receipt["status"]
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
        Value::String(s) => vec![format!("{prefix}: {s}")],
        _ => vec![format!("{prefix}: {v}")],
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Value {
        json!({"version":1,"groups":[{"id":"synthetic-group","name":"Synthetic reciprocal group","memberships":[{"id":"left","companySlug":"left","validFrom":"2000-01-01"},{"id":"right","companySlug":"right","validFrom":"2000-01-01"}],"ownership":[]}]})
    }
    fn mapping() -> Value {
        json!({"id":"synthetic-map","groupId":"synthetic-group","leftCompanySlug":"left","rightCompanySlug":"right","leftAccountNos":["source-asset"],"rightAccountNos":["source-liability"],"leftPosition":"receivable","rightPosition":"payable","validFrom":"2000-01-01","evidenceRefs":["actual-reviewed-source"]})
    }
    fn report() -> Value {
        json!({"ok":true,"scope":"intercompany-reconciliation","asOf":"2026-06-01","rows":[{"mappingId":"synthetic-map","mappingHash":"b".repeat(64),"status":"matched","difference":"0.00","left":{"companySlug":"left","balance":"12.34","currency":"DKK","sourceRefs":[{"entryId":1,"amount":"12.34"}],"sourceSnapshot":{"selectionHash":"c".repeat(64),"ledgerHeadHash":"d".repeat(64),"entryCount":1}},"right":{"companySlug":"right","balance":"12.34","currency":"DKK","sourceRefs":[{"entryId":2,"amount":"-12.34"}],"sourceSnapshot":{"selectionHash":"e".repeat(64),"ledgerHeadHash":"f".repeat(64),"entryCount":1}},"blockers":[]}],"futureEvidence":{"retained":true}})
    }
    fn data(principal: &str, status: &str) -> Snapshot {
        Snapshot::parse("left",&Selection{as_of:"2026-06-01".into(),id:Some("synthetic-map".into())},json!({"ok":true,"manifest":manifest(),"manifestHash":"a".repeat(64),"mappings":[{"mapping":mapping(),"mappingHash":"b".repeat(64),"status":status,"actor":"user:author"}],"consolidationAvailable":false}),report(),json!({"ok":true,"user":{"id":principal},"workspaceRole":"workspace_owner","companies":[{"slug":"left","name":"Synthetic left","role":"owner","archived":false},{"slug":"right","name":"Synthetic right","role":"owner","archived":false}]}),json!({"ok":true,"journal":{"slug":"left","ledgerUuid":"ledger-left"}})).unwrap()
    }
    #[test]
    fn explicit_manifest_mapping_intent_freezes_supplied_facts_without_defaults_keys_or_money() {
        let data = data("author", "revoked");
        for (action, input) in [(Action::Manifest, manifest()), (Action::Propose, mapping())] {
            let review = Review::prepare(&data, action, &input.to_string()).unwrap();
            let wire: Value = serde_json::from_slice(&review.body).unwrap();
            let mut expected = input.clone();
            expected["confirm"] = json!(true);
            assert_eq!(wire, expected);
            assert_eq!(review.clone().body, review.body);
            for absent in [
                "actor",
                "principal",
                "idempotencyKey",
                "amount",
                "currency",
                "mappingHash",
            ] {
                assert!(wire.get(absent).is_none());
            }
            assert_eq!(review.snapshot.report, data.report);
            assert!(wire.get("validToExclusive").is_none());
        }
        for raw in ["", "[]", "null", "not JSON", "{\"confirm\":false}"] {
            assert!(Review::prepare(&data, Action::Manifest, raw).is_err());
        }
    }
    #[test]
    fn workspace_and_all_current_old_new_company_owners_are_required() {
        let data = data("author", "proposed");
        let raw = manifest().to_string();
        let mut next = data.clone();
        next.workspace_role = "member".into();
        assert!(Review::prepare(&next, Action::Manifest, &raw).is_err());
        for i in [0, 1] {
            let mut next = data.clone();
            next.companies[i]["role"] = json!("reviewer");
            assert!(Review::prepare(&next, Action::Manifest, &raw).is_err());
            assert!(Review::prepare(&next, Action::Propose, &mapping().to_string()).is_err());
        }
        let mut next = data.clone();
        next.companies[1]["archived"] = json!(true);
        let mut fewer = manifest();
        fewer["groups"][0]["memberships"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(Review::prepare(&next, Action::Manifest, &fewer.to_string()).is_err());
        let mut unknown = mapping();
        unknown["rightCompanySlug"] = json!("hidden");
        assert!(Review::prepare(&data, Action::Propose, &unknown.to_string()).is_err());
        let mut unconfigured = data;
        unconfigured.current["manifest"] = Value::Null;
        unconfigured.current["manifestHash"] = Value::Null;
        assert!(Review::prepare(&unconfigured, Action::Propose, &mapping().to_string()).is_err());
    }
    #[test]
    fn approval_requires_different_authenticated_actor_and_exact_proposed_mapping_hash() {
        let author = data("author", "proposed");
        assert!(author.self_approval());
        assert!(Review::prepare(&author, Action::Approve, "").is_err());
        let reviewer = data("reviewer", "proposed");
        let review = Review::prepare(&reviewer, Action::Approve, "").unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&review.body).unwrap(),
            json!({"mappingId":"synthetic-map","mappingHash":"b".repeat(64),"confirm":true})
        );
        for status in ["approved", "revoked"] {
            assert!(Review::prepare(&data("reviewer", status), Action::Approve, "").is_err());
        }
        let mut unauthorized = reviewer;
        unauthorized.companies[1]["role"] = json!("bookkeeper");
        assert!(Review::prepare(&unauthorized, Action::Approve, "").is_err());
    }
    #[test]
    fn revoke_is_separate_explicit_current_approved_source_operation() {
        let source = data("author", "approved");
        let review = Review::prepare(&source, Action::Revoke, "").unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&review.body).unwrap(),
            json!({"mappingId":"synthetic-map","mappingHash":"b".repeat(64),"confirm":true})
        );
        for status in ["proposed", "revoked"] {
            assert!(Review::prepare(&data("author", status), Action::Revoke, "").is_err());
        }
        let mut absent = source;
        absent.selected = None;
        assert!(Review::prepare(&absent, Action::Approve, "").is_err());
        assert!(Review::prepare(&absent, Action::Revoke, "").is_err());
    }
    #[test]
    fn source_period_decimal_strings_are_preserved_without_conversion_or_inferred_consolidation() {
        let source = data("author", "approved");
        assert_eq!(source.report, report());
        assert_eq!(source.report["rows"][0]["difference"], "0.00");
        assert_eq!(
            source.report["rows"][0]["right"]["sourceRefs"][0]["amount"],
            "-12.34"
        );
        assert!(
            lines(&source.report, "report")
                .iter()
                .any(|line| line.ends_with("-12.34"))
        );
        for what in ["difference", "balance", "amount"] {
            let mut wrong = report();
            match what {
                "difference" => wrong["rows"][0]["difference"] = json!(0.0),
                "balance" => wrong["rows"][0]["left"]["balance"] = json!(12.34),
                _ => wrong["rows"][0]["left"]["sourceRefs"][0]["amount"] = json!(12.34),
            }
            assert!(!report_valid(&wrong, "2026-06-01"));
        }
        assert!(!report_valid(&report(), "2026-06-02"));
        let mut hidden = report();
        hidden["rows"] = json!([]);
        assert!(report_valid(&hidden, "2026-06-01"));
    }
    #[test]
    fn frozen_context_refuses_changed_roles_manifest_mapping_and_actual_period_proofs() {
        let source = data("reviewer", "proposed");
        let review = Review::prepare(&source, Action::Approve, "").unwrap();
        assert!(review.current(&source));
        for what in [
            "principal",
            "ledger",
            "role",
            "companies",
            "manifest",
            "mapping",
            "period",
            "proof",
        ] {
            let mut next = source.clone();
            match what {
                "principal" => next.principal = "other".into(),
                "ledger" => next.ledger = "other".into(),
                "role" => next.workspace_role = "member".into(),
                "companies" => next.companies.clear(),
                "manifest" => next.current["manifestHash"] = json!("c".repeat(64)),
                "mapping" => next.selected.as_mut().unwrap()["mappingHash"] = json!("c".repeat(64)),
                "period" => next.report["asOf"] = json!("2026-06-02"),
                _ => {
                    next.report["rows"][0]["left"]["sourceSnapshot"]["ledgerHeadHash"] =
                        json!("c".repeat(64))
                }
            };
            assert!(!review.current(&next));
        }
        let local = review.marker("http://127.0.0.1:1", Some("trusted-profile"));
        assert!(scope(
            &local,
            "http://127.0.0.1:2",
            &source,
            Some("trusted-profile")
        ));
        assert!(!scope(
            &local,
            "http://127.0.0.1:2",
            &source,
            Some("other-profile")
        ));
        let remote = review.marker("https://one.invalid", None);
        assert!(!scope(&remote, "https://two.invalid", &source, None));
    }
    #[test]
    fn canonical_receipts_and_readback_match_only_exact_current_state_hashes() {
        let source = data("reviewer", "proposed");
        let approve = Review::prepare(&source, Action::Approve, "").unwrap();
        let receipt = json!({"ok":true,"mappingId":"synthetic-map","mappingHash":"b".repeat(64),"status":"approved"});
        assert!(approve.receipt(&receipt));
        assert!(!approve.readback(&receipt, &source));
        let approved = data("reviewer", "approved");
        assert!(approve.readback(&receipt, &approved));
        for key in ["mappingId", "mappingHash", "status"] {
            let mut wrong = receipt.clone();
            wrong[key] = json!("wrong");
            assert!(!approve.receipt(&wrong));
        }
        let manifest = Review::prepare(&source, Action::Manifest, &manifest().to_string()).unwrap();
        let receipt = json!({"ok":true,"status":"applied","manifestHash":"a".repeat(64)});
        assert!(manifest.receipt(&receipt));
        assert!(manifest.readback(&receipt, &source));
        let mut changed = source;
        changed.current["manifestHash"] = json!("f".repeat(64));
        assert!(!manifest.readback(&receipt, &changed));
    }
    #[test]
    fn private_unknown_attempt_is_payload_free_preserved_and_never_replaced() {
        let mut random = [0u8; 16];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random).unwrap();
        let suffix = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("native-intercompany-{suffix}"));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let source = data("author", "revoked");
        let review = Review::prepare(&source, Action::Propose, &mapping().to_string()).unwrap();
        let marker = review.marker("https://synthetic.invalid", None);
        assert!(valid_record(&marker));
        assert!(marker["mappingHash"].is_null());
        for key in [
            "body",
            "mapping",
            "leftAccountNos",
            "balance",
            "amount",
            "evidenceRefs",
            "validFrom",
        ] {
            assert!(marker.get(key).is_none());
        }
        let mut unsafe_marker = marker.clone();
        unsafe_marker["body"] = json!("private source");
        assert!(!valid_record(&unsafe_marker));
        begin_at(&dir, &marker).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(marker.clone()));
        assert!(begin_at(&dir, &marker).is_err());
        let mut wrong = marker.clone();
        wrong["mappingId"] = json!("other");
        assert!(finish_at(&dir, &wrong, false).is_err());
        assert_eq!(load_at(&dir).unwrap(), Some(marker.clone()));
        finish_at(&dir, &marker, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
