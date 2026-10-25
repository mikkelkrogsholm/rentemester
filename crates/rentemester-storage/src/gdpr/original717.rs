//! Source GDPR contracts using the same subject/retention/tombstone custody as native GDPR.
use super::*;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubjectInput {
    pub cvr: Option<String>,
    pub name: Option<String>,
    pub as_of: Option<String>,
    #[serde(default)]
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditInput {
    pub since: Option<String>,
    pub until: Option<String>,
    pub as_of: Option<String>,
    #[serde(default)]
    pub sign_with_ed25519: bool,
}
fn by_table() -> Value {
    json!({"customers":0,"vendors":0,"documents":0,"bank_transactions":0,"journal_entries":0,"journal_lines":0,"audit_log":0})
}
pub fn subject(
    tx: &Transaction<'_>,
    slug: &str,
    i: &SubjectInput,
    today: &str,
    actor: &str,
    discovery: bool,
) -> Result<Value, Error> {
    let subject = json!({"cvr":trimmed(i.cvr.as_deref()),"name":trimmed(i.name.as_deref())});
    let as_of = trimmed(i.as_of.as_deref()).unwrap_or_else(|| today.into());
    let mut failure = if discovery {
        json!({"ok":false,"subject":subject,"rows":[],"byTable":by_table(),"errors":[]})
    } else {
        json!({"ok":false,"asOf":as_of,"subject":subject,"records":[],"appliedRules":["GDPR-SUBJECT-EXPORT"],"errors":[]})
    };
    if !i.confirm {
        failure["errors"] = json!(["confirmation required"]);
        failure["code"] = json!("CONFIRM_REQUIRED");
        return Ok(failure);
    }
    crate::validate(tx)?;
    let input = Input {
        cvr: i.cvr.clone(),
        name: i.name.clone(),
        as_of: Some(as_of.clone()),
        confirm: true,
    };
    // Business refusals are source outcomes; storage failures remain failures.
    if !discovery && Date::parse(&as_of).is_err() {
        failure["errors"] = json!(["asOf must be a valid YYYY-MM-DD date"]);
        return Ok(failure);
    }
    let s = match scope(tx, &input) {
        Ok(s) => s,
        Err(e) if e.code == "gdpr_refused" => {
            failure["errors"] = json!([e.message]);
            return Ok(failure);
        }
        Err(e) => return Err(e),
    };
    if !discovery {
        return Ok(export(tx, slug, &input, today, actor)?["export"].clone());
    }
    let rs = collect(tx, &s)?;
    let erasures = erased(tx, &s)?;
    strengthen(tx, &s, &rs, &erasures)?;
    let mut counts = by_table();
    let result=rs.iter().map(|r|{
        let fields=erasures.get(&(r.source,r.id)); let mut personal=r.personal.clone(); let mut label=r.label.clone();
        if let Some(f)=fields {if f.contains("name"){personal.name=Some("[redigeret — GDPR]".into());if label.is_some(){label=personal.name.clone()}}if f.contains("address"){personal.address=None}if f.contains("email"){personal.email=None}if f.contains("vatOrCvr"){personal.vat_or_cvr=None}}
        counts[r.source]=json!(counts[r.source].as_u64().unwrap()+1);
        json!({"source":r.source,"sourceRowId":r.id,"label":label,"personalData":personal,"retainUntil":r.deadline,"erased":fields.is_some_and(|f|!f.is_empty())})
    }).collect::<Vec<_>>();
    let tables = counts
        .as_object()
        .unwrap()
        .values()
        .filter(|n| n.as_u64().unwrap() > 0)
        .count();
    audit(
        tx,
        "gdpr_discover",
        &s,
        &format!(
            "GDPR discover {}: {} row(s) across {tables} table(s)",
            s.reference,
            result.len()
        ),
        actor,
    )?;
    Ok(
        json!({"ok":true,"subject":subject,"rows":result,"byTable":counts,"errors":[],"coverage":coverage()}),
    )
}
/// Read-only: no audit append and no alias strengthening.
pub fn audit_events(tx: &Transaction<'_>, i: &AuditInput, today: &str) -> Result<Value, Error> {
    crate::validate(tx)?;
    let as_of = trimmed(i.as_of.as_deref()).unwrap_or_else(|| today.into());
    let since = trimmed(i.since.as_deref());
    let until = trimmed(i.until.as_deref());
    let mut errors = vec![];
    for (name, date) in [
        ("asOf", Some(as_of.as_str())),
        ("since", since.as_deref()),
        ("until", until.as_deref()),
    ] {
        if date.is_some_and(|d| Date::parse(d).is_err()) {
            errors.push(format!("{name} must be a valid YYYY-MM-DD date"))
        }
    }
    if errors.is_empty()
        && since
            .as_ref()
            .zip(until.as_ref())
            .is_some_and(|(s, u)| s > u)
    {
        errors.push("since must be on or before until".into())
    }
    let events = if errors.is_empty() {
        rows(
            tx,
            "SELECT id,created_at AS occurredAt,event_type AS eventType,entity_id AS subjectKey,actor,message FROM audit_log WHERE substr(event_type,1,5)='gdpr_' AND (?1 IS NULL OR substr(created_at,1,10)>=?1) AND (?2 IS NULL OR substr(created_at,1,10)<=?2) ORDER BY id",
            params![since, until],
        )?
    } else {
        vec![]
    };
    Ok(
        json!({"ok":errors.is_empty(),"asOf":as_of,"since":since,"until":until,"events":events,"errors":errors}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original717_privacy_subject_effects_and_readonly_audit() {
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        let mut company = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic Privacy")
            .unwrap();
        company.execute_batch("INSERT INTO customers(name,vat_or_cvr,email)VALUES('Synthetic Subject','SYN-PRIVACY','subject@example.invalid');").unwrap();
        let tx = company.transaction().unwrap();
        let input: SubjectInput =
            serde_json::from_value(json!({"cvr":"SYN-PRIVACY","confirm":true})).unwrap();
        let discover = subject(&tx, "synthetic", &input, "2026-01-01", "synthetic", true).unwrap();
        assert_eq!(discover["ok"], true);
        assert_eq!(discover["byTable"]["customers"], 1);
        assert_eq!(
            discover["rows"][0]["personalData"]["vatOrCvr"],
            "SYN-PRIVACY"
        );
        assert!(discover.get("records").is_none());
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM audit_log WHERE event_type='gdpr_export'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM audit_log WHERE event_type='gdpr_discover'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        let export = subject(&tx, "synthetic", &input, "2026-01-01", "synthetic", false).unwrap();
        assert_eq!(export["ok"], true);
        assert!(export["records"].is_array());
        assert_eq!(export["asOf"], "2026-01-01");
        let audit: AuditInput = serde_json::from_value(json!({})).unwrap();
        let before = tx.total_changes();
        let report = audit_events(&tx, &audit, "2026-01-01").unwrap();
        assert_eq!(report["events"].as_array().unwrap().len(), 2);
        assert_eq!(tx.total_changes(), before);
        let no: SubjectInput = serde_json::from_value(json!({"cvr":"SYN-PRIVACY"})).unwrap();
        assert_eq!(
            subject(&tx, "synthetic", &no, "2026-01-01", "synthetic", true).unwrap()["code"],
            "CONFIRM_REQUIRED"
        );
        let mismatch: SubjectInput = serde_json::from_value(
            json!({"cvr":"SYN-PRIVACY","name":"Other Subject","confirm":true}),
        )
        .unwrap();
        assert_eq!(
            subject(&tx, "synthetic", &mismatch, "2026-01-01", "synthetic", true).unwrap()["ok"],
            false
        );
        assert_eq!(tx.total_changes(), before);
        assert!(serde_json::from_value::<SubjectInput>(json!({"confirm":"true"})).is_err());
        assert!(serde_json::from_value::<AuditInput>(json!({"confirm":true})).is_err());
        assert_eq!(
            tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
