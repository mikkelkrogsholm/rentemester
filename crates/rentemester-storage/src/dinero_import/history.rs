//! Immutable accepted-source facts. Physical bytes are admitted separately and
//! produce opaque proofs bound to this ledger, attempt and complete plan.
use super::*;
use rentemester_engine::manual_journal::ManualJournal;
#[derive(Clone, Debug)]
pub struct OriginalRef {
    pub attempt_id: i64,
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}
#[derive(Clone, Debug)]
pub struct VerifiedImport {
    attempt_id: i64,
    source_hash: String,
    plan_hash: String,
    ledger_uuid: String,
}
impl VerifiedImport {
    pub fn attempt_id(&self) -> i64 {
        self.attempt_id
    }
}
struct Checked {
    ledger_uuid: String,
    imports: Vec<Attestation>,
}
fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn id(id: i64) -> bool {
    (1..=9_007_199_254_740_991).contains(&id)
}
fn source_shape(source: &SourceFacts, kind: Option<&str>) -> Result<(), Error> {
    let extension = match kind {
        None => "json",
        Some("zip") => "zip",
        _ => return Err(refused("unsupported accepted source kind")),
    };
    if !hex(&source.sha256)
        || source.path != format!("documents/originals/{}.{extension}", source.sha256)
        || source.size_bytes == 0
        || source.size_bytes > 25 * 1024 * 1024
        || source.entries.is_empty()
        || source.entries.len() > 4096
    {
        return Err(refused("accepted source shape differs"));
    }
    let mut previous = None;
    let mut total = 0u64;
    for e in &source.entries {
        if e.path.is_empty()
            || e.path.len() > 1024
            || e.path.starts_with('/')
            || e.path.contains('\\')
            || e.path.contains("..")
            || e.path.chars().any(char::is_control)
            || e.path.split('/').any(|s| s.is_empty() || s == ".")
            || !hex(&e.sha256)
            || e.size_bytes > 16 * 1024 * 1024
            || previous.is_some_and(|s: &str| s >= e.path.as_str())
        {
            return Err(refused("accepted source inventory differs"));
        }
        total = total
            .checked_add(e.size_bytes)
            .ok_or_else(|| refused("source size overflow"))?;
        previous = Some(e.path.as_str());
    }
    if total > 18 * 1024 * 1024 {
        return Err(refused("accepted source inventory exceeds bounds"));
    }
    Ok(())
}
fn provenance(db: &Connection, e: &Attestation) -> Result<(), Error> {
    let source = &e.binding.source;
    source_shape(source, e.binding.source_kind.as_deref())?;
    let listing = hash(&source.entries)?;
    let count = source.entries.len() as i64;
    let total = source.entries.iter().try_fold(0i64, |n, s| {
        n.checked_add(s.size_bytes as i64)
            .ok_or_else(|| refused("source total overflow"))
    })?;
    let actual:Value=db.query_row("SELECT raw_sha256,raw_size_bytes,canonical_listing_sha256,canonical_listing_count,received_at FROM dinero_import_sources WHERE id=?1",[e.source_id],|r|Ok(json!([r.get::<_,String>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,String>(4)?]))).map_err(invalid)?;
    if actual
        != json!([
            source.sha256,
            source.size_bytes,
            listing,
            count,
            e.accepted_at
        ])
    {
        return Err(refused("accepted source row differs"));
    }
    let actual:Value=db.query_row("SELECT source_id,source_raw_sha256,canonical_listing_sha256,canonical_listing_count,entry_count,total_size_bytes,created_at FROM dinero_import_inventories WHERE id=?1",[e.inventory_id],|r|Ok(json!([r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?,r.get::<_,i64>(5)?,r.get::<_,String>(6)?]))).map_err(invalid)?;
    if actual
        != json!([
            e.source_id,
            source.sha256,
            listing,
            count,
            count,
            total,
            e.accepted_at
        ])
    {
        return Err(refused("accepted source inventory row differs"));
    }
    let mut q=db.prepare("SELECT entry_path,entry_size_bytes,entry_sha256 FROM dinero_import_inventory_entries WHERE inventory_id=?1 ORDER BY entry_path,id").map_err(invalid)?;
    let mut rows = q.query([e.inventory_id]).map_err(invalid)?;
    for entry in &source.entries {
        let row = rows
            .next()
            .map_err(invalid)?
            .ok_or_else(|| refused("accepted source entry missing"))?;
        if row.get::<_, String>(0).map_err(invalid)? != entry.path
            || row.get::<_, i64>(1).map_err(invalid)? < 0
            || row.get::<_, i64>(1).map_err(invalid)? as u64 != entry.size_bytes
            || row.get::<_, String>(2).map_err(invalid)? != entry.sha256
        {
            return Err(refused("accepted source entry differs"));
        }
    }
    if rows.next().map_err(invalid)?.is_some() {
        return Err(refused("unexpected accepted source entry"));
    }
    let cutoff = e.binding.parsed["cutOverDate"]
        .as_str()
        .or_else(|| e.binding.parsed["controlDate"].as_str())
        .unwrap_or(&e.accepted_at[..10]);
    Date::parse(cutoff)?;
    let actual:Value=db.query_row("SELECT inventory_id,source_id,source_raw_sha256,parser_contract,actor,cutover_date,outcome,result_sha256,recorded_at FROM dinero_import_attempts WHERE id=?1",[e.attempt_id],|r|Ok(json!([r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,String>(7)?,r.get::<_,String>(8)?]))).map_err(invalid)?;
    if actual
        != json!([
            e.inventory_id,
            e.source_id,
            source.sha256,
            CONTRACT,
            e.actor,
            cutoff,
            "accepted",
            hash(&e.receipt)?,
            e.accepted_at
        ])
    {
        return Err(refused("accepted source attempt differs"));
    }
    Ok(())
}
fn journal(
    db: &Connection,
    e: &Attestation,
    p: &JournalProof,
    source: &Value,
    previous: &mut String,
) -> Result<(), Error> {
    let request: ManualJournal = serde_json::from_value(p.request.clone()).map_err(invalid)?;
    if p.kind != "historical"
        || p.voucher_ref != source["voucherRef"].as_str().unwrap_or("")
        || request.transaction_date.to_string() != source["transactionDate"].as_str().unwrap_or("")
        || request.currency.as_str() != "DKK"
        || request.document_id.is_some()
        || request.source_bank_transaction_id.is_some()
        || request.amount_foreign.is_some()
        || request.amount_dkk.is_some()
        || request.fx_rate_to_dkk.is_some()
        || request.created_by_program.as_deref() != Some("rentemester-import-postings")
    {
        return Err(refused("historical journal request identity differs"));
    }
    let entry_type = source["entryType"].as_str().unwrap_or("").trim();
    let source_text = source["text"]
        .as_str()
        .ok_or_else(|| refused("historical source text unavailable"))?;
    let expected_text = if entry_type.is_empty() {
        format!("Import: bilag {} — {}", p.voucher_ref, source_text)
    } else {
        format!(
            "Import: {} (bilag {}) — {}",
            entry_type, p.voucher_ref, source_text
        )
    };
    if request.text != expected_text {
        return Err(refused("historical journal text differs from source"));
    }
    let source_lines = source["lines"]
        .as_array()
        .ok_or_else(|| refused("historical source lines unavailable"))?;
    if request.lines.len() != source_lines.len() {
        return Err(refused("historical source line count differs"));
    }
    let chart = e.binding.parsed["chart"]
        .as_array()
        .ok_or_else(|| refused("original chart unavailable"))?;
    for (line, s) in request.lines.iter().zip(source_lines) {
        let expected_code = s["vatCode"].as_str().or_else(|| {
            chart
                .iter()
                .find(|a| a["accountNo"] == s["accountNo"])
                .and_then(|a| a["defaultVatCode"].as_str())
        });
        if line.account_no.as_str() != s["accountNo"].as_str().unwrap_or("")
            || serde_json::to_value(&line.debit_amount).map_err(invalid)? != s["debitAmount"]
            || serde_json::to_value(&line.credit_amount).map_err(invalid)? != s["creditAmount"]
            || line.vat_code.as_deref() != expected_code
            || line.text.as_deref() != s["text"].as_str()
        {
            return Err(refused(
                "historical journal differs from parsed source line",
            ));
        }
    }
    let actual:Value=db.query_row("SELECT transaction_date,text,currency,created_by,created_by_program,status,previous_hash,entry_hash,document_id,source_bank_transaction_id,amount_foreign,amount_dkk,fx_rate_to_dkk,reversal_of_entry_id FROM journal_entries WHERE id=?1",[p.id],|r|Ok(json!([r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,Option<String>>(6)?,r.get::<_,String>(7)?,r.get::<_,Option<i64>>(8)?,r.get::<_,Option<i64>>(9)?,r.get::<_,Option<f64>>(10)?,r.get::<_,Option<f64>>(11)?,r.get::<_,Option<f64>>(12)?,r.get::<_,Option<i64>>(13)?]))).map_err(invalid)?;
    if actual
        != json!([
            request.transaction_date.to_string(),
            request.text,
            "DKK",
            e.actor,
            "rentemester-import-postings",
            "posted",
            previous,
            p.hash,
            null,
            null,
            null,
            null,
            null,
            null
        ])
        || crate::journal_hash::stored_hash(db, p.id, previous)? != p.hash
    {
        return Err(refused("historical journal original header/hash differs"));
    }
    let entry_no: String = db
        .query_row(
            "SELECT entry_no FROM journal_entries WHERE id=?1",
            [p.id],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let posting_events:i64=db.query_row("SELECT count(*) FROM audit_log WHERE event_type='journal_post' AND entity_type='journal_entry' AND entity_id=?1 AND actor=?2 AND message=?3",params![p.id.to_string(),format!("{} via rentemester-import-postings",e.actor),format!("Posted journal entry {entry_no}")],|r|r.get(0)).map_err(invalid)?;
    if posting_events != 1 {
        return Err(refused("historical journal posting audit differs"));
    }
    let mut q=db.prepare("SELECT a.account_no,l.debit_amount,l.credit_amount,l.vat_code,l.text,l.currency FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 ORDER BY l.id").map_err(invalid)?;
    let mut rows = q.query([p.id]).map_err(invalid)?;
    for line in &request.lines {
        let r = rows
            .next()
            .map_err(invalid)?
            .ok_or_else(|| refused("historical journal line missing"))?;
        if r.get::<_, String>(0).map_err(invalid)? != line.account_no.as_str()
            || crate::legacy_money(r.get_ref(1).map_err(invalid)?)? != line.debit_amount
            || crate::legacy_money(r.get_ref(2).map_err(invalid)?)? != line.credit_amount
            || r.get::<_, Option<String>>(3).map_err(invalid)? != line.vat_code
            || r.get::<_, Option<String>>(4).map_err(invalid)? != line.text
            || r.get::<_, String>(5).map_err(invalid)? != "DKK"
        {
            return Err(refused("historical journal original line differs"));
        }
    }
    if rows.next().map_err(invalid)?.is_some() {
        return Err(refused("unexpected historical journal line"));
    }
    *previous = p.hash.clone();
    Ok(())
}
fn archive(db: &Connection, e: &Attestation) -> Result<(), Error> {
    if e.binding.archive.is_none() {
        if !e.archive_import_ids.is_empty() {
            return Err(refused("unexpected derived archive references"));
        }
        return Ok(());
    }
    let bound = e.binding.archive.as_ref().unwrap();
    if e.archive_import_ids.len() != 1 {
        return Err(refused("complete derived archive reference required"));
    }
    crate::archive_import::validate_history(db)?;
    let originals = crate::archive_import::originals(db)?;
    let archive_id = e.archive_import_ids[0];
    let reference = originals
        .iter()
        .find(|r| r.import_id == archive_id)
        .ok_or_else(|| refused("derived archive reference unavailable"))?;
    let body:String=db.query_row("SELECT message FROM audit_log WHERE id=?1 AND event_type='native_closed_history_archive_import'",[archive_id],|r|r.get(0)).map_err(invalid)?;
    let event: Value = serde_json::from_str(&body).map_err(invalid)?;
    if event["binding"]["source"] != bound["source"]
        || event["binding"]["archive"] != bound["parsed"]
        || reference.sha256 != bound["source"]["sha256"].as_str().unwrap_or("")
    {
        return Err(refused(
            "derived archive differs from accepted source binding",
        ));
    }
    Ok(())
}
fn checked(db: &Connection) -> Result<Checked, Error> {
    crate::journal_preview::require_current(db)?;
    let ledger_uuid: String = db
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let opening = opening_balance::validate_history(db)?;
    let mut q=db.prepare("SELECT entity_type,entity_id,message,actor,created_at FROM audit_log WHERE event_type=?1 ORDER BY id").map_err(invalid)?;
    let mut rows = q.query([EVENT]).map_err(invalid)?;
    let mut imports = vec![];
    let mut attempts = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut journal_ids = BTreeSet::new();
    let mut opening_claimed = false;
    while let Some(row) = rows.next().map_err(invalid)? {
        let body: String = row.get(2).map_err(invalid)?;
        let e: Attestation = serde_json::from_str(&body).map_err(invalid)?;
        if e.binding.format != CONTRACT
            || e.binding.context["ledgerUuid"] != ledger_uuid
            || e.plan_hash != hash(&e.binding)?
            || !hex(&e.plan_hash)
            || !id(e.attempt_id)
            || !id(e.source_id)
            || !id(e.inventory_id)
            || !e.actor.starts_with("user:")
            || e.actor.len() <= 5
            || e.actor.chars().any(char::is_control)
            || serde_json::to_string(&e).map_err(invalid)? != body
            || row.get::<_, String>(0).map_err(invalid)? != "dinero_import"
            || row.get::<_, String>(1).map_err(invalid)? != e.attempt_id.to_string()
            || row.get::<_, String>(3).map_err(invalid)? != e.actor
            || row.get::<_, String>(4).map_err(invalid)? != e.accepted_at
            || !attempts.insert(e.attempt_id)
            || !sources.insert(e.binding.source.sha256.clone())
        {
            return Err(refused("native import immutable attestation differs"));
        }
        rentemester_engine::company_knowledge::iso(&e.accepted_at)?;
        provenance(db, &e)?;
        archive(db, &e)?;
        if e.binding.parsed["parserContract"] != policy::CONTRACT
            || e.binding.parsed["declaredCurrency"] != "DKK"
        {
            return Err(refused("accepted parser contract differs"));
        }
        if !e.binding.context["head"].is_null() {
            let head = e.binding.context["head"]
                .as_array()
                .filter(|a| a.len() == 2)
                .ok_or_else(|| refused("original ledger head shape differs"))?;
            let head_id = head[0]
                .as_i64()
                .filter(|n| id(*n))
                .ok_or_else(|| refused("original ledger head identity differs"))?;
            let head_hash = head[1]
                .as_str()
                .filter(|s| hex(s))
                .ok_or_else(|| refused("original ledger head hash differs"))?;
            let actual: String = db
                .query_row(
                    "SELECT entry_hash FROM journal_entries WHERE id=?1",
                    [head_id],
                    |r| r.get(0),
                )
                .map_err(invalid)?;
            if actual != head_hash {
                return Err(refused("original ledger prefix differs"));
            }
        }
        let mut previous = e.binding.context["head"]
            .as_array()
            .and_then(|a| a.get(1))
            .and_then(Value::as_str)
            .unwrap_or("GENESIS")
            .to_owned();
        if let Some(o) = &e.opening {
            if opening_claimed {
                return Err(refused("opening source claimed more than once"));
            }
            opening_claimed = true;
            let retained = opening
                .as_ref()
                .ok_or_else(|| refused("accepted opening proof missing"))?;
            let record = retained.record();
            if serde_json::to_value(record).map_err(invalid)? != *o
                || o["cutOverDate"] != e.binding.parsed["cutOverDate"]
                || o["actor"] != e.actor
            {
                return Err(refused("accepted opening record differs"));
            }
            let mut q=db.prepare("SELECT a.account_no,l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 ORDER BY l.id").map_err(invalid)?;
            let mut ls = q.query([record.journal_entry_id]).map_err(invalid)?;
            for line in e.binding.parsed["openingLines"]
                .as_array()
                .ok_or_else(|| refused("source opening lines unavailable"))?
            {
                let r = ls
                    .next()
                    .map_err(invalid)?
                    .ok_or_else(|| refused("source opening line missing"))?;
                if json!(r.get::<_, String>(0).map_err(invalid)?) != line["accountNo"]
                    || json!(crate::legacy_money(r.get_ref(1).map_err(invalid)?)?.decimal())
                        != line["debitAmount"]
                    || json!(crate::legacy_money(r.get_ref(2).map_err(invalid)?)?.decimal())
                        != line["creditAmount"]
                {
                    return Err(refused("accepted opening differs from parsed source"));
                }
            }
            if ls.next().map_err(invalid)?.is_some() {
                return Err(refused("unexpected accepted opening line"));
            }
            previous = record.journal_entry_hash.clone();
        } else if e.binding.parsed["openingLines"]
            .as_array()
            .is_none_or(|a| !a.is_empty())
        {
            return Err(refused("parsed opening has no accepted marker"));
        }
        let vouchers = e.binding.parsed["vouchers"]
            .as_array()
            .ok_or_else(|| refused("source vouchers unavailable"))?;
        if vouchers.len() != e.journals.len() {
            return Err(refused("accepted journal inventory differs"));
        }
        let mut last_id = e
            .opening
            .as_ref()
            .and_then(|o| o["journalEntryId"].as_i64())
            .or_else(|| e.binding.context["head"][0].as_i64())
            .unwrap_or(0);
        for (p, v) in e.journals.iter().zip(vouchers) {
            if !id(p.id) || p.id <= last_id || !hex(&p.hash) || !journal_ids.insert(p.id) {
                return Err(refused("duplicate or unordered accepted journal identity"));
            }
            journal(db, &e, p, v, &mut previous)?;
            last_id = p.id;
        }
        if !e.binding.parsed["schedule"].is_null() {
            let schedule = rentemester_engine::imported_receivables::validate(
                &e.binding.parsed["schedule"]["source"],
            )?;
            if serde_json::to_value(&schedule).map_err(invalid)? != e.binding.parsed["schedule"] {
                return Err(refused("accepted schedule parsed facts differ"));
            }
            crate::imported_receivables::verify_schedule(db, e.attempt_id, &schedule)?;
        } else {
            let count:i64=db.query_row("SELECT count(*) FROM imported_receivable_headers WHERE dinero_import_attempt_id=?1",[e.attempt_id],|r|r.get(0)).map_err(invalid)?;
            if count != 0 {
                return Err(refused("unexpected native imported schedule"));
            }
        }
        if e.receipt["ok"] != true
            || e.receipt["attemptId"] != e.attempt_id
            || e.receipt["sourceSha256"] != e.binding.source.sha256
            || e.receipt["sourceSizeBytes"] != e.binding.source.size_bytes
            || e.receipt["parserContract"] != CONTRACT
            || e.receipt["cutOverDate"] != e.binding.parsed["cutOverDate"]
            || e.receipt["controlDate"] != e.binding.parsed["controlDate"]
            || e.receipt["acceptedAt"] != e.accepted_at
            || e.receipt["sourceAssurance"] != "retained-native"
            || e.receipt["archiveImportIds"] != json!(e.archive_import_ids)
        {
            return Err(refused("accepted import receipt differs"));
        }
        receipts::validate_event(db, &e)?;
        imports.push(e);
    }
    let mut q=db.prepare("SELECT id FROM dinero_import_attempts WHERE parser_contract LIKE 'rentemester-native-dinero/%' AND outcome='accepted' ORDER BY id").map_err(invalid)?;
    let actual = q
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(invalid)?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(invalid)?;
    if actual != attempts {
        return Err(refused(
            "native accepted attempt lacks exact immutable attestation",
        ));
    }
    Ok(Checked {
        ledger_uuid,
        imports,
    })
}
pub fn validate_history(db: &Connection) -> Result<(), Error> {
    checked(db).map(|_| ())
}
pub fn originals(db: &Connection) -> Result<Vec<OriginalRef>, Error> {
    Ok(checked(db)?
        .imports
        .into_iter()
        .map(|e| OriginalRef {
            attempt_id: e.attempt_id,
            path: e.binding.source.path,
            sha256: e.binding.source.sha256,
            size_bytes: e.binding.source.size_bytes,
        })
        .collect())
}
pub fn verify_original(
    db: &Connection,
    reference: &OriginalRef,
    bytes: &[u8],
) -> Result<VerifiedImport, Error> {
    let checked = checked(db)?;
    let e = checked
        .imports
        .iter()
        .find(|e| e.attempt_id == reference.attempt_id)
        .ok_or_else(|| refused("native import original not found"))?;
    let decoded = decode_source(bytes)?;
    if let Some(plan) = &e.binding.receipt_plan
        && serde_json::to_value(policy::receipts::plan(&decoded.files, &decoded.parsed)?)
            .map_err(invalid)?
            != *plan
    {
        return Err(refused("physical source receipt plan differs"));
    }
    let archive = decoded
        .archive
        .as_ref()
        .map(|a| json!({"source":a.source,"parsed":a.parsed}));
    if decoded.source.path != reference.path
        || decoded.source.sha256 != reference.sha256
        || decoded.source.size_bytes != reference.size_bytes
        || decoded.source != e.binding.source
        || serde_json::to_value(&decoded.parsed).map_err(invalid)? != e.binding.parsed
        || archive != e.binding.archive
    {
        return Err(refused(
            "physical original differs from full accepted source facts",
        ));
    }
    Ok(VerifiedImport {
        attempt_id: e.attempt_id,
        source_hash: decoded.source.sha256,
        plan_hash: e.plan_hash.clone(),
        ledger_uuid: checked.ledger_uuid,
    })
}
fn coverage(c: &Checked, proofs: &[VerifiedImport]) -> Result<(), Error> {
    let mut ids = BTreeSet::new();
    for p in proofs {
        let e = c
            .imports
            .iter()
            .find(|e| e.attempt_id == p.attempt_id)
            .ok_or_else(|| refused("unknown physical source proof"))?;
        if !ids.insert(p.attempt_id)
            || p.source_hash != e.binding.source.sha256
            || p.plan_hash != e.plan_hash
            || p.ledger_uuid != c.ledger_uuid
        {
            return Err(refused("physical source proof binding differs"));
        }
    }
    if ids.len() != c.imports.len() {
        return Err(Error::new(
            "dinero_original_unverified",
            "complete native physical source verification required",
        ));
    }
    Ok(())
}
pub fn read_imports(db: &Connection, proofs: &[VerifiedImport]) -> Result<Value, Error> {
    let c = checked(db)?;
    coverage(&c, proofs)?;
    let native = c
        .imports
        .iter()
        .map(|e| (e.attempt_id, e))
        .collect::<BTreeMap<_, _>>();
    let mut q=db.prepare("SELECT a.id,a.source_raw_sha256,a.parser_contract,a.actor,a.cutover_date,a.outcome,a.recorded_at,s.raw_size_bytes FROM dinero_import_attempts a JOIN dinero_import_sources s ON s.id=a.source_id ORDER BY a.id").map_err(invalid)?;
    let mut rows = q.query([]).map_err(invalid)?;
    let mut imports = vec![];
    while let Some(r) = rows.next().map_err(invalid)? {
        let attempt_id: i64 = r.get(0).map_err(invalid)?;
        if let Some(e) = native.get(&attempt_id) {
            let mut row = e.receipt.clone();
            row["source"] = serde_json::to_value(&e.binding.source).map_err(invalid)?;
            row["latestYear"] = e.binding.parsed["latestYear"].clone();
            row["actor"] = json!(e.actor);
            row["outcome"] = json!("accepted");
            row["sourceAssurance"] = json!("retained-native");
            imports.push(row);
        } else {
            imports.push(json!({"attemptId":attempt_id,"sourceSha256":r.get::<_,String>(1).map_err(invalid)?,"parserContract":r.get::<_,String>(2).map_err(invalid)?,"actor":r.get::<_,String>(3).map_err(invalid)?,"cutOverDate":r.get::<_,String>(4).map_err(invalid)?,"outcome":r.get::<_,String>(5).map_err(invalid)?,"acceptedAt":r.get::<_,String>(6).map_err(invalid)?,"sourceSizeBytes":r.get::<_,i64>(7).map_err(invalid)?,"controlDate":null,"counts":null,"sourceAssurance":"legacy-unavailable","limitations":["Original source package is unavailable; persisted declarations do not authenticate invoice original bytes."]}));
        }
    }
    Ok(json!({"ok":true,"attempts":imports,"count":imports.len(),"errors":[]}))
}
/// Source-history qualification only. These IDs grant no physical-source or live authority.
pub(crate) fn accepted_journal_ids(db: &Connection) -> Result<BTreeSet<i64>, Error> {
    Ok(checked(db)?
        .imports
        .into_iter()
        .flat_map(|e| e.journals.into_iter().map(|p| p.id))
        .collect())
}
/// Source-history qualification only; legacy declarations remain outside this set.
pub(crate) fn accepted_header_ids(db: &Connection) -> Result<BTreeSet<i64>, Error> {
    let c = checked(db)?;
    let mut ids = BTreeSet::new();
    for e in c.imports {
        let mut q=db.prepare("SELECT id FROM imported_receivable_headers WHERE dinero_import_attempt_id=?1 ORDER BY id").map_err(invalid)?;
        for id in q
            .query_map([e.attempt_id], |r| r.get::<_, i64>(0))
            .map_err(invalid)?
        {
            ids.insert(id.map_err(invalid)?);
        }
    }
    Ok(ids)
}
