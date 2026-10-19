//! One private unresolved scheduled-recurring attempt; no payload persistence or replay.
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
    let m = fs::symlink_metadata(path).map_err(|_| "Privat planlægningsstatus kunne ikke åbnes")?;
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
            "Planlægningsstatus kræver din egen private mappe0700/fil0600 uden symbolske links."
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
        return Err("Privat planlægningsstatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-scheduled-recurring.json");
    if !path
        .try_exists()
        .map_err(|_| "Planlægningsstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Planlægningsstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Planlægningsstatus overskrider grænsen".into());
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Planlægningsstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret planlægningsforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    value["format"] == "rentemester-desktop-scheduled-recurring/1"
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
                        "templateId",
                        "stateHash",
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
            Some("enable" | "pause" | "resume" | "cancel" | "catch-up" | "skip" | "dispatch")
        )
        && value["templateId"]
            .as_i64()
            .is_some_and(|id| (1..=9_007_199_254_740_991).contains(&id))
        && ["localIdentity", "stateHash"]
            .iter()
            .all(|k| value[*k].is_null() || value[*k].as_str().is_some_and(|s| s.len() < 1024))
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig planlægningsstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-scheduled-recurring.json"))
        .map_err(|_| "Et planlægningsforsøg kræver afklaring før ny bogføring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Planlægningsstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Planlægningsstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Planlægningsstatus ændrede sig og bevares".into());
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
            .open(dir.join(format!(
                "scheduled-recurring-manually-resolved-{suffix}.json"
            )))
            .map_err(|_| "Planlægningsafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Planlægningsafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-scheduled-recurring.json"))
        .map_err(|_| "Planlægningsstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Planlægningsstatus kunne ikke synkroniseres".into())
}

