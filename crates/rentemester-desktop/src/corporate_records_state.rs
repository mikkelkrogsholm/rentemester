//! One private unresolved corporate-records attempt; no payload persistence or replay.
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
    let m = fs::symlink_metadata(path).map_err(|_| "Privat governancestatus kunne ikke åbnes")?;
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
            "Governancestatus kræver din egen private mappe0700/fil0600 uden symbolske links."
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
        return Err("Privat governancestatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-corporate-records.json");
    if !path
        .try_exists()
        .map_err(|_| "Governancestatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Governancestatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Governancestatus overskrider grænsen".into());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Governancestatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret governanceforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-corporate-records/1"
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
                        "recordId",
                        "replacementId",
                        "sha256",
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
            Some("ingest" | "link" | "enrich" | "supersede")
        )
        && ["localIdentity", "recordId", "replacementId", "sha256"]
            .iter()
            .all(|k| value[*k].is_null() || value[*k].as_str().is_some_and(|s| s.len() < 1024))
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig governancestatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-corporate-records.json"))
        .map_err(|_| "Et governanceforsøg kræver afklaring før ny bogføring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Governancestatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Governancestatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Governancestatus ændrede sig og bevares".into());
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
            .open(dir.join(format!("corporate-records-manually-resolved-{suffix}.json")))
            .map_err(|_| "Governanceafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Governanceafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-corporate-records.json"))
        .map_err(|_| "Governancestatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Governancestatus kunne ikke synkroniseres".into())
}

