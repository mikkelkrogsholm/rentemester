use super::*;
use rentemester_engine::{
    dinero_import::receipts::Receipt,
    fiscal_year::{self, LabelStrategy, StartMonth},
};
const TASK: &str = "IMPORTED_RECEIPT_NOT_BOOKED";
const ACTION: &str = "Review the receipt and either book a journal entry for it or confirm it is not a business document.";
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Proof {
    pub source_path: String,
    pub document_id: i64,
    pub document: Value,
    pub provenance_id: i64,
    pub link_id: Option<i64>,
    pub exception_id: Option<i64>,
    pub exception: Option<Value>,
    pub duplicate: bool,
}
fn plan(decoded: &Decoded) -> Result<policy::receipts::ReceiptPlan, Error> {
    policy::receipts::plan(&decoded.files, &decoded.parsed)
}
fn raw<'a>(decoded: &'a Decoded, r: &Receipt) -> Result<&'a [u8], Error> {
    let f = decoded
        .files
        .iter()
        .find(|f| f.path == r.source_path)
        .ok_or_else(|| refused("planned receipt raw original missing"))?;
    if sha(&f.bytes) != r.sha256 || f.bytes.len() as u64 != r.size_bytes {
        return Err(refused("planned receipt raw identity differs"));
    }
    Ok(&f.bytes)
}
fn existing(db: &Connection, hash: &str) -> Result<Option<i64>, Error> {
    db.query_row(
        "SELECT id FROM documents WHERE sha256_hash=?1",
        [hash],
        |r| r.get(0),
    )
    .optional()
    .map_err(invalid)
}
/// Only new content is published. Existing content is reused before MIME or
/// requested metadata validation, preserving the source's duplicate semantics.
pub fn receipt_publications(db: &Connection, decoded: &Decoded) -> Result<Vec<SourceFile>, Error> {
    validate_history(db)?;
    let mut seen = BTreeSet::new();
    let mut out = vec![];
    for r in plan(decoded)?.receipts {
        let bytes = raw(decoded, &r)?;
        if seen.insert(r.sha256.clone()) && existing(db, &r.sha256)?.is_none() {
            out.push(SourceFile {
                path: r.original_filename,
                bytes: bytes.to_vec(),
            });
        }
    }
    Ok(out)
}
fn document(db: &Connection, id: i64) -> Result<Value, Error> {
    db.query_row("SELECT document_no,source,original_filename,stored_path,mime_type,sha256_hash,document_type,currency,payload_json,retain_until,upload_datetime FROM documents WHERE id=?1",[id],|r|Ok(json!({"documentNo":r.get::<_,String>(0)?,"source":r.get::<_,String>(1)?,"filename":r.get::<_,String>(2)?,"path":r.get::<_,String>(3)?,"mime":r.get::<_,String>(4)?,"hash":r.get::<_,String>(5)?,"type":r.get::<_,String>(6)?,"currency":r.get::<_,String>(7)?,"payload":r.get::<_,Option<String>>(8)?,"retainUntil":r.get::<_,Option<String>>(9)?,"uploadedAt":r.get::<_,String>(10)?}))).map_err(invalid)
}
fn new_document(
    tx: &Transaction<'_>,
    r: &Receipt,
    bytes: &[u8],
    at: &str,
    actor: &str,
) -> Result<i64, Error> {
    // The application validates/publishes through runtime Prepared. Recheck its
    // deterministic byte contract here before any source-only metadata insert.
    let expected = match r.mime_type.as_str() {
        "application/pdf" => bytes.starts_with(b"%PDF-"),
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(b"\xff\xd8\xff"),
        _ => false,
    };
    if bytes.is_empty()
        || bytes.len() > 25 * 1024 * 1024
        || r.original_filename.is_empty()
        || r.original_filename.len() > 255
        || r.original_filename.contains(['/', '\\'])
        || r.original_filename.chars().any(char::is_control)
        || !expected
    {
        return Err(refused(
            "new source receipt original violates document admission",
        ));
    }
    let (cvr,month,strategy):(Option<String>,i32,String)=tx.query_row("SELECT cvr,fiscal_year_start_month,fiscal_year_label_strategy FROM companies WHERE id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(invalid)?;
    let today = &at[..10];
    let label = fiscal_year::for_date(
        Date::parse(today)?,
        StartMonth::new(month)?,
        LabelStrategy::try_from(strategy.as_str())?,
    )?
    .identifier_label;
    let prefix = format!("DOC-{label}-");
    let pattern = format!("{prefix}[0-9][0-9][0-9][0-9][0-9][0-9]");
    let floor:i64=tx.query_row("SELECT COALESCE(MAX(CAST(substr(document_no,-6) AS INTEGER)),0) FROM documents WHERE document_no GLOB ?1",[pattern],|r|r.get(0)).map_err(invalid)?;
    let scope = crate::sequences::company_scope(cvr.as_deref(), 1, &label);
    let value = crate::sequences::next(tx, "document", &scope, floor)?;
    let number = format!("{prefix}{value:06}");
    let extension = r
        .original_filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let path = format!("documents/originals/{}.{}", r.sha256, extension);
    let retention = rentemester_engine::calendar::retention_deadline(today, month as u32)?;
    let payload = serde_json::to_string(
        &json!({"source":"dinero-import-bilag","documentType":r.document_type}),
    )
    .map_err(invalid)?;
    let id:i64=tx.query_row("INSERT INTO documents(document_no,source,original_filename,stored_path,mime_type,sha256_hash,document_type,currency,payload_json,retain_until,upload_datetime)VALUES(?1,'dinero-import-bilag',?2,?3,?4,?5,?6,'DKK',?7,?8,?9)RETURNING id",params![number,r.original_filename,path,r.mime_type,r.sha256,r.document_type,payload,retention,at],|r|r.get(0)).map_err(invalid)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES('document_ingest','document',?1,?2,?3,?4)",params![id.to_string(),format!("Ingested {number} ({})",r.sha256),actor,at]).map_err(invalid)?;
    Ok(id)
}
fn exception(db: &Connection, id: i64) -> Result<Value, Error> {
    db.query_row("SELECT type,severity,related_document_id,message,required_action,source_evidence,created_at FROM exceptions WHERE id=?1",[id],|r|Ok(json!([r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<i64>>(2)?,r.get::<_,String>(3)?,r.get::<_,Option<String>>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,String>(6)?]))).map_err(invalid)
}
pub(super) fn register(
    tx: &Transaction<'_>,
    p: &Prepared,
    journals: &[JournalProof],
    at: &str,
    actor: &str,
) -> Result<Vec<Proof>, Error> {
    let plan = plan(&p.decoded)?;
    if p.binding.receipt_plan != Some(serde_json::to_value(&plan).map_err(invalid)?) {
        return Err(refused("receipt plan differs from reviewed source"));
    }
    let mut out = vec![];
    for r in &plan.receipts {
        let bytes = raw(&p.decoded, r)?;
        let old = existing(tx, &r.sha256)?;
        let id = match old {
            Some(id) => id,
            None => new_document(tx, r, bytes, at, actor)?,
        };
        let doc = document(tx, id)?;
        let mut link_id = None;
        let mut exception_id = None;
        let mut task = None;
        if let Some(v) = &r.voucher_ref {
            let j = journals
                .iter()
                .find(|j| &j.voucher_ref == v)
                .ok_or_else(|| refused("source receipt has no actually posted journal"))?;
            let prior:Option<(i64,String,String)>=tx.query_row("SELECT id,source_system,voucher_ref FROM import_document_links WHERE document_id=?1 AND journal_entry_id=?2",params![id,j.id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(invalid)?;
            link_id = Some(if let Some((id, system, reference)) = prior {
                if system != "dinero" || reference != *v {
                    return Err(refused("existing source document link conflicts"));
                }
                id
            } else {
                tx.query_row("INSERT INTO import_document_links(source_system,voucher_ref,document_id,journal_entry_id,created_at)VALUES('dinero',?1,?2,?3,?4)RETURNING id",params![v,id,j.id,at],|row|row.get(0)).map_err(invalid)?
            });
        }
        if r.unbooked {
            let number = doc["documentNo"]
                .as_str()
                .ok_or_else(|| refused("source document number unavailable"))?;
            let message =
                format!("Imported receipt {number} is present but was never booked in dinero");
            let prior:Option<i64>=tx.query_row("SELECT id FROM exceptions WHERE status='open' AND type=?1 AND related_document_id=?2 AND related_bank_transaction_id IS NULL AND message=?3 AND required_action=?4 AND resolution_key IS NULL ORDER BY id LIMIT 1",params![TASK,id,message,ACTION],|row|row.get(0)).optional().map_err(invalid)?;
            let task_id = if let Some(id) = prior {
                id
            } else {
                let evidence=serde_json::to_string(&json!({"sourceSystem":"dinero","fileName":r.source_path,"documentNo":number,"sha256":r.sha256})).map_err(invalid)?;
                tx.query_row("INSERT INTO exceptions(type,severity,status,related_document_id,message,required_action,source_evidence,created_at)VALUES(?1,'medium','open',?2,?3,?4,?5,?6)RETURNING id",params![TASK,id,message,ACTION,evidence,at],|row|row.get(0)).map_err(invalid)?
            };
            exception_id = Some(task_id);
            task = Some(exception(tx, task_id)?);
        }
        out.push(Proof {
            source_path: r.source_path.clone(),
            document_id: id,
            document: doc,
            provenance_id: 0,
            link_id,
            exception_id,
            exception: task,
            duplicate: old.is_some(),
        });
    }
    Ok(out)
}
pub(super) fn summary(proofs: &[Proof]) -> Value {
    json!({"registered":proofs.iter().filter(|p|!p.duplicate).count(),"duplicates":proofs.iter().filter(|p|p.duplicate).count(),"linked":proofs.iter().filter(|p|p.link_id.is_some()).count(),"unbooked":proofs.iter().filter(|p|p.exception_id.is_some()).count()})
}
pub(super) fn link_provenance(
    tx: &Transaction<'_>,
    p: &Prepared,
    attempt: i64,
    inventory: i64,
    mut proofs: Vec<Proof>,
    at: &str,
) -> Result<Vec<Proof>, Error> {
    let plan = plan(&p.decoded)?;
    for (r, proof) in plan.receipts.iter().zip(&mut proofs) {
        let journal = r
            .voucher_ref
            .as_ref()
            .map(|v| {
                tx.query_row(
                    "SELECT journal_entry_id FROM import_document_links WHERE id=?1",
                    [proof.link_id.unwrap_or(0)],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|_| refused(format!("source voucher {v} link unavailable")))
            })
            .transpose()?;
        proof.provenance_id=tx.query_row("INSERT INTO dinero_import_document_links(attempt_id,inventory_id,entry_path,entry_sha256,document_id,journal_entry_id,voucher_reference,disposition,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)RETURNING id",params![attempt,inventory,r.source_path,r.sha256,proof.document_id,journal,r.voucher_ref,if r.unbooked{"excluded"}else{"linked"},at],|row|row.get(0)).map_err(invalid)?;
    }
    Ok(proofs)
}
pub(super) fn validate_event(db: &Connection, e: &Attestation) -> Result<(), Error> {
    let (Some(plan), Some(proofs)) = (&e.binding.receipt_plan, &e.supporting_receipts) else {
        if e.binding.receipt_plan.is_some() || e.supporting_receipts.is_some() {
            return Err(refused("source receipt plan/proof missing"));
        }
        let count: i64 = db
            .query_row(
                "SELECT count(*) FROM dinero_import_document_links WHERE attempt_id=?1",
                [e.attempt_id],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        if count != 0 {
            return Err(refused("unattested source document provenance"));
        }
        return Ok(());
    };
    let receipts = plan["receipts"]
        .as_array()
        .filter(|r| !r.is_empty())
        .ok_or_else(|| refused("source receipt plan shape unavailable"))?;
    if receipts.len() != proofs.len() || e.receipt["supportingDocuments"] != summary(proofs) {
        return Err(refused("source receipt proof inventory differs"));
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for (r, p) in receipts.iter().zip(proofs) {
        let path = r["sourcePath"]
            .as_str()
            .ok_or_else(|| refused("source receipt path unavailable"))?;
        let digest = r["sha256"]
            .as_str()
            .ok_or_else(|| refused("source receipt hash unavailable"))?;
        let unbooked = r["unbooked"]
            .as_bool()
            .ok_or_else(|| refused("source receipt disposition unavailable"))?;
        let source = e
            .binding
            .source
            .entries
            .iter()
            .find(|s| s.path == path)
            .ok_or_else(|| refused("receipt not in accepted inventory"))?;
        if p.source_path != path
            || !paths.insert(path)
            || !ids.insert(p.provenance_id)
            || p.provenance_id <= 0
            || source.sha256 != digest
            || json!(source.size_bytes) != r["sizeBytes"]
            || document(db, p.document_id)? != p.document
            || p.document["hash"] != digest
        {
            return Err(refused(
                "retained supporting document metadata or inventory differs",
            ));
        }
        if !p.duplicate {
            let filename = r["originalFilename"]
                .as_str()
                .ok_or_else(|| refused("receipt filename unavailable"))?;
            let extension = filename
                .rsplit('.')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            let expected_path = format!("documents/originals/{digest}.{extension}");
            let payload = serde_json::to_string(
                &json!({"source":"dinero-import-bilag","documentType":r["documentType"]}),
            )
            .map_err(invalid)?;
            if p.document["source"] != "dinero-import-bilag"
                || p.document["filename"] != filename
                || p.document["path"] != expected_path
                || p.document["mime"] != r["mimeType"]
                || p.document["type"] != r["documentType"]
                || p.document["currency"] != "DKK"
                || p.document["payload"] != payload
                || p.document["uploadedAt"] != e.accepted_at
            {
                return Err(refused("new source document derived metadata differs"));
            }
            // Retention is original company context, not today's mutable profile.
            let month = e.binding.context["company"][3]
                .as_u64()
                .ok_or_else(|| refused("source fiscal retention basis unavailable"))?;
            let retention = rentemester_engine::calendar::retention_deadline(
                &e.accepted_at[..10],
                u32::try_from(month).map_err(invalid)?,
            )?;
            if p.document["retainUntil"] != retention {
                return Err(refused("source original retention differs"));
            }
            let number = p.document["documentNo"]
                .as_str()
                .ok_or_else(|| refused("source original number unavailable"))?;
            let count:i64=db.query_row("SELECT count(*) FROM audit_log WHERE event_type='document_ingest' AND entity_type='document' AND entity_id=?1 AND actor=?2 AND created_at=?3 AND message=?4",params![p.document_id.to_string(),e.actor,e.accepted_at,format!("Ingested {number} ({digest})")],|r|r.get(0)).map_err(invalid)?;
            if count != 1 {
                return Err(refused("source document original audit differs"));
            }
        }
        let voucher = r["voucherRef"].as_str();
        let journal = if let Some(v) = voucher {
            Some(
                e.journals
                    .iter()
                    .find(|j| j.voucher_ref == v)
                    .ok_or_else(|| refused("receipt journal not in accepted source"))?
                    .id,
            )
        } else {
            None
        };
        if unbooked != (voucher.is_none())
            || unbooked != (p.exception_id.is_some())
            || unbooked != (p.exception.is_some())
            || p.link_id.is_some() != journal.is_some()
        {
            return Err(refused("source receipt effect disposition differs"));
        }
        let actual:Value=db.query_row("SELECT attempt_id,inventory_id,entry_path,entry_sha256,document_id,journal_entry_id,voucher_reference,disposition,created_at FROM dinero_import_document_links WHERE id=?1",[p.provenance_id],|r|Ok(json!([r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?,r.get::<_,Option<i64>>(5)?,r.get::<_,Option<String>>(6)?,r.get::<_,String>(7)?,r.get::<_,String>(8)?]))).map_err(invalid)?;
        if actual
            != json!([
                e.attempt_id,
                e.inventory_id,
                path,
                digest,
                p.document_id,
                journal,
                voucher,
                if unbooked { "excluded" } else { "linked" },
                e.accepted_at
            ])
        {
            return Err(refused("source document provenance differs"));
        }
        if let Some(link) = p.link_id {
            let actual:Value=db.query_row("SELECT source_system,voucher_ref,document_id,journal_entry_id FROM import_document_links WHERE id=?1",[link],|r|Ok(json!([r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?]))).map_err(invalid)?;
            if actual != json!(["dinero", voucher, p.document_id, journal]) {
                return Err(refused("source additive journal link differs"));
            }
        }
        if let Some(task_id) = p.exception_id {
            let task = exception(db, task_id)?;
            if Some(task.clone()) != p.exception
                || task[0] != TASK
                || task[1] != "medium"
                || task[2] != p.document_id
                || task[3]
                    != format!(
                        "Imported receipt {} is present but was never booked in dinero",
                        p.document["documentNo"].as_str().unwrap_or("")
                    )
                || task[4] != ACTION
            {
                return Err(refused("source unbooked review evidence differs"));
            }
            let evidence: Value = serde_json::from_str(
                task[5]
                    .as_str()
                    .ok_or_else(|| refused("source task evidence missing"))?,
            )
            .map_err(invalid)?;
            if evidence["sourceSystem"] != "dinero"
                || evidence["sha256"] != digest
                || evidence["documentNo"] != p.document["documentNo"]
                || !evidence["fileName"].is_string()
            {
                return Err(refused("source unbooked task document identity differs"));
            }
        }
    }
    let count: i64 = db
        .query_row(
            "SELECT count(*) FROM dinero_import_document_links WHERE attempt_id=?1",
            [e.attempt_id],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if count != proofs.len() as i64 {
        return Err(refused("unexpected supporting source provenance"));
    }
    Ok(())
}
pub fn supporting_originals(db: &Connection) -> Result<Vec<crate::documents::Evidence>, Error> {
    validate_history(db)?;
    let mut q = db
        .prepare("SELECT message FROM audit_log WHERE event_type=?1 ORDER BY id")
        .map_err(invalid)?;
    let mut rows = q.query([EVENT]).map_err(invalid)?;
    let mut ids = BTreeSet::new();
    let mut out = vec![];
    while let Some(r) = rows.next().map_err(invalid)? {
        let e: Attestation =
            serde_json::from_str(&r.get::<_, String>(0).map_err(invalid)?).map_err(invalid)?;
        for p in e.supporting_receipts.unwrap_or_default() {
            if ids.insert(p.document_id) {
                let d = document(db, p.document_id)?;
                out.push(crate::documents::Evidence {
                    path: d["path"]
                        .as_str()
                        .ok_or_else(|| refused("document path unavailable"))?
                        .into(),
                    hash: d["hash"]
                        .as_str()
                        .ok_or_else(|| refused("document hash unavailable"))?
                        .into(),
                    filename: d["filename"]
                        .as_str()
                        .ok_or_else(|| refused("document filename unavailable"))?
                        .into(),
                    mime: d["mime"]
                        .as_str()
                        .ok_or_else(|| refused("document MIME unavailable"))?
                        .into(),
                });
            }
        }
    }
    Ok(out)
}
