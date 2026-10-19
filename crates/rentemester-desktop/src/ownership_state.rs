//! One private unresolved ownership attempt; no payload persistence or replay.
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
    let m = fs::symlink_metadata(path).map_err(|_| "Privat ejerforholdsstatus kunne ikke åbnes")?;
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
            "Ejerforholdsstatus kræver din egen private mappe0700/fil0600 uden symbolske links."
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
        return Err("Privat ejerforholdsstatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-ownership.json");
    if !path
        .try_exists()
        .map_err(|_| "Ejerforholdsstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Ejerforholdsstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Ejerforholdsstatus overskrider grænsen".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Ejerforholdsstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret ejerforholdsforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-ownership/1"
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
                        "snapshotId",
                        "snapshotHash",
                        "diffHash",
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
            Some("propose" | "review" | "apply")
        )
        && ["localIdentity", "snapshotId", "snapshotHash", "diffHash"]
            .iter()
            .all(|k| value[*k].is_null() || value[*k].as_str().is_some_and(|s| s.len() < 1024))
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig ejerforholdsstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-ownership.json"))
        .map_err(|_| "Et ejerforholdsforsøg kræver afklaring før ny bogføring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Ejerforholdsstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Ejerforholdsstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Ejerforholdsstatus ændrede sig og bevares".into());
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
            .open(dir.join(format!("ownership-manually-resolved-{suffix}.json")))
            .map_err(|_| "Ejerforholdsafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Ejerforholdsafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-ownership.json"))
        .map_err(|_| "Ejerforholdsstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Ejerforholdsstatus kunne ikke synkroniseres".into())
}

