//! One private unresolved knowledge attempt; no payload persistence or replay.
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
        fs::symlink_metadata(path).map_err(|_| "Privat leverandørvidensstatus kunne ikke åbnes")?;
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
            "Leverandørvidensstatus kræver din egen private mappe0700/fil0600 uden symbolske links."
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
        return Err("Privat leverandørvidensstatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-knowledge.json");
    if !path
        .try_exists()
        .map_err(|_| "Leverandørvidensstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Leverandørvidensstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Leverandørvidensstatus overskrider grænsen".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Leverandørvidensstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret leverandørvidensforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-knowledge/1"
        && value.as_object().is_some_and(|m| {
            m.len() == 8
                && m.keys().all(|k| {
                    [
                        "format",
                        "origin",
                        "company",
                        "principal",
                        "ledger",
                        "localIdentity",
                        "operation",
                        "assertionId",
                    ]
                    .contains(&k.as_str())
                })
        })
        && ["origin", "company", "principal", "ledger", "operation"]
            .iter()
            .all(|k| {
                value[*k]
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s.len() < 1024)
            })
        && matches!(
            value["operation"].as_str(),
            Some("propose" | "approve" | "reject" | "supersede")
        )
        && (value["localIdentity"].is_null() || value["localIdentity"].is_string())
        && (value["assertionId"].is_null() || value["assertionId"].is_string())
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig leverandørvidensstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-knowledge.json"))
        .map_err(|_| "Et leverandørvidensforsøg kræver afklaring før ny bogføring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Leverandørvidensstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Leverandørvidensstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Leverandørvidensstatus ændrede sig og bevares".into());
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
            .open(dir.join(format!("knowledge-manually-resolved-{suffix}.json")))
            .map_err(|_| "Vidensafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Vidensafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-knowledge.json"))
        .map_err(|_| "Leverandørvidensstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Leverandørvidensstatus kunne ikke synkroniseres".into())
}

