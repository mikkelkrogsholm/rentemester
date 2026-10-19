//! One private unresolved commitments attempt; no payload persistence or replay.
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
    let m = fs::symlink_metadata(path).map_err(|_| "Privat forsøgsstatus kunne ikke åbnes")?;
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
            "Leverandørforsøgsstatussstatus kræver din egen private mappe0700/fil0600 uden symbolske links."
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
        return Err("Privat forsøgsstatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-commitments.json");
    if !path
        .try_exists()
        .map_err(|_| "Leverandørforsøgsstatussstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Leverandørforsøgsstatussstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Leverandørforsøgsstatussstatus overskrider grænsen".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Leverandørforsøgsstatussstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret leverandørforsøgsstatussforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-commitments/1"
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
                        "commitmentId",
                        "payloadHash",
                        "occurrenceDate",
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
            Some("apply" | "paused" | "ended" | "superseded" | "match")
        )
        && (value["localIdentity"].is_null() || value["localIdentity"].is_string())
        && (value["commitmentId"].is_null() || value["commitmentId"].is_string())
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig forsøgsstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-commitments.json"))
        .map_err(|_| "Et leverandørforsøgsstatussforsøg kræver afklaring før ny bogføring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Leverandørforsøgsstatussstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Leverandørforsøgsstatussstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Leverandørforsøgsstatussstatus ændrede sig og bevares".into());
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
            .open(dir.join(format!("commitments-manually-resolved-{suffix}.json")))
            .map_err(|_| "Vidensafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Vidensafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-commitments.json"))
        .map_err(|_| "Leverandørforsøgsstatussstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Leverandørforsøgsstatussstatus kunne ikke synkroniseres".into())
}