use rentemester_client::OwnershipMutation;
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
    pub companies: Value,
    pub workspace_role: Value,
    pub graph: Value,
    pub history: Value,
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
fn snapshot_valid(s: &Value) -> bool {
    ["snapshotId", "source", "observedAt", "actor"]
        .iter()
        .all(|k| s[*k].as_str().is_some_and(|s| !s.is_empty()))
        && hash(&s["snapshotHash"])
        && hash(&s["diffHash"])
        && matches!(
            s["state"].as_str(),
            Some("proposed" | "approved" | "rejected" | "applied")
        )
        && text(&s["principal"], "kind").is_ok()
        && text(&s["principal"], "id").is_ok()
        && s["history"].is_array()
        && s["diff"]["version"] == 2
        && ["add", "unchanged", "conflicts", "endings"]
            .iter()
            .all(|k| s["diff"][*k].is_array())
        && s["diff"]["note"].is_string()
}
impl Snapshot {
    pub fn parse(
        company: &str,
        selection: &Selection,
        graph: Value,
        history: Value,
        selected: Option<Value>,
        me: Value,
        journal: Value,
    ) -> Result<Self, String> {
        if graph["ok"] != true
            || graph["asOf"] != selection.as_of
            || !graph["facts"].is_array()
            || !graph["partial"].is_boolean()
            || !graph["consolidation"].is_object()
            || history["ok"] != true
            || history["history"]
                .as_array()
                .is_none_or(|a| a.iter().any(|s| !snapshot_valid(s)))
            || me["ok"] != true
            || !me["companies"].is_array()
            || journal["ok"] != true
            || journal["journal"]["slug"] != company
        {
            return Err(
                "Serveren bekræftede ikke pr-dato, synlig graf/historik, virksomhed og bruger"
                    .into(),
            );
        }
        let selected = match (selected, &selection.id) {
            (None, None) => None,
            (Some(root), Some(id)) if root["ok"] == true => {
                let rows = root["history"]
                    .as_array()
                    .ok_or("Serveren mangler valgt snapshots historik")?;
                match rows.as_slice() {
                    [] => None,
                    [s] if s["snapshotId"] == *id && snapshot_valid(s) => Some(s.clone()),
                    _ => return Err("Valgt snapshot matcher ikke serverens kildeidentitet".into()),
                }
            }
            _ => return Err("Valgt snapshot mangler".into()),
        };
        Ok(Self {
            company: company.into(),
            principal: text(&me["user"], "id")?,
            ledger: text(&journal["journal"], "ledgerUuid")?,
            companies: me["companies"].clone(),
            workspace_role: me["workspaceRole"].clone(),
            graph,
            history,
            selected,
        })
    }
    pub fn self_review(&self) -> bool {
        self.selected.as_ref().is_some_and(|s| {
            s["principal"]["kind"] == "user" && s["principal"]["id"] == self.principal
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Propose,
    Review,
    Apply,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Propose => "propose",
            Self::Review => "review",
            Self::Apply => "apply",
        }
    }
    pub fn sdk(self) -> OwnershipMutation {
        match self {
            Self::Propose => OwnershipMutation::Propose,
            Self::Review => OwnershipMutation::Review,
            Self::Apply => OwnershipMutation::Apply,
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
    pub fn prepare(
        data: &Snapshot,
        action: Action,
        source: &str,
        observed: &str,
        facts: &str,
        decision: &str,
    ) -> Result<Self, String> {
        let body = match action {
            Action::Propose => {
                if source.trim().is_empty() || observed.trim().is_empty() {
                    return Err("Angiv faktisk kildehenvisning og observationstid eksplicit".into());
                }
                let facts: Value = serde_json::from_str(facts)
                    .map_err(|e| format!("Fakta skal være et eksplicit typed JSON-array: {e}"))?;
                if facts.as_array().is_none_or(|a| {
                    a.is_empty() || a.len() > 2048 || a.iter().any(|f| !f.is_object())
                }) {
                    return Err("Angiv1–2048 komplette kildebelagte fakta som JSON-objekter; ingen facts eller ejerandele udledes".into());
                }
                json!({"source":source,"observedAt":observed,"facts":facts,"confirm":true})
            }
            Action::Review => {
                let selected = data
                    .selected
                    .as_ref()
                    .ok_or("Vælg et aktuelt synligt snapshot og gennemgå dets server-diff først")?;
                if data.self_review() {
                    return Err("Forslagsstilleren må ikke reviewe eget forslag. En ANDEN autentificeret reviewer med aktuel adgang skal åbne dette snapshot; der skiftes eller oprettes ingen session automatisk.".into());
                }
                if selected["state"] != "proposed" || !["approved", "rejected"].contains(&decision)
                {
                    return Err(
                        "Vælg godkend/afvis eksplicit for et aktuelt foreslået snapshot".into(),
                    );
                }
                json!({"snapshotId":selected["snapshotId"],"decision":decision,"confirm":true})
            }
            Action::Apply => {
                let selected = data
                    .selected
                    .as_ref()
                    .ok_or("Vælg aktuelt synligt reviewet snapshot")?;
                if selected["state"] != "approved" {
                    return Err("Apply kræver præcis det aktuelt godkendte snapshot; ingen afviste/foreslåede snapshots anvendes".into());
                }
                json!({"snapshotId":selected["snapshotId"],"snapshotHash":selected["snapshotHash"],"diffHash":selected["diffHash"],"confirm":true})
            }
        };
        let body_bytes =
            serde_json::to_vec(&body).map_err(|_| "Anmodningen kunne ikke fastholdes")?;
        let max = if action == Action::Propose {
            rentemester_client::MAX_OWNERSHIP_PROPOSE_BODY
        } else {
            rentemester_client::MAX_OWNERSHIP_BODY
        };
        if body_bytes.len() > max {
            return Err("Anmodningen overskrider native transportgrænsen".into());
        }
        Ok(Self {
            action,
            snapshot: data.clone(),
            body: Arc::new(body_bytes),
            display: body,
        })
    }
    pub fn current(&self, data: &Snapshot) -> bool {
        self.snapshot == *data
    }
    pub fn marker(&self, origin: &str, local: Option<&str>) -> Value {
        let source = if self.action == Action::Propose {
            None
        } else {
            self.snapshot.selected.as_ref()
        };
        json!({"format":"rentemester-desktop-ownership/1","origin":origin,"company":self.snapshot.company,"principal":self.snapshot.principal,"ledger":self.snapshot.ledger,"localIdentity":local,"operation":self.action.name(),"snapshotId":source.map(|s|s["snapshotId"].clone()),"snapshotHash":source.map(|s|s["snapshotHash"].clone()),"diffHash":source.map(|s|s["diffHash"].clone())})
    }
    pub fn receipt(&self, value: &Value) -> Option<Value> {
        let s = &value["snapshot"];
        if value["ok"] != true || !snapshot_valid(s) {
            return None;
        }
        match self.action {
            Action::Propose => {
                if s["state"] != "proposed"
                    || s["principal"]["kind"] != "user"
                    || s["principal"]["id"] != self.snapshot.principal
                {
                    return None;
                }
            }
            Action::Review | Action::Apply => {
                let old = self.snapshot.selected.as_ref()?;
                if [
                    "snapshotId",
                    "snapshotHash",
                    "diffHash",
                    "diff",
                    "source",
                    "observedAt",
                    "actor",
                    "principal",
                ]
                .iter()
                .any(|k| s[*k] != old[*k])
                {
                    return None;
                }
                if self.action == Action::Review {
                    if s["state"] != self.display["decision"]
                        || s["history"].as_array()?.iter().all(|h| {
                            h["event_type"] != self.display["decision"]
                                || h["principal_kind"] != "user"
                                || h["principal_id"] != self.snapshot.principal
                        })
                    {
                        return None;
                    }
                } else if s["state"] != "applied"
                    || !matches!(value["status"].as_str(), Some("applied" | "unchanged"))
                {
                    return None;
                }
            }
        }
        Some(s.clone())
    }
}
pub fn scope(record: &Value, origin: &str, data: &Snapshot, local: Option<&str>) -> bool {
    record["company"] == data.company
        && record["principal"] == data.principal
        && record["ledger"] == data.ledger
        && record["localIdentity"].as_str() == local
        && (record["origin"] == origin || local.is_some())
}
pub fn readback(receipt: &Value, data: &Snapshot) -> bool {
    data.selected.as_ref() == Some(receipt)
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
    fn fact() -> Value {
        json!({"owner":{"kind":"party","partyId":"party-synthetic"},"ownedCompanySlug":"synthetic","validFrom":"2026-01-01","economicIntervalBasisPoints":{"min":1000,"max":2000},"controlType":"agreement","jurisdiction":"DK","evidenceRefs":["source-reviewed-governance-record"]})
    }
    fn proposal() -> Value {
        json!({"snapshotId":"ownership-synthetic","source":"actual reviewed source","observedAt":"2026-01-03T00:00:00.000Z","snapshotHash":"a".repeat(64),"diffHash":"b".repeat(64),"diff":{"version":2,"add":[fact()],"unchanged":["c".repeat(64)],"conflicts":[],"endings":[{"factHash":"d".repeat(64),"effectiveToExclusive":"2026-01-01","successorFactHash":"e".repeat(64)}],"note":"Only an explicit later fact can end an approved legal fact"},"state":"proposed","actor":"user:author","principal":{"kind":"user","id":"author"},"history":[{"event_type":"proposed","actor":"user:author","principal_kind":"user","principal_id":"author","created_at":"2026-01-03T00:00:00.000Z"}]})
    }
    fn data(principal: &str, selected: Option<Value>) -> Snapshot {
        let selection = Selection {
            as_of: "2026-06-01".into(),
            id: selected
                .as_ref()
                .map(|s| s["snapshotId"].as_str().unwrap().to_owned()),
        };
        Snapshot::parse("synthetic",&selection,json!({"ok":true,"asOf":"2026-06-01","facts":[fact()],"partial":true,"consolidation":{"eligible":false,"reason":"partial visibility"}}),json!({"ok":true,"history":[proposal()]}),selected.map(|s|json!({"ok":true,"history":[s]})),json!({"ok":true,"user":{"id":principal},"workspaceRole":"owner","companies":[{"slug":"synthetic","name":"Synthetic company","role":"owner"}]}),json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"synthetic-ledger"}})).unwrap()
    }
    fn approved() -> Value {
        let mut s = proposal();
        s["state"] = json!("approved");
        s["history"].as_array_mut().unwrap().push(json!({"event_type":"approved","actor":"user:reviewer","principal_kind":"user","principal_id":"reviewer","created_at":"2026-01-04T00:00:00.000Z"}));
        s
    }
    #[test]
    fn complete_typed_facts_are_frozen_without_local_hash_diff_defaults_or_revocation() {
        let data = data("author", None);
        let raw = serde_json::to_string(&vec![fact()]).unwrap();
        let review = Review::prepare(
            &data,
            Action::Propose,
            "actual reviewed source",
            "2026-01-03",
            &raw,
            "",
        )
        .unwrap();
        let body: Value = serde_json::from_slice(&review.body).unwrap();
        assert_eq!(
            body,
            json!({"source":"actual reviewed source","observedAt":"2026-01-03","facts":[fact()],"confirm":true})
        );
        for absent in [
            "snapshotId",
            "snapshotHash",
            "diffHash",
            "diff",
            "idempotencyKey",
            "revoke",
            "actor",
            "principal",
        ] {
            assert!(body.get(absent).is_none());
        }
        assert!(body["facts"][0].get("economicBasisPoints").is_none());
        assert!(body["facts"][0].get("votingBasisPoints").is_none());
        assert!(body["facts"][0].get("validToExclusive").is_none());
        assert_eq!(review.clone().body, review.body);
        assert_eq!(review.snapshot.graph, data.graph);
        assert_eq!(data.graph["facts"], json!([fact()]));
        let mut receipt = proposal();
        receipt["principal"]["id"] = json!("author");
        assert!(
            review
                .receipt(&json!({"ok":true,"snapshot":receipt}))
                .is_some()
        );
    }
    #[test]
    fn missing_source_observed_or_complete_json_array_refuses_before_transport() {
        let data = data("author", None);
        for facts in ["", "not json", "{}", "[]", "[null]", "[1]"] {
            assert!(
                Review::prepare(&data, Action::Propose, "source", "2026-01-03", facts, "").is_err()
            );
        }
        assert!(Review::prepare(&data, Action::Propose, "", "2026-01-03", "[{}]", "").is_err());
        assert!(Review::prepare(&data, Action::Propose, "source", "", "[{}]", "").is_err());
        // Domain validity belongs to the shared SDK/server; syntax checks never invent legal facts.
        assert!(
            Review::prepare(&data, Action::Propose, "source", "2026-01-03", "[{}]", "").is_ok()
        );
    }
    #[test]
    fn independent_authenticated_reviewer_required_and_decision_is_separate() {
        let author = data("author", Some(proposal()));
        assert!(author.self_review());
        for decision in ["approved", "rejected"] {
            assert!(Review::prepare(&author, Action::Review, "", "", "", decision).is_err());
        }
        let reviewer = data("reviewer", Some(proposal()));
        assert!(!reviewer.self_review());
        for decision in ["approved", "rejected"] {
            let review = Review::prepare(&reviewer, Action::Review, "", "", "", decision).unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&review.body).unwrap(),
                json!({"snapshotId":"ownership-synthetic","decision":decision,"confirm":true})
            );
        }
        assert!(Review::prepare(&reviewer, Action::Review, "", "", "", "").is_err());
        assert!(
            Review::prepare(
                &data("reviewer", Some(approved())),
                Action::Review,
                "",
                "",
                "",
                "rejected"
            )
            .is_err()
        );
    }
    #[test]
    fn apply_uses_only_exact_canonical_approved_source_and_diff_hashes() {
        for state in ["proposed", "rejected", "applied"] {
            let mut s = proposal();
            s["state"] = json!(state);
            assert!(
                Review::prepare(&data("author", Some(s)), Action::Apply, "", "", "", "").is_err()
            );
        }
        let data = data("author", Some(approved()));
        let review = Review::prepare(&data, Action::Apply, "", "", "", "").unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&review.body).unwrap(),
            json!({"snapshotId":"ownership-synthetic","snapshotHash":"a".repeat(64),"diffHash":"b".repeat(64),"confirm":true})
        );
        let mut applied = approved();
        applied["state"] = json!("applied");
        applied["history"].as_array_mut().unwrap().push(json!({"event_type":"applied","actor":"user:author","principal_kind":"user","principal_id":"author"}));
        for status in ["applied", "unchanged"] {
            assert!(
                review
                    .receipt(&json!({"ok":true,"status":status,"snapshot":applied}))
                    .is_some()
            );
        }
        for key in [
            "snapshotId",
            "snapshotHash",
            "diffHash",
            "diff",
            "principal",
        ] {
            let mut wrong = applied.clone();
            wrong[key] = json!("changed");
            assert!(
                review
                    .receipt(&json!({"ok":true,"status":"applied","snapshot":wrong}))
                    .is_none()
            );
        }
        let mut next = data;
        next.selected = Some(applied.clone());
        assert!(readback(&applied, &next));
        next.selected.as_mut().unwrap()["history"] = json!([]);
        assert!(!readback(&applied, &next));
    }
    #[test]
    fn review_receipt_must_preserve_source_hash_diff_and_current_reviewer_provenance() {
        let data = data("reviewer", Some(proposal()));
        let review = Review::prepare(&data, Action::Review, "", "", "", "approved").unwrap();
        assert!(
            review
                .receipt(&json!({"ok":true,"snapshot":approved()}))
                .is_some()
        );
        let mut wrong = approved();
        wrong["history"][1]["principal_id"] = json!("author");
        assert!(
            review
                .receipt(&json!({"ok":true,"snapshot":wrong}))
                .is_none()
        );
        let mut wrong = approved();
        wrong["diff"]["endings"] = json!([]);
        assert!(
            review
                .receipt(&json!({"ok":true,"snapshot":wrong}))
                .is_none()
        );
        let mut wrong = approved();
        wrong["state"] = json!("rejected");
        assert!(
            review
                .receipt(&json!({"ok":true,"snapshot":wrong}))
                .is_none()
        );
    }
    #[test]
    fn current_graph_history_principal_roles_and_ledger_are_bound_to_frozen_request() {
        let data = data("author", Some(approved()));
        let review = Review::prepare(&data, Action::Apply, "", "", "", "").unwrap();
        assert!(review.current(&data));
        for what in [
            "graph",
            "history",
            "selected",
            "principal",
            "roles",
            "companies",
            "ledger",
        ] {
            let mut next = data.clone();
            match what {
                "graph" => next.graph["facts"] = json!([]),
                "history" => next.history["history"] = json!([]),
                "selected" => next.selected.as_mut().unwrap()["diffHash"] = json!("f".repeat(64)),
                "principal" => next.principal = "other".into(),
                "roles" => next.workspace_role = json!("viewer"),
                "companies" => next.companies = json!([]),
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
            Some("other-profile")
        ));
        let remote = review.marker("https://one.invalid", None);
        assert!(!scope(&remote, "https://two.invalid", &data, None));
    }
    #[test]
    fn hidden_unknown_selected_snapshot_is_unavailable_not_adopted_from_history() {
        let base = data("reviewer", None);
        let selection = Selection {
            as_of: "2026-06-01".into(),
            id: Some("hidden-or-unknown".into()),
        };
        let current = Snapshot::parse(
            "synthetic",
            &selection,
            base.graph.clone(),
            base.history.clone(),
            Some(json!({"ok":true,"history":[]})),
            json!({"ok":true,"user":{"id":"reviewer"},"companies":[]}),
            json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"synthetic-ledger"}}),
        )
        .unwrap();
        assert!(current.selected.is_none());
        assert!(Review::prepare(&current, Action::Review, "", "", "", "approved").is_err());
        assert!(Review::prepare(&current, Action::Apply, "", "", "", "").is_err());
        assert!(
            Snapshot::parse(
                "synthetic",
                &selection,
                base.graph,
                base.history,
                Some(json!({"ok":true,"history":[proposal()]})),
                json!({"ok":true,"user":{"id":"reviewer"},"companies":[]}),
                json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"synthetic-ledger"}})
            )
            .is_err()
        );
    }
    #[test]
    fn private_unknown_marker_has_no_facts_or_replay_and_cannot_replace_unresolved_attempt() {
        let mut random = [0u8; 16];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut random).unwrap();
        let suffix = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let dir = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("native-ownership-{suffix}"));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let data = data("author", None);
        let review = Review::prepare(
            &data,
            Action::Propose,
            "private source",
            "2026-01-03",
            &json!([fact()]).to_string(),
            "",
        )
        .unwrap();
        let marker = review.marker("https://synthetic.invalid", None);
        assert!(valid_record(&marker));
        assert!(marker["snapshotId"].is_null());
        for key in [
            "facts",
            "source",
            "observedAt",
            "economicBasisPoints",
            "body",
            "decision",
        ] {
            assert!(marker.get(key).is_none());
        }
        let mut unsafe_marker = marker.clone();
        unsafe_marker["facts"] = json!([fact()]);
        assert!(!valid_record(&unsafe_marker));
        begin_at(&dir, &marker).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(marker.clone()));
        assert!(begin_at(&dir, &marker).is_err());
        let mut wrong = marker.clone();
        wrong["principal"] = json!("other");
        assert!(finish_at(&dir, &wrong, false).is_err());
        assert_eq!(load_at(&dir).unwrap(), Some(marker.clone()));
        finish_at(&dir, &marker, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
