//! Reviewed planning evidence. Never creates invoices, payables, payments or journals.
mod evidence;
mod forecast;
#[cfg(test)]
mod tests;
use crate::{invalid, legacy_money};
pub use evidence::{Match, evidence_document, match_occurrence};
use rentemester_engine::{
    Error,
    calendar::valid_iso_date,
    document::money,
    fiscal_year::Date,
    money::Money,
    sales_invoice::{text, wire},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value, json};
use sha2::{Digest, Sha256};
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub commitment_id: String,
    pub vendor_party_id: String,
    pub r#type: String,
    pub description: String,
    pub business_purpose: String,
    pub amount: Number,
    pub currency: String,
    pub frequency: String,
    pub next_date: String,
    pub start_date: Option<String>,
    pub renewal_date: Option<String>,
    pub notice_date: Option<String>,
    pub end_date: Option<String>,
    pub evidence_refs: Vec<String>,
    pub vat_proposal: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Plan {
    pub commitment: Input,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Apply {
    pub commitment: Input,
    pub payload_hash: String,
    pub confirm: bool,
    pub idempotency_key: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Change {
    pub commitment_id: String,
    pub action: String,
    pub reason: String,
    pub confirm: bool,
    pub expected_payload_hash: String,
}
fn refused(message: impl Into<String>) -> Error {
    Error::new("supplier_commitment_refused", message)
}
fn canonical(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort();
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|k| format!("{}:{}", json!(k), canonical(&m[k])))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        _ => v.to_string(),
    }
}
fn hash(v: &Value) -> String {
    format!("{:x}", Sha256::digest(canonical(v).as_bytes()))
}
fn add_days(date: &str, days: i64) -> Result<String, Error> {
    let d = rentemester_engine::vat_period::date(date)?
        .checked_add(time::Duration::days(days))
        .ok_or_else(|| refused("calendar horizon exceeded"))?
        .to_string();
    Date::parse(&d)?;
    Ok(d)
}
fn occurrence(first: &str, frequency: &str, index: u32) -> Result<String, Error> {
    Date::parse(first)?;
    let step = match frequency {
        "weekly" => return add_days(first, i64::from(index) * 7),
        "monthly" => 1,
        "quarterly" => 3,
        "yearly" => 12,
        _ => return Err(refused("unsupported frequency")),
    };
    let target = first[..4].parse::<u32>().unwrap() * 12 + first[5..7].parse::<u32>().unwrap() - 1
        + index * step;
    if target / 12 > 9999 {
        return Err(refused("calendar horizon exceeded"));
    }
    let mut day = first[8..].parse::<u32>().unwrap();
    loop {
        let date = format!("{:04}-{:02}-{day:02}", target / 12, target % 12 + 1);
        if valid_iso_date(&date) {
            return Ok(date);
        }
        day -= 1;
    }
}
fn nearby_index(first: &str, frequency: &str, target: &str) -> Result<u32, Error> {
    Date::parse(first)?;
    Date::parse(target)?;
    if target <= first {
        return Ok(0);
    }
    if frequency == "weekly" {
        return Ok(((rentemester_engine::vat_period::date(target)?
            - rentemester_engine::vat_period::date(first)?)
        .whole_days()
            / 7) as u32);
    }
    let step = match frequency {
        "monthly" => 1,
        "quarterly" => 3,
        "yearly" => 12,
        _ => return Err(refused("unsupported cadence")),
    };
    let months = (target[..4].parse::<u32>().unwrap() * 12 + target[5..7].parse::<u32>().unwrap())
        - (first[..4].parse::<u32>().unwrap() * 12 + first[5..7].parse::<u32>().unwrap());
    Ok((months / step).saturating_sub(1))
}
fn latest(tx: &Transaction<'_>, id: &str) -> Result<Option<Value>, Error> {
    let row:Option<(i64,String,String,String,String)>=tx.query_row("SELECT id,event_type,payload_json,payload_hash,created_at FROM supplier_commitment_events WHERE commitment_id=?1 ORDER BY id DESC LIMIT 1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(invalid)?;
    row.map(|(id, event, payload, hash, at)| {
        let payload: Value = serde_json::from_str(&payload).map_err(invalid)?;
        Ok(json!({"id":id,"event":event,"payload":payload,"payloadHash":hash,"createdAt":at}))
    })
    .transpose()
}
pub fn source_documents(input: &Input) -> Result<Vec<i64>, Error> {
    if input.evidence_refs.is_empty() || input.evidence_refs.len() > 20 {
        return Err(refused("select1–20 original source documents"));
    }
    input
        .evidence_refs
        .iter()
        .map(|s| {
            s.strip_prefix("document:")
                .and_then(|n| n.parse::<i64>().ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| refused("native source reference must be document:<positive id>"))
        })
        .collect()
}
pub fn plan(tx: &Transaction<'_>, input: &Input) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let id = text(&input.commitment_id, 96)?;
    if id != input.commitment_id || id.len() < 3 {
        return Err(refused("commitment id must have3–96 characters"));
    }
    let vendor_id=input.vendor_party_id.strip_prefix("vendor:").and_then(|s|s.parse::<i64>().ok()).filter(|n|*n>0).ok_or_else(||refused("select an existing ledger-local vendor:<id>; no cross-workspace equivalence implied"))?;
    let vendor:Value=tx.query_row("SELECT id,name,vat_or_cvr,country_code,identifier_kind FROM vendors WHERE id=?1 AND archived=0",[vendor_id],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"name":r.get::<_,String>(1)?,"vatOrCvr":r.get::<_,Option<String>>(2)?,"countryCode":r.get::<_,Option<String>>(3)?,"identifierKind":r.get::<_,Option<String>>(4)?}))).optional().map_err(invalid)?.ok_or_else(||refused("active vendor required"))?;
    let amount = money(&input.amount)?;
    if amount.is_negative() || amount == Money::default() {
        return Err(refused("positive amount in cents required"));
    }
    if input.currency.len() != 3 || !input.currency.bytes().all(|c| c.is_ascii_uppercase()) {
        return Err(refused("uppercase ISO currency code required"));
    }
    Date::parse(&input.next_date)?;
    for date in [
        &input.start_date,
        &input.renewal_date,
        &input.notice_date,
        &input.end_date,
    ]
    .into_iter()
    .flatten()
    {
        Date::parse(date)?;
    }
    if input
        .start_date
        .as_ref()
        .is_some_and(|s| s > &input.next_date)
        || input
            .end_date
            .as_ref()
            .is_some_and(|s| s < &input.next_date)
    {
        return Err(refused("next date must fall within start/end bounds"));
    }
    let mut evidence = vec![];
    for doc in source_documents(input)? {
        let row: Option<(String, String)> = tx
            .query_row(
                "SELECT sha256_hash,COALESCE(original_filename,document_no,'Original') FROM documents WHERE id=?1",
                [doc],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(invalid)?;
        let (hash, name) = row.ok_or_else(|| refused("source original not found"))?;
        evidence.push(json!({"documentId":doc,"sha256":hash,"filename":name}));
    }
    let previous = latest(tx, &id)?;
    if previous.as_ref().is_some_and(|p| p["event"] == "ended") {
        return Err(refused(
            "ended commitment cannot be reactivated; create a new identifier",
        ));
    }
    let mut payload = json!({"commitmentId":id,"vendorPartyId":input.vendor_party_id,"vendorSnapshot":vendor["name"],"vendorProvenance":{"kind":"ledger_vendor","vendor":vendor},"type":text(&input.r#type,256)?,"description":text(&input.description,2000)?,"businessPurpose":text(&input.business_purpose,2000)?,"amount":wire(&amount)?,"currency":input.currency,"frequency":input.frequency,"nextDate":input.next_date,"startDate":input.start_date,"renewalDate":input.renewal_date,"noticeDate":input.notice_date,"endDate":input.end_date,"evidenceRefs":input.evidence_refs,"sourceEvidence":evidence,"vatProposal":input.vat_proposal.as_deref().map(|s|text(s,2000)).transpose()?,"status":"active","requestedInput":input,"previousPayloadHash":previous.map(|p|p["payloadHash"].clone())});
    let mut schedule = vec![];
    let until = add_days(&input.next_date, 90)?;
    for index in 0..=13 {
        let date = occurrence(&input.next_date, &input.frequency, index)?;
        if date > until || input.end_date.as_ref().is_some_and(|e| date > *e) {
            break;
        }
        schedule.push(json!({"date":date,"amount":wire(&amount)?,"currency":input.currency}));
    }
    let digest = hash(&payload);
    payload["payloadHash"] = json!(digest);
    payload["schedule"] = json!(schedule);
    payload["warnings"] = json!([
        "Planlægning alene; opretter ingen faktura, kreditor, betaling eller bogføring. Momsforslag kræver separat bilagskontrol."
    ]);
    Ok(payload)
}
pub fn apply(tx: &Transaction<'_>, input: &Apply, actor: &str, now: &str) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(refused("confirmation required"));
    }
    if let Some(key) = &input.idempotency_key {
        text(key, 160)?;
        let old:Option<(String,String,String)>=tx.query_row("SELECT commitment_id,payload_hash,payload_json FROM supplier_commitment_events WHERE idempotency_key=?1",[key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(invalid)?;
        if let Some((id, hash, payload)) = old {
            let original: Value = serde_json::from_str(&payload).map_err(invalid)?;
            if original["requestedInput"] != json!(input.commitment)
                || id != input.commitment.commitment_id
                || hash != input.payload_hash
            {
                return Err(refused("idempotency conflict"));
            }
            return Ok(json!({"commitmentId":id,"payloadHash":hash,"idempotent":true}));
        }
    }
    let proposal = plan(tx, &input.commitment)?;
    if proposal["payloadHash"] != input.payload_hash {
        return Err(refused(
            "PLAN_HASH_MISMATCH: source or previous revision changed; review a new plan",
        ));
    }
    tx.execute("INSERT INTO supplier_commitment_events(commitment_id,event_type,payload_json,payload_hash,previous_payload_hash,idempotency_key,actor,principal,created_at) VALUES(?1,'approved',?2,?3,?4,?5,?6,?7,?8)",params![input.commitment.commitment_id,canonical(&proposal),input.payload_hash,proposal["previousPayloadHash"].as_str(),input.idempotency_key,format!("user:{actor}"),actor,now]).map_err(invalid)?;
    Ok(
        json!({"commitmentId":input.commitment.commitment_id,"payloadHash":input.payload_hash,"idempotent":false}),
    )
}
pub fn change(
    tx: &Transaction<'_>,
    input: &Change,
    actor: &str,
    now: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm || !["paused", "ended", "superseded"].contains(&input.action.as_str()) {
        return Err(refused("confirmed pause/end/supersede required"));
    }
    let reason = text(&input.reason, 2000)?;
    let previous =
        latest(tx, &input.commitment_id)?.ok_or_else(|| refused("commitment not found"))?;
    if previous["payloadHash"] != input.expected_payload_hash || previous["event"] == "ended" {
        return Err(refused("stale commitment or already ended"));
    }
    let payload = json!({"commitmentId":input.commitment_id,"reason":reason,"previousPayloadHash":input.expected_payload_hash});
    let digest = hash(&payload);
    tx.execute("INSERT INTO supplier_commitment_events(commitment_id,event_type,payload_json,payload_hash,previous_payload_hash,actor,principal,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![input.commitment_id,input.action,canonical(&payload),digest,input.expected_payload_hash,format!("user:{actor}"),actor,now]).map_err(invalid)?;
    Ok(json!({"commitmentId":input.commitment_id,"payloadHash":digest,"status":input.action}))
}
fn rows(tx: &Transaction<'_>, sql: &str) -> Result<Vec<Value>, Error> {
    let mut q = tx.prepare(sql).map_err(invalid)?;
    let records = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })
        .map_err(invalid)?;
    let mut out = vec![];
    for row in records {
        let (id, event, payload, hash, created, actor, principal) = row.map_err(invalid)?;
        let p: Value = serde_json::from_str(&payload).map_err(invalid)?;
        out.push(json!({"commitmentId":id,"event":event,"payload":p,"payloadHash":hash,"createdAt":created,"actor":actor,"principal":principal}));
    }
    Ok(out)
}
pub fn matches(tx: &Transaction<'_>) -> Result<Vec<Value>, Error> {
    let mut q=tx.prepare("SELECT canonical_payload,payload_hash,actor,principal,created_at FROM supplier_commitment_occurrence_matches ORDER BY commitment_id,occurrence_date,id").map_err(invalid)?;
    let mut rs = q.query([]).map_err(invalid)?;
    let mut out = vec![];
    while let Some(r) = rs.next().map_err(invalid)? {
        let raw: String = r.get(0).map_err(invalid)?;
        let mut p: Value = serde_json::from_str(&raw).map_err(invalid)?;
        if p["variance"].is_null() {
            let approval:Option<String>=tx.query_row("SELECT payload_json FROM supplier_commitment_events WHERE commitment_id=?1 AND payload_hash=?2 AND event_type='approved' ORDER BY id DESC LIMIT 1",params![p["commitmentId"].as_str(),p["expected"]["payloadHash"].as_str()],|r|r.get(0)).optional().map_err(invalid)?;
            let approval = approval
                .map(|raw| serde_json::from_str::<Value>(&raw).map_err(invalid))
                .transpose()?;
            p["variance"] = evidence::variance(
                &p["expected"],
                &p["evidence"],
                p["occurrenceDate"]
                    .as_str()
                    .ok_or_else(|| refused("stored match occurrence missing"))?,
                approval.as_ref().and_then(|a| a["vendorPartyId"].as_str()),
            )?;
        }
        p["payloadHash"] = json!(r.get::<_, String>(1).map_err(invalid)?);
        p["actor"] = json!(r.get::<_, String>(2).map_err(invalid)?);
        p["createdAt"] = json!(r.get::<_, String>(4).map_err(invalid)?);
        out.push(p);
    }
    Ok(out)
}
pub fn list(tx: &Transaction<'_>, slug: &str, start: &str) -> Result<Value, Error> {
    list_horizon(tx, slug, start, false)
}
/// Same canonical flow projection, reduced over three complete calendar months.
pub(crate) fn dashboard_forecast(
    tx: &Transaction<'_>,
    slug: &str,
    start: &str,
) -> Result<Value, Error> {
    Ok(list_horizon(tx, slug, start, true)?["forecast"].take())
}
fn list_horizon(
    tx: &Transaction<'_>,
    slug: &str,
    start: &str,
    monthly: bool,
) -> Result<Value, Error> {
    Date::parse(start)?;
    crate::validate(tx)?;
    let history = rows(
        tx,
        "SELECT commitment_id,event_type,payload_json,payload_hash,created_at,actor,principal FROM supplier_commitment_events ORDER BY id",
    )?;
    let approvals = rows(
        tx,
        "SELECT commitment_id,event_type,payload_json,payload_hash,created_at,actor,principal FROM supplier_commitment_events e WHERE event_type='approved' AND id=(SELECT MAX(id) FROM supplier_commitment_events x WHERE x.commitment_id=e.commitment_id AND x.event_type='approved') ORDER BY commitment_id",
    )?;
    let mut commitments = vec![];
    let mut occurrences = vec![];
    let matched = matches(tx)?;
    let until = forecast::intervals(start, monthly)?
        .last()
        .ok_or_else(|| refused("forecast horizon missing"))?
        .1
        .clone();
    let alert_end = add_days(start, 30)?;
    let mut alerts = vec![];
    for row in approvals {
        let id = row["commitmentId"].as_str().unwrap();
        let last = latest(tx, id)?.unwrap();
        let p = &row["payload"];
        let status = if last["event"] == "approved" {
            p["status"].as_str().unwrap_or("active")
        } else {
            last["event"].as_str().unwrap()
        };
        commitments.push(json!({"commitmentId":id,"payloadHash":last["payloadHash"],"createdAt":row["createdAt"],"vendor":p["vendorSnapshot"].as_str().unwrap_or(p["vendorPartyId"].as_str().unwrap_or("Ukendt")),"purpose":p["businessPurpose"],"amount":p["amount"],"currency":p["currency"],"frequency":p["frequency"],"nextDate":p["nextDate"],"renewalDate":p["renewalDate"],"status":status,"evidenceRefs":p["evidenceRefs"],"missingDocument":false,"proposal":p}));
        if status != "active" {
            continue;
        }
        for (kind, key) in [("notice", "noticeDate"), ("renewal", "renewalDate")] {
            if let Some(date) = p[key].as_str()
                && date >= start
                && date <= alert_end.as_str()
            {
                alerts.push(json!({"commitmentId":id,"kind":kind,"date":date,"severity":if date==start{"due"}else{"upcoming"}}));
            }
        }
        let first_index = nearby_index(
            p["nextDate"]
                .as_str()
                .ok_or_else(|| refused("stored schedule date missing"))?,
            p["frequency"]
                .as_str()
                .ok_or_else(|| refused("stored cadence missing"))?,
            start,
        )?;
        for index in first_index..first_index + 16 {
            let date = occurrence(
                p["nextDate"]
                    .as_str()
                    .ok_or_else(|| refused("stored commitment missing date"))?,
                p["frequency"]
                    .as_str()
                    .ok_or_else(|| refused("stored commitment missing cadence"))?,
                index,
            )?;
            if date > until || p["endDate"].as_str().is_some_and(|e| date.as_str() > e) {
                break;
            }
            if date.as_str() >= start
                && !matched
                    .iter()
                    .any(|m| m["commitmentId"] == id && m["occurrenceDate"] == date)
            {
                occurrences.push(json!({"commitmentId":id,"date":date,"amount":p["amount"],"currency":p["currency"],"payloadHash":row["payloadHash"]}));
            }
        }
    }
    let forecast = if monthly {
        forecast::build_horizon(tx, slug, start, &occurrences, true)?
    } else {
        forecast::build(tx, slug, start, &occurrences)?
    };
    let vendors = crate::sales::contacts(tx, slug, start)?["vendors"].clone();
    let docs = crate::documents::list(tx, slug)?;
    let unique_documents: std::collections::BTreeMap<i64, Value> = docs["documents"]
        .as_array()
        .ok_or_else(|| refused("document register unavailable"))?
        .iter()
        .map(|d| {
            Ok((
                d["id"]
                    .as_i64()
                    .ok_or_else(|| refused("invalid original id"))?,
                d.clone(),
            ))
        })
        .collect::<Result<_, Error>>()?;
    let mut options = vec![];
    for (kind, sql) in [
        (
            "canonical_document",
            "SELECT id,invoice_date,amount_inc_vat,currency,COALESCE(document_no,original_filename) FROM documents WHERE document_type IN ('purchase_sale','cash_register_receipt') AND invoice_date IS NOT NULL AND amount_inc_vat>0 ORDER BY id DESC LIMIT 500",
        ),
        (
            "payable",
            "SELECT id,due_date,gross_amount,currency,COALESCE(bill_no,supplier_name,'Kreditor') FROM payables ORDER BY id DESC LIMIT 500",
        ),
        (
            "bank_transaction",
            "SELECT id,transaction_date,amount,currency,text FROM bank_transactions WHERE amount<0 AND transaction_date IS NOT NULL ORDER BY id DESC LIMIT 500",
        ),
    ] {
        let mut q = tx.prepare(sql).map_err(invalid)?;
        let mut rs = q.query([]).map_err(invalid)?;
        while let Some(r) = rs.next().map_err(invalid)? {
            let raw_amount = r.get_ref(2).map_err(invalid)?;
            let amount = if kind == "bank_transaction" {
                Money::default().subtract(&crate::bank::signed_money(raw_amount)?)
            } else {
                legacy_money(raw_amount)?
            };
            options.push(json!({"kind":kind,"id":r.get::<_,i64>(0).map_err(invalid)?.to_string(),"date":r.get::<_,String>(1).map_err(invalid)?,"amount":wire(&amount)?,"currency":r.get::<_,String>(3).map_err(invalid)?,"label":r.get::<_,String>(4).map_err(invalid)?}));
        }
    }
    let context = crate::sales::context(tx, slug, start, None)?;
    Ok(
        json!({"slug":slug,"company":crate::company_setup::profile(tx)?,"fiscalYears":context["fiscalYears"],"selectedYear":context["selectedYear"],"startDate":start,"commitments":commitments,"occurrences":occurrences,"history":history,"matches":matched,"alerts":alerts,"vendors":vendors,"documents":unique_documents.into_values().collect::<Vec<_>>(),"evidenceOptions":options,"forecast":forecast}),
    )
}