use rentemester_client::CommitmentMutation;
use serde_json::json;
pub const FREQUENCIES: &[&str] = &["weekly", "monthly", "quarterly", "yearly"];
#[derive(Clone)]
pub struct Snapshot {
    pub values: Value,
    pub matches: Vec<Value>,
    pub principal: String,
    pub ledger: String,
}
fn text(v: &Value, k: &str) -> Result<String, String> {
    v[k].as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("Serveren mangler {k}"))
}
pub fn digest(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
impl Snapshot {
    pub fn parse(
        company: &str,
        value: Value,
        matches: Value,
        me: Value,
        journal: Value,
    ) -> Result<Self, String> {
        if [&value, &matches, &me, &journal]
            .iter()
            .any(|v| v["ok"] != true)
            || value["supplierCommitments"]["slug"] != company
            || journal["journal"]["slug"] != company
        {
            return Err("Serveren bekræftede ikke virksomhed, forpligtelser og identitet".into());
        }
        let v = value["supplierCommitments"].clone();
        text(&v, "startDate")?;
        for k in [
            "commitments",
            "occurrences",
            "history",
            "matches",
            "alerts",
            "vendors",
            "documents",
            "evidenceOptions",
        ] {
            if !v[k].is_array() {
                return Err(format!("Serveren mangler {k}"));
            }
        }
        let forecast = &v["forecast"];
        if forecast["ok"] != true
            || forecast["periods"].as_array().is_none_or(|p| p.len() != 13)
            || !forecast["completeness"].is_object()
            || !forecast["completeness"]["included"].is_array()
            || !forecast["completeness"]["excluded"].is_array()
            || !forecast["errors"].is_array()
        {
            return Err(
                "Serverens fulde13-ugers prognose og dækning mangler; ingen tal udledes lokalt"
                    .into(),
            );
        }
        let matches = matches["matches"]["rows"]
            .as_array()
            .cloned()
            .ok_or("Kanoniske match mangler")?;
        if v["matches"].as_array() != Some(&matches) {
            return Err("Match ændrede sig mellem serverlæsninger; genlæs før handling".into());
        }
        Ok(Self {
            values: v,
            matches,
            principal: text(&me["user"], "id")?,
            ledger: text(&journal["journal"], "ledgerUuid")?,
        })
    }
    pub fn rows(&self, k: &str) -> Vec<Value> {
        self.values[k].as_array().cloned().unwrap_or_default()
    }
    pub fn current(&self, other: &Self) -> bool {
        self.principal == other.principal
            && self.ledger == other.ledger
            && self.values == other.values
            && self.matches == other.matches
    }
}
#[derive(Default)]
pub struct Form<'a> {
    pub id: &'a str,
    pub vendor: Option<i64>,
    pub documents: &'a [i64],
    pub kind: &'a str,
    pub description: &'a str,
    pub purpose: &'a str,
    pub amount: &'a str,
    pub currency: &'a str,
    pub frequency: &'a str,
    pub next: &'a str,
    pub start: &'a str,
    pub end: &'a str,
    pub renewal: &'a str,
    pub notice: &'a str,
    pub vat: &'a str,
}
pub fn input(data: &Snapshot, form: Form<'_>) -> Result<Value, String> {
    let vendor = form.vendor.ok_or("Vælg en faktisk aktuel leverandør")?;
    if !data.rows("vendors").iter().any(|v| v["id"] == vendor)
        || form.documents.is_empty()
        || form
            .documents
            .iter()
            .any(|id| !data.rows("documents").iter().any(|d| d["id"] == *id))
    {
        return Err(
            "Vælg aktuelle leverandør- og originale bilagsreferencer fra serverlisten".into(),
        );
    }
    if [
        form.id,
        form.kind,
        form.description,
        form.purpose,
        form.currency,
        form.next,
    ]
    .iter()
    .any(|s| s.trim().is_empty())
        || !FREQUENCIES.contains(&form.frequency)
    {
        return Err(
            "Angiv reference, type, beskrivelse, formål, valuta, frekvens og næste dato eksplicit"
                .into(),
        );
    }
    let amount:serde_json::Number=form.amount.parse().map_err(|_|"Beløb kræver JSON-tal med punktum uden tusindtalsseparator; serveren validerer præcise øre")?;
    let mut v = json!({"commitmentId":form.id,"vendorPartyId":format!("vendor:{vendor}"),"type":form.kind,"description":form.description,"businessPurpose":form.purpose,"amount":amount,"currency":form.currency,"frequency":form.frequency,"nextDate":form.next,"evidenceRefs":form.documents.iter().map(|id|format!("document:{id}")).collect::<Vec<_>>()});
    for (k, value) in [
        ("startDate", form.start),
        ("endDate", form.end),
        ("renewalDate", form.renewal),
        ("noticeDate", form.notice),
        ("vatProposal", form.vat),
    ] {
        v[k] = if value.is_empty() {
            Value::Null
        } else {
            json!(value)
        };
    }
    Ok(v)
}
#[derive(Clone)]
pub enum Action {
    Apply { input: Value, plan: Value },
    Change { row: Value, status: String },
    Match { occurrence: Value, evidence: Value },
}
#[derive(Clone)]
pub struct Review {
    pub action: Action,
    pub source: Snapshot,
    pub body: Vec<u8>,
}
fn serial(v: &Value) -> Result<Vec<u8>, String> {
    serde_json::to_vec(v).map_err(|_| "Anmodningen kunne ikke fastholdes".into())
}
impl Review {
    pub fn planned(data: &Snapshot, input: Value, root: Value) -> Result<Self, String> {
        let plan = &root["plan"];
        if root["ok"] != true
            || plan["requestedInput"] != input
            || plan["commitmentId"] != input["commitmentId"]
            || !digest(&plan["payloadHash"])
            || !plan["vendorProvenance"].is_object()
            || plan["sourceEvidence"].as_array().is_none_or(Vec::is_empty)
            || !plan["schedule"].is_array()
            || !plan["warnings"].is_array()
        {
            return Err("Serverplanen mangler præcis inputbinding, hash, leverandørprovenance, originalkilder eller advarsler".into());
        }
        let body =
            serial(&json!({"commitment":input,"payloadHash":plan["payloadHash"],"confirm":true}))?;
        Ok(Self {
            action: Action::Apply {
                input,
                plan: plan.clone(),
            },
            source: data.clone(),
            body,
        })
    }
    pub fn change(
        data: &Snapshot,
        row: &Value,
        status: &str,
        reason: &str,
    ) -> Result<Self, String> {
        if !data.rows("commitments").contains(row)
            || !digest(&row["payloadHash"])
            || row["status"] == "ended"
            || !["paused", "ended", "superseded"].contains(&status)
            || reason.trim().is_empty()
        {
            return Err("Vælg faktisk ikke-afsluttet forpligtelse, aktuel hash, understøttet status og konkret begrundelse. Serveren afgør tilstanden.".into());
        }
        let body = serial(
            &json!({"commitmentId":row["commitmentId"],"action":status,"reason":reason,"expectedPayloadHash":row["payloadHash"],"confirm":true}),
        )?;
        Ok(Self {
            action: Action::Change {
                row: row.clone(),
                status: status.into(),
            },
            source: data.clone(),
            body,
        })
    }
    pub fn matching(data: &Snapshot, occurrence: &Value, evidence: &Value) -> Result<Self, String> {
        if !data.rows("occurrences").contains(occurrence)
            || !data.rows("evidenceOptions").contains(evidence)
        {
            return Err(
                "Vælg faktisk uafstemt forekomst og kanonisk evidens fra aktuelle servermuligheder"
                    .into(),
            );
        }
        let body = serial(
            &json!({"commitmentId":occurrence["commitmentId"],"occurrenceDate":occurrence["date"],"evidence":{"kind":evidence["kind"],"id":evidence["id"]},"confirm":true}),
        )?;
        Ok(Self {
            action: Action::Match {
                occurrence: occurrence.clone(),
                evidence: evidence.clone(),
            },
            source: data.clone(),
            body,
        })
    }
    pub fn name(&self) -> &str {
        match &self.action {
            Action::Apply { .. } => "apply",
            Action::Change { status, .. } => status,
            Action::Match { .. } => "match",
        }
    }
    pub fn sdk(&self) -> CommitmentMutation {
        match self.action {
            Action::Apply { .. } => CommitmentMutation::Apply,
            Action::Change { .. } => CommitmentMutation::Change,
            Action::Match { .. } => CommitmentMutation::Match,
        }
    }
    pub fn marker(&self, origin: &str, local: Option<&str>) -> Value {
        let (id, hash) = match &self.action {
            Action::Apply { plan, .. } => {
                (plan["commitmentId"].clone(), plan["payloadHash"].clone())
            }
            Action::Change { row, .. } => (row["commitmentId"].clone(), row["payloadHash"].clone()),
            Action::Match { occurrence, .. } => (
                occurrence["commitmentId"].clone(),
                occurrence["payloadHash"].clone(),
            ),
        };
        json!({"format":"rentemester-desktop-commitments/1","origin":origin,"company":self.source.values["slug"],"principal":self.source.principal,"ledger":self.source.ledger,"localIdentity":local,"operation":self.name(),"commitmentId":id,"payloadHash":hash,"occurrenceDate":match &self.action{Action::Match{occurrence,..}=>occurrence["date"].clone(),_=>Value::Null}})
    }
    pub fn receipt(&self, value: &Value) -> Option<Value> {
        if value["ok"] != true {
            return None;
        }
        let r = &value[if matches!(self.action, Action::Match { .. }) {
            "supplierCommitmentMatch"
        } else {
            "supplierCommitment"
        }];
        if !digest(&r["payloadHash"]) {
            return None;
        }
        match &self.action {
            Action::Apply { plan, .. } => {
                if r["commitmentId"] != plan["commitmentId"]
                    || r["payloadHash"] != plan["payloadHash"]
                {
                    return None;
                }
            }
            Action::Change { row, status } => {
                if r["commitmentId"] != row["commitmentId"] || r["status"] != *status {
                    return None;
                }
            }
            Action::Match { .. } => {
                if !r["variance"].is_object() {
                    return None;
                }
            }
        }
        Some(r.clone())
    }
    pub fn readback(&self, receipt: &Value, data: &Snapshot) -> bool {
        let body: Value = serde_json::from_slice(&self.body).unwrap_or(Value::Null);
        match &self.action {
            Action::Apply { plan, .. } => data.rows("history").iter().any(|r| {
                r["commitmentId"] == plan["commitmentId"]
                    && r["event"] == "approved"
                    && r["payloadHash"] == receipt["payloadHash"]
                    && r["payload"] == *plan
            }),
            Action::Change { row, status } => data.rows("history").iter().any(|r| {
                r["commitmentId"] == row["commitmentId"]
                    && r["event"] == *status
                    && r["payloadHash"] == receipt["payloadHash"]
                    && r["payload"]["previousPayloadHash"] == row["payloadHash"]
                    && r["payload"]["reason"] == body["reason"]
            }),
            Action::Match {
                occurrence,
                evidence,
            } => data.matches.iter().any(|r| {
                r["commitmentId"] == occurrence["commitmentId"]
                    && r["occurrenceDate"] == occurrence["date"]
                    && r["payloadHash"] == receipt["payloadHash"]
                    && r["expected"]["payloadHash"] == occurrence["payloadHash"]
                    && r["evidence"]["kind"] == evidence["kind"]
                    && r["evidence"]["id"] == evidence["id"]
                    && r["variance"] == receipt["variance"]
            }),
        }
    }
}
pub fn scope(record: &Value, origin: &str, data: &Snapshot, local: Option<&str>) -> bool {
    record["company"] == data.values["slug"]
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
    fn snapshot() -> Snapshot {
        let v = json!({"slug":"synthetic","startDate":"2026-01-01","vendors":[{"id":2,"name":"Synthetic supplier"}],"documents":[{"id":7,"sha256Hash":"a".repeat(64)}],"commitments":[],"occurrences":[],"history":[],"matches":[],"alerts":[],"evidenceOptions":[],"forecast":{"ok":true,"periods":vec![json!({"openingCash":null,"closingCash":null,"sources":[],"excluded":[]});13],"completeness":{"status":"incomplete","included":[],"excluded":["no independently confirmed opening balance"]},"errors":[]}});
        Snapshot::parse(
            "synthetic",
            json!({"ok":true,"supplierCommitments":v}),
            json!({"ok":true,"matches":{"rows":[]}}),
            json!({"ok":true,"user":{"id":"owner"}}),
            json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"immutable-ledger"}}),
        )
        .unwrap()
    }
    fn form() -> Form<'static> {
        Form {
            id: "synthetic-standing-plan",
            vendor: Some(2),
            documents: &[7],
            kind: "subscription",
            description: "Synthetic service",
            purpose: "Synthetic operating service",
            amount: "123.45",
            currency: "DKK",
            frequency: "monthly",
            next: "2026-01-15",
            ..Form::default()
        }
    }
    fn plan(input: &Value) -> Value {
        json!({"ok":true,"plan":{"commitmentId":input["commitmentId"],"requestedInput":input,"payloadHash":"b".repeat(64),"previousPayloadHash":null,"vendorProvenance":{"kind":"ledger_vendor","vendor":{"id":2,"name":"Synthetic supplier"}},"sourceEvidence":[{"documentId":7,"sha256":"a".repeat(64)}],"schedule":[{"date":"2026-01-15","amount":123.45,"currency":"DKK"}],"warnings":["Planning only; no payment, payable, journal or FX conversion"]}})
    }
    #[test]
    fn exact_inputs_choose_actual_sources_and_preserve_amount_dates_without_arithmetic() {
        let data = snapshot();
        let v = input(&data, form()).unwrap();
        assert_eq!(v["amount"].to_string(), "123.45");
        assert_eq!(v["vendorPartyId"], "vendor:2");
        assert_eq!(v["evidenceRefs"], json!(["document:7"]));
        assert_eq!(v["startDate"], Value::Null);
        assert_eq!(v["nextDate"], "2026-01-15");
        assert!(
            input(
                &data,
                Form {
                    vendor: Some(99),
                    ..form()
                }
            )
            .is_err()
        );
        assert!(
            input(
                &data,
                Form {
                    documents: &[99],
                    ..form()
                }
            )
            .is_err()
        );
        assert!(
            input(
                &data,
                Form {
                    frequency: "",
                    ..form()
                }
            )
            .is_err()
        );
        assert!(
            input(
                &data,
                Form {
                    amount: "123,45",
                    ..form()
                }
            )
            .is_err()
        );
    }
    #[test]
    fn only_exact_server_plan_hash_can_be_approved_without_generated_replay_keys() {
        let data = snapshot();
        let input = input(&data, form()).unwrap();
        let root = plan(&input);
        let review = Review::planned(&data, input.clone(), root.clone()).unwrap();
        let body: Value = serde_json::from_slice(&review.body).unwrap();
        assert_eq!(body["commitment"], input);
        assert_eq!(body["payloadHash"], root["plan"]["payloadHash"]);
        assert_eq!(body["confirm"], true);
        assert!(body.get("idempotencyKey").is_none());
        let mut bad = root.clone();
        bad["plan"]["requestedInput"]["amount"] = json!(1);
        assert!(Review::planned(&data, input.clone(), bad).is_err());
        let mut bad = root;
        bad["plan"]["payloadHash"] = json!("invented");
        assert!(Review::planned(&data, input, bad).is_err());
    }
    #[test]
    fn forecast_unknowns_coverage_and_source_changes_are_not_inferred_or_ignored() {
        let data = snapshot();
        assert_eq!(
            data.values["forecast"]["periods"].as_array().unwrap().len(),
            13
        );
        assert_eq!(
            data.values["forecast"]["periods"][0]["openingCash"],
            Value::Null
        );
        assert_eq!(
            data.values["forecast"]["completeness"]["status"],
            "incomplete"
        );
        let lines = lines(&data.values["forecast"], "forecast");
        assert!(lines.iter().any(|s| s.contains("no independently")));
        for field in ["principal", "ledger", "forecast", "source"] {
            let mut changed = data.clone();
            match field {
                "principal" => changed.principal = "other".into(),
                "ledger" => changed.ledger = "other".into(),
                "forecast" => changed.values["forecast"]["periods"][0]["openingCash"] = json!(0),
                _ => changed.values["documents"][0]["sha256Hash"] = json!("changed"),
            };
            assert!(!data.current(&changed));
        }
    }
    #[test]
    fn lifecycle_requires_latest_returned_hash_reason_and_never_reactivates_ended() {
        let mut data = snapshot();
        let row = json!({"commitmentId":"synthetic-standing-plan","payloadHash":"b".repeat(64),"status":"active"});
        data.values["commitments"] = json!([row]);
        let row = &data.values["commitments"][0];
        for status in ["paused", "ended", "superseded"] {
            let review = Review::change(&data, row, status, "reviewed synthetic reason").unwrap();
            let body: Value = serde_json::from_slice(&review.body).unwrap();
            assert_eq!(body["expectedPayloadHash"], row["payloadHash"]);
            assert_eq!(body["reason"], "reviewed synthetic reason");
        }
        assert!(Review::change(&data, row, "paused", "").is_err());
        assert!(Review::change(&data, row, "active", "reason").is_err());
        let mut ended = data.clone();
        ended.values["commitments"][0]["status"] = json!("ended");
        assert!(
            Review::change(&ended, &ended.values["commitments"][0], "paused", "reason").is_err()
        );
    }
    #[test]
    fn canonical_match_uses_only_selected_occurrence_and_evidence_no_client_variance() {
        let mut data = snapshot();
        let o = json!({"commitmentId":"synthetic-standing-plan","date":"2026-01-15","amount":123.45,"currency":"DKK","payloadHash":"b".repeat(64)});
        let e = json!({"kind":"bank_transaction","id":"9","date":"2026-01-16","amount":-123.45,"currency":"DKK","label":"synthetic outgoing"});
        data.values["occurrences"] = json!([o]);
        data.values["evidenceOptions"] = json!([e]);
        let review = Review::matching(&data, &o, &e).unwrap();
        let body: Value = serde_json::from_slice(&review.body).unwrap();
        assert_eq!(
            body["evidence"],
            json!({"kind":"bank_transaction","id":"9"})
        );
        assert!(body.get("amount").is_none());
        assert!(body.get("variance").is_none());
        assert!(
            Review::matching(
                &data,
                &o,
                &json!({"kind":"bank_transaction","id":"unknown"})
            )
            .is_err()
        );
        let root = json!({"ok":true,"supplierCommitmentMatch":{"payloadHash":"c".repeat(64),"variance":{"dateDays":1,"documentation":"missing"},"idempotent":false}});
        let receipt = review.receipt(&root).unwrap();
        let mut next = data.clone();
        next.matches = vec![
            json!({"commitmentId":o["commitmentId"],"occurrenceDate":o["date"],"payloadHash":"c".repeat(64),"expected":{"payloadHash":o["payloadHash"]},"evidence":{"kind":e["kind"],"id":e["id"]},"variance":receipt["variance"]}),
        ];
        assert!(review.readback(&receipt, &next));
        next.matches[0]["evidence"]["id"] = json!("other");
        assert!(!review.readback(&receipt, &next));
    }
    #[test]
    fn known_approval_receipt_requires_exact_canonical_history_readback() {
        let data = snapshot();
        let input = input(&data, form()).unwrap();
        let root = plan(&input);
        let review = Review::planned(&data, input, root.clone()).unwrap();
        let receipt=review.receipt(&json!({"ok":true,"supplierCommitment":{"commitmentId":"synthetic-standing-plan","payloadHash":"b".repeat(64),"idempotent":false}})).unwrap();
        assert!(!review.readback(&receipt, &data));
        let mut next = data;
        next.values["history"] = json!([{"commitmentId":"synthetic-standing-plan","event":"approved","payloadHash":"b".repeat(64),"payload":root["plan"]}]);
        assert!(review.readback(&receipt, &next));
        assert!(review.receipt(&json!({"ok":true,"supplierCommitment":{"commitmentId":"other","payloadHash":"b".repeat(64)}})).is_none());
    }
    #[test]
    fn pending_is_payload_free_scoped_durable_and_requires_manual_resolution() {
        let data = snapshot();
        let input = input(&data, form()).unwrap();
        let review = Review::planned(&data, input.clone(), plan(&input)).unwrap();
        let record = review.marker("https://synthetic.invalid", None);
        assert!(valid_record(&record));
        for field in [
            "amount",
            "commitment",
            "body",
            "reason",
            "vendor",
            "sourceEvidence",
        ] {
            let mut invalid = record.clone();
            invalid[field] = json!("not persisted");
            assert!(!valid_record(&invalid));
        }
        assert!(scope(&record, "https://synthetic.invalid", &data, None));
        assert!(!scope(&record, "https://other.invalid", &data, None));
        let local = review.marker("http://127.0.0.1:10001", Some("owned-profile"));
        assert!(scope(
            &local,
            "http://127.0.0.1:10002",
            &data,
            Some("owned-profile")
        ));
        assert!(!scope(
            &local,
            "http://127.0.0.1:10002",
            &data,
            Some("other-profile")
        ));
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "rm-native-commitments-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        begin_at(&dir, &record).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(record.clone()));
        assert!(begin_at(&dir, &record).is_err());
        assert!(finish_at(&dir, &json!({"wrong":"record"}), false).is_err());
        finish_at(&dir, &record, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
