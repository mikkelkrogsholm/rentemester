//! One private unresolved workspace-inbox attempt; no payload persistence or replay.
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
    let m = fs::symlink_metadata(path).map_err(|_| "Privat indbakkestatus kunne ikke åbnes")?;
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
            "Indbakkestatus kræver din egen private mappe0700/fil0600 uden symbolske links.".into(),
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
        return Err("Privat indbakkestatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-workspace-inbox.json");
    if !path
        .try_exists()
        .map_err(|_| "Indbakkestatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Indbakkestatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Indbakkestatus overskrider grænsen".into());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Indbakkestatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret indbakkeforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-workspace-inbox/1"
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
                        "sourceId",
                        "targetCompany",
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
            Some("ingest" | "assign" | "complete")
        )
        && ["localIdentity", "sourceId", "targetCompany", "sha256"]
            .iter()
            .all(|k| value[*k].is_null() || value[*k].as_str().is_some_and(|s| s.len() < 1024))
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig indbakkestatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-workspace-inbox.json"))
        .map_err(|_| "Et indbakkeforsøg kræver afklaring før ny bogføring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Indbakkestatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Indbakkestatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Indbakkestatus ændrede sig og bevares".into());
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
            .open(dir.join(format!("workspace-inbox-manually-resolved-{suffix}.json")))
            .map_err(|_| "Indbakkeafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Indbakkeafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-workspace-inbox.json"))
        .map_err(|_| "Indbakkestatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Indbakkestatus kunne ikke synkroniseres".into())
}

