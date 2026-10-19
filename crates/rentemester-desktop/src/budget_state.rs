//! Reviewed budget revision and a scoped, payload-free durable unknown-outcome marker.
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
    let m = fs::symlink_metadata(path).map_err(|_| "Privat budgetstatus kunne ikke åbnes")?;
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
            "Budgetstatus kræver din egen private mappe0700/fil0600 uden symbolske links.".into(),
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
        return Err("Privat budgetstatus må ikke have udvidede adgangslister.".into());
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
    let path = dir.join("pending-budget.json");
    if !path
        .try_exists()
        .map_err(|_| "Budgetstatus kunne ikke undersøges")?
    {
        return Ok(None);
    }
    private(dir, true)?;
    private(&path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Budgetstatus kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Budgetstatus overskrider grænsen".into());
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Budgetstatus er ugyldig")?;
    if !valid_record(&value) {
        return Err("Ukendt uafklaret budgetforsøg. Filen bevares.".into());
    }
    Ok(Some(value))
}

fn valid_record(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.len() == 8
        && object.keys().all(|key| {
            [
                "format",
                "origin",
                "company",
                "principal",
                "ledger",
                "localIdentity",
                "operation",
                "year",
            ]
            .contains(&key.as_str())
        })
        && value["format"] == "rentemester-desktop-budget/1"
        && ["origin", "company", "principal", "ledger", "year"]
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
        && matches!(value["operation"].as_str(), Some("set_budget"))
}
pub fn begin(value: &Value) -> Result<(), String> {
    let dir = ensure()?;
    begin_at(&dir, value)
}
fn begin_at(dir: &Path, value: &Value) -> Result<(), String> {
    if !valid_record(value) || value.to_string().len() > 65536 {
        return Err("Ugyldig budgetstatus; intet sendt".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(dir.join("pending-budget.json"))
        .map_err(|_| "Et budgetforsøg kræver afklaring før ny ændring")?;
    file.write_all(value.to_string().as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "Budgetstatus kunne ikke gemmes; intet sendt")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Budgetstatus kunne ikke synkroniseres; intet sendt".into())
}
pub fn finish(expected: &Value, manual: bool) -> Result<(), String> {
    finish_at(&ensure()?, expected, manual)
}
fn finish_at(dir: &Path, expected: &Value, manual: bool) -> Result<(), String> {
    if load_at(dir)?.as_ref() != Some(expected) {
        return Err("Budgetstatus ændrede sig og bevares".into());
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
            .open(dir.join(format!("budget-manually-resolved-{suffix}.json")))
            .map_err(|_| "Budgetafklaring kunne ikke arkiveres")?;
        file.write_all(expected.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Budgetafklaring kunne ikke gemmes")?;
    }
    fs::remove_file(dir.join("pending-budget.json"))
        .map_err(|_| "Budgetstatus kunne ikke afsluttes")?;
    fs::File::open(dir)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Budgetstatus kunne ikke synkroniseres".into())
}

use serde_json::json;
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Selection {
    pub dimension: Option<String>,
    pub member: Option<String>,
}
impl Selection {
    pub fn from_option(option: &Value) -> Result<Self, String> {
        let dimension = option["dimensionId"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or("Dimension mangler i serverens valgmulighed")?;
        let member = match option.get("memberId") {
            Some(Value::String(v)) if !v.is_empty() => Some(v.clone()),
            None | Some(Value::Null) => None,
            _ => return Err("Ugyldigt medlem i serverens valgmulighed".into()),
        };
        Ok(Self {
            dimension: Some(dimension.into()),
            member,
        })
    }
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub plan: Value,
    pub compare: Value,
    pub dimensions: Option<Value>,
    pub chart: Value,
    pub principal: String,
    pub ledger: String,
}
impl Snapshot {
    pub fn parse(
        company: &str,
        year: &str,
        plan: Value,
        compare: Value,
        dimensions: Option<Value>,
        chart: Value,
        identity: (Value, Value),
    ) -> Result<Self, String> {
        let (me, journal) = identity;
        let p = &plan["budget"];
        let c = &compare["budgetVsActual"];
        if [&plan, &compare, &chart, &me, &journal]
            .iter()
            .any(|v| v["ok"] != true)
            || journal["journal"]["slug"] != company
            || [p, c]
                .iter()
                .any(|v| v["slug"] != company || v["selectedYear"] != year)
            || chart["accounts"]["slug"] != company
            || !chart["accounts"]["accounts"].is_array()
            || !p["archived"].is_boolean()
            || p["archived"] != c["archived"]
            || !p["periods"]
                .as_array()
                .is_some_and(|r| !r.is_empty() && r.iter().all(|v| v.as_str().is_some_and(period)))
            || p["company"] != c["company"]
            || p["company"] != chart["accounts"]["company"]
            || p["totalBudget"] != c["totalBudget"]
            || p["periods"] != c["periods"]
            || p["fiscalYears"] != c["fiscalYears"]
            || !p["fiscalYears"]
                .as_array()
                .is_some_and(|r| r.iter().any(|y| y["label"] == year))
            || !p["lines"].is_array()
            || !c["lines"].is_array()
            || ![
                &p["totalBudget"],
                &c["totalBudget"],
                &c["totalActual"],
                &c["totalVariance"],
            ]
            .iter()
            .all(|v| v.is_number())
        {
            return Err("Serveren bekræftede ikke budget, regnskabsår og kontoplan".into());
        }
        let principal = me["user"]["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Aktuel bruger mangler")?
            .into();
        let ledger = journal["journal"]["ledgerUuid"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Ledgeridentitet mangler")?
            .into();
        let dimensions = match dimensions {
            Some(d) => {
                let v = &d["budgetDimensionActuals"];
                if d["ok"] != true
                    || v["slug"] != company
                    || v["selectedYear"] != year
                    || v["serverCalculated"] != true
                    || !v["dimensionOptions"].is_array()
                    || !v["comparison"].is_array()
                {
                    return Err("Serverens dimensionsrapport mangler korrekt scope".into());
                }
                Some(v.clone())
            }
            None if p["dimensionActualsAvailable"] == false => None,
            None => return Err("Dimensionsrapport mangler".into()),
        };
        Ok(Self {
            plan: p.clone(),
            compare: c.clone(),
            dimensions,
            chart: chart["accounts"].clone(),
            principal,
            ledger,
        })
    }
    pub fn marker(&self, origin: &str, company: &str, local: Option<&str>) -> Value {
        json!({"format":"rentemester-desktop-budget/1","origin":origin,"company":company,"principal":self.principal,"ledger":self.ledger,"localIdentity":local,"operation":"set_budget","year":self.plan["selectedYear"]})
    }
    pub fn matches_marker(
        &self,
        marker: &Value,
        origin: &str,
        company: &str,
        local: Option<&str>,
    ) -> bool {
        valid_record(marker)
            && marker["company"] == company
            && marker["principal"] == self.principal
            && marker["ledger"] == self.ledger
            && marker["year"] == self.plan["selectedYear"]
            && marker["localIdentity"].as_str() == local
            && (marker["origin"] == origin
                || local.is_some_and(|v| marker["localIdentity"].as_str() == Some(v)))
    }
    pub fn selection(&self) -> Selection {
        self.dimensions
            .as_ref()
            .map(|d| Selection {
                dimension: d["selection"]["dimensionId"].as_str().map(str::to_owned),
                member: d["selection"]["memberId"].as_str().map(str::to_owned),
            })
            .unwrap_or_default()
    }
}
fn period(value: &str) -> bool {
    value.len() == 7
        && value.is_ascii()
        && value.as_bytes()[4] == b'-'
        && value[..4].bytes().all(|b| b.is_ascii_digit())
        && matches!(
            &value[5..],
            "01" | "02" | "03" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12"
        )
}
#[derive(Clone)]
pub struct Review {
    pub bytes: Vec<u8>,
    pub before: Snapshot,
}
impl Review {
    pub fn prepare(
        before: &Snapshot,
        account: &str,
        month: &str,
        amount: &str,
        notes: &str,
    ) -> Result<Self, String> {
        if before.plan["archived"] != false {
            return Err("Arkiveret år er skrivebeskyttet".into());
        }
        if !before.chart["accounts"]
            .as_array()
            .is_some_and(|r| r.iter().any(|a| a["accountNo"] == account))
        {
            return Err("Vælg en eksisterende konto fra serverens kontoplan".into());
        }
        if !before.plan["periods"]
            .as_array()
            .is_some_and(|r| r.iter().any(|p| p == month))
        {
            return Err("Vælg en måned fra serverens regnskabsår".into());
        }
        let amount = amount.trim().replace(',', ".");
        // Preserve the exact chosen numeric lexeme in SDK bytes. No local rounding or totals.
        if amount.len() > 128
            || amount.is_empty()
            || !amount.bytes().all(|b| b.is_ascii_digit() || b == b'.')
            || !serde_json::from_str::<Value>(&amount).is_ok_and(|v| v.is_number())
        {
            return Err(
                "Beløb skal være et eksplicit ikke-negativt decimaltal uden tusindtalsseparator"
                    .into(),
            );
        }
        if notes.len() > 4096 {
            return Err("Noter må højst fylde4096bytes".into());
        }
        let note = if notes.trim().is_empty() {
            Value::Null
        } else {
            json!(notes.trim())
        };
        let bytes = format!(
            "{{\"accountNo\":{},\"period\":{},\"amount\":{},\"notes\":{}}}",
            json!(account),
            json!(month),
            amount,
            note
        )
        .into_bytes();
        Ok(Self {
            bytes,
            before: before.clone(),
        })
    }
    pub fn current(&self, now: &Snapshot) -> bool {
        self.before.principal == now.principal
            && self.before.ledger == now.ledger
            && self.before.plan == now.plan
            && self.before.chart == now.chart
    }
    pub fn receipt<'a>(&self, response: &'a Value) -> Option<&'a Value> {
        let value: Value = serde_json::from_slice(&self.bytes).ok()?;
        let r = &response["budget"];
        (response["ok"] == true
            && r["id"].as_i64().is_some_and(|id| id > 0)
            && r["accountNo"] == value["accountNo"]
            && r["period"] == value["period"]
            && r["amount"].is_number())
        .then_some(r)
    }
    pub fn readback(&self, now: &Snapshot, receipt: &Value) -> bool {
        let value: Value = serde_json::from_slice(&self.bytes).unwrap_or(Value::Null);
        now.principal == self.before.principal
            && now.ledger == self.before.ledger
            && now.plan["selectedYear"] == self.before.plan["selectedYear"]
            && now.plan["lines"].as_array().is_some_and(|r| {
                r.iter().any(|r| {
                    r["id"] == receipt["id"]
                        && r["accountNo"] == receipt["accountNo"]
                        && r["period"] == receipt["period"]
                        && r["amount"] == receipt["amount"]
                        && r["notes"] == value["notes"]
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        Snapshot {
            plan: json!({"slug":"synthetic","selectedYear":"2026/27","archived":false,"company":{"id":1,"name":"Synthetic company"},"periods":["2026-07","2027-01","2027-06"],"fiscalYears":[{"label":"2026/27"}],"periodStart":"2026-07","periodEnd":"2027-06","totalBudget":0,"lines":[],"dimensionActualsAvailable":true}),
            compare: json!({"slug":"synthetic","selectedYear":"2026/27","archived":false,"company":{"id":1,"name":"Synthetic company"},"periods":["2026-07","2027-01","2027-06"],"fiscalYears":[{"label":"2026/27"}],"totalBudget":0,"totalActual":-12.34,"totalVariance":-12.34,"lines":[]}),
            dimensions: Some(
                json!({"slug":"synthetic","selectedYear":"2026/27","serverCalculated":true,"dimensionOptions":[{"value":"project:a","label":"Synthetic member","dimensionId":"project","memberId":"a"}],"selection":{"dimensionId":"project","memberId":"a"},"comparison":[{"actual":12.34,"budget":null,"variance":null}]}),
            ),
            chart: json!({"slug":"synthetic","company":{"id":1,"name":"Synthetic company"},"accounts":[{"accountNo":"X","name":"Synthetic expense","active":true,"type":"expense"}]}),
            principal: "synthetic-user".into(),
            ledger: "synthetic-ledger".into(),
        }
    }
    fn parse(data: &Snapshot, company: &str, year: &str) -> Result<Snapshot, String> {
        Snapshot::parse(
            company,
            year,
            json!({"ok":true,"budget":data.plan}),
            json!({"ok":true,"budgetVsActual":data.compare}),
            data.dimensions
                .clone()
                .map(|v| json!({"ok":true,"budgetDimensionActuals":v})),
            json!({"ok":true,"accounts":data.chart}),
            (
                json!({"ok":true,"user":{"id":data.principal}}),
                json!({"ok":true,"journal":{"slug":"synthetic","selectedYear":"2027","ledgerUuid":data.ledger}}),
            ),
        )
    }
    #[test]
    fn canonical_noncalendar_scope_and_unknown_dimension_variance_preserved() {
        let s = snapshot();
        let parsed = parse(&s, "synthetic", "2026/27").unwrap();
        assert_eq!(parsed.compare["totalActual"], -12.34);
        assert!(parsed.dimensions.as_ref().unwrap()["comparison"][0]["budget"].is_null());
        assert!(parse(&s, "other", "2026/27").is_err());
        assert!(parse(&s, "synthetic", "2026").is_err());
        let mut bad = s;
        bad.plan["periods"] = json!(["2026-13"]);
        assert!(parse(&bad, "synthetic", "2026/27").is_err());
    }
    #[test]
    fn exact_chosen_amount_lexeme_notes_and_zero_without_defaults() {
        let s = snapshot();
        let r = Review::prepare(&s, "X", "2027-01", "1,0050", " Reviewed note ").unwrap();
        let text = String::from_utf8(r.bytes).unwrap();
        assert!(text.contains("\"amount\":1.0050"));
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["notes"], "Reviewed note");
        assert!(v.get("key").is_none());
        assert!(v.get("confirm").is_none());
        assert_eq!(
            serde_json::from_slice::<Value>(
                &Review::prepare(&s, "X", "2026-07", "0", "").unwrap().bytes
            )
            .unwrap()["amount"],
            0
        );
    }
    #[test]
    fn explicit_account_and_server_month_only_archive_and_bad_amount_refused() {
        let s = snapshot();
        for (a, p) in [
            ("", "2026-07"),
            ("invented", "2026-07"),
            ("X", "2026-06"),
            ("X", "2027-07"),
        ] {
            assert!(Review::prepare(&s, a, p, "10", "").is_err());
        }
        for bad in [
            "", "-1", "NaN", "1e3", "1.000,00", "1,2,3", "01", "Infinity",
        ] {
            assert!(Review::prepare(&s, "X", "2026-07", bad, "").is_err());
        }
        let mut archived = s;
        archived.plan["archived"] = json!(true);
        assert!(Review::prepare(&archived, "X", "2026-07", "10", "").is_err());
    }
    #[test]
    fn stale_budget_chart_principal_and_ledger_invalidate_review() {
        let s = snapshot();
        let review = Review::prepare(&s, "X", "2026-07", "10", "").unwrap();
        assert!(review.current(&s));
        for key in ["principal", "ledger", "budget", "chart", "year"] {
            let mut n = s.clone();
            match key {
                "principal" => n.principal = "other".into(),
                "ledger" => n.ledger = "other".into(),
                "budget" => n.plan["lines"] = json!([{"id":9}]),
                "chart" => n.chart["accounts"] = json!([]),
                _ => n.plan["selectedYear"] = json!("other"),
            };
            assert!(!review.current(&n));
        }
    }
    #[test]
    fn receipt_requires_identity_and_server_rounded_revision_readback() {
        let s = snapshot();
        let r = Review::prepare(&s, "X", "2026-07", "1.005", "note").unwrap();
        let value =
            json!({"ok":true,"budget":{"id":8,"accountNo":"X","period":"2026-07","amount":1.01}});
        let receipt = r.receipt(&value).unwrap();
        let mut now = s;
        now.plan["lines"] =
            json!([{"id":8,"accountNo":"X","period":"2026-07","amount":1.01,"notes":"note"}]);
        assert!(r.readback(&now, receipt));
        now.plan["lines"][0]["id"] = json!(9);
        assert!(!r.readback(&now, receipt));
        let mut wrong = value;
        wrong["budget"]["accountNo"] = json!("other");
        assert!(r.receipt(&wrong).is_none());
    }
    #[test]
    fn marker_remote_origin_and_local_identity_rebind_are_scoped_without_payload() {
        let s = snapshot();
        let record = s.marker("https://one.invalid", "synthetic", None);
        assert!(valid_record(&record));
        assert!(!s.matches_marker(&record, "https://two.invalid", "synthetic", None));
        let local = s.marker(
            "http://127.0.0.1:1111",
            "synthetic",
            Some("synthetic-local"),
        );
        assert!(s.matches_marker(
            &local,
            "http://127.0.0.1:2222",
            "synthetic",
            Some("synthetic-local")
        ));
        assert!(!s.matches_marker(&local, "http://127.0.0.1:2222", "synthetic", Some("other")));
        let mut other = s;
        other.plan["selectedYear"] = json!("other");
        assert!(!other.matches_marker(&record, "https://one.invalid", "synthetic", None));
        let mut payload = record;
        payload["amount"] = json!(99);
        assert!(!valid_record(&payload));
    }
    #[test]
    fn exact_dimension_option_uses_ids_and_null_member_without_label_splitting() {
        let v = json!({"value":"a:b:c","label":"Human label: not selector","dimensionId":"a:b","memberId":"c"});
        let selection = Selection::from_option(&v).unwrap();
        assert_eq!(selection.dimension.as_deref(), Some("a:b"));
        assert_eq!(selection.member.as_deref(), Some("c"));
        assert_eq!(
            Selection::from_option(&json!({"dimensionId":"a:b","memberId":null}))
                .unwrap()
                .member,
            None
        );
        assert!(Selection::from_option(&json!({"label":"guess"})).is_err());
    }
    #[test]
    fn durable_attempt_blocks_repeat_and_manual_archive_never_stores_amount_or_notes() {
        let path = std::env::temp_dir().join(format!(
            "rm-budget-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        let marker = snapshot().marker("https://one.invalid", "synthetic", None);
        begin_at(&path, &marker).unwrap();
        assert!(begin_at(&path, &marker).is_err());
        assert_eq!(load_at(&path).unwrap(), Some(marker.clone()));
        let mut wrong = marker.clone();
        wrong["ledger"] = json!("other");
        assert!(finish_at(&path, &wrong, true).is_err());
        finish_at(&path, &marker, true).unwrap();
        assert!(load_at(&path).unwrap().is_none());
        let archive = fs::read_dir(&path).unwrap().next().unwrap().unwrap().path();
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(archive).unwrap()).unwrap(),
            marker
        );
        fs::remove_dir_all(path).unwrap();
    }
}
