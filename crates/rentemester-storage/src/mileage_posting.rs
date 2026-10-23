//! Explicit mileage accounting. Caller owns authority, physical originals and rollback.
pub(crate) mod schema;
use crate::{idempotency::PrincipalKind, invalid};
pub use engine::refused;
use rentemester_engine::{
    Error, fiscal_year::Date, manual_journal::ManualJournal, mileage, mileage_posting as engine,
    money::Money,
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Configure {
    pub version: String,
    pub valid_from: String,
    pub valid_to: String,
    pub vehicle: String,
    pub rate_per_km: String,
    pub basis: String,
    pub source: String,
    pub expense_account_no: String,
    pub liability_account_no: String,
    pub confirm: bool,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preview {
    pub mileage_entry_id: i64,
    pub rate_version: String,
    pub document_id: i64,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Post {
    pub mileage_entry_id: i64,
    pub rate_version: String,
    pub document_id: i64,
    pub expected_preview_hash: String,
    pub operation_key: String,
    pub confirm: bool,
}
fn principal(subject: &str, kind: PrincipalKind) -> Result<String, Error> {
    engine::text(subject, 512)?;
    Ok(format!(
        "{}:{subject}",
        match kind {
            PrincipalKind::User => "user",
            PrincipalKind::ServiceAccount => "service_account",
        }
    ))
}
fn account(tx: &Transaction<'_>, no: &str, kind: &str, normal: &str) -> Result<(), Error> {
    let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE account_no=?1 AND active=1 AND allow_direct_posting=1 AND type=?2 AND normal_balance=?3)",params![no,kind,normal],|r|r.get(0)).map_err(invalid)?;
    if !valid {
        return Err(refused(
            "Active directly postable expense/debit and liability/credit accounts required",
        ));
    }
    Ok(())
}
fn accounts(tx: &Transaction<'_>, v: &Value) -> Result<(), Error> {
    let expense = string(v, "expenseAccountNo")?;
    let liability = string(v, "liabilityAccountNo")?;
    if expense == liability {
        return Err(refused("Distinct accounts required"));
    }
    account(tx, expense, "expense", "debit")?;
    account(tx, liability, "liability", "credit")
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str, Error> {
    v[key]
        .as_str()
        .ok_or_else(|| refused(format!("Invalid {key}")))
}
fn gate(tx: &Transaction<'_>) -> Result<bool, Error> {
    crate::journal_post::require_current(tx)?;
    crate::bank_original::verify_journals(tx)?;
    let installed = schema::installed(tx)?;
    if installed {
        let reversed: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM native_mileage_posting_receipts r JOIN journal_entries j ON j.id=r.journal_entry_id WHERE j.status!='posted' OR EXISTS(SELECT 1 FROM journal_entries rev WHERE rev.reversal_of_entry_id=j.id))", [], |r| r.get(0)).map_err(invalid)?;
        if reversed {
            return Err(refused(
                "Mileage journal reversed; receipt is historical, not an effective posting",
            ));
        }
    }
    Ok(installed)
}
pub fn rate_versions(tx: &Transaction<'_>) -> Result<Value, Error> {
    if !gate(tx)? {
        return Ok(json!({"rateVersions":[]}));
    }
    let mut q = tx
        .prepare("SELECT version,payload FROM native_mileage_posting_versions ORDER BY version")
        .map_err(invalid)?;
    let mut versions = vec![];
    for row in q
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(invalid)?
    {
        let (version, payload) = row.map_err(invalid)?;
        let v: Value = serde_json::from_str(&payload).map_err(invalid)?;
        validate_version(&v)?;
        if v["version"] != version {
            return Err(refused("Version identity differs"));
        }
        versions.push(v);
    }
    Ok(json!({"rateVersions":versions}))
}
fn validate_version(v: &Value) -> Result<(), Error> {
    for key in ["version", "expenseAccountNo", "liabilityAccountNo"] {
        engine::text(string(v, key)?, 128)?;
    }
    for key in ["vehicle", "basis", "source", "ratePerKm"] {
        engine::text(string(v, key)?, 4096)?;
    }
    engine::version(
        string(v, "validFrom")?,
        string(v, "validTo")?,
        string(v, "ratePerKm")?,
    )
}
pub fn configure(
    tx: &Transaction<'_>,
    input: &Configure,
    subject: &str,
    kind: PrincipalKind,
) -> Result<Value, Error> {
    gate(tx)?;
    if !input.confirm {
        return Err(refused("Explicit confirmation required"));
    }
    let actor = principal(subject, kind)?;
    let mut v = serde_json::to_value(input).map_err(invalid)?;
    v.as_object_mut()
        .ok_or_else(|| refused("Invalid version"))?
        .remove("confirm");
    validate_version(&v)?;
    accounts(tx, &v)?;
    if schema::installed(tx)? {
        let old: Option<String> = tx
            .query_row(
                "SELECT payload FROM native_mileage_posting_versions WHERE version=?1",
                [&input.version],
                |r| r.get(0),
            )
            .optional()
            .map_err(invalid)?;
        if let Some(old) = old {
            let old: Value = serde_json::from_str(&old).map_err(invalid)?;
            let mut business = old.clone();
            business.as_object_mut().unwrap().remove("actor");
            business.as_object_mut().unwrap().remove("createdAt");
            if business != v {
                return Err(refused(
                    "Version already exists with different configuration",
                ));
            }
            return Ok(json!({"rateVersion":old,"replayed":true}));
        }
    }
    v["actor"] = json!(actor);
    let now: String = tx
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
            r.get(0)
        })
        .map_err(invalid)?;
    v["createdAt"] = json!(now);
    schema::install(tx)?;
    let payload = serde_json::to_string(&v).map_err(invalid)?;
    tx.execute("INSERT INTO native_mileage_posting_versions(version,payload,payload_hash) VALUES(?1,?2,?3)",params![input.version,payload,hash(&payload)]).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('mileage_rate_configure','mileage_rate',?1,?2,?3)",params![input.version,format!("Configured explicit mileage version {}",input.version),actor]).map_err(invalid)?;
    Ok(json!({"rateVersion":v,"replayed":false}))
}
fn selected(tx: &Transaction<'_>, version: &str) -> Result<Value, Error> {
    engine::text(version, 128)?;
    if !schema::installed(tx)? {
        return Err(refused("Explicit rate version missing"));
    }
    let payload: Option<String> = tx
        .query_row(
            "SELECT payload FROM native_mileage_posting_versions WHERE version=?1",
            [version],
            |r| r.get(0),
        )
        .optional()
        .map_err(invalid)?;
    let v: Value =
        serde_json::from_str(&payload.ok_or_else(|| refused("Explicit rate version missing"))?)
            .map_err(invalid)?;
    validate_version(&v)?;
    if v["version"] != version {
        return Err(refused("Version identity differs"));
    }
    Ok(v)
}
fn safe(id: i64) -> Result<(), Error> {
    if !(1..=9_007_199_254_740_991).contains(&id) {
        return Err(refused("Positive safe integer ID required"));
    }
    Ok(())
}
struct Prepared {
    preview: Value,
    request: ManualJournal,
}
fn prepare(
    tx: &Transaction<'_>,
    input: &Preview,
    document_hash: &str,
    latest: Date,
    validate_journal: bool,
) -> Result<Prepared, Error> {
    safe(input.mileage_entry_id)?;
    safe(input.document_id)?;
    let v = selected(tx, &input.rate_version)?;
    let evidence = crate::documents::evidence(tx, input.document_id)
        .map_err(|_| refused("Original document missing"))?;
    if evidence.hash != document_hash || !lower_hash(document_hash) {
        return Err(refused("Physical original hash differs"));
    }
    let row:Option<Value>=tx.query_row("SELECT entry_no,trip_date,purpose,from_location,to_location,kilometers,vehicle,driver,rate_per_km,amount_basis,rate_basis,rate_source,notes,created_at FROM mileage_entries WHERE id=?1",[input.mileage_entry_id],|r|Ok(json!({"entryNo":r.get::<_,String>(0)?,"tripDate":r.get::<_,String>(1)?,"purpose":r.get::<_,String>(2)?,"fromLocation":r.get::<_,String>(3)?,"toLocation":r.get::<_,String>(4)?,"kilometers":r.get::<_,f64>(5)?,"vehicle":r.get::<_,String>(6)?,"driver":r.get::<_,String>(7)?,"ratePerKm":r.get::<_,f64>(8)?,"amountBasis":r.get::<_,f64>(9)?,"rateBasis":r.get::<_,String>(10)?,"rateSource":r.get::<_,Option<String>>(11)?,"notes":r.get::<_,Option<String>>(12)?,"createdAt":r.get::<_,String>(13)?}))).optional().map_err(invalid)?;
    let trip = row.ok_or_else(|| refused("Mileage entry missing"))?;
    for key in [
        "purpose",
        "fromLocation",
        "toLocation",
        "driver",
        "rateBasis",
        "vehicle",
    ] {
        engine::text(string(&trip, key)?, 4096)?;
    }
    let date = string(&trip, "tripDate")?;
    engine::matches(
        string(&v, "validFrom")?,
        string(&v, "validTo")?,
        string(&v, "vehicle")?,
        date,
        string(&trip, "vehicle")?,
        string(&v, "ratePerKm")?,
        &trip["ratePerKm"].to_string(),
    )?;
    let (km, amount) =
        mileage::calculate(&trip["kilometers"].to_string(), string(&v, "ratePerKm")?)
            .map_err(|_| refused("Invalid stored mileage arithmetic"))?;
    if amount == Money::default() || amount != Money::parse(&trip["amountBasis"].to_string())? {
        return Err(refused("Historical amount does not match explicit version"));
    }
    let currency: String = tx
        .query_row("SELECT currency FROM companies WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(invalid)?;
    if currency != "DKK" {
        return Err(refused("Only DKK company mileage posting supported"));
    }
    accounts(tx, &v)?;
    let request:ManualJournal=serde_json::from_value(json!({"transactionDate":date,"text":format!("Mileage {}",string(&trip,"entryNo")?),"currency":"DKK","documentId":input.document_id.to_string(),"lines":[{"accountNo":v["expenseAccountNo"],"debitAmount":amount.decimal(),"creditAmount":"0.00"},{"accountNo":v["liabilityAccountNo"],"debitAmount":"0.00","creditAmount":amount.decimal()}]})).map_err(invalid)?;
    let rules = crate::journal_preview::compiled_rules()?;
    if validate_journal {
        crate::opening_balance::require_posting_date(tx, request.transaction_date)?;
        let facts = crate::journal_preview::collect_facts(tx, &request, latest)?;
        let validation = rentemester_engine::manual_journal::validate(
            &request,
            &facts,
            &rules,
            rules.version(),
        )?;
        if !validation.ok {
            return Err(refused(format!(
                "Canonical journal rules refused: {}",
                validation
                    .errors
                    .iter()
                    .map(|e| e.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }
    }
    // All exact trip/config/document fields plus account identity and canonical rule version.
    let mut account_facts = vec![];
    for no in [
        string(&v, "expenseAccountNo")?,
        string(&v, "liabilityAccountNo")?,
    ] {
        let fact:Value=tx.query_row("SELECT id,account_no,type,normal_balance,active,allow_direct_posting FROM accounts WHERE account_no=?1",[no],|r|Ok(json!([r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?,r.get::<_,i64>(5)?]))).map_err(invalid)?;
        account_facts.push(fact);
    }
    let proof = json!({"rule":engine::RULE,"journalRules":rules.version(),"tripId":input.mileage_entry_id,"trip":trip,"version":v,"accounts":account_facts,"documentId":input.document_id,"documentHash":document_hash,"documentPath":evidence.path,"request":request});
    let preview_hash = hash(&serde_json::to_string(&proof).map_err(invalid)?);
    let preview = json!({"mileageEntryId":input.mileage_entry_id,"entryNo":trip["entryNo"],"tripDate":date,"vehicle":trip["vehicle"],"kilometers":crate::journal_post::money(&km)?,"ratePerKm":v["ratePerKm"],"amount":crate::journal_post::money(&amount)?,"rateVersion":input.rate_version,"expenseAccountNo":v["expenseAccountNo"],"liabilityAccountNo":v["liabilityAccountNo"],"documentId":input.document_id,"documentHash":document_hash,"previewHash":preview_hash,"appliedRules":[engine::RULE]});
    Ok(Prepared { preview, request })
}
fn lower_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub fn preview(
    tx: &Transaction<'_>,
    input: &Preview,
    document_hash: &str,
    latest: Date,
) -> Result<Value, Error> {
    gate(tx)?;
    Ok(prepare(tx, input, document_hash, latest, true)?.preview)
}
pub fn post(
    tx: &Transaction<'_>,
    input: &Post,
    subject: &str,
    kind: PrincipalKind,
    document_hash: &str,
    latest: Date,
) -> Result<Value, Error> {
    gate(tx)?;
    if !input.confirm || !lower_hash(&input.expected_preview_hash) {
        return Err(refused("Confirmation and exact preview hash required"));
    }
    engine::text(&input.operation_key, 128)?;
    let actor = principal(subject, kind)?;
    let intent = serde_json::to_string(input).map_err(invalid)?;
    let existing = if schema::installed(tx)? {
        tx.query_row("SELECT mileage_entry_id,operation_key,principal,intent,payload FROM native_mileage_posting_receipts WHERE mileage_entry_id=?1 OR operation_key=?2",params![input.mileage_entry_id,input.operation_key],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?))).optional().map_err(invalid)?
    } else {
        None
    };
    let p = Preview {
        mileage_entry_id: input.mileage_entry_id,
        rate_version: input.rate_version.clone(),
        document_id: input.document_id,
    };
    if let Some((trip, key, who, body, payload)) = existing {
        // Verify the full durable journal/intent/receipt binding before replay.
        postings(tx, latest)?;
        if trip != input.mileage_entry_id
            || key != input.operation_key
            || who != actor
            || body != intent
        {
            return Err(refused(
                "Trip or operation key already committed for another intent/principal",
            ));
        }
        let current = prepare(tx, &p, document_hash, latest, false)?;
        let mut receipt: Value = serde_json::from_str(&payload).map_err(invalid)?;
        if current.preview["previewHash"] != receipt["previewHash"]
            || receipt["previewHash"] != input.expected_preview_hash
        {
            return Err(refused("Committed mileage evidence changed"));
        }
        receipt["replayed"] = json!(true);
        return Ok(receipt);
    }
    let prepared = prepare(tx, &p, document_hash, latest, true)?;
    if prepared.preview["previewHash"] != input.expected_preview_hash {
        return Err(refused("Stale mileage preview"));
    }
    // No write before all original/config/current journal validation succeeds.
    schema::install(tx)?;
    let journal = crate::journal_post::post(
        tx,
        &prepared.request,
        &crate::journal_preview::manual_journal_rule_version()?,
        latest,
        crate::journal_post::Actor {
            subject: &actor,
            program: "rentemester-mileage-posting",
        },
    )?;
    if journal["ok"] != true {
        return Err(refused("Canonical mileage journal refused"));
    }
    let mut receipt = prepared.preview;
    receipt["journalEntryId"] = journal["entryId"].clone();
    receipt["journalEntryNo"] = journal["entryNo"].clone();
    receipt["replayed"] = json!(false);
    let payload = serde_json::to_string(&receipt).map_err(invalid)?;
    tx.execute("INSERT INTO native_mileage_posting_receipts(mileage_entry_id,operation_key,principal,intent,payload,payload_hash,journal_entry_id,journal_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![input.mileage_entry_id,input.operation_key,actor,intent,payload,hash(&payload),journal["entryId"].as_i64(),journal["entryHash"].as_str()]).map_err(invalid)?;
    Ok(receipt)
}
/// Physical originals for these IDs MUST be admitted before returning these receipts.
pub fn posting_documents(tx: &Transaction<'_>) -> Result<Vec<i64>, Error> {
    if !gate(tx)? {
        return Ok(vec![]);
    }
    let mut q = tx
        .prepare("SELECT payload FROM native_mileage_posting_receipts ORDER BY mileage_entry_id")
        .map_err(invalid)?;
    let mut ids = vec![];
    for row in q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?
    {
        let v: Value = serde_json::from_str(&row.map_err(invalid)?).map_err(invalid)?;
        ids.push(
            v["documentId"]
                .as_i64()
                .ok_or_else(|| refused("Invalid receipt document"))?,
        );
    }
    Ok(ids)
}
pub fn postings(tx: &Transaction<'_>, latest: Date) -> Result<Value, Error> {
    if !gate(tx)? {
        return Ok(json!({"postings":[]}));
    }
    let mut q=tx.prepare("SELECT mileage_entry_id,operation_key,principal,intent,payload,journal_entry_id FROM native_mileage_posting_receipts ORDER BY mileage_entry_id").map_err(invalid)?;
    let mut values = vec![];
    for row in q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })
        .map_err(invalid)?
    {
        let (trip, key, who, intent, payload, journal) = row.map_err(invalid)?;
        let input: Post = serde_json::from_str(&intent).map_err(invalid)?;
        let receipt: Value = serde_json::from_str(&payload).map_err(invalid)?;
        let current = prepare(
            tx,
            &Preview {
                mileage_entry_id: trip,
                rate_version: input.rate_version.clone(),
                document_id: input.document_id,
            },
            string(&receipt, "documentHash")?,
            latest,
            false,
        )?;
        let mut expected = current.preview;
        expected["journalEntryId"] = json!(journal);
        let (number,created_by,program,document):(String,String,String,Option<i64>)=tx.query_row("SELECT entry_no,created_by,created_by_program,document_id FROM journal_entries WHERE id=?1",[journal],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(invalid)?;
        expected["journalEntryNo"] = json!(number);
        expected["replayed"] = json!(false);
        if expected != receipt
            || input.mileage_entry_id != trip
            || input.operation_key != key
            || input.expected_preview_hash != receipt["previewHash"]
            || !input.confirm
            || created_by != who
            || program != "rentemester-mileage-posting"
            || document != Some(input.document_id)
        {
            return Err(refused("Mileage receipt integrity differs"));
        }
        values.push(receipt);
    }
    Ok(json!({"postings":values}))
}
#[cfg(test)]
mod tests;
