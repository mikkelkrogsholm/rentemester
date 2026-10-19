//! One private unresolved eliminations attempt; no payload persistence or replay.
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
    let m = fs::symlink_metadata(path).map_err(|_| "Privat elimineringsstatus kunne ikke åbnes")?;
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
            "Elimineringsstatus kræver din egen private mappe0700/fil0600 uden symbolske links."
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
        return Err("Privat elimineringsstatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-eliminations.json");
    if !path
        .try_exists()
        .map_err(|_| "Elimineringsstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Elimineringsstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Elimineringsstatus overskrider grænsen".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Elimineringsstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret elimineringsforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-eliminations/1"
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
                        "eliminationId",
                        "payloadHash",
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
            Some("propose" | "approve" | "reject" | "apply" | "reverse")
        )
        && ["localIdentity", "eliminationId", "payloadHash"]
            .iter()
            .all(|k| value[*k].is_null() || value[*k].as_str().is_some_and(|s| s.len() < 1024))
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig elimineringsstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-eliminations.json"))
        .map_err(|_| "Et elimineringsforsøg kræver afklaring før ny bogføring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Elimineringsstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Elimineringsstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Elimineringsstatus ændrede sig og bevares".into());
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
            .open(dir.join(format!("eliminations-manually-resolved-{suffix}.json")))
            .map_err(|_| "Elimineringsafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Elimineringsafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-eliminations.json"))
        .map_err(|_| "Elimineringsstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Elimineringsstatus kunne ikke synkroniseres".into())
}