use base64::{Engine, engine::general_purpose::STANDARD};
use rentemester_client::WorkspaceInboxMutation;
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::json;
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct Selection {
    pub cursor: usize,
    pub id: Option<String>,
}
#[derive(Clone, PartialEq)]
pub struct Snapshot {
    pub company: String,
    pub principal: String,
    pub ledger: String,
    pub targets: Vec<Value>,
    pub page: Value,
    pub source: Option<Value>,
}
fn text(v: &Value, key: &str) -> Result<String, String> {
    v[key]
        .as_str()
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Serveren mangler {key}"))
}
fn hash(v: &Value) -> bool {
    v.as_str().is_some_and(|v| {
        v.len() == 64
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn source_valid(source: &Value, company: &str) -> bool {
    source["visibilityAnchorSlug"] == company
        && [
            "sourceId",
            "filename",
            "mimeType",
            "receivedAt",
            "createdBy",
            "createdAt",
        ]
        .iter()
        .all(|k| source[*k].as_str().is_some_and(|s| !s.is_empty()))
        && hash(&source["sha256"])
        && source["transport"] == "upload"
        && source["metadata"].is_object()
        && ["assignments", "handoffs", "history"]
            .iter()
            .all(|k| source[*k].is_array())
}
impl Snapshot {
    pub fn parse(
        company: &str,
        selection: &Selection,
        page: Value,
        source: Option<Value>,
        me: Value,
        journal: Value,
    ) -> Result<Self, String> {
        if page["ok"] != true
            || me["ok"] != true
            || journal["ok"] != true
            || journal["journal"]["slug"] != company
            || !page["count"].is_u64()
            || !(page["nextCursor"].is_null() || page["nextCursor"].is_u64())
            || page["rows"]
                .as_array()
                .is_none_or(|a| a.iter().any(|s| !source_valid(s, company)))
        {
            return Err("Serveren bekræftede ikke synlig indbakke, anker og bruger".into());
        }
        let source = match (source, &selection.id) {
            (None, None) => None,
            (Some(root), Some(id))
                if root["ok"] == true
                    && root["source"]["sourceId"] == *id
                    && source_valid(&root["source"], company) =>
            {
                Some(root["source"].clone())
            }
            _ => return Err("Valgt originalkilde matcher ikke ankervirksomheden".into()),
        };
        let targets = me["companies"]
            .as_array()
            .ok_or("Serveren mangler synlige virksomheder")?
            .clone();
        if targets
            .iter()
            .any(|t| text(t, "slug").is_err() || text(t, "name").is_err())
        {
            return Err("Serveren mangler synlig målidentitet".into());
        }
        Ok(Self {
            company: company.into(),
            principal: text(&me["user"], "id")?,
            ledger: text(&journal["journal"], "ledgerUuid")?,
            targets,
            page,
            source,
        })
    }
    pub fn target(&self, target: &str) -> bool {
        self.targets
            .iter()
            .any(|t| t["slug"] == target && t["archived"] != true)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Ingest,
    Assign,
    Complete,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ingest => "ingest",
            Self::Assign => "assign",
            Self::Complete => "complete",
        }
    }
}
#[derive(Clone)]
pub struct Review {
    pub action: Action,
    pub snapshot: Snapshot,
    pub body: Arc<Vec<u8>>,
    pub display: Value,
    pub target: Option<String>,
    pub sha256: String,
}
impl Review {
    pub fn ingest(
        data: &Snapshot,
        file: &crate::documents::Source,
        kind: &str,
        fields: &[(String, String)],
        received: &str,
        transport_identity: &str,
    ) -> Result<Self, String> {
        if !["purchase_sale", "cash_register_receipt"].contains(&kind) {
            return Err("Vælg købs-/salgsbilag eller kassekvittering eksplicit. Interne bilag oprettes i virksomhedens bilagsflow.".into());
        }
        if received.trim().is_empty() {
            return Err("Angiv faktisk modtagelsestid med eksplicit tidszone".into());
        }
        let metadata = crate::documents::metadata(kind, fields)?;
        let mut random = [0u8; 24];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| "Uploadintention kunne ikke fastholdes")?;
        let key = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let display = json!({"filename":file.filename,"mimeType":file.mime,"byteCount":file.bytes.len(),"sha256":file.hash,"transport":"upload","transportIdentity":if transport_identity.trim().is_empty(){Value::Null}else{json!(transport_identity)},"receivedAt":received,"metadata":metadata});
        let mut body = display.clone();
        body.as_object_mut()
            .ok_or("Uploadobjekt mangler")?
            .remove("sha256");
        body.as_object_mut()
            .ok_or("Uploadobjekt mangler")?
            .remove("byteCount");
        body["confirm"] = json!(true);
        body["idempotencyKey"] = json!(key);
        body["bytesBase64"] = json!(STANDARD.encode(file.bytes.as_ref()));
        let body = serde_json::to_vec(&body).map_err(|_| "Upload kunne ikke fastholdes")?;
        if body.len() > rentemester_client::MAX_WORKSPACE_INBOX_BODY {
            return Err("Upload overstiger35MiB transportgrænsen".into());
        }
        Ok(Self {
            action: Action::Ingest,
            snapshot: data.clone(),
            body: Arc::new(body),
            display,
            target: None,
            sha256: file.hash.clone(),
        })
    }
    pub fn handoff(data: &Snapshot, action: Action, target: &str) -> Result<Self, String> {
        let source = data
            .source
            .as_ref()
            .ok_or("Vælg og inspicér en faktisk originalkilde")?;
        if action == Action::Ingest || !data.target(target) {
            return Err(
                "Vælg en konkret synlig målvirksomhed; ankervirksomheden vælges aldrig automatisk"
                    .into(),
            );
        }
        let assignments = source["assignments"]
            .as_array()
            .ok_or("Tildelinger mangler")?;
        if action == Action::Assign
            && assignments
                .iter()
                .any(|a| matches!(a["state"].as_str(), Some("approved" | "completed")))
        {
            return Err("Kilden har allerede en godkendt/fuldført destination. Inspicér den eksisterende tildeling.".into());
        }
        if action == Action::Complete
            && !assignments
                .iter()
                .any(|a| a["companySlug"] == target && a["state"] == "approved")
        {
            return Err("Fuldendelse kræver den præcise allerede godkendte målvirksomhed. Fuldførte bilag genlæses uden ny skrivning.".into());
        }
        let display = json!({"companySlug":target,"confirm":true});
        Ok(Self {
            action,
            snapshot: data.clone(),
            body: Arc::new(
                serde_json::to_vec(&display).map_err(|_| "Tildeling kunne ikke fastholdes")?,
            ),
            display,
            target: Some(target.into()),
            sha256: text(source, "sha256")?,
        })
    }
    pub fn current(&self, data: &Snapshot) -> bool {
        self.snapshot == *data
    }
    pub fn sdk(&self) -> Result<WorkspaceInboxMutation, String> {
        if self.action == Action::Ingest {
            return Ok(WorkspaceInboxMutation::Ingest);
        }
        let id = text(
            self.snapshot
                .source
                .as_ref()
                .ok_or("Originalkilde mangler")?,
            "sourceId",
        )?;
        Ok(match self.action {
            Action::Assign => WorkspaceInboxMutation::Assign(id),
            Action::Complete => WorkspaceInboxMutation::Complete(id),
            _ => return Err("Ukendt handling".into()),
        })
    }
    pub fn marker(&self, origin: &str, local: Option<&str>) -> Value {
        json!({"format":"rentemester-desktop-workspace-inbox/1","origin":origin,"company":self.snapshot.company,"principal":self.snapshot.principal,"ledger":self.snapshot.ledger,"localIdentity":local,"operation":self.action.name(),"sourceId":if self.action==Action::Ingest {Value::Null}else{self.snapshot.source.as_ref().map(|s|s["sourceId"].clone()).unwrap_or(Value::Null)},"targetCompany":self.target,"sha256":self.sha256})
    }
    pub fn receipt(&self, value: &Value) -> Option<Value> {
        let source = &value["source"];
        if value["ok"] != true
            || !source_valid(source, &self.snapshot.company)
            || source["sha256"] != self.sha256
        {
            return None;
        }
        if self.action == Action::Ingest {
            if [
                "filename",
                "mimeType",
                "metadata",
                "transport",
                "transportIdentity",
            ]
            .iter()
            .any(|k| source[*k] != self.display[*k])
            {
                return None;
            }
        } else {
            let old = self.snapshot.source.as_ref()?;
            if [
                "sourceId",
                "filename",
                "mimeType",
                "metadata",
                "receivedAt",
                "createdBy",
                "createdAt",
            ]
            .iter()
            .any(|k| source[*k] != old[*k])
            {
                return None;
            }
            let required = if self.action == Action::Assign {
                "approved"
            } else {
                "completed"
            };
            if source["assignments"].as_array()?.iter().all(|a| {
                a["companySlug"].as_str() != self.target.as_deref()
                    || a["state"] != required
                    || (self.action == Action::Complete
                        && a["documentId"].as_i64().is_none_or(|id| id <= 0))
            }) {
                return None;
            }
        }
        Some(source.clone())
    }
}
pub fn scope(record: &Value, origin: &str, data: &Snapshot, local: Option<&str>) -> bool {
    record["company"] == data.company
        && record["principal"] == data.principal
        && record["ledger"] == data.ledger
        && record["localIdentity"].as_str() == local
        && (record["origin"] == origin || local.is_some())
}
/// Exact target identity/hash readback after handoff. A hash alone never proves an unknown mutation.
pub fn completed_document(
    source: &Value,
    target: &str,
    documents: &Value,
) -> Result<(i64, String, String), String> {
    let assignment = source["assignments"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|a| a["companySlug"] == target && a["state"] == "completed")
        })
        .ok_or("Målet har ikke et bekræftet fuldført bilag")?;
    let id = assignment["documentId"]
        .as_i64()
        .filter(|id| *id > 0)
        .ok_or("Kanonisk bilags-id mangler")?;
    if documents["ok"] != true || documents["documents"]["slug"] != target {
        return Err("Serveren bekræftede ikke målvirksomhedens bilag".into());
    }
    let document = documents["documents"]["documents"]
        .as_array()
        .and_then(|a| a.iter().find(|d| d["id"] == id))
        .ok_or("Det fuldførte bilag mangler i målvirksomheden")?;
    if document["sha256Hash"] != source["sha256"]
        || document["mimeType"] != source["mimeType"]
        || document["documentNo"] != assignment["documentNo"]
    {
        return Err(
            "Målbilagets nummer/hash/type matcher ikke den permanente originalkilde".into(),
        );
    }
    Ok((id, text(source, "mimeType")?, text(source, "sha256")?))
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
    fn source() -> Value {
        json!({"sourceId":"synthetic-source","visibilityAnchorSlug":"anchor","sha256":"a".repeat(64),"filename":"original.txt","mimeType":"text/plain","transport":"upload","transportIdentity":null,"receivedAt":"2026-01-03T10:00:00.000Z","createdBy":"user:owner","createdAt":"2026-01-03T10:01:00.000Z","metadata":{"documentType":"purchase_sale"},"assignments":[],"handoffs":[],"history":[{"event_type":"ingested","payload_hash":"b".repeat(64),"actor":"user:owner","payload":{"transport":"upload"}}],"exception":null})
    }
    fn snapshot(selected: bool) -> Snapshot {
        Snapshot::parse("anchor",&Selection{cursor:0,id:selected.then(||"synthetic-source".into())},
            json!({"ok":true,"rows":[source()],"count":1,"nextCursor":25}),
            selected.then(||json!({"ok":true,"source":source()})),
            json!({"ok":true,"user":{"id":"user:owner"},"companies":[{"slug":"anchor","name":"Synthetic anchor"},{"slug":"target","name":"Synthetic target"},{"slug":"archived","name":"Synthetic archived","archived":true}]}),
            json!({"ok":true,"journal":{"slug":"anchor","ledgerUuid":"synthetic-ledger"}})).unwrap()
    }
    fn approved() -> Snapshot {
        let mut data = snapshot(true);
        data.source.as_mut().unwrap()["assignments"] = json!([{"companySlug":"target","state":"approved","documentId":null,"documentNo":null}]);
        data
    }
    fn fields() -> Vec<(String, String)> {
        [
            ("source", "owner-provided-original"),
            ("issueDate", "2026-01-02"),
            ("deliveryDescription", "Synthetic supplies"),
            ("currency", "DKK"),
            ("amountIncVat", "125.50"),
            ("vatAmount", "25.10"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }
    fn file() -> crate::documents::Source {
        crate::documents::Source {
            filename: "original.txt".into(),
            mime: "text/plain",
            bytes: Arc::new(b"synthetic original".to_vec()),
            hash: "a".repeat(64),
        }
    }
    #[test]
    fn upload_freezes_one_key_exact_bytes_and_explicit_provenance_without_destination() {
        let data = snapshot(false);
        let mut file = file();
        let review = Review::ingest(
            &data,
            &file,
            "purchase_sale",
            &fields(),
            "2026-01-03T10:00:00+00:00",
            "source mailbox",
        )
        .unwrap();
        let body: Value = serde_json::from_slice(&review.body).unwrap();
        let clone = review.clone();
        assert_eq!(clone.body, review.body);
        assert_eq!(body["idempotencyKey"].as_str().unwrap().len(), 48);
        assert_eq!(body["bytesBase64"], STANDARD.encode(file.bytes.as_ref()));
        assert_eq!(body["transport"], "upload");
        assert_eq!(body["transportIdentity"], "source mailbox");
        assert_eq!(body["receivedAt"], "2026-01-03T10:00:00+00:00");
        assert!(body.get("companySlug").is_none());
        assert!(body.get("candidates").is_none());
        assert!(body.get("sourceId").is_none());
        assert!(review.display.get("bytesBase64").is_none());
        assert!(review.display.get("idempotencyKey").is_none());
        file.bytes = Arc::new(b"changed local file".to_vec());
        assert_eq!(clone.body, review.body);
        let marker = review.marker("https://synthetic.invalid", None);
        assert!(valid_record(&marker));
        assert!(marker.get("metadata").is_none());
        assert!(marker.get("idempotencyKey").is_none());
        assert!(marker.get("filename").is_none());
        assert!(marker["sourceId"].is_null());
        let mut unsafe_marker = marker;
        unsafe_marker["metadata"] = body["metadata"].clone();
        assert!(!valid_record(&unsafe_marker));
    }
    #[test]
    fn no_guessed_classification_time_or_source_facts() {
        let data = snapshot(false);
        let file = file();
        for kind in ["", "internal_voucher", "guessed"] {
            assert!(
                Review::ingest(&data, &file, kind, &fields(), "2026-01-03T10:00:00Z", "").is_err()
            );
        }
        assert!(Review::ingest(&data, &file, "purchase_sale", &fields(), "", "").is_err());
        assert!(
            Review::ingest(
                &data,
                &file,
                "purchase_sale",
                &[],
                "2026-01-03T10:00:00Z",
                ""
            )
            .is_err()
        );
    }
    #[test]
    fn assignment_requires_visible_chosen_target_and_completion_is_separate() {
        let data = snapshot(true);
        for target in ["", "hidden", "archived"] {
            assert!(Review::handoff(&data, Action::Assign, target).is_err());
        }
        assert!(Review::handoff(&data, Action::Complete, "target").is_err());
        let review = Review::handoff(&data, Action::Assign, "target").unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&review.body).unwrap(),
            json!({"companySlug":"target","confirm":true})
        );
        assert!(
            matches!(review.sdk().unwrap(),WorkspaceInboxMutation::Assign(id) if id=="synthetic-source")
        );
        let approved = approved();
        assert!(Review::handoff(&approved, Action::Assign, "anchor").is_err());
        assert!(Review::handoff(&approved, Action::Complete, "anchor").is_err());
        assert!(Review::handoff(&approved, Action::Complete, "target").is_ok());
    }
    #[test]
    fn frozen_scope_rejects_changed_source_target_visibility_principal_and_ledger() {
        let data = snapshot(true);
        let review = Review::handoff(&data, Action::Assign, "target").unwrap();
        assert!(review.current(&data));
        for key in ["source", "target", "principal", "ledger", "page"] {
            let mut next = data.clone();
            match key {
                "source" => next.source.as_mut().unwrap()["history"] = json!([]),
                "target" => next.targets.clear(),
                "principal" => next.principal = "other".into(),
                "ledger" => next.ledger = "other".into(),
                _ => next.page["nextCursor"] = json!(50),
            };
            assert!(!review.current(&next));
        }
        let marker = review.marker("http://127.0.0.1:1", Some("trusted-local-profile"));
        assert!(scope(
            &marker,
            "http://127.0.0.1:2",
            &data,
            Some("trusted-local-profile")
        ));
        assert!(!scope(
            &marker,
            "http://127.0.0.1:2",
            &data,
            Some("different-profile")
        ));
        let remote = review.marker("https://original.invalid", None);
        assert!(!scope(&remote, "https://other.invalid", &data, None));
    }
    #[test]
    fn completed_receipt_and_canonical_document_require_exact_identity_and_original() {
        let data = approved();
        let review = Review::handoff(&data, Action::Complete, "target").unwrap();
        let mut completed = data.source.clone().unwrap();
        completed["assignments"] = json!([{"companySlug":"target","state":"completed","documentId":17,"documentNo":"D17"}]);
        assert!(
            review
                .receipt(&json!({"ok":true,"source":completed}))
                .is_some()
        );
        let documents = json!({"ok":true,"documents":{"slug":"target","documents":[{"id":17,"documentNo":"D17","sha256Hash":"a".repeat(64),"mimeType":"text/plain"}]}});
        assert_eq!(
            completed_document(&completed, "target", &documents).unwrap(),
            (17, "text/plain".into(), "a".repeat(64))
        );
        for key in ["id", "documentNo", "sha256Hash", "mimeType"] {
            let mut wrong = documents.clone();
            wrong["documents"]["documents"][0][key] = json!("wrong");
            assert!(completed_document(&completed, "target", &wrong).is_err());
        }
        assert!(completed_document(&completed, "anchor", &documents).is_err());
        let mut wrong = completed.clone();
        wrong["sha256"] = json!("c".repeat(64));
        assert!(review.receipt(&json!({"ok":true,"source":wrong})).is_none());
        let mut done = data;
        done.source = Some(completed);
        assert!(Review::handoff(&done, Action::Complete, "target").is_err());
    }
    #[test]
    fn anchor_cannot_disclose_another_anchors_source() {
        let data = snapshot(true);
        let mut wrong = data.source.clone().unwrap();
        wrong["visibilityAnchorSlug"] = json!("hidden");
        assert!(!source_valid(&wrong, "anchor"));
        assert!(
            Snapshot::parse(
                "anchor",
                &Selection::default(),
                data.page,
                None,
                json!({"ok":true,"user":{"id":"user"},"companies":[]}),
                json!({"ok":true,"journal":{"slug":"other","ledgerUuid":"ledger"}})
            )
            .is_err()
        );
    }
    #[test]
    fn private_unknown_marker_blocks_new_attempt_and_retains_no_replay() {
        let mut random = [0u8; 16];
        SystemRandom::new().fill(&mut random).unwrap();
        let name = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("native-inbox-state-{name}"));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let record = Review::handoff(&snapshot(true), Action::Assign, "target")
            .unwrap()
            .marker("https://synthetic.invalid", None);
        begin_at(&dir, &record).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(record.clone()));
        assert!(begin_at(&dir, &record).is_err());
        let mut wrong = record.clone();
        wrong["sourceId"] = json!("other");
        assert!(finish_at(&dir, &wrong, false).is_err());
        assert_eq!(load_at(&dir).unwrap(), Some(record.clone()));
        finish_at(&dir, &record, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        assert!(fs::read_dir(&dir).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("workspace-inbox-manually-resolved-")
        }));
        fs::remove_dir_all(dir).unwrap();
    }
}