use rentemester_client::ScheduledRecurringMutation;
use serde_json::json;
use std::sync::Arc;
#[derive(Clone, PartialEq)]
pub struct Snapshot {
    pub company: String,
    pub principal: String,
    pub ledger: String,
    pub role: String,
    pub templates: Value,
    pub invoices: Value,
    pub status: Value,
    pub selected: Option<Value>,
    pub template: Option<Value>,
    pub plan: Option<Value>,
    pub plan_error: Option<String>,
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
fn id(v: &Value) -> Option<i64> {
    v.as_i64()
        .filter(|i| (1..=9_007_199_254_740_991).contains(i))
}
fn occurrence(v: &Value) -> bool {
    v["periodIndex"]
        .as_u64()
        .is_some_and(|i| i <= 9_007_199_254_740_991)
        && text(v, "issueDate").is_ok()
}
fn row(v: &Value) -> bool {
    id(&v["templateId"]).is_some()
        && hash(&v["templateHash"])
        && hash(&v["stateHash"])
        && matches!(
            v["state"].as_str(),
            Some("disabled" | "enabled" | "paused" | "cancelled")
        )
        && (v["ownerUserId"].is_null() || v["ownerUserId"].is_string())
        && (v["lastRun"].is_null() || v["lastRun"].is_object())
        && (v["nextOccurrence"].is_null() || occurrence(&v["nextOccurrence"]))
        && v["missedCount"].is_u64()
        && v["queued"]
            .as_array()
            .is_some_and(|a| a.iter().all(occurrence))
        && (v["error"].is_null() || v["error"].is_string())
        && v["history"].is_array()
}
impl Snapshot {
    pub fn parse(
        company: &str,
        selected_id: Option<i64>,
        templates: Value,
        invoices: Value,
        status: Value,
        plan: Option<Value>,
        plan_error: Option<String>,
        me: Value,
        journal: Value,
    ) -> Result<Self, String> {
        let all = &status["scheduledRecurring"];
        if plan.is_some() && plan_error.is_some()
            || invoices["ok"] != true
            || templates["ok"] != true
            || templates["recurringInvoices"]["slug"] != company
            || !templates["recurringInvoices"]["templates"].is_array()
            || status["ok"] != true
            || text(all, "asOfDate").is_err()
            || all["timeZone"] != "Europe/Copenhagen"
            || all["localService"] != false
            || !all["rows"].as_array().is_some_and(|a| a.iter().all(row))
            || me["ok"] != true
            || journal["ok"] != true
            || journal["journal"]["slug"] != company
        {
            return Err("Serveren bekræftede ikke faktiske skabeloner, kanonisk køstatus, bruger, tidszone og ledger".into());
        }
        let selected = selected_id
            .and_then(|i| {
                all["rows"]
                    .as_array()
                    .and_then(|a| a.iter().find(|r| r["templateId"] == i))
            })
            .cloned();
        let template = selected_id
            .and_then(|i| {
                templates["recurringInvoices"]["templates"]
                    .as_array()
                    .and_then(|a| a.iter().find(|r| r["id"] == i))
            })
            .cloned();
        if let Some(v) = &plan {
            let p = &v["plan"];
            let Some(s) = &selected else {
                return Err("Plan uden synlig køkilde afvises".into());
            };
            if template.is_none()
                || v["ok"] != true
                || p["templateId"] != s["templateId"]
                || p["templateHash"] != s["templateHash"]
                || p["stateHash"] != s["stateHash"]
                || p["asOfDate"] != all["asOfDate"]
                || !hash(&p["planHash"])
                || !(p["normalDue"].is_null() || occurrence(&p["normalDue"]))
                || !p["missed"]
                    .as_array()
                    .is_some_and(|a| a.iter().all(occurrence))
                || !p["queued"]
                    .as_array()
                    .is_some_and(|a| a.iter().all(occurrence))
                || !p["missedCount"].is_u64()
                || !p["truncated"].is_boolean()
            {
                return Err("Serverplan matcher ikke præcis aktuel skabelon/stateHash/dato eller kanoniske forekomster".into());
            }
        }
        let role = me["companies"]
            .as_array()
            .and_then(|a| {
                a.iter()
                    .find(|c| c["slug"] == company && c["archived"] != true)
            })
            .and_then(|c| c["role"].as_str())
            .ok_or("Aktuel virksomhedsrolle mangler")?
            .to_owned();
        Ok(Self {
            company: company.into(),
            principal: text(&me["user"], "id")?,
            ledger: text(&journal["journal"], "ledgerUuid")?,
            role,
            templates,
            invoices,
            status,
            selected,
            template,
            plan,
            plan_error,
        })
    }
    pub fn can_write(&self) -> bool {
        matches!(self.role.as_str(), "owner" | "bookkeeper")
    }
    pub fn same_owner(&self) -> bool {
        self.selected
            .as_ref()
            .is_some_and(|s| s["ownerUserId"] == self.principal)
    }
    pub fn visible_missed(&self) -> Vec<Value> {
        self.plan
            .as_ref()
            .and_then(|p| p["plan"]["missed"].as_array())
            .cloned()
            .unwrap_or_default()
    }
    pub fn dispatchable(&self) -> Vec<Value> {
        let Some(p) = &self.plan else {
            return vec![];
        };
        let mut out = Vec::new();
        if p["plan"]["normalDue"].is_object() {
            out.push(p["plan"]["normalDue"].clone());
        }
        if let Some(a) = p["plan"]["queued"].as_array() {
            out.extend(a.iter().cloned());
        }
        out
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Enable,
    Pause,
    Resume,
    Cancel,
    CatchUp,
    Skip,
    Dispatch,
}
impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Self::Enable => "enable",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Cancel => "cancel",
            Self::CatchUp => "catch-up",
            Self::Skip => "skip",
            Self::Dispatch => "dispatch",
        }
    }
    pub fn sdk(self) -> ScheduledRecurringMutation {
        match self {
            Self::Enable => ScheduledRecurringMutation::Enable,
            Self::Pause => ScheduledRecurringMutation::Pause,
            Self::Resume => ScheduledRecurringMutation::Resume,
            Self::Cancel => ScheduledRecurringMutation::Cancel,
            Self::CatchUp => ScheduledRecurringMutation::CatchUp,
            Self::Skip => ScheduledRecurringMutation::Skip,
            Self::Dispatch => ScheduledRecurringMutation::Dispatch,
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
        reason: &str,
        indexes: &[u64],
        dispatch: Option<u64>,
        consent: bool,
    ) -> Result<Self, String> {
        let s = data
            .selected
            .as_ref()
            .filter(|_| data.template.is_some())
            .ok_or("Vælg en faktisk aktuelt synlig fakturaskabelon først")?;
        if !data.can_write() {
            return Err("Aktuel CompanyDraftWrite (owner/bookkeeper) kræves; serveren genkontrollerer ved afsendelse".into());
        }
        let state = s["state"].as_str().ok_or("Ukendt køstatus")?;
        let allowed = match action {
            Action::Enable => state == "disabled",
            Action::Pause => state == "enabled",
            Action::Resume => state == "paused" && data.same_owner(),
            Action::Cancel | Action::CatchUp | Action::Skip => {
                matches!(state, "enabled" | "paused")
            }
            Action::Dispatch => state == "enabled" && data.same_owner(),
        };
        if !allowed {
            return Err("Handlingen passer ikke aktuel tilstand/ejer. Cancelled er terminal; resume/dispatch kræver samme samtykkende bruger og aktuelle rettigheder.".into());
        }
        let mut body = json!({"templateId":s["templateId"]});
        match action {
            Action::Enable => {
                if !consent {
                    return Err("Giv eksplicit samtykke til skabelonens planlagte udstedelse før aktivering. Ingen automatisk bogføring eller mail.".into());
                }
                body["templateHash"] = s["templateHash"].clone();
            }
            Action::Pause | Action::Resume | Action::Cancel => {
                body["expectedStateHash"] = s["stateHash"].clone();
            }
            Action::CatchUp | Action::Skip | Action::Dispatch => {
                let p = data
                    .plan
                    .as_ref()
                    .ok_or("Genlæs readonly serverplan før forekomstvalg")?;
                body["planHash"] = p["plan"]["planHash"].clone();
                if action == Action::Dispatch {
                    let index = dispatch.ok_or(
                        "Vælg eksplicit normal dagens forekomst eller en faktisk køsat forekomst",
                    )?;
                    if !data
                        .dispatchable()
                        .iter()
                        .any(|r| r["periodIndex"] == index)
                    {
                        return Err("Forekomsten er hverken serverens normalDue eller eksplicit køsat; missed må først gennemgås som catch-up/skip".into());
                    }
                    body["periodIndex"] = json!(index);
                } else {
                    if indexes.is_empty()
                        || indexes.len() > 100
                        || indexes
                            .iter()
                            .enumerate()
                            .any(|(i, n)| indexes[..i].contains(n))
                        || !indexes
                            .iter()
                            .all(|n| data.visible_missed().iter().any(|r| r["periodIndex"] == *n))
                    {
                        return Err("Vælg1–100 forskellige faktisk viste missed-forekomster; ingen datointervaller/forekomster beregnes lokalt".into());
                    }
                    body["periodIndexes"] = json!(indexes);
                }
            }
        }
        if matches!(
            action,
            Action::Pause | Action::Resume | Action::Cancel | Action::Skip
        ) {
            let reason = reason.trim();
            if reason.is_empty() || reason.encode_utf16().count() > 1000 {
                return Err("Angiv konkret begrundelse på1–1000UTF16-tegn".into());
            }
            body["reason"] = json!(reason);
        }
        body["confirm"] = json!(true);
        let bytes = serde_json::to_vec(&body).map_err(|_| "Anmodningen kunne ikke fastholdes")?;
        if bytes.len() > rentemester_client::MAX_SCHEDULED_RECURRING_BODY {
            return Err("Anmodningen overstiger native16KiB".into());
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
        json!({"format":"rentemester-desktop-scheduled-recurring/1","origin":origin,"company":self.snapshot.company,"principal":self.snapshot.principal,"ledger":self.snapshot.ledger,"localIdentity":local,"operation":self.action.name(),"templateId":self.display["templateId"],"stateHash":self.snapshot.selected.as_ref().map(|s|s["stateHash"].clone()).unwrap_or(Value::Null)})
    }
    pub fn receipt(&self, v: &Value) -> bool {
        v["ok"] == true
            && row(&v["scheduledRecurring"])
            && v["scheduledRecurring"]["templateId"] == self.display["templateId"]
            && v["scheduledRecurring"]["templateHash"]
                == self.snapshot.selected.as_ref().unwrap()["templateHash"]
            && match self.action {
                Action::Enable | Action::Resume => v["scheduledRecurring"]["state"] == "enabled",
                Action::Pause => v["scheduledRecurring"]["state"] == "paused",
                Action::Cancel => v["scheduledRecurring"]["state"] == "cancelled",
                Action::Dispatch => match v["dispatch"]["status"].as_str() {
                    Some("succeeded") => {
                        id(&v["generation"]["documentId"]).is_some()
                            && text(&v["generation"], "invoiceNumber").is_ok()
                    }
                    Some("failed") => text(&v["dispatch"], "error").is_ok(),
                    _ => false,
                },
                _ => true,
            }
    }
    pub fn readback(&self, v: &Value, data: &Snapshot) -> bool {
        self.receipt(v) && data.selected.as_ref() == Some(&v["scheduledRecurring"])
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
    fn queue(state: &str) -> Value {
        json!({"templateId":7,"templateHash":"a".repeat(64),"state":state,"stateHash":"b".repeat(64),"ownerUserId":if state=="disabled"{Value::Null}else{json!("owner")},"lastRun":null,"nextOccurrence":{"periodIndex":5,"issueDate":"2026-10-19"},"missedCount":2,"queued":[{"periodIndex":2,"issueDate":"2026-07-19"}],"error":null,"history":[]})
    }
    fn templates() -> Value {
        json!({"ok":true,"recurringInvoices":{"slug":"synthetic","nativeRecurring":true,"templates":[{"id":7,"name":"Synthetic recurring source","active":true,"interval":"monthly","nextIssueDate":"2026-10-19","generations":[]}]}})
    }
    fn status(state: &str) -> Value {
        json!({"ok":true,"scheduledRecurring":{"asOfDate":"2026-10-19","timeZone":"Europe/Copenhagen","localService":false,"rows":[queue(state)]}})
    }
    fn plan() -> Value {
        json!({"ok":true,"plan":{"templateId":7,"templateHash":"a".repeat(64),"stateHash":"b".repeat(64),"asOfDate":"2026-10-19","normalDue":{"periodIndex":5,"issueDate":"2026-10-19"},"missed":[{"periodIndex":0,"issueDate":"2026-05-19"},{"periodIndex":1,"issueDate":"2026-06-19"}],"missedCount":2,"truncated":false,"queued":[{"periodIndex":2,"issueDate":"2026-07-19"}],"planHash":"c".repeat(64)}})
    }
    fn me(principal: &str, role: &str) -> Value {
        json!({"ok":true,"user":{"id":principal},"companies":[{"slug":"synthetic","role":role}]})
    }
    fn journal() -> Value {
        json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"immutable-ledger"}})
    }
    fn data(state: &str, principal: &str) -> Snapshot {
        Snapshot::parse(
            "synthetic",
            Some(7),
            templates(),
            json!({"ok":true,"invoices":[]}),
            status(state),
            Some(plan()),
            None,
            me(principal, "owner"),
            journal(),
        )
        .unwrap()
    }
    #[test]
    fn readonly_status_and_plan_preserve_last_next_missed_history_error_without_local_calendar_or_dispatch()
     {
        let mut s = status("paused");
        s["scheduledRecurring"]["rows"][0]["lastRun"] = json!({"status":"failed","periodIndex":3});
        s["scheduledRecurring"]["rows"][0]["error"] = json!("source_refused");
        s["scheduledRecurring"]["rows"][0]["history"] =
            json!([{"event":"paused","actor":"user:owner"}]);
        let mut p = plan();
        p["plan"]["truncated"] = json!(true);
        let d = Snapshot::parse(
            "synthetic",
            Some(7),
            templates(),
            json!({"ok":true}),
            s.clone(),
            Some(p.clone()),
            None,
            me("owner", "owner"),
            journal(),
        )
        .unwrap();
        assert_eq!(d.status, s);
        assert_eq!(d.plan, Some(p));
        assert_eq!(
            d.dispatchable(),
            vec![
                plan()["plan"]["normalDue"].clone(),
                plan()["plan"]["queued"][0].clone()
            ]
        );
        assert_eq!(d.visible_missed().len(), 2);
        let plain = Snapshot::parse(
            "synthetic",
            None,
            templates(),
            json!({"ok":true}),
            status("enabled"),
            None,
            None,
            me("owner", "owner"),
            journal(),
        )
        .unwrap();
        assert!(plain.selected.is_none());
        assert!(plain.plan.is_none());
        assert!(plain.dispatchable().is_empty());
        let blocked = Snapshot::parse(
            "synthetic",
            Some(7),
            templates(),
            json!({"ok":true}),
            status("enabled"),
            None,
            Some("HTTP422".into()),
            me("owner", "owner"),
            journal(),
        )
        .unwrap();
        assert!(blocked.plan.is_none());
        assert_eq!(blocked.plan_error.as_deref(), Some("HTTP422"));
        assert!(
            Review::prepare(
                &blocked,
                Action::Pause,
                "Pause blocked source",
                &[],
                None,
                false
            )
            .is_ok()
        );
        assert!(Review::prepare(&blocked, Action::Dispatch, "", &[], Some(5), false).is_err());
    }
    #[test]
    fn explicit_enable_consent_freezes_actual_template_hash_without_keys_dates_or_invoice_defaults()
    {
        let d = data("disabled", "owner");
        assert!(Review::prepare(&d, Action::Enable, "", &[], None, false).is_err());
        let r = Review::prepare(&d, Action::Enable, "", &[], None, true).unwrap();
        assert_eq!(
            r.display,
            json!({"templateId":7,"templateHash":"a".repeat(64),"confirm":true})
        );
        assert_eq!(serde_json::from_slice::<Value>(&r.body).unwrap(), r.display);
        for k in [
            "actor",
            "idempotencyKey",
            "asOfDate",
            "periodIndex",
            "invoice",
            "amount",
        ] {
            assert!(r.display.get(k).is_none());
        }
        assert!(
            Review::prepare(
                &data("cancelled", "owner"),
                Action::Enable,
                "",
                &[],
                None,
                true
            )
            .is_err()
        );
    }
    #[test]
    fn exact_pause_resume_cancel_states_current_permission_and_same_consenting_owner_are_guarded() {
        let d = data("enabled", "other");
        assert!(Review::prepare(&d, Action::Pause, "explicit pause", &[], None, false).is_ok());
        let r = Review::prepare(&d, Action::Cancel, "explicit cancel", &[], None, false).unwrap();
        assert_eq!(
            r.display,
            json!({"templateId":7,"expectedStateHash":"b".repeat(64),"reason":"explicit cancel","confirm":true})
        );
        assert!(
            Review::prepare(
                &data("paused", "other"),
                Action::Resume,
                "resume",
                &[],
                None,
                false
            )
            .is_err()
        );
        assert!(
            Review::prepare(
                &data("paused", "owner"),
                Action::Resume,
                "resume",
                &[],
                None,
                false
            )
            .is_ok()
        );
        assert!(Review::prepare(&d, Action::Resume, "resume", &[], None, false).is_err());
        for reason in ["".to_owned(), " ".into(), "a".repeat(1001)] {
            assert!(Review::prepare(&d, Action::Pause, &reason, &[], None, false).is_err());
        }
        for role in ["reader", "reviewer"] {
            let mut denied = d.clone();
            denied.role = role.into();
            assert!(Review::prepare(&denied, Action::Pause, "pause", &[], None, false).is_err());
        }
        let mut bk = d;
        bk.role = "bookkeeper".into();
        assert!(Review::prepare(&bk, Action::Pause, "pause", &[], None, false).is_ok());
    }
    #[test]
    fn missed_requires_explicit_selected_catchup_or_reasoned_skip_and_never_direct_dispatch() {
        let d = data("enabled", "owner");
        for indexes in [vec![], vec![0, 0], vec![9], (0..101).collect()] {
            assert!(Review::prepare(&d, Action::CatchUp, "", &indexes, None, false).is_err());
        }
        let r = Review::prepare(&d, Action::CatchUp, "", &[1, 0], None, false).unwrap();
        assert_eq!(
            r.display,
            json!({"templateId":7,"planHash":"c".repeat(64),"periodIndexes":[1,0],"confirm":true})
        );
        assert!(Review::prepare(&d, Action::Skip, "", &[0], None, false).is_err());
        let skip = Review::prepare(
            &d,
            Action::Skip,
            "Reviewed missed occurrence",
            &[0],
            None,
            false,
        )
        .unwrap();
        assert_eq!(skip.display["reason"], "Reviewed missed occurrence");
        assert!(Review::prepare(&d, Action::Dispatch, "", &[], Some(0), false).is_err());
        assert!(
            Review::prepare(
                &data("paused", "owner"),
                Action::CatchUp,
                "",
                &[0],
                None,
                false
            )
            .is_ok()
        );
    }
    #[test]
    fn dispatch_is_explicit_current_owner_enabled_plan_due_or_queued_and_never_pause_or_unknown() {
        let d = data("enabled", "owner");
        for i in [5, 2] {
            let r = Review::prepare(&d, Action::Dispatch, "", &[], Some(i), false).unwrap();
            assert_eq!(
                r.display,
                json!({"templateId":7,"planHash":"c".repeat(64),"periodIndex":i,"confirm":true})
            );
        }
        assert!(Review::prepare(&d, Action::Dispatch, "", &[], None, false).is_err());
        assert!(
            Review::prepare(
                &data("enabled", "other"),
                Action::Dispatch,
                "",
                &[],
                Some(5),
                false
            )
            .is_err()
        );
        for state in ["disabled", "paused", "cancelled"] {
            assert!(
                Review::prepare(
                    &data(state, "owner"),
                    Action::Dispatch,
                    "",
                    &[],
                    Some(5),
                    false
                )
                .is_err()
            );
        }
        let mut without = d;
        without.plan = None;
        assert!(Review::prepare(&without, Action::Dispatch, "", &[], Some(5), false).is_err());
    }
    #[test]
    fn frozen_full_source_context_rejects_changed_principal_ledger_template_state_plan_and_readback()
     {
        let d = data("enabled", "owner");
        let r = Review::prepare(&d, Action::Dispatch, "", &[], Some(5), false).unwrap();
        for change in 0..7 {
            let mut next = d.clone();
            match change {
                0 => next.principal = "other".into(),
                1 => next.ledger = "other".into(),
                2 => next.role = "reader".into(),
                3 => next.templates["recurringInvoices"]["templates"][0]["name"] = json!("changed"),
                4 => {
                    next.status["scheduledRecurring"]["rows"][0]["stateHash"] =
                        json!("d".repeat(64))
                }
                5 => next.plan.as_mut().unwrap()["plan"]["planHash"] = json!("d".repeat(64)),
                _ => next.invoices = json!({"ok":true,"changed":true}),
            }
            assert!(!r.current(&next));
        }
        let mut bad = plan();
        bad["plan"]["stateHash"] = json!("d".repeat(64));
        assert!(
            Snapshot::parse(
                "synthetic",
                Some(7),
                templates(),
                json!({"ok":true}),
                status("enabled"),
                Some(bad),
                None,
                me("owner", "owner"),
                journal()
            )
            .is_err()
        );
    }
    #[test]
    fn dispatch_receipt_distinguishes_actual_issued_original_from_known_failed_or_unknown() {
        let d = data("enabled", "owner");
        let r = Review::prepare(&d, Action::Dispatch, "", &[], Some(5), false).unwrap();
        let good = json!({"ok":true,"scheduledRecurring":queue("enabled"),"generation":{"created":true,"templateId":7,"periodIndex":5,"documentId":11,"invoiceNumber":"SYN-11"},"dispatch":{"status":"succeeded"}});
        assert!(r.receipt(&good));
        assert!(r.readback(&good, &d));
        let mut bad = good.clone();
        bad["generation"] = Value::Null;
        assert!(!r.receipt(&bad));
        let mut bad = good.clone();
        bad["dispatch"] = Value::Null;
        assert!(!r.receipt(&bad));
        let failed = json!({"ok":true,"scheduledRecurring":queue("enabled"),"dispatch":{"status":"failed","error":"source_refused"}});
        assert!(r.receipt(&failed));
        for key in ["templateId", "templateHash"] {
            let mut bad = good.clone();
            bad["scheduledRecurring"][key] = json!("wrong");
            assert!(!r.receipt(&bad));
        }
        assert!(good["generation"].get("posted").is_none());
        assert!(good["generation"].get("sent").is_none());
    }
    #[test]
    fn private_unknown_marker_is_payload_free_scoped_preserved_and_requires_manual_resolution() {
        let d = data("enabled", "owner");
        let r = Review::prepare(&d, Action::Dispatch, "", &[], Some(5), false).unwrap();
        let marker = r.marker("https://one.invalid", None);
        assert!(valid_record(&marker));
        for key in [
            "body",
            "plan",
            "generation",
            "periodIndex",
            "reason",
            "invoice",
            "cookie",
        ] {
            assert!(marker.get(key).is_none());
        }
        assert!(!scope(&marker, "https://two.invalid", &d, None));
        let local = r.marker("http://127.0.0.1:1", Some("trusted"));
        assert!(scope(&local, "http://127.0.0.1:2", &d, Some("trusted")));
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
            .join(format!("native-scheduled-recurring-{suffix}"));
        fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
        begin_at(&dir, &marker).unwrap();
        assert_eq!(load_at(&dir).unwrap(), Some(marker.clone()));
        assert!(begin_at(&dir, &marker).is_err());
        let mut wrong = marker.clone();
        wrong["templateId"] = json!(8);
        assert!(finish_at(&dir, &wrong, false).is_err());
        wrong = marker.clone();
        wrong["body"] = json!("private");
        assert!(!valid_record(&wrong));
        finish_at(&dir, &marker, true).unwrap();
        assert!(load_at(&dir).unwrap().is_none());
        fs::remove_dir_all(dir).unwrap();
    }
}