use rentemester_client::EliminationMutation;
use serde_json::json;
use std::sync::Arc;
#[derive(Clone, Default)]
pub struct Selection {
    pub as_of: String,
    pub id: Option<String>,
    pub mapping_id: Option<String>,
}
#[derive(Clone, PartialEq)]
pub struct Snapshot {
    pub company: String,
    pub principal: String,
    pub ledger: String,
    pub workspace_role: String,
    pub companies: Vec<Value>,
    pub as_of: String,
    pub current: Value,
    pub mappings: Value,
    pub overview: Value,
    pub selected: Option<Value>,
    pub mapping: Option<Value>,
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
fn payload_valid(p: &Value) -> bool {
    if !matches!(p["version"].as_u64(), Some(1 | 2))
        || ["id", "mappingId", "asOf", "currency", "amountOre"]
            .iter()
            .any(|k| text(p, k).is_err())
        || !hash(&p["mappingHash"])
        || !p["evidenceRefs"].is_array()
    {
        return false;
    }
    ["left", "right"].iter().all(|k| {
        let s = &p[*k];
        text(s, "companySlug").is_ok()
            && s["accountNos"].is_array()
            && hash(&s["selectionHash"])
            && (s["ledgerHeadHash"].is_null() || hash(&s["ledgerHeadHash"]))
            && s["entryCount"].is_u64()
            && (p["version"] == 1
                || (s["position"].is_string()
                    && s["debitOre"].is_string()
                    && s["creditOre"].is_string()))
    })
}
fn current_valid(v: &Value) -> bool {
    v["ok"] == true
        && v["rows"].as_array().is_some_and(|a| {
            a.iter().all(|r| {
                payload_valid(&r["payload"])
                    && hash(&r["payloadHash"])
                    && hash(&r["eventHash"])
                    && matches!(
                        r["status"].as_str(),
                        Some("proposed" | "approved" | "rejected" | "applied" | "reversed")
                    )
                    && text(r, "actor").is_ok()
            })
        })
}
impl Snapshot {
    pub fn parse(
        company: &str,
        s: &Selection,
        current: Value,
        mappings: Value,
        overview: Value,
        me: Value,
        journal: Value,
    ) -> Result<Self, String> {
        if !date(&s.as_of)
            || !current_valid(&current)
            || mappings["ok"] != true
            || !mappings["mappings"].is_array()
            || overview["ok"] != true
            || overview["scope"] != "consolidation-eliminations"
            || overview["asOf"] != s.as_of
            || !overview["rows"].is_array()
            || me["ok"] != true
            || journal["ok"] != true
            || journal["journal"]["slug"] != company
        {
            return Err("Serveren bekræftede ikke kanoniske elimineringer, mappingkilder, præcis dato, bruger og ledger".into());
        }
        let selected =
            s.id.as_ref()
                .and_then(|id| {
                    current["rows"]
                        .as_array()
                        .and_then(|a| a.iter().find(|r| r["payload"]["id"] == *id))
                })
                .cloned();
        let mapping = s
            .mapping_id
            .as_ref()
            .and_then(|id| {
                mappings["mappings"]
                    .as_array()
                    .and_then(|a| a.iter().find(|r| r["mapping"]["id"] == *id))
            })
            .cloned();
        Ok(Self {
            company: company.into(),
            principal: text(&me["user"], "id")?,
            ledger: text(&journal["journal"], "ledgerUuid")?,
            workspace_role: text(&me, "workspaceRole")?,
            companies: me["companies"]
                .as_array()
                .ok_or("Aktuelle virksomhedsroller mangler")?
                .clone(),
            as_of: s.as_of.clone(),
            current,
            mappings,
            overview,
            selected,
            mapping,
        })
    }
    fn owner(&self, slug: &str) -> bool {
        self.companies
            .iter()
            .any(|c| c["slug"] == slug && c["role"] == "owner" && c["archived"] != true)
    }
    fn can_write(&self, p: &Value) -> bool {
        self.workspace_role == "workspace_owner"
            && self.owner(&self.company)
            && ["left", "right"]
                .iter()
                .all(|k| p[*k]["companySlug"].as_str().is_some_and(|s| self.owner(s)))
    }
    pub fn self_approval(&self) -> bool {
        self.selected.as_ref().is_some_and(|s| {
            s["status"] == "proposed" && s["actor"] == format!("user:{}", self.principal)
        })
    }
}
#[derive(Clone)]
pub struct Plan {
    pub input: Value,
    pub proof: Value,
    pub snapshot: Snapshot,
}
pub fn input(data: &Snapshot, id: &str, evidence: &str) -> Result<Value, String> {
    let m = data
        .mapping
        .as_ref()
        .filter(|m| m["status"] == "approved")
        .ok_or("Vælg en aktuelt godkendt synlig mapping først")?;
    if id.trim().is_empty() {
        return Err(
            "Angiv et eksplicit ID for dette kildebelagte forslag; intet ID genereres".into(),
        );
    }
    let refs: Value = serde_json::from_str(evidence)
        .map_err(|e| format!("Angiv eksplicit evidenceRefs JSON-array: {e}"))?;
    if !refs.as_array().is_some_and(|a| {
        !a.is_empty()
            && a.iter()
                .all(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()))
    }) {
        return Err("Angiv faktiske evidencereferencer som ikke-tomt array af strenge".into());
    }
    Ok(json!({"id":id,"mappingId":m["mapping"]["id"],"asOf":data.as_of,"evidenceRefs":refs}))
}
impl Plan {
    pub fn parse(data: &Snapshot, input: Value, proof: Value) -> Result<Self, String> {
        let p = &proof["plan"]["payload"];
        if proof["ok"] != true
            || !hash(&proof["plan"]["payloadHash"])
            || !payload_valid(p)
            || data
                .mapping
                .as_ref()
                .is_none_or(|m| m["status"] != "approved" || m["mappingHash"] != p["mappingHash"])
            || ["id", "mappingId", "asOf"]
                .iter()
                .any(|k| p[*k] != input[*k])
        {
            return Err("Serveren bekræftede ikke den præcise readonly kildeplan, V1/V2-payload og kanonisk payloadHash".into());
        }
        Ok(Self {
            input,
            proof,
            snapshot: data.clone(),
        })
    }
}
pub fn selected_input(payload: &Value) -> Value {
    json!({"id":payload["id"],"mappingId":payload["mappingId"],"asOf":payload["asOf"],"evidenceRefs":payload["evidenceRefs"]})
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Propose,
    Approve,
    Reject,
    Apply,
    Reverse,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Propose => "propose",
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Apply => "apply",
            Self::Reverse => "reverse",
        }
    }
    pub fn sdk(self) -> EliminationMutation {
        match self {
            Self::Propose => EliminationMutation::Propose,
            Self::Approve => EliminationMutation::Approve,
            Self::Reject => EliminationMutation::Reject,
            Self::Apply => EliminationMutation::Apply,
            Self::Reverse => EliminationMutation::Reverse,
        }
    }
    pub fn needs_plan(self) -> bool {
        matches!(self, Self::Propose | Self::Approve | Self::Apply)
    }
}
#[derive(Clone)]
pub struct Review {
    pub action: Action,
    pub snapshot: Snapshot,
    pub body: Arc<Vec<u8>>,
    pub display: Value,
    pub payload: Value,
    pub payload_hash: Value,
}
impl Review {
    pub fn prepare(data: &Snapshot, action: Action, plan: Option<&Plan>) -> Result<Self, String> {
        let (payload, payload_hash, mut body) = if action == Action::Propose {
            let plan = plan.filter(|p| p.snapshot == *data).ok_or(
                "Hent og gennemgå en readonly serverplan for præcis nuværende kilde først",
            )?;
            let mut body = plan.input.clone();
            body["expectedPayloadHash"] = plan.proof["plan"]["payloadHash"].clone();
            (
                plan.proof["plan"]["payload"].clone(),
                plan.proof["plan"]["payloadHash"].clone(),
                body,
            )
        } else {
            let s = data
                .selected
                .as_ref()
                .ok_or("Vælg en aktuelt synlig kanonisk eliminering først")?;
            let expected = match action {
                Action::Approve | Action::Reject => "proposed",
                Action::Apply => "approved",
                Action::Reverse => "applied",
                Action::Propose => unreachable!(),
            };
            if s["status"] != expected {
                return Err(format!("Handlingen kræver præcis aktuel status {expected}"));
            }
            if action == Action::Approve && data.self_approval() {
                return Err("Eget forslag må ikke godkendes. En ANDEN autentificeret reviewer skal gennemgå præcis payloadHash.".into());
            }
            (
                s["payload"].clone(),
                s["payloadHash"].clone(),
                json!({"eliminationId":s["payload"]["id"],"payloadHash":s["payloadHash"]}),
            )
        };
        if !data.can_write(&payload) {
            return Err("Aktuel WorkspaceOwner og CompanyAdmin/Owner til begge kildevirksomheder og anker kræves. Serveren kontrollerer alle relevante kilde-/grupperettigheder.".into());
        }
        body["confirm"] = json!(true);
        let bytes = serde_json::to_vec(&body).map_err(|_| "Anmodningen kunne ikke fastholdes")?;
        if bytes.len() > rentemester_client::MAX_ELIMINATION_BODY {
            return Err("Anmodningen overskrider native256KiB".into());
        }
        Ok(Self {
            action,
            snapshot: data.clone(),
            body: Arc::new(bytes),
            display: body,
            payload,
            payload_hash,
        })
    }
    pub fn current(&self, data: &Snapshot) -> bool {
        self.snapshot == *data
    }
    pub fn fresh_plan_matches(&self, proof: &Value) -> bool {
        proof["ok"] == true
            && proof["plan"]["payload"] == self.payload
            && proof["plan"]["payloadHash"] == self.payload_hash
    }
    pub fn marker(&self, origin: &str, local: Option<&str>) -> Value {
        json!({"format":"rentemester-desktop-eliminations/1","origin":origin,"company":self.snapshot.company,"principal":self.snapshot.principal,"ledger":self.snapshot.ledger,"localIdentity":local,"operation":self.action.name(),"eliminationId":self.payload["id"],"payloadHash":self.payload_hash})
    }
    pub fn receipt(&self, v: &Value) -> bool {
        v["ok"] == true
            && v["eliminationId"] == self.payload["id"]
            && v["payloadHash"] == self.payload_hash
            && hash(&v["eventHash"])
            && v["status"]
                == match self.action {
                    Action::Propose => "proposed",
                    Action::Approve => "approved",
                    Action::Reject => "rejected",
                    Action::Apply => "applied",
                    Action::Reverse => "reversed",
                }
    }
    pub fn readback(&self, v: &Value, data: &Snapshot) -> bool {
        self.receipt(v)
            && data.current["rows"].as_array().is_some_and(|a| {
                a.iter().any(|s| {
                    s["payload"]["id"] == v["eliminationId"]
                        && s["payload"] == self.payload
                        && s["payloadHash"] == v["payloadHash"]
                        && s["eventHash"] == v["eventHash"]
                        && s["status"] == v["status"]
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
    fn payload(version: u8, id: &str) -> Value {
        let mut p = json!({"version":version,"id":id,"kind":"balance","groupId":"synthetic-group","mappingId":"source-map","mappingHash":"a".repeat(64),"asOf":"2026-06-01","currency":"DKK","amountOre":"1234","left":{"companySlug":"left","accountNos":["source-asset"],"selectionHash":"c".repeat(64),"ledgerHeadHash":"d".repeat(64),"entryCount":2},"right":{"companySlug":"right","accountNos":["source-liability"],"selectionHash":"e".repeat(64),"ledgerHeadHash":"f".repeat(64),"entryCount":2},"evidenceRefs":["actual-reviewed-original"]});
        if version == 2 {
            p["left"]["position"] = json!("receivable");
            p["left"]["debitOre"] = json!("0");
            p["left"]["creditOre"] = json!("1234");
            p["right"]["position"] = json!("payable");
            p["right"]["debitOre"] = json!("1234");
            p["right"]["creditOre"] = json!("0");
        }
        p
    }
    fn selection() -> Selection {
        Selection {
            as_of: "2026-06-01".into(),
            id: Some("synthetic-elimination".into()),
            mapping_id: Some("source-map".into()),
        }
    }
    fn data(principal: &str, status: &str) -> Snapshot {
        let actor = if status == "proposed" {
            "user:author"
        } else {
            "user:reviewer"
        };
        let current = json!({"ok":true,"rows":[{"payload":payload(2,"synthetic-elimination"),"payloadHash":"b".repeat(64),"eventHash":"9".repeat(64),"status":status,"actor":actor}]});
        let mappings = json!({"ok":true,"manifest":null,"manifestHash":null,"mappings":[{"mapping":{"id":"source-map","leftCompanySlug":"left","rightCompanySlug":"right"},"mappingHash":"a".repeat(64),"status":"approved","actor":"user:reviewer"}],"consolidationAvailable":false});
        let overview = json!({"ok":true,"scope":"consolidation-eliminations","asOf":"2026-06-01","rows":[],"sourceCoverage":{"authority":"server"}});
        let me = json!({"ok":true,"user":{"id":principal},"workspaceRole":"workspace_owner","companies":[{"slug":"left","role":"owner"},{"slug":"right","role":"owner"}]});
        let journal = json!({"ok":true,"journal":{"slug":"left","ledgerUuid":"immutable-ledger"}});
        Snapshot::parse(
            "left",
            &selection(),
            current,
            mappings,
            overview,
            me,
            journal,
        )
        .unwrap()
    }
    fn plan(data: &Snapshot, version: u8) -> Plan {
        let input = input(data, "synthetic-new", r#"["actual-reviewed-original"]"#).unwrap();
        Plan::parse(data,input,json!({"ok":true,"plan":{"payload":payload(version,"synthetic-new"),"payloadHash":"b".repeat(64)}})).unwrap()
    }
    #[test]
    fn explicit_mapping_id_date_evidence_readonly_plan_is_required_without_invented_defaults() {
        let d = data("author", "rejected");
        for evidence in ["null", "[]", r#"[123]"#, r#"[""]"#, "{"] {
            assert!(input(&d, "explicit", evidence).is_err());
        }
        assert!(input(&d, "", r#"["source"]"#).is_err());
        let mut hidden = d.clone();
        hidden.mapping = None;
        assert!(input(&hidden, "explicit", r#"["source"]"#).is_err());
        let mut revoked = d.clone();
        revoked.mapping.as_mut().unwrap()["status"] = json!("revoked");
        assert!(input(&revoked, "explicit", r#"["source"]"#).is_err());
        let p = plan(&d, 2);
        assert_eq!(
            p.input,
            json!({"id":"synthetic-new","mappingId":"source-map","asOf":"2026-06-01","evidenceRefs":["actual-reviewed-original"]})
        );
        assert!(Review::prepare(&d, Action::Propose, None).is_err());
    }
    #[test]
    fn frozen_proposal_uses_exact_server_hash_and_source_fields_without_amount_or_key_injection() {
        let d = data("author", "rejected");
        let p = plan(&d, 2);
        let r = Review::prepare(&d, Action::Propose, Some(&p)).unwrap();
        assert_eq!(
            r.display,
            json!({"id":"synthetic-new","mappingId":"source-map","asOf":"2026-06-01","evidenceRefs":["actual-reviewed-original"],"expectedPayloadHash":"b".repeat(64),"confirm":true})
        );
        assert_eq!(serde_json::from_slice::<Value>(&r.body).unwrap(), r.display);
        for k in [
            "actor",
            "idempotencyKey",
            "amountOre",
            "debitOre",
            "creditOre",
            "payload",
        ] {
            assert!(r.display.get(k).is_none());
        }
        assert!(r.current(&d));
    }
    #[test]
    fn v1_v2_source_positions_ore_strings_and_unknown_provenance_are_not_recomputed() {
        let d = data("author", "rejected");
        let legacy = plan(&d, 1);
        assert!(
            legacy.proof["plan"]["payload"]["left"]
                .get("position")
                .is_none()
        );
        let modern = plan(&d, 2);
        assert_eq!(modern.proof["plan"]["payload"]["left"]["creditOre"], "1234");
        assert_eq!(modern.proof["plan"]["payload"]["right"]["debitOre"], "1234");
        let mut proof = modern.proof.clone();
        proof["plan"]["futureSource"] = json!({"preserved":true});
        assert_eq!(
            Plan::parse(&d, modern.input.clone(), proof.clone())
                .unwrap()
                .proof,
            proof
        );
        for key in ["amountOre"] {
            let mut bad = modern.proof.clone();
            bad["plan"]["payload"][key] = json!(1234);
            assert!(Plan::parse(&d, modern.input.clone(), bad).is_err());
        }
        let mut bad = modern.proof.clone();
        bad["plan"]["payload"]["left"]["creditOre"] = json!(1234);
        assert!(Plan::parse(&d, modern.input, bad).is_err());
    }
    #[test]
    fn source_exact_lifecycle_independent_approval_and_server_only_original_proposer_application_rule()
     {
        let author = data("author", "proposed");
        assert!(Review::prepare(&author, Action::Approve, None).is_err());
        let reviewer = data("reviewer", "proposed");
        let r = Review::prepare(&reviewer, Action::Approve, None).unwrap();
        assert_eq!(
            r.display,
            json!({"eliminationId":"synthetic-elimination","payloadHash":"b".repeat(64),"confirm":true})
        );
        assert!(Review::prepare(&author, Action::Reject, None).is_ok());
        assert!(Review::prepare(&reviewer, Action::Apply, None).is_err());
        let approved = data("author", "approved");
        assert!(
            approved
                .selected
                .as_ref()
                .unwrap()
                .get("proposer")
                .is_none()
        );
        assert!(Review::prepare(&approved, Action::Apply, None).is_ok());
        assert!(Review::prepare(&approved, Action::Reverse, None).is_err());
        assert!(Review::prepare(&data("reviewer", "applied"), Action::Reverse, None).is_ok());
        assert!(Review::prepare(&data("reviewer", "reversed"), Action::Apply, None).is_err());
    }
    #[test]
    fn current_workspace_and_both_real_company_source_permissions_are_required() {
        let source = data("reviewer", "proposed");
        for change in 0..4 {
            let mut d = source.clone();
            match change {
                0 => d.workspace_role = "member".into(),
                1 => d.companies[1]["role"] = json!("bookkeeper"),
                2 => d.companies[0]["archived"] = json!(true),
                _ => {
                    d.selected.as_mut().unwrap()["payload"]["right"]["companySlug"] =
                        json!("hidden")
                }
            }
            assert!(Review::prepare(&d, Action::Approve, None).is_err());
        }
        let mut hidden = source;
        hidden.selected = None;
        assert!(Review::prepare(&hidden, Action::Reject, None).is_err());
    }
    #[test]
    fn fresh_context_and_readonly_plan_refuse_changed_source_hashes_mapping_dates_or_permissions() {
        let d = data("author", "rejected");
        let p = plan(&d, 2);
        let r = Review::prepare(&d, Action::Propose, Some(&p)).unwrap();
        assert!(r.fresh_plan_matches(&p.proof));
        for change in 0..6 {
            let mut next = d.clone();
            match change {
                0 => next.principal = "other".into(),
                1 => next.ledger = "other".into(),
                2 => next.as_of = "2026-06-02".into(),
                3 => next.companies[1]["role"] = json!("bookkeeper"),
                4 => next.mappings["mappings"][0]["mappingHash"] = json!("8".repeat(64)),
                _ => next.overview["sourceCoverage"] = json!({"changed":true}),
            }
            assert!(!r.current(&next));
            assert!(Review::prepare(&next, Action::Propose, Some(&p)).is_err());
        }
        for change in 0..3 {
            let mut proof = p.proof.clone();
            match change {
                0 => proof["plan"]["payloadHash"] = json!("8".repeat(64)),
                1 => proof["plan"]["payload"]["left"]["selectionHash"] = json!("8".repeat(64)),
                _ => proof["plan"]["payload"]["left"]["ledgerHeadHash"] = json!("8".repeat(64)),
            }
            assert!(!r.fresh_plan_matches(&proof));
        }
        let mut proof = p.proof;
        proof["plan"]["payload"]["mappingHash"] = json!("8".repeat(64));
        assert!(Plan::parse(&d, p.input, proof).is_err());
    }
    #[test]
    fn confirmed_receipt_readback_requires_exact_payload_status_and_event_hash() {
        let d = data("reviewer", "approved");
        let r = Review::prepare(&d, Action::Apply, None).unwrap();
        let receipt = json!({"ok":true,"eliminationId":"synthetic-elimination","payloadHash":"b".repeat(64),"eventHash":"9".repeat(64),"status":"applied"});
        assert!(r.receipt(&receipt));
        assert!(!r.readback(&receipt, &d));
        let applied = data("reviewer", "applied");
        assert!(r.readback(&receipt, &applied));
        for key in ["eliminationId", "payloadHash", "status", "eventHash"] {
            let mut bad = receipt.clone();
            bad[key] = json!("wrong");
            assert!(!r.receipt(&bad));
        }
        let mut stale = applied;
        stale.current["rows"][0]["payload"]["left"]["selectionHash"] = json!("8".repeat(64));
        assert!(!r.readback(&receipt, &stale));
        assert!(!Action::Reverse.needs_plan());
        assert!(!Action::Reject.needs_plan());
        assert!(Action::Apply.needs_plan());
    }
    #[test]
    fn private_unknown_marker_is_payload_free_exact_scoped_and_never_replaced_or_replayed() {
        let d = data("reviewer", "approved");
        let r = Review::prepare(&d, Action::Apply, None).unwrap();
        let marker = r.marker("https://one.invalid", None);
        assert!(valid_record(&marker));
        for k in [
            "body",
            "payload",
            "accountNos",
            "amountOre",
            "debitOre",
            "asOf",
            "evidenceRefs",
        ] {
            assert!(marker.get(k).is_none());
        }
        assert!(!scope(&marker, "https://two.invalid", &d, None));
        let local = r.marker("http://127.0.0.1:1", Some("trusted-profile"));
        assert!(scope(
            &local,
            "http://127.0.0.1:2",
            &d,
            Some("trusted-profile")
        ));
        assert!(!scope(&local, "http://127.0.0.1:2", &d, Some("other")));
        let mut random = [0u8; 16];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random).unwrap();
        let suffix = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("native-eliminations-{suffix}"));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        begin_at(&dir, &marker).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(marker.clone()));
        assert!(begin_at(&dir, &marker).is_err());
        let mut bad = marker.clone();
        bad["eliminationId"] = json!("other");
        assert!(finish_at(&dir, &bad, false).is_err());
        bad = marker.clone();
        bad["payload"] = json!({"private":"source"});
        assert!(!valid_record(&bad));
        finish_at(&dir, &marker, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
