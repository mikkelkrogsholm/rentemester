//! Subject-scoped overlay only. Protected originals and journal/audit bytes are never rewritten.
use crate::{invalid, retention};
use rentemester_engine::{Error, fiscal_year::Date};
use rusqlite::{Transaction, params, types::ValueRef};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
const ERASURE: &str = "GDPR-RETENTION-BOUNDED-ERASURE";
fn refused(message: &str) -> Error {
    Error::new("gdpr_refused", message)
}
fn hash(s: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(s.as_bytes()))
}
fn trimmed(v: Option<&str>) -> Option<String> {
    v.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
pub(crate) fn rows(
    tx: &Transaction<'_>,
    sql: &str,
    p: impl rusqlite::Params,
) -> Result<Vec<Value>, Error> {
    let mut q = tx.prepare(sql).map_err(invalid)?;
    let names = q
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut rs = q.query(p).map_err(invalid)?;
    let mut out = vec![];
    while let Some(r) = rs.next().map_err(invalid)? {
        let mut o = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            o.insert(
                name.clone(),
                match r.get_ref(i).map_err(invalid)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Text(s) => json!(std::str::from_utf8(s).map_err(invalid)?),
                    ValueRef::Integer(n) if n.unsigned_abs() <= 9_007_199_254_740_991 => json!(n),
                    _ => return Err(invalid("unsupported GDPR evidence storage")),
                },
            );
        }
        out.push(Value::Object(o));
    }
    Ok(out)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub cvr: Option<String>,
    pub name: Option<String>,
    pub as_of: Option<String>,
    pub confirm: bool,
}
struct Scope {
    cvr: Option<String>,
    name: Option<String>,
    terms: BTreeSet<String>,
    reference: String,
    references: BTreeSet<String>,
    tombstones: BTreeSet<String>,
    keys: BTreeSet<String>,
}
const PARTIES: &str = "SELECT 'customers' AS source,id AS row_id,name,vat_or_cvr AS cvr FROM customers UNION ALL SELECT 'vendors',id,name,vat_or_cvr FROM vendors UNION ALL SELECT 'documents:sender',id,sender_name,sender_vat_cvr FROM documents UNION ALL SELECT 'documents:recipient',id,recipient_name,recipient_vat_cvr FROM documents";
fn scope(tx: &Transaction<'_>, i: &Input) -> Result<Scope, Error> {
    let cvr = trimmed(i.cvr.as_deref());
    let name = trimmed(i.name.as_deref());
    if cvr.is_none() && name.is_none() {
        return Err(refused("a GDPR subject must be identified by cvr or name"));
    }
    if cvr.as_ref().is_some_and(|v| v.len() > 512) || name.as_ref().is_some_and(|v| v.len() > 1000)
    {
        return Err(refused("subject identity exceeds supported length"));
    }
    let all = rows(tx, PARTIES, [])?;
    let parties = all
        .iter()
        .filter(|r| {
            cvr.as_deref().is_none_or(|c| r["cvr"] == c)
                && name.as_deref().is_none_or(|n| r["name"] == n)
        })
        .collect::<Vec<_>>();
    if cvr.is_some() && name.is_some() && parties.is_empty() {
        return Err(refused("cvr and name must identify the same GDPR subject"));
    }
    let identities = parties
        .iter()
        .filter_map(|r| {
            trimmed(r["cvr"].as_str())
                .map(|c| format!("cvr:{c}"))
                .or_else(|| {
                    r["name"]
                        .as_str()
                        .map(|_| format!("row:{}:{}", r["source"].as_str().unwrap(), r["row_id"]))
                })
        })
        .collect::<BTreeSet<_>>();
    if cvr.is_none() && name.is_some() && identities.len() > 1 {
        return Err(refused(
            "name matches multiple GDPR subjects; supply cvr to disambiguate",
        ));
    }
    let mut names = BTreeSet::new();
    if let Some(n) = &name {
        names.insert(n.clone());
    }
    for p in &parties {
        if let Some(n) = trimmed(p["name"].as_str()) {
            names.insert(n);
        }
    }
    let mut safe = BTreeSet::new();
    for n in &names {
        let matching = all.iter().filter(|p| p["name"] == *n).collect::<Vec<_>>();
        let cvrs = matching
            .iter()
            .filter_map(|p| trimmed(p["cvr"].as_str()))
            .collect::<BTreeSet<_>>();
        let no_cvr = matching
            .iter()
            .filter(|p| trimmed(p["cvr"].as_str()).is_none())
            .map(|p| format!("{}:{}", p["source"], p["row_id"]))
            .collect::<BTreeSet<_>>();
        let conflict = cvr
            .as_ref()
            .map(|c| cvrs.iter().any(|v| v != c))
            .unwrap_or(cvrs.len() > 1);
        if !conflict && no_cvr.len() <= 1 {
            safe.insert(n.clone());
        }
    }
    let canonical = cvr.as_ref().map(|c| format!("cvr:{c}")).unwrap_or_else(|| {
        if identities.len() == 1 {
            identities.first().unwrap().clone()
        } else {
            format!("name:{}", name.as_deref().unwrap_or("unknown"))
        }
    });
    let reference = hash(&if canonical.starts_with("row:") && name.is_some() {
        format!("name:{}", name.as_deref().unwrap())
    } else {
        canonical.clone()
    });
    let mut aliases = BTreeSet::from([canonical.clone()]);
    let mut tombstones = BTreeSet::from([canonical]);
    let mut terms = safe.clone();
    if let Some(c) = &cvr {
        terms.insert(c.clone());
    }
    for n in safe {
        aliases.insert(format!("name:{n}"));
        tombstones.insert(format!("name:{n}"));
    }
    for p in parties {
        let c = trimmed(p["cvr"].as_str());
        if let Some(c) = &c {
            aliases.insert(format!("cvr:{c}"));
        }
        if ["customers", "vendors"].contains(&p["source"].as_str().unwrap()) || c.is_none() {
            let r = format!("row:{}:{}", p["source"].as_str().unwrap(), p["row_id"]);
            aliases.insert(r.clone());
            tombstones.insert(r);
        }
    }
    let references = aliases.iter().map(|s| hash(s)).collect::<BTreeSet<_>>();
    let keys = references
        .iter()
        .cloned()
        .chain(terms.iter().cloned())
        .chain(names)
        .collect();
    Ok(Scope {
        cvr,
        name,
        terms,
        reference,
        references,
        tombstones: tombstones.iter().map(|s| hash(s)).collect(),
        keys,
    })
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Personal {
    name: Option<String>,
    address: Option<String>,
    email: Option<String>,
    vat_or_cvr: Option<String>,
}
#[derive(Clone)]
struct Raw {
    source: &'static str,
    id: i64,
    label: Option<String>,
    personal: Personal,
    deadline: Option<String>,
    redactable: bool,
    unknown: bool,
}
fn value(r: &Value, k: &str) -> Option<String> {
    r[k].as_str().map(str::to_owned)
}
fn raw(source: &'static str, r: &Value) -> Raw {
    Raw {
        source,
        id: r["id"].as_i64().unwrap(),
        label: value(r, "label"),
        personal: Personal {
            name: value(r, "name"),
            address: value(r, "address"),
            email: value(r, "email"),
            vat_or_cvr: value(r, "cvr"),
        },
        deadline: None,
        redactable: !["journal_entries", "journal_lines"].contains(&source),
        unknown: false,
    }
}
fn collect(tx: &Transaction<'_>, s: &Scope) -> Result<Vec<Raw>, Error> {
    let mut books = BTreeMap::new();
    let documents = rows(
        tx,
        "SELECT id,document_no AS label,sender_name,sender_address,sender_vat_cvr,recipient_name,recipient_address,recipient_vat_cvr,retain_until,COALESCE(invoice_date,substr(upload_datetime,1,10)) AS basis FROM documents WHERE ((?1 IS NULL OR sender_vat_cvr=?1)AND(?2 IS NULL OR sender_name=?2))OR((?1 IS NULL OR recipient_vat_cvr=?1)AND(?2 IS NULL OR recipient_name=?2)) ORDER BY id",
        params![s.cvr, s.name],
    )?;
    for r in documents {
        let sender = s.cvr.as_deref().is_none_or(|c| r["sender_vat_cvr"] == c)
            && s.name.as_deref().is_none_or(|n| r["sender_name"] == n);
        let prefix = if sender { "sender" } else { "recipient" };
        let mut row = raw("documents", &r);
        row.personal = Personal {
            name: value(&r, &format!("{prefix}_name")),
            address: value(&r, &format!("{prefix}_address")),
            email: None,
            vat_or_cvr: value(&r, &format!("{prefix}_vat_cvr")),
        };
        row.deadline = retention::effective(tx, r["retain_until"].as_str(), r["basis"].as_str())?;
        row.unknown = row.deadline.is_none();
        books.insert((row.source, row.id), row);
    }
    for term in &s.terms {
        let like = format!(
            "%{}%",
            term.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        for (source, sql) in [
            (
                "bank_transactions",
                "SELECT id,reference AS label,text AS name,retain_until,COALESCE(booking_date,transaction_date)AS basis FROM bank_transactions WHERE text LIKE ?1 ESCAPE '\\' ORDER BY id",
            ),
            (
                "journal_entries",
                "SELECT id,entry_no AS label,text AS name,retain_until,transaction_date AS basis FROM journal_entries WHERE text LIKE ?1 ESCAPE '\\' ORDER BY id",
            ),
            (
                "journal_lines",
                "SELECT jl.id,je.entry_no AS label,jl.text AS name,je.retain_until,je.transaction_date AS basis FROM journal_lines jl JOIN journal_entries je ON je.id=jl.journal_entry_id WHERE jl.text LIKE ?1 ESCAPE '\\' ORDER BY jl.id",
            ),
        ] {
            for r in rows(tx, sql, [&like])? {
                let mut row = raw(source, &r);
                row.deadline =
                    retention::effective(tx, r["retain_until"].as_str(), r["basis"].as_str())?;
                row.unknown = row.deadline.is_none();
                books.entry((source, row.id)).or_insert(row);
            }
        }
        for r in rows(
            tx,
            "SELECT id,event_type AS label,message AS name,entity_id FROM audit_log WHERE message LIKE ?1 ESCAPE '\\' OR entity_id=?2 ORDER BY id",
            params![like, term],
        )? {
            let mut row = raw("audit_log", &r);
            if s.cvr.as_deref().is_some_and(|c| r["entity_id"] == c) {
                row.personal.vat_or_cvr = s.cvr.clone();
            }
            books.entry((row.source, row.id)).or_insert(row);
        }
    }
    for reference in &s.references {
        for r in rows(
            tx,
            "SELECT id,event_type AS label,message AS name FROM audit_log WHERE entity_id=?1 ORDER BY id",
            [reference],
        )? {
            let mut row = raw("audit_log", &r);
            row.redactable = !r["label"].as_str().unwrap().starts_with("gdpr_")
                || s.terms
                    .iter()
                    .any(|t| r["name"].as_str().unwrap().contains(t));
            books.entry((row.source, row.id)).or_insert(row);
        }
    }
    let deadline = books.values().filter_map(|r| r.deadline.clone()).max();
    let unknown = books.values().any(|r| r.unknown);
    for row in books.values_mut().filter(|r| r.source == "audit_log") {
        row.deadline = deadline.clone();
        row.unknown = unknown;
    }
    let mut out = vec![];
    for (source, sql) in [
        (
            "customers",
            "SELECT id,name AS label,name,address,email,vat_or_cvr AS cvr FROM customers WHERE (?1 IS NULL OR vat_or_cvr=?1)AND(?2 IS NULL OR name=?2)ORDER BY id",
        ),
        (
            "vendors",
            "SELECT id,name AS label,name,address,vat_or_cvr AS cvr FROM vendors WHERE (?1 IS NULL OR vat_or_cvr=?1)AND(?2 IS NULL OR name=?2)ORDER BY id",
        ),
    ] {
        for r in rows(tx, sql, params![s.cvr, s.name])? {
            let mut row = raw(source, &r);
            row.deadline = deadline.clone();
            row.unknown = unknown;
            out.push(row);
        }
    }
    out.extend(books.into_values());
    Ok(out)
}
type Erasures = BTreeMap<(&'static str, i64), BTreeSet<String>>;
fn source(v: &str) -> Option<&'static str> {
    [
        "customers",
        "vendors",
        "documents",
        "bank_transactions",
        "journal_entries",
        "journal_lines",
        "audit_log",
    ]
    .into_iter()
    .find(|s| *s == v)
}
fn erased(tx: &Transaction<'_>, s: &Scope) -> Result<Erasures, Error> {
    let mut out = Erasures::new();
    for key in &s.keys {
        for r in rows(
            tx,
            "SELECT source,source_row_id,redacted_fields FROM gdpr_erasures WHERE subject_key=?1",
            [key],
        )? {
            let Some(source) = r["source"].as_str().and_then(source) else {
                continue;
            };
            let fields: Vec<String> = serde_json::from_str(r["redacted_fields"].as_str().unwrap())
                .map_err(|_| refused("existing erasure evidence cannot be interpreted"))?;
            out.entry((source, r["source_row_id"].as_i64().unwrap()))
                .or_default()
                .extend(fields);
        }
    }
    Ok(out)
}
fn tombstone(
    tx: &Transaction<'_>,
    s: &Scope,
    r: &Raw,
    fields: &BTreeSet<String>,
    reason: &str,
) -> Result<(), Error> {
    for alias in &s.tombstones {
        tx.execute("INSERT OR IGNORE INTO gdpr_erasures(subject_key,source,source_row_id,redacted_fields,rule_id,reason,retained_until_at_erasure)VALUES(?1,?2,?3,?4,?5,?6,?7)",params![alias,r.source,r.id,json!(fields).to_string(),ERASURE,reason,r.deadline]).map_err(invalid)?;
    }
    Ok(())
}
fn strengthen(
    tx: &Transaction<'_>,
    s: &Scope,
    rs: &[Raw],
    erasures: &Erasures,
) -> Result<(), Error> {
    for r in rs {
        let Some(fields) = erasures.get(&(r.source, r.id)).filter(|f| !f.is_empty()) else {
            continue;
        };
        if !r.redactable {
            continue;
        }
        if s.cvr.as_ref().is_some_and(|c| {
            r.personal.vat_or_cvr.as_ref() != Some(c)
                && !r.personal.name.as_ref().is_some_and(|n| n.contains(c))
                && !r.label.as_ref().is_some_and(|n| n.contains(c))
        }) {
            continue;
        }
        tombstone(
            tx,
            s,
            r,
            fields,
            "pseudonymous identity alias linked to a prior GDPR erasure",
        )?;
    }
    Ok(())
}
fn audit(
    tx: &Transaction<'_>,
    event: &str,
    s: &Scope,
    message: &str,
    actor: &str,
) -> Result<(), Error> {
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES(?1,'gdpr_subject',?2,?3,?4)",params![event,s.reference,message,format!("user:{actor}")]).map_err(invalid)?;
    Ok(())
}
fn coverage() -> Value {
    json!({"complete":false,"notice":"Denne rapport dækker kunders og leverandørers navn, adresse, e-mail og CVR samt matchende bilags-, bank-, journal- og audittekst. Originalfiler, øvrige kontaktfelter, sikkerhedskopier og andre produktvisninger anonymiseres ikke. Journaltekst bevares af hensyn til integritet. Vedligeholdelse af identiteter efter anonymisering er fortsat begrænset på denne server."})
}
pub fn export(
    tx: &Transaction<'_>,
    slug: &str,
    i: &Input,
    today: &str,
    actor: &str,
) -> Result<Value, Error> {
    crate::validate(tx)?;
    if !i.confirm {
        return Err(refused("confirmation required"));
    }
    let as_of = i.as_of.as_deref().unwrap_or(today);
    Date::parse(as_of)?;
    let s = scope(tx, i)?;
    let rs = collect(tx, &s)?;
    let erasures = erased(tx, &s)?;
    strengthen(tx, &s, &rs, &erasures)?;
    let records=rs.iter().map(|r|{let fields=erasures.get(&(r.source,r.id));let mut personal=r.personal.clone();let mut label=r.label.clone();if let Some(f)=fields{if f.contains("name"){personal.name=Some("[redigeret — GDPR]".into());if label.is_some(){label=personal.name.clone();}}
if f.contains("address"){personal.address=None}
if f.contains("email"){personal.email=None}
if f.contains("vatOrCvr"){personal.vat_or_cvr=None}}
 let retained=r.unknown||r.deadline.as_deref().is_some_and(|d|d>=as_of);json!({"source":r.source,"sourceRowId":r.id,"label":label,"personalData":personal,"retainUntil":r.deadline,"retentionUnknown":r.unknown,"underRetention":retained,"erased":fields.is_some_and(|f|!f.is_empty()),"erasable":!retained&&fields.is_none()&&r.redactable})}).collect::<Vec<_>>();
    audit(
        tx,
        "gdpr_export",
        &s,
        &format!(
            "GDPR export {}: {} record(s) returned (as-of {as_of})",
            s.reference,
            records.len()
        ),
        actor,
    )?;
    Ok(
        json!({"slug":slug,"company":crate::company_setup::profile(tx)?,"export":{"ok":true,"asOf":as_of,"appliedRules":["GDPR-SUBJECT-EXPORT",retention::RULE],"subject":{"cvr":s.cvr,"name":s.name},"records":records,"errors":[],"coverage":coverage()}}),
    )
}
pub fn erase(tx: &Transaction<'_>, i: &Input, today: &str, actor: &str) -> Result<Value, Error> {
    crate::validate(tx)?;
    Date::parse(today)?;
    if !i.confirm {
        return Err(refused("confirmation required"));
    }
    if i.as_of.is_some() {
        return Err(refused(
            "erasure does not accept asOf; retention uses the trusted current date",
        ));
    }
    let s = scope(tx, i)?;
    let rs = collect(tx, &s)?;
    let existing = erased(tx, &s)?;
    let (mut erased, mut refused_rows, mut already) = (vec![], vec![], 0);
    for r in rs {
        if r.source == "audit_log" && !r.redactable {
            continue;
        }
        let decision = if let Some(fields) = existing.get(&(r.source, r.id)) {
            tombstone(
                tx,
                &s,
                &r,
                fields,
                "pseudonymous identity alias linked to a prior GDPR erasure",
            )?;
            already += 1;
            "already_erased"
        } else if r.unknown || r.deadline.as_deref().is_some_and(|d| d >= today) {
            refused_rows.push(json!({"source":r.source,"sourceRowId":r.id,"label":r.label,"retainUntil":r.deadline,"reason":if r.unknown{"retention deadline cannot be established; erasure refused".to_owned()}else{format!("bookkeeping retention requires this record until {}; erasure refused (rule {})",r.deadline.as_deref().unwrap(),retention::RULE)}}));
            "refused_retention"
        } else if !r.redactable {
            refused_rows.push(json!({"source":r.source,"sourceRowId":r.id,"label":r.label,"retainUntil":r.deadline,"reason":"append-only journal integrity prevents erasure; a product-wide redaction overlay is required"}));
            "refused_ledger_integrity"
        } else {
            let fields = match r.source {
                "customers" => vec!["name", "address", "email", "vatOrCvr"],
                "vendors" | "documents" => vec!["name", "address", "vatOrCvr"],
                "audit_log" => vec!["name", "vatOrCvr"],
                _ => vec!["name"],
            }
            .into_iter()
            .map(str::to_owned)
            .collect();
            tombstone(
                tx,
                &s,
                &r,
                &fields,
                &format!(
                    "personal data redacted: no longer under bookkeeping retention as of {today}"
                ),
            )?;
            erased.push(json!({"source":r.source,"sourceRowId":r.id,"label":r.label,"redactedFields":fields}));
            "erased"
        };
        audit(
            tx,
            "gdpr_erasure_decision",
            &s,
            &format!(
                "GDPR erasure decision {}: source={} row={} outcome={decision}",
                s.reference, r.source, r.id
            ),
            actor,
        )?;
    }
    if erased.is_empty() && refused_rows.is_empty() && already == 0 {
        audit(
            tx,
            "gdpr_erasure_decision",
            &s,
            &format!(
                "GDPR erasure decision {}: outcome=no_matching_records",
                s.reference
            ),
            actor,
        )?;
    }
    Ok(
        json!({"ok":true,"asOf":today,"subject":{"cvr":s.cvr,"name":s.name},"appliedRules":[ERASURE,retention::RULE],"erasedCount":erased.len(),"refusedCount":refused_rows.len(),"alreadyErasedCount":already,"erased":erased,"refused":refused_rows,"errors":[],"coverage":coverage()}),
    )
}
#[cfg(test)]
#[path = "gdpr_tests.rs"]
mod tests;