use rentemester_client::KnowledgeMutation;
use serde_json::json;
pub const PREDICATES: &[&str] = &[
    "business_description",
    "operating_status",
    "products_services",
    "revenue_models",
    "markets",
    "customer_types",
    "industry_tags",
    "operational_systems",
    "document_sources",
    "reporting_dimensions",
    "responsible_party",
    "canonical_reference",
];
pub const SOURCES: &[&str] = &[
    "user",
    "registry",
    "party",
    "corporate_record",
    "external_snapshot",
    "canonical_setting",
];
pub const CERTAINTIES: &[&str] = &["confirmed", "probable", "uncertain"];
#[derive(Clone)]
pub struct Snapshot {
    pub context: Value,
    pub history: Vec<Value>,
    pub principal: String,
    pub ledger: String,
}
impl Snapshot {
    pub fn parse(company: &str, value: Value, me: Value, journal: Value) -> Result<Self, String> {
        if value["ok"] != true
            || value["context"]["companySlug"] != company
            || me["ok"] != true
            || journal["ok"] != true
            || journal["journal"]["slug"] != company
            || !value["context"]["assertions"].is_array()
            || !value["context"]["conflicts"].is_array()
            || !value["context"]["safeForProductBehavior"].is_boolean()
        {
            return Err("Serveren bekræftede ikke virksomhed, kontekst og identitet".into());
        }
        let history = value["history"]
            .as_array()
            .cloned()
            .ok_or("Kanonisk videnshistorik mangler")?;
        for row in &history {
            if row["companySlug"] != company
                || row["assertionId"].as_str().is_none()
                || !row["history"].is_array()
                || row["source"].as_object().is_none()
                || row["valueHash"].as_str().is_none()
            {
                return Err("Videnshistorikken mangler kilde, hash eller hændelser".into());
            }
        }
        Ok(Self {
            context: value["context"].clone(),
            history,
            principal: required(&me["user"], "id")?,
            ledger: required(&journal["journal"], "ledgerUuid")?,
        })
    }
    pub fn find(&self, id: &str) -> Option<&Value> {
        self.history.iter().find(|r| r["assertionId"] == id)
    }
}
fn required(v: &Value, k: &str) -> Result<String, String> {
    v[k].as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Serveren mangler {k}"))
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Propose,
    Approve,
    Reject,
    Supersede,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Propose => "propose",
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Supersede => "supersede",
        }
    }
    pub fn sdk(self) -> KnowledgeMutation {
        match self {
            Self::Propose => KnowledgeMutation::Propose,
            Self::Approve | Self::Reject => KnowledgeMutation::Review,
            Self::Supersede => KnowledgeMutation::Supersede,
        }
    }
}
#[derive(Default)]
pub struct Form<'a> {
    pub predicate: &'a str,
    pub value: &'a str,
    pub source: &'a str,
    pub reference: &'a str,
    pub from: &'a str,
    pub to: &'a str,
    pub certainty: &'a str,
    pub reason: &'a str,
}
#[derive(Clone)]
pub struct Review {
    pub action: Action,
    pub target: Option<String>,
    pub source: Snapshot,
    pub body: Vec<u8>,
}
impl Review {
    pub fn prepare(
        snapshot: &Snapshot,
        action: Action,
        target: Option<&str>,
        form: Form<'_>,
    ) -> Result<Self, String> {
        let old = if action != Action::Propose {
            Some(
                snapshot
                    .find(target.ok_or("Vælg en faktisk assertion i historikken")?)
                    .ok_or("Assertion findes ikke i aktuel virksomhed")?,
            )
        } else {
            None
        };
        let body = match action {
            Action::Propose | Action::Supersede => {
                if action == Action::Supersede && old.is_none_or(|r| r["reviewState"] != "approved")
                {
                    return Err(
                        "Kun godkendt viden kan erstattes; serveren afgør den aktuelle tilstand"
                            .into(),
                    );
                }
                if !PREDICATES.contains(&form.predicate)
                    || !SOURCES.contains(&form.source)
                    || !CERTAINTIES.contains(&form.certainty)
                    || form.value.trim().is_empty()
                    || form.reference.trim().is_empty()
                    || form.from.trim().is_empty()
                {
                    return Err("Vælg emne, kildeart og sikkerhed og angiv værdi, kildehenvisning og gyldig-fra eksplicit. Serveren validerer datoer og kilder.".into());
                }
                let value:Value=serde_json::from_str(form.value).map_err(|error|format!("Værdien er ikke gyldig JSON: {error}. Tekst skal stå i dobbelte anførselstegn; intet er sendt."))?;
                let proposal = json!({"predicate":form.predicate,"value":value,"source":{"kind":form.source,"ref":form.reference},"validFrom":form.from,"validToExclusive":if form.to.is_empty(){Value::Null}else{json!(form.to)},"certainty":form.certainty});
                if action == Action::Supersede {
                    json!({"assertionId":target,"replacement":proposal,"confirm":true})
                } else {
                    let mut p = proposal;
                    p["confirm"] = json!(true);
                    p
                }
            }
            Action::Approve | Action::Reject => {
                if old.is_none_or(|r| r["reviewState"] != "proposed") {
                    return Err("Vælg et aktuelt forslag, der endnu ikke er reviewet".into());
                }
                json!({"assertionId":target,"decision":if action==Action::Approve{"approved"}else{"rejected"},"reason":if form.reason.is_empty(){Value::Null}else{json!(form.reason)},"confirm":true})
            }
        };
        let body = serde_json::to_vec(&body).map_err(|_| "Anmodningen kunne ikke fastholdes")?;
        if body.len() > rentemester_client::MAX_KNOWLEDGE_BODY {
            return Err("Forslaget overskrider serverens transportgrænse".into());
        }
        Ok(Self {
            action,
            target: if action == Action::Propose {
                None
            } else {
                target.map(str::to_owned)
            },
            source: snapshot.clone(),
            body,
        })
    }
    pub fn current(&self, snapshot: &Snapshot) -> bool {
        self.source.principal == snapshot.principal
            && self.source.ledger == snapshot.ledger
            && self.source.history == snapshot.history
    }
    pub fn marker(&self, origin: &str, company: &str, local: Option<&str>) -> Value {
        json!({"format":"rentemester-desktop-knowledge/1","origin":origin,"company":company,"principal":self.source.principal,"ledger":self.source.ledger,"localIdentity":local,"operation":self.action.name(),"assertionId":self.target})
    }
    pub fn receipt(&self, value: &Value) -> Option<Vec<Value>> {
        if value["ok"] != true {
            return None;
        }
        let body: Value = serde_json::from_slice(&self.body).ok()?;
        let matching = |r: &Value, p: &Value| {
            r["companySlug"] == self.source.context["companySlug"]
                && r["assertionId"].as_str().is_some_and(|s| !s.is_empty())
                && r["validFrom"].is_string()
                && (r["validToExclusive"].is_null() || r["validToExclusive"].is_string())
                && r["valueHash"].is_string()
                && r["history"].is_array()
                && matches!(
                    r["reviewState"].as_str(),
                    Some("proposed" | "approved" | "rejected" | "superseded")
                )
                && ["predicate", "value", "source", "certainty"]
                    .iter()
                    .all(|k| r[*k] == p[*k])
        };
        match self.action {
            Action::Propose => {
                matching(&value["assertion"], &body).then(|| vec![value["assertion"].clone()])
            }
            Action::Approve | Action::Reject => {
                let r = &value["assertion"];
                let old = self.source.find(self.target.as_deref()?)?;
                (r["assertionId"] == old["assertionId"]
                    && [
                        "companySlug",
                        "valueHash",
                        "predicate",
                        "value",
                        "source",
                        "validFrom",
                        "validToExclusive",
                        "certainty",
                    ]
                    .iter()
                    .all(|k| r[*k] == old[*k])
                    && r["reviewState"] == body["decision"])
                    .then(|| vec![r.clone()])
            }
            Action::Supersede => {
                let old = &value["superseded"];
                let new = &value["replacement"];
                (old["assertionId"].as_str() == self.target.as_deref()
                    && old["reviewState"] == "superseded"
                    && self.source.find(self.target.as_deref()?).is_some_and(|r| {
                        [
                            "companySlug",
                            "valueHash",
                            "predicate",
                            "value",
                            "source",
                            "validFrom",
                            "validToExclusive",
                            "certainty",
                        ]
                        .iter()
                        .all(|k| r[*k] == old[*k])
                    })
                    && new["assertionId"] != old["assertionId"]
                    && matching(new, &body["replacement"]))
                .then(|| vec![old.clone(), new.clone()])
            }
        }
    }
}
pub fn readback(receipt: &[Value], snapshot: &Snapshot) -> bool {
    !receipt.is_empty()
        && receipt
            .iter()
            .all(|r| r["assertionId"].as_str().and_then(|id| snapshot.find(id)) == Some(r))
}
pub fn scope(
    record: &Value,
    origin: &str,
    company: &str,
    snapshot: &Snapshot,
    local: Option<&str>,
) -> bool {
    record["company"] == company
        && record["principal"] == snapshot.principal
        && record["ledger"] == snapshot.ledger
        && record["localIdentity"].as_str() == local
        && (record["origin"] == origin || local.is_some())
}
/// Render every returned provenance/history field; never infer effective state locally.
pub fn lines(v: &Value, prefix: &str) -> Vec<String> {
    match v {
        Value::Object(m) => m
            .iter()
            .flat_map(|(k, v)| lines(v, &format!("{prefix}.{k}")))
            .collect(),
        Value::Array(a) => {
            if a.is_empty() {
                vec![format!("{prefix}: []")]
            } else {
                a.iter()
                    .enumerate()
                    .flat_map(|(i, v)| lines(v, &format!("{prefix}[{i}]")))
                    .collect()
            }
        }
        Value::String(s) => vec![format!("{prefix}: {s}")],
        _ => vec![format!("{prefix}: {v}")],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(id: &str, status: &str) -> Value {
        json!({"assertionId":id,"companySlug":"synthetic","predicate":"business_description","value":"Synthetic services","valueHash":"a".repeat(64),"source":{"kind":"user","ref":"synthetic owner declaration"},"validFrom":"2026-01-01T00:00:00.000Z","validToExclusive":null,"certainty":"confirmed","reviewState":status,"history":[{"event_type":status,"principal_kind":"user","principal_id":"owner","actor":"user:owner","reason":null,"created_at":"2026-01-02T00:00:00.000Z"}]})
    }
    fn snapshot() -> Snapshot {
        Snapshot::parse("synthetic",json!({"ok":true,"context":{"companySlug":"synthetic","asOf":"2026-01-03T00:00:00.000Z","assertions":[],"conflicts":[],"safeForProductBehavior":true},"history":[row("assertion-old","proposed")]}),json!({"ok":true,"user":{"id":"owner"}}),json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"immutable-ledger"}})).unwrap()
    }
    fn form() -> Form<'static> {
        Form {
            predicate: "business_description",
            value: "\"Synthetic services\"",
            source: "user",
            reference: "synthetic owner declaration",
            from: "2026-01-01",
            to: "",
            certainty: "confirmed",
            reason: "reviewed source",
        }
    }
    #[test]
    fn proposal_is_explicit_exact_and_has_no_generated_id_or_domain_inference() {
        let data = snapshot();
        let review = Review::prepare(&data, Action::Propose, None, form()).unwrap();
        let body: Value = serde_json::from_slice(&review.body).unwrap();
        assert_eq!(body["value"], "Synthetic services");
        assert_eq!(body["validFrom"], "2026-01-01");
        assert_eq!(body["validToExclusive"], Value::Null);
        assert_eq!(body["confirm"], true);
        assert!(body.get("assertionId").is_none());
        assert!(body.get("idempotencyKey").is_none());
        assert!(
            Review::prepare(
                &data,
                Action::Propose,
                None,
                Form {
                    certainty: "",
                    ..form()
                }
            )
            .is_err()
        );
        assert!(
            Review::prepare(&data, Action::Propose, None, Form { from: "", ..form() }).is_err()
        );
    }
    #[test]
    fn structured_json_values_are_frozen_for_proposal_and_replacement_and_syntax_errors_refused() {
        let mut data = snapshot();
        data.history[0] = row("assertion-old", "approved");
        let source = r#"{"systems":["ERP","CRM"],"active":true,"branches":2,"optional":null}"#;
        for action in [Action::Propose, Action::Supersede] {
            let review = Review::prepare(
                &data,
                action,
                Some("assertion-old"),
                Form {
                    value: source,
                    ..form()
                },
            )
            .unwrap();
            let frozen: Value = serde_json::from_slice(&review.body).unwrap();
            let value = if action == Action::Supersede {
                &frozen["replacement"]["value"]
            } else {
                &frozen["value"]
            };
            assert_eq!(value, &serde_json::from_str::<Value>(source).unwrap());
            assert!(review.current(&data));
        }
        for invalid in ["bare unquoted text", "{", "[1,]", ""] {
            let error = Review::prepare(
                &data,
                Action::Propose,
                None,
                Form {
                    value: invalid,
                    ..form()
                },
            );
            assert!(error.is_err());
        }
    }
    #[test]
    fn reviewed_source_is_guarded_but_server_current_timestamp_may_advance() {
        let data = snapshot();
        let review =
            Review::prepare(&data, Action::Approve, Some("assertion-old"), form()).unwrap();
        let mut fresh = data.clone();
        fresh.context["asOf"] = json!("2026-01-04T00:00:00.000Z");
        assert!(review.current(&fresh));
        for field in ["principal", "ledger", "history"] {
            let mut changed = data.clone();
            match field {
                "principal" => changed.principal = "different".into(),
                "ledger" => changed.ledger = "different".into(),
                _ => changed.history[0]["source"]["ref"] = json!("different"),
            };
            assert!(!review.current(&changed));
        }
        assert!(Review::prepare(&data, Action::Approve, Some("unknown"), form()).is_err());
    }
    #[test]
    fn approved_rejected_superseded_receipts_require_current_target_and_canonical_readback() {
        let data = snapshot();
        for (action, status) in [(Action::Approve, "approved"), (Action::Reject, "rejected")] {
            let review = Review::prepare(&data, action, Some("assertion-old"), form()).unwrap();
            let r = row("assertion-old", status);
            let receipt = review.receipt(&json!({"ok":true,"assertion":r})).unwrap();
            let mut next = data.clone();
            next.history = receipt.clone();
            assert!(readback(&receipt, &next));
            assert!(!readback(&receipt, &data));
            let mut bad = receipt[0].clone();
            bad["source"]["ref"] = json!("changed");
            assert!(
                review
                    .receipt(&json!({"ok":true,"assertion":bad}))
                    .is_none()
            );
        }
        let mut approved = data.clone();
        approved.history[0] = row("assertion-old", "approved");
        assert!(
            Review::prepare(&approved, Action::Approve, Some("assertion-old"), form()).is_err()
        );
        let supersede =
            Review::prepare(&approved, Action::Supersede, Some("assertion-old"), form()).unwrap();
        let old = row("assertion-old", "superseded");
        let replacement = row("assertion-new", "proposed");
        let receipt = supersede
            .receipt(&json!({"ok":true,"superseded":old,"replacement":replacement}))
            .unwrap();
        assert_eq!(receipt[1]["reviewState"], "proposed");
        let mut next = approved;
        next.history = receipt.clone();
        assert!(readback(&receipt, &next));
        assert!(Review::prepare(&data, Action::Supersede, Some("assertion-old"), form()).is_err());
        assert!(supersede.receipt(&json!({"ok":true,"superseded":row("assertion-old","superseded"),"replacement":row("assertion-old","proposed")})).is_none());
    }
    #[test]
    fn minimal_marker_has_no_value_source_dates_reason_or_replay_and_binds_scope() {
        let data = snapshot();
        let review = Review::prepare(&data, Action::Propose, None, form()).unwrap();
        let record = review.marker("https://synthetic.invalid", "synthetic", None);
        assert!(valid_record(&record));
        for field in ["value", "source", "reason", "body", "validFrom"] {
            let mut bad = record.clone();
            bad[field] = json!("never persist");
            assert!(!valid_record(&bad));
        }
        assert!(scope(
            &record,
            "https://synthetic.invalid",
            "synthetic",
            &data,
            None
        ));
        assert!(!scope(
            &record,
            "https://other.invalid",
            "synthetic",
            &data,
            None
        ));
        let local = review.marker("http://127.0.0.1:10001", "synthetic", Some("owned-profile"));
        assert!(scope(
            &local,
            "http://127.0.0.1:10002",
            "synthetic",
            &data,
            Some("owned-profile")
        ));
        assert!(!scope(
            &local,
            "http://127.0.0.1:10002",
            "synthetic",
            &data,
            Some("other-profile")
        ));
        let mut changed = data;
        changed.ledger = "other".into();
        assert!(!scope(
            &local,
            "http://127.0.0.1:10002",
            "synthetic",
            &changed,
            Some("owned-profile")
        ));
    }
    #[test]
    fn pending_survives_restart_and_blocks_overwrite_until_explicit_manual_resolution() {
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm-native-knowledge-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        let record = Review::prepare(&snapshot(), Action::Propose, None, form())
            .unwrap()
            .marker("https://synthetic.invalid", "synthetic", None);
        begin_at(&dir, &record).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(record.clone()));
        assert!(begin_at(&dir, &record).is_err());
        assert!(finish_at(&dir, &json!({"format":"wrong"}), false).is_err());
        finish_at(&dir, &record, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        assert!(fs::read_dir(&dir).unwrap().any(|p| {
            p.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("knowledge-manually-resolved-")
        }));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn provenance_history_and_conflicts_are_not_silently_removed_or_inferred() {
        let data = snapshot();
        let all = lines(&data.history[0], "assertion");
        assert!(all.iter().any(|s| s.contains("source.ref")));
        assert!(all.iter().any(|s| s.contains("principal_id")));
        let mut value = json!({"ok":true,"context":{"companySlug":"wrong","assertions":[],"conflicts":[],"safeForProductBehavior":true},"history":[]});
        assert!(
            Snapshot::parse(
                "synthetic",
                value.clone(),
                json!({"ok":true,"user":{"id":"owner"}}),
                json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"immutable"}})
            )
            .is_err()
        );
        value["context"]["companySlug"] = json!("synthetic");
        value["history"] = json!([{"assertionId":"incomplete"}]);
        assert!(
            Snapshot::parse(
                "synthetic",
                value,
                json!({"ok":true,"user":{"id":"owner"}}),
                json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"immutable"}})
            )
            .is_err()
        );
    }
}