use base64::{Engine, engine::general_purpose::STANDARD};
use rentemester_client::CorporateRecordMutation;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const TYPES: &[&str] = &[
    "incorporation",
    "articles",
    "registration",
    "shareholder_register",
    "shareholder_agreement",
    "board_resolution",
    "general_meeting_minutes",
    "capital_or_dividend_resolution",
    "annual_report",
    "auditor_deliverable",
    "intercompany_agreement",
    "ownership_register",
    "share_transfer",
    "authority_letter",
    "bank_mandate",
    "tax_registration",
    "other",
];
pub const LINKS: &[&str] = &[
    "company",
    "party",
    "ownership",
    "disposition",
    "legacy_evidence_ref",
];
#[derive(Clone, Default)]
pub struct Selection {
    pub cursor: usize,
    pub id: Option<String>,
    pub replacement: Option<String>,
    pub party_cursor: usize,
}
#[derive(Clone, PartialEq)]
pub struct Snapshot {
    pub company: String,
    pub principal: String,
    pub ledger: String,
    pub companies: Vec<Value>,
    pub page: Value,
    pub parties: Value,
    pub record: Option<Value>,
    pub replacement: Option<Value>,
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
fn record_valid(r: &Value) -> bool {
    ["recordId", "filename", "source", "receivedAt", "uploader"]
        .iter()
        .all(|k| r[*k].as_str().is_some_and(|s| !s.is_empty()))
        && hash(&r["sha256"])
        && TYPES.contains(&r["type"].as_str().unwrap_or(""))
        && matches!(
            r["sensitivity"].as_str(),
            Some("normal" | "confidential" | "restricted")
        )
        && r["links"].is_array()
        && r["history"].is_array()
}
fn page_valid(v: &Value) -> bool {
    v["ok"] == true
        && v["rows"].is_array()
        && v["count"].is_u64()
        && (v["nextCursor"].is_null() || v["nextCursor"].is_u64())
}
fn selected(root: Option<Value>, id: &Option<String>) -> Result<Option<Value>, String> {
    match (root, id) {
        (None, None) => Ok(None),
        (Some(v), Some(id))
            if v["ok"] == true && v["record"]["recordId"] == *id && record_valid(&v["record"]) =>
        {
            Ok(Some(v["record"].clone()))
        }
        _ => Err("Valgt governance-record er ikke aktuelt tilgængelig med denne adgang".into()),
    }
}
impl Snapshot {
    pub fn parse(
        company: &str,
        selection: &Selection,
        page: Value,
        record: Option<Value>,
        replacement: Option<Value>,
        parties: Value,
        me: Value,
        journal: Value,
    ) -> Result<Self, String> {
        if !page_valid(&page)
            || page["rows"]
                .as_array()
                .is_none_or(|a| a.iter().any(|r| !record_valid(r)))
            || !page_valid(&parties)
            || me["ok"] != true
            || journal["ok"] != true
            || journal["journal"]["slug"] != company
        {
            return Err(
                "Serveren bekræftede ikke synlige governance-records, partside, bruger og ledger"
                    .into(),
            );
        }
        let companies = me["companies"]
            .as_array()
            .ok_or("Synlige virksomheder mangler")?
            .clone();
        if companies
            .iter()
            .any(|c| text(c, "slug").is_err() || text(c, "name").is_err())
            || parties["rows"].as_array().is_none_or(|a| {
                a.iter()
                    .any(|p| text(p, "partyId").is_err() || text(p, "name").is_err())
            })
        {
            return Err("Aktuelle linkmål mangler kanonisk identitet".into());
        }
        Ok(Self {
            company: company.into(),
            principal: text(&me["user"], "id")?,
            ledger: text(&journal["journal"], "ledgerUuid")?,
            companies,
            page,
            parties,
            record: selected(record, &selection.id)?,
            replacement: selected(replacement, &selection.replacement)?,
        })
    }
    pub fn link(&self, kind: &str, id: &str) -> Result<Value, String> {
        if !LINKS.contains(&kind) || id.trim().is_empty() {
            return Err("Vælg linktype og konkret kildebelagt reference eksplicit".into());
        }
        if kind == "company"
            && !self
                .companies
                .iter()
                .any(|c| c["slug"] == id && c["archived"] != true)
        {
            return Err("Vælg en aktuelt synlig virksomhed".into());
        }
        if kind == "party"
            && !self.parties["rows"]
                .as_array()
                .is_some_and(|a| a.iter().any(|p| p["partyId"] == id))
        {
            return Err("Vælg en faktisk part på den aktuelt synlige side".into());
        }
        Ok(json!({"type":kind,"id":id}))
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Ingest,
    Link,
    Enrich,
    Supersede,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ingest => "ingest",
            Self::Link => "link",
            Self::Enrich => "enrich",
            Self::Supersede => "supersede",
        }
    }
}
#[derive(Default)]
pub struct Form<'a> {
    pub kind: &'a str,
    pub source: &'a str,
    pub received: &'a str,
    pub uploader: &'a str,
    pub sensitivity: &'a str,
    pub date: &'a str,
    pub retention: &'a str,
    pub assertion: &'a str,
    pub reason: &'a str,
    pub links: &'a [Value],
}
#[derive(Clone)]
pub struct Review {
    pub action: Action,
    pub snapshot: Snapshot,
    pub body: Arc<Vec<u8>>,
    pub display: Value,
    pub sha256: Option<String>,
}
impl Review {
    pub fn prepare(
        data: &Snapshot,
        action: Action,
        form: Form<'_>,
        file: Option<&crate::documents::Source>,
    ) -> Result<Self, String> {
        let mut body = match action {
            Action::Ingest => {
                let file = file.ok_or("Indlæs faktisk lokal original først")?;
                if file.bytes.is_empty() || file.bytes.len() > MAX_BYTES {
                    return Err("Governance-original må ikke være tom og skal være højst16MiB".into());
                }
                if !TYPES.contains(&form.kind)
                    || !["normal", "confidential", "restricted"].contains(&form.sensitivity)
                    || [form.source, form.received, form.uploader]
                        .iter()
                        .any(|s| s.trim().is_empty())
                {
                    return Err("Angiv eksplicit type, kilde, modtagelsestid, uploader og følsomhed fra faktisk kontekst".into());
                }
                for link in form.links {
                    data.link(
                        link["type"].as_str().unwrap_or(""),
                        link["id"].as_str().unwrap_or(""),
                    )?;
                }
                json!({"type":form.kind,"filename":file.filename,"bytesBase64":STANDARD.encode(file.bytes.as_ref()),"source":form.source,"receivedAt":form.received,"uploader":form.uploader,"sensitivity":form.sensitivity,"recordDate":if form.date.is_empty(){Value::Null}else{json!(form.date)},"retentionUntil":if form.retention.is_empty(){Value::Null}else{json!(form.retention)},"links":form.links,"confirm":true})
            }
            Action::Link => {
                if form.links.len() != 1 {
                    return Err("Vælg præcis ét gennemgået link til denne handling".into());
                }
                let link = &form.links[0];
                let mut body = data.link(
                    link["type"].as_str().unwrap_or(""),
                    link["id"].as_str().unwrap_or(""),
                )?;
                body["confirm"] = json!(true);
                body
            }
            Action::Enrich => {
                if form.assertion.trim().is_empty() {
                    return Err(
                        "Angiv konkret kildebelagt assertion; originalmetadata overskrives ikke"
                            .into(),
                    );
                }
                json!({"assertion":form.assertion,"confirm":true})
            }
            Action::Supersede => {
                let replacement = data
                    .replacement
                    .as_ref()
                    .ok_or("Vælg og inspicér en faktisk tilgængelig erstatnings-record")?;
                if form.reason.trim().is_empty() {
                    return Err("Angiv den faktiske begrundelse for supersession".into());
                }
                json!({"replacementRecordId":replacement["recordId"],"reason":form.reason,"confirm":true})
            }
        };
        let sha256 = if action == Action::Ingest {
            Some(file.ok_or("Original mangler")?.hash.clone())
        } else {
            Some(text(
                data.record
                    .as_ref()
                    .ok_or("Vælg og inspicér record først")?,
                "sha256",
            )?)
        };
        let bytes = serde_json::to_vec(&body).map_err(|_| "Anmodningen kunne ikke fastholdes")?;
        let max = if action == Action::Ingest {
            rentemester_client::MAX_CORPORATE_INGEST_BODY
        } else {
            rentemester_client::MAX_CORPORATE_BODY
        };
        if bytes.len() > max {
            return Err("Anmodningen overskrider native transportgrænsen".into());
        }
        if let Some(o) = body.as_object_mut() {
            o.remove("bytesBase64");
        }
        if let Some(file) = file.filter(|_| action == Action::Ingest) {
            body["byteCount"] = json!(file.bytes.len());
            body["sha256"] = json!(file.hash);
        }
        Ok(Self {
            action,
            snapshot: data.clone(),
            body: Arc::new(bytes),
            display: body,
            sha256,
        })
    }
    pub fn current(&self, data: &Snapshot) -> bool {
        self.snapshot == *data
    }
    pub fn sdk(&self) -> Result<CorporateRecordMutation, String> {
        if self.action == Action::Ingest {
            return Ok(CorporateRecordMutation::Ingest);
        }
        let id = text(
            self.snapshot.record.as_ref().ok_or("Record mangler")?,
            "recordId",
        )?;
        Ok(match self.action {
            Action::Link => CorporateRecordMutation::Link(id),
            Action::Enrich => CorporateRecordMutation::Enrich(id),
            Action::Supersede => CorporateRecordMutation::Supersede(id),
            _ => return Err("Ukendt handling".into()),
        })
    }
    pub fn marker(&self, origin: &str, local: Option<&str>) -> Value {
        json!({"format":"rentemester-desktop-corporate-records/1","origin":origin,"company":self.snapshot.company,"principal":self.snapshot.principal,"ledger":self.snapshot.ledger,"localIdentity":local,"operation":self.action.name(),"recordId":if self.action==Action::Ingest{Value::Null}else{self.snapshot.record.as_ref().map(|r|r["recordId"].clone()).unwrap_or(Value::Null)},"replacementId":if self.action==Action::Supersede{self.snapshot.replacement.as_ref().map(|r|r["recordId"].clone()).unwrap_or(Value::Null)}else{Value::Null},"sha256":self.sha256})
    }
    pub fn receipt(&self, response: &Value) -> bool {
        if response["ok"] != true {
            return false;
        }
        if matches!(self.action, Action::Enrich | Action::Supersede) {
            return hash(&response["payloadHash"]);
        }
        let r = &response["record"];
        if !record_valid(r) || r["sha256"].as_str() != self.sha256.as_deref() {
            return false;
        }
        if self.action == Action::Ingest {
            ["type", "filename", "source", "uploader", "sensitivity"]
                .iter()
                .all(|k| r[*k] == self.display[*k])
        } else {
            self.immutable(r)
                && r["links"].as_array().is_some_and(|a| {
                    a.iter()
                        .any(|l| l["type"] == self.display["type"] && l["id"] == self.display["id"])
                })
        }
    }
    fn immutable(&self, r: &Value) -> bool {
        self.snapshot.record.as_ref().is_some_and(|old| {
            [
                "recordId",
                "sha256",
                "filename",
                "type",
                "source",
                "receivedAt",
                "recordDate",
                "retentionUntil",
                "uploader",
                "sensitivity",
            ]
            .iter()
            .all(|k| r[*k] == old[*k])
        })
    }
    pub fn readback(&self, response: &Value, data: &Snapshot) -> bool {
        let Some(record) = &data.record else {
            return false;
        };
        if matches!(self.action, Action::Ingest | Action::Link) {
            return response["record"] == *record;
        }
        let event = if self.action == Action::Enrich {
            "enriched"
        } else {
            "superseded"
        };
        self.immutable(record)
            && record["history"].as_array().is_some_and(|h| {
                h.iter().any(|e| {
                    e["event_type"] == event && e["payload_hash"] == response["payloadHash"]
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
pub fn original(value: Value, record: &Value) -> Result<Vec<u8>, String> {
    if value["ok"] != true || !hash(&value["sha256"]) || value["sha256"] != record["sha256"] {
        return Err("Originalsvarets SHA-256 matcher ikke den aktuelle kanoniske record".into());
    }
    let encoded = value["bytesBase64"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 22_369_624)
        .ok_or("Originalens base64 mangler eller overstiger16MiB")?;
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "Originalsvarets base64 er ugyldig")?;
    if bytes.is_empty()
        || bytes.len() > MAX_BYTES
        || format!("{:x}", Sha256::digest(&bytes))
            != value["sha256"].as_str().ok_or("Hash mangler")?
    {
        return Err("Originalbytes matcher ikke kanonisk SHA-256/størrelse; intet gemt".into());
    }
    Ok(bytes)
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
    fn record(id: &str) -> Value {
        json!({"recordId":id,"sha256":"a".repeat(64),"filename":"source.txt","type":"articles","source":"actual source","receivedAt":"2026-01-03T10:00:00.000Z","recordDate":null,"retentionUntil":null,"uploader":"source custodian","sensitivity":"confidential","links":[{"type":"company","id":"synthetic"}],"history":[{"event_type":"ingested","payload_hash":"b".repeat(64),"actor":"user:owner","created_at":"2026-01-03T10:00:00.000Z"}]})
    }
    fn snapshot() -> Snapshot {
        Snapshot::parse("synthetic",&Selection{id:Some("record-old".into()),replacement:Some("record-new".into()),..Selection::default()},json!({"ok":true,"rows":[record("record-old"),record("record-new")],"count":2,"nextCursor":25}),Some(json!({"ok":true,"record":record("record-old")})),Some(json!({"ok":true,"record":record("record-new")})),json!({"ok":true,"rows":[{"partyId":"party-current","name":"Synthetic supplier","roles":[]}],"count":1,"nextCursor":null}),json!({"ok":true,"user":{"id":"owner"},"companies":[{"slug":"synthetic","name":"Synthetic company"},{"slug":"archived","name":"Synthetic archived","archived":true}]}),json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"ledger-synthetic"}})).unwrap()
    }
    fn form<'a>(links: &'a [Value]) -> Form<'a> {
        Form {
            kind: "articles",
            source: "actual source",
            received: "2026-01-03T10:00:00+00:00",
            uploader: "source custodian",
            sensitivity: "confidential",
            date: "2026-01-02",
            retention: "",
            assertion: "source-backed observation",
            reason: "actual reviewed replacement reason",
            links,
        }
    }
    fn file() -> crate::documents::Source {
        let bytes = b"synthetic governance original".to_vec();
        crate::documents::Source {
            filename: "source.txt".into(),
            mime: "text/plain",
            hash: format!("{:x}", Sha256::digest(&bytes)),
            bytes: Arc::new(bytes),
        }
    }
    #[test]
    fn frozen_ingest_preserves_exact_source_and_bytes_without_invented_title_id_or_key() {
        let data = snapshot();
        let links = vec![data.link("party", "party-current").unwrap()];
        let file = file();
        let review = Review::prepare(&data, Action::Ingest, form(&links), Some(&file)).unwrap();
        let frozen = review.clone();
        let body: Value = serde_json::from_slice(&review.body).unwrap();
        assert_eq!(body["bytesBase64"], STANDARD.encode(file.bytes.as_ref()));
        assert_eq!(body["filename"], "source.txt");
        assert_eq!(body["links"], json!(links));
        assert_eq!(body["sensitivity"], "confidential");
        assert_eq!(body["receivedAt"], "2026-01-03T10:00:00+00:00");
        assert_eq!(body["recordDate"], "2026-01-02");
        assert!(body["retentionUntil"].is_null());
        assert_eq!(review.body, frozen.body);
        for absent in ["title", "metadata", "recordId", "idempotencyKey"] {
            assert!(body.get(absent).is_none());
        }
        assert!(review.display.get("bytesBase64").is_none());
        let marker = review.marker("https://synthetic.invalid", None);
        assert!(valid_record(&marker));
        assert!(marker["recordId"].is_null());
        for absent in [
            "filename",
            "source",
            "uploader",
            "bytesBase64",
            "assertion",
            "reason",
        ] {
            assert!(marker.get(absent).is_none());
        }
        let mut forbidden = marker;
        forbidden["source"] = json!("private source");
        assert!(!valid_record(&forbidden));
    }
    #[test]
    fn explicit_type_sensitivity_source_and_strict_sixteen_mib_bound() {
        let data = snapshot();
        let file = file();
        let mut input = form(&[]);
        input.kind = "";
        assert!(Review::prepare(&data, Action::Ingest, input, Some(&file)).is_err());
        let mut input = form(&[]);
        input.sensitivity = "";
        assert!(Review::prepare(&data, Action::Ingest, input, Some(&file)).is_err());
        let mut input = form(&[]);
        input.uploader = "";
        assert!(Review::prepare(&data, Action::Ingest, input, Some(&file)).is_err());
        let mut large = file;
        large.bytes = Arc::new(vec![0; MAX_BYTES + 1]);
        assert!(Review::prepare(&data, Action::Ingest, form(&[]), Some(&large)).is_err());
        assert!(Review::prepare(&data, Action::Ingest, form(&[]), None).is_err());
    }
    #[test]
    fn actual_link_choices_and_single_reviewed_link_body() {
        let data = snapshot();
        for (kind, id) in [
            ("company", "hidden"),
            ("company", "archived"),
            ("party", "hidden-party"),
            ("invented", "id"),
            ("ownership", ""),
        ] {
            assert!(data.link(kind, id).is_err());
        }
        for kind in ["ownership", "disposition", "legacy_evidence_ref"] {
            assert_eq!(
                data.link(kind, "source-reference").unwrap(),
                json!({"type":kind,"id":"source-reference"})
            );
        }
        let links = vec![data.link("party", "party-current").unwrap()];
        let review = Review::prepare(&data, Action::Link, form(&links), None).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&review.body).unwrap(),
            json!({"type":"party","id":"party-current","confirm":true})
        );
        assert!(
            matches!(review.sdk().unwrap(),CorporateRecordMutation::Link(id)if id=="record-old")
        );
        assert!(Review::prepare(&data, Action::Link, form(&[]), None).is_err());
        assert!(
            Review::prepare(
                &data,
                Action::Link,
                form(&[links[0].clone(), links[0].clone()]),
                None
            )
            .is_err()
        );
    }
    #[test]
    fn frozen_scope_guards_changed_original_replacement_party_and_principal() {
        let data = snapshot();
        let review = Review::prepare(&data, Action::Supersede, form(&[]), None).unwrap();
        assert!(review.current(&data));
        for what in [
            "original",
            "replacement",
            "parties",
            "companies",
            "principal",
            "ledger",
        ] {
            let mut next = data.clone();
            match what {
                "original" => next.record.as_mut().unwrap()["sha256"] = json!("c".repeat(64)),
                "replacement" => next.replacement = None,
                "parties" => next.parties["rows"] = json!([]),
                "companies" => next.companies.clear(),
                "principal" => next.principal = "other".into(),
                _ => next.ledger = "other".into(),
            };
            assert!(!review.current(&next));
        }
        let local = review.marker("http://127.0.0.1:1", Some("trusted-profile"));
        assert!(scope(
            &local,
            "http://127.0.0.1:2",
            &data,
            Some("trusted-profile")
        ));
        assert!(!scope(
            &local,
            "http://127.0.0.1:2",
            &data,
            Some("different-profile")
        ));
        let remote = review.marker("https://one.invalid", None);
        assert!(!scope(&remote, "https://two.invalid", &data, None));
        let mut no_replacement = data;
        no_replacement.replacement = None;
        assert!(Review::prepare(&no_replacement, Action::Supersede, form(&[]), None).is_err());
    }
    #[test]
    fn enrich_and_supersede_use_real_hash_receipts_with_immutable_readback() {
        let data = snapshot();
        for (action, event) in [
            (Action::Enrich, "enriched"),
            (Action::Supersede, "superseded"),
        ] {
            let review = Review::prepare(&data, action, form(&[]), None).unwrap();
            let receipt = json!({"ok":true,"payloadHash":"d".repeat(64)});
            assert!(review.receipt(&receipt));
            assert!(!review.receipt(&json!({"ok":true})));
            assert!(!review.readback(&receipt, &data));
            let mut next = data.clone();
            next.record.as_mut().unwrap()["history"]
                .as_array_mut()
                .unwrap()
                .push(
                    json!({"event_type":event,"payload_hash":"d".repeat(64),"actor":"user:owner"}),
                );
            assert!(review.readback(&receipt, &next));
            next.record.as_mut().unwrap()["filename"] = json!("silently-replaced.txt");
            assert!(!review.readback(&receipt, &next));
            let body: Value = serde_json::from_slice(&review.body).unwrap();
            if action == Action::Supersede {
                assert_eq!(
                    body,
                    json!({"replacementRecordId":"record-new","reason":"actual reviewed replacement reason","confirm":true})
                );
            } else {
                assert_eq!(
                    body,
                    json!({"assertion":"source-backed observation","confirm":true})
                );
            }
        }
    }
    #[test]
    fn original_json_bytes_must_match_both_returned_and_canonical_sha256() {
        let file = file();
        let record = json!({"sha256":file.hash});
        let value = json!({"ok":true,"bytesBase64":STANDARD.encode(file.bytes.as_ref()),"sha256":file.hash});
        assert_eq!(original(value.clone(), &record).unwrap(), *file.bytes);
        for key in ["ok", "bytesBase64", "sha256"] {
            let mut wrong = value.clone();
            wrong[key] = json!("wrong");
            assert!(original(wrong, &record).is_err());
        }
        assert!(original(value, &json!({"sha256":"b".repeat(64)})).is_err());
        assert!(
            original(
                json!({"ok":true,"bytesBase64":"","sha256":file.hash}),
                &record
            )
            .is_err()
        );
    }
    fn dir() -> PathBuf {
        let mut random = [0u8; 16];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random).unwrap();
        let suffix = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("native-corporate-{suffix}"));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        dir
    }
    #[test]
    fn verified_original_save_is_create_new_private_and_never_overwrites_or_follows_links() {
        let dir = dir();
        let path = dir.join("chosen-original.txt");
        let bytes = b"synthetic original";
        crate::backup_state::save_bytes(bytes, &path).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
        assert!(crate::backup_state::save_bytes(b"overwrite", &path).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let symlink = dir.join("link.txt");
        std::os::unix::fs::symlink(&path, &symlink).unwrap();
        assert!(crate::backup_state::save_bytes(b"overwrite", &symlink).is_err());
        assert!(crate::backup_state::save_bytes(bytes, &dir.join("../traversal.txt")).is_err());
        assert!(crate::backup_state::save_bytes(bytes, Path::new("relative.txt")).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unknown_marker_blocks_new_attempt_without_persisting_private_payload() {
        let dir = dir();
        let record = Review::prepare(&snapshot(), Action::Enrich, form(&[]), None)
            .unwrap()
            .marker("https://synthetic.invalid", None);
        begin_at(&dir, &record).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(record.clone()));
        assert!(begin_at(&dir, &record).is_err());
        let mut wrong = record.clone();
        wrong["recordId"] = json!("other");
        assert!(finish_at(&dir, &wrong, false).is_err());
        assert_eq!(load_at(&dir).unwrap(), Some(record.clone()));
        finish_at(&dir, &record, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
