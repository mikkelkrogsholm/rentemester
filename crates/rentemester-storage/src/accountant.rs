//! Accountant handoff built inside an admitted transaction; no file paths are opened here.
use crate::invalid;
use rentemester_engine::{
    Error,
    fiscal_year::{self, Date, LabelStrategy, StartMonth},
    money::Money,
    trial_balance::Period,
};
use rusqlite::{ToSql, Transaction, params, types::ValueRef};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub period_start: String,
    pub period_end: String,
    pub confirm: bool,
}
pub struct Download {
    pub bytes: Vec<u8>,
    pub journal_entry_count: usize,
    pub document_count: usize,
    pub bank_transaction_count: usize,
}
pub struct Original {
    pub id: i64,
    pub path: String,
    pub hash: String,
    pub exported_path: String,
}
pub struct Prepared {
    pub files: BTreeMap<String, Vec<u8>>,
    pub originals: Vec<Original>,
    pub manifest: Value,
    pub root: String,
}
fn refused(message: impl Into<String>) -> Error {
    Error::new("accountant_export_refused", message)
}
fn rows(tx: &Transaction<'_>, sql: &str, args: &[&dyn ToSql]) -> Result<Vec<Value>, Error> {
    let mut q = tx.prepare(sql).map_err(invalid)?;
    let names = q
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut rs = q.query(args).map_err(invalid)?;
    let mut output = vec![];
    while let Some(r) = rs.next().map_err(invalid)? {
        let mut object = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            let v = match r.get_ref(i).map_err(invalid)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(n) => serde_json::Number::from_f64(n)
                    .map(Value::Number)
                    .ok_or_else(|| refused("nonfinite persisted export value"))?,
                ValueRef::Text(s) => json!(std::str::from_utf8(s).map_err(invalid)?),
                ValueRef::Blob(_) => {
                    return Err(refused("unexpected binary field in export projection"));
                }
            };
            object.insert(name.clone(), v);
        }
        output.push(Value::Object(object));
    }
    Ok(output)
}
fn json_bytes(value: &Value) -> Result<Vec<u8>, Error> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(invalid)?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn cell(v: &Value) -> String {
    let s = match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        _ => v.to_string(),
    };
    if s.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}
fn csv_line(values: Vec<Value>) -> String {
    values.iter().map(cell).collect::<Vec<_>>().join(",") + "\r\n"
}
fn money(v: &Value) -> Result<Value, Error> {
    if v.is_null() {
        return Ok(Value::Null);
    }
    Ok(json!(Money::parse(&v.to_string())?.decimal()))
}
fn retention(record: &mut Value, basis: &str, start: i32) -> Result<(), Error> {
    if record["retainUntil"]
        .as_str()
        .is_some_and(|d| Date::parse(d).is_ok())
    {
        return Ok(());
    }
    let Some(date) = record[basis].as_str().filter(|d| Date::parse(d).is_ok()) else {
        return Ok(());
    };
    let fiscal = fiscal_year::for_date(
        Date::parse(date)?,
        StartMonth::new(start)?,
        LabelStrategy::EndYear,
    )?;
    let target = fiscal
        .end_year
        .checked_add(5)
        .filter(|y| *y <= 9999)
        .ok_or_else(|| refused("retention date exceeds supported range"))?;
    let month = if start == 1 { 12 } else { start - 1 } as u8;
    for day in (28..=31).rev() {
        if let Ok(date) = Date::new(target, month, day) {
            record["retainUntil"] = json!(date.to_string());
            break;
        }
    }
    Ok(())
}
pub fn prepare(tx: &Transaction<'_>, slug: &str, input: &Input) -> Result<Prepared, Error> {
    crate::journal_post::require_current(tx)?;
    if !input.confirm {
        return Err(refused("export confirmation required"));
    }
    let period = Period::new(&input.period_start, &input.period_end)?;
    if slug.is_empty()
        || !slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(refused("invalid company slug"));
    }
    let args: &[&dyn ToSql] = &[&period.start, &period.end];
    let companies = rows(
        tx,
        "SELECT id,name,country,currency,cvr,fiscal_year_start_month AS fiscalYearStartMonth,fiscal_year_label_strategy AS fiscalYearLabelStrategy,created_at AS createdAt FROM companies ORDER BY id",
        &[],
    )?;
    let start = companies
        .first()
        .and_then(|c| c["fiscalYearStartMonth"].as_i64())
        .ok_or_else(|| refused("company fiscal calendar missing"))? as i32;
    let mut journal = rows(
        tx,
        "SELECT id,entry_no AS entryNo,transaction_date AS transactionDate,registration_datetime AS registrationDatetime,text,document_id AS documentId,source_bank_transaction_id AS sourceBankTransactionId,currency,amount_foreign AS amountForeign,amount_dkk AS amountDkk,fx_rate_to_dkk AS fxRateToDkk,status,reversal_of_entry_id AS reversalOfEntryId,created_by AS createdBy,created_by_program AS createdByProgram,retain_until AS retainUntil FROM journal_entries WHERE transaction_date BETWEEN ?1 AND ?2 ORDER BY transaction_date,id",
        args,
    )?;
    let mut csv = csv_line(
        [
            "entry_no",
            "transaction_date",
            "registration_datetime",
            "entry_text",
            "account_no",
            "account_name",
            "debit",
            "credit",
            "vat_code",
            "line_text",
            "currency",
            "amount_foreign",
            "amount_dkk",
            "fx_rate_to_dkk",
            "status",
        ]
        .map(|s| json!(s))
        .to_vec(),
    );
    for entry in &mut journal {
        let lines = rows(
            tx,
            "SELECT a.account_no AS accountNo,a.name AS accountName,l.debit_amount AS debitAmount,l.credit_amount AS creditAmount,l.vat_code AS vatCode,l.text FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 ORDER BY l.id",
            &[&entry["id"]
                .as_i64()
                .ok_or_else(|| refused("entry identity"))?],
        )?;
        for line in &lines {
            let mut values = ["entryNo", "transactionDate", "registrationDatetime", "text"]
                .map(|k| entry[k].clone())
                .to_vec();
            values.extend([
                line["accountNo"].clone(),
                line["accountName"].clone(),
                money(&line["debitAmount"])?,
                money(&line["creditAmount"])?,
                line["vatCode"].clone(),
                line["text"].clone(),
                entry["currency"].clone(),
                entry["amountForeign"].clone(),
                entry["amountDkk"].clone(),
                entry["fxRateToDkk"].clone(),
                entry["status"].clone(),
            ]);
            csv += &csv_line(values);
        }
        entry["lines"] = json!(lines);
        retention(entry, "transactionDate", start)?;
    }
    let mut documents = rows(
        tx,
        "SELECT id,document_no AS documentNo,document_type AS documentType,invoice_no AS invoiceNo,invoice_date AS invoiceDate,original_filename AS originalFilename,stored_path AS storedPath,mime_type AS mimeType,source,amount_inc_vat AS amountIncVat,vat_amount AS vatAmount,payload_json AS payloadJson,supplier_name AS supplierName,sender_vat_cvr AS supplierVatOrCvr,supplier_country_code AS supplierCountryCode,supplier_identifier_kind AS supplierIdentifierKind,supplier_identity_status AS supplierIdentityStatus,status,retain_until AS retainUntil,sha256_hash AS sha256 FROM documents WHERE invoice_date BETWEEN ?1 AND ?2 OR id IN (SELECT document_id FROM journal_entries WHERE transaction_date BETWEEN ?1 AND ?2) ORDER BY id",
        args,
    )?;
    let mut originals = vec![];
    for d in &mut documents {
        retention(d, "invoiceDate", start)?;
        let payload: Value = match d["payloadJson"].as_str() {
            Some(s) => serde_json::from_str(s).map_err(invalid)?,
            None => Value::Null,
        };
        d["purchaseVatLines"] = payload["purchaseVatLines"].clone();
        if !d["purchaseVatLines"].is_null() {
            let metadata: rentemester_engine::document::Metadata = serde_json::from_value(payload)
                .map_err(|_| refused("purchase VAT split metadata cannot be validated"))?;
            metadata.validate()?;
        }
        d.as_object_mut().unwrap().remove("payloadJson");
        if let Some(path) = d["storedPath"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
        {
            let id = d["id"]
                .as_i64()
                .ok_or_else(|| refused("document identity"))?;
            let name = d["originalFilename"]
                .as_str()
                .or_else(|| path.rsplit('/').next())
                .unwrap_or("original");
            let name: String = name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || "._-".contains(c) {
                        c
                    } else {
                        '_'
                    }
                })
                .take(70)
                .collect();
            let target = format!("documents-readable/document-{id}-{name}");
            let hash = d["sha256"]
                .as_str()
                .ok_or_else(|| refused("registered original hash missing"))?
                .to_owned();
            originals.push(Original {
                id,
                path: path.clone(),
                hash,
                exported_path: target.clone(),
            });
            d["storedPathRelativeToCompany"] = json!(path);
            d["exportedReadablePath"] = json!(target);
        } else {
            d["storedPathRelativeToCompany"] = Value::Null;
            d["exportedReadablePath"] = Value::Null;
        }
    }
    let mut bank = rows(
        tx,
        "SELECT id,transaction_date AS transactionDate,booking_date AS bookingDate,text,amount,currency,amount_dkk AS amountDkk,fx_rate_to_dkk AS fxRateToDkk,reference,import_batch_id AS importBatchId,status,retain_until AS retainUntil FROM bank_transactions WHERE COALESCE(booking_date,transaction_date) BETWEEN ?1 AND ?2 OR id IN (SELECT source_bank_transaction_id FROM journal_entries WHERE transaction_date BETWEEN ?1 AND ?2) OR id IN (SELECT bank_transaction_id FROM bank_journal_reconciliations WHERE journal_entry_id IN (SELECT id FROM journal_entries WHERE transaction_date BETWEEN ?1 AND ?2)) ORDER BY id",
        args,
    )?;
    for row in &mut bank {
        let basis = if row["bookingDate"].is_null() {
            "transactionDate"
        } else {
            "bookingDate"
        };
        retention(row, basis, start)?;
    }
    let links = rows(
        tx,
        "SELECT link.id,link.bank_transaction_id,link.journal_entry_id,link.match_method,link.source_reference,link.note,link.created_by,link.created_by_program,link.created_at FROM bank_journal_reconciliation_links link JOIN bank_transactions bt ON bt.id=link.bank_transaction_id JOIN journal_entries je ON je.id=link.journal_entry_id WHERE COALESCE(bt.booking_date,bt.transaction_date) BETWEEN ?1 AND ?2 OR je.transaction_date BETWEEN ?1 AND ?2 ORDER BY link.id",
        args,
    )?;
    let audit = rows(
        tx,
        "SELECT id,event_type AS eventType,entity_type AS entityType,entity_id AS entityId,message,actor,created_at AS createdAt FROM audit_log WHERE substr(created_at,1,10) BETWEEN ?1 AND ?2 ORDER BY id",
        args,
    )?;
    let exceptions = rows(
        tx,
        "SELECT id,type,severity,status,related_bank_transaction_id AS relatedBankTransactionId,related_document_id AS relatedDocumentId,message,required_action AS requiredAction,created_at AS createdAt,resolved_at AS resolvedAt,resolved_by AS resolvedBy,resolution_note AS resolutionNote FROM exceptions WHERE substr(created_at,1,10) BETWEEN ?1 AND ?2 OR (status='open' AND substr(created_at,1,10)<?1) ORDER BY id",
        args,
    )?;
    let mut accounts = rows(
        tx,
        "SELECT id,account_no AS accountNo,name,type,normal_balance AS normalBalance,active,default_vat_code AS defaultVatCode,allow_direct_posting AS allowDirectPosting FROM accounts ORDER BY account_no",
        &[],
    )?;
    for a in &mut accounts {
        for field in ["active", "allowDirectPosting"] {
            a[field] = json!(a[field].as_i64() == Some(1));
        }
    }
    let migrations = rows(
        tx,
        "SELECT id,name,checksum,applied_at AS appliedAt,applied_by_version AS appliedByVersion,applied_by_commit AS appliedByCommit FROM schema_migrations ORDER BY id",
        &[],
    )?;
    let report = crate::trial_balance_owned(tx, period.clone())?;
    let mut trial_csv = csv_line(
        [
            "account_no",
            "account_name",
            "type",
            "normal_balance",
            "debit",
            "credit",
            "balance",
        ]
        .map(|s| json!(s))
        .to_vec(),
    );
    for a in &report.accounts {
        trial_csv += &csv_line(vec![
            json!(a.account_no),
            json!(a.name),
            json!(a.kind),
            json!(a.normal_balance),
            json!(a.debit.decimal()),
            json!(a.credit.decimal()),
            json!(a.balance.decimal()),
        ]);
    }
    trial_csv += &csv_line(vec![
        json!("TOTAL"),
        Value::Null,
        Value::Null,
        Value::Null,
        json!(report.total_debit.decimal()),
        json!(report.total_credit.decimal()),
        json!(report.total_debit.subtract(&report.total_credit).decimal()),
    ]);
    let mut files = BTreeMap::new();
    let mut paths = serde_json::Map::new();
    for (key, name, data) in [
        ("journalEntries", "journal-entries.json", json!(journal)),
        ("documents", "documents.json", json!(documents)),
        ("bankTransactions", "bank-transactions.json", json!(bank)),
        (
            "bankJournalReconciliationLinks",
            "bank-journal-reconciliation-links.json",
            json!(links),
        ),
        ("auditLog", "audit-log.json", json!(audit)),
        ("exceptions", "exceptions.json", json!(exceptions)),
        ("accounts", "accounts.json", json!(accounts)),
        ("companies", "companies.json", json!(companies)),
        (
            "schemaMigrations",
            "schema-migrations.json",
            json!(migrations),
        ),
        (
            "vatReport",
            "vat-report.json",
            crate::vat_report::range(tx, &period.start, &period.end)?,
        ),
    ] {
        let path = format!("machine-readable/{name}");
        paths.insert(key.into(), json!(path));
        files.insert(path, json_bytes(&data)?);
    }
    files.insert(
        "machine-readable/journal-entries.csv".into(),
        csv.into_bytes(),
    );
    paths.insert(
        "journalEntriesCsv".into(),
        json!("machine-readable/journal-entries.csv"),
    );
    files.insert(
        "machine-readable/trial-balance.csv".into(),
        trial_csv.into_bytes(),
    );
    paths.insert(
        "trialBalanceCsv".into(),
        json!("machine-readable/trial-balance.csv"),
    );
    paths.insert("readableDocumentsDir".into(), json!("documents-readable"));
    paths.insert("readme".into(), json!("README.txt"));
    files.insert("README.txt".into(),format!("Rentemester — revisor-eksportpakke\n\nPeriode: {}..{}\nGenereret (deterministisk, udledt af periodeslut — ikke et reelt ur-tidspunkt): {}T23:59:59.000Z\n\nLokal overdragelsespakke uden adgang til virksomhedens kørende data.\nJSON og CSV findes i machine-readable/. Originaler findes i documents-readable/. Manifestet angiver SHA-256 og byteantal for hver fil.\nEksporten ændrer ingen posteringer. Audit-loggen indeholder revisionssporet før denne eksport; eksporthandlingen registreres efter pakningen.\n\nBegrænsning: Rust-buildets fulde release-/regelkorpus-proveniens er endnu ikke tilgængelig; provenance er null. Skemaets migrationsidentiteter følger med. Pakken er ikke en myndighedseksport eller en signeret backup.\n",period.start,period.end,period.end).into_bytes());
    let root = format!(
        "revisor-eksport-{}_{}_{}T235959Z",
        period.start,
        period.end,
        period.end.replace('-', "")
    );
    let manifest = json!({"manifestVersion":2,"provenance":null,"packageType":"accountant_handoff_export","generatedAt":format!("{}T23:59:59.000Z",period.end),"generatedAtExplicit":false,"requestedAt":null,"deadlineAt":null,"requester":null,"periodStart":period.start,"periodEnd":period.end,"sourceCompanyRootName":slug,"appliedRules":["DK-BOOKKEEPING-AUTHORITY-EXPORT-001"],"machineReadableFormat":"json+csv","files":paths,"counts":{"journalEntries":journal.len(),"bankTransactions":bank.len(),"bankJournalReconciliationLinks":links.len(),"documents":documents.len(),"auditLog":audit.len(),"exceptions":exceptions.len(),"accounts":accounts.len(),"companies":companies.len(),"schemaMigrations":migrations.len(),"copiedReadableDocuments":originals.len()},"handoffModel":"local_export_package","accessModel":"no_runtime_access","outOfScope":["hosted_multi_user_access","role_based_write_access","real_time_collaboration"],"limitations":["native_release_provenance_unavailable"]});
    Ok(Prepared {
        files,
        originals,
        manifest,
        root,
    })
}
pub fn finish(
    tx: &Transaction<'_>,
    mut prepared: Prepared,
    original_bytes: Vec<Vec<u8>>,
    actor: &str,
) -> Result<Download, Error> {
    if original_bytes.len() != prepared.originals.len() {
        return Err(refused("original evidence count mismatch"));
    }
    let mut copied = vec![];
    for (original, bytes) in prepared.originals.iter().zip(original_bytes) {
        let hash = format!("{:x}", Sha256::digest(&bytes));
        if hash != original.hash {
            return Err(refused("original evidence integrity mismatch"));
        }
        copied.push(json!({"documentId":original.id,"sourcePathRelativeToCompany":original.path,"exportedPath":original.exported_path,"sha256":hash,"sizeBytes":bytes.len()}));
        prepared.files.insert(original.exported_path.clone(), bytes);
    }
    let outputs:Vec<_>=prepared.files.iter().map(|(path,bytes)|json!({"path":path,"sha256":format!("{:x}",Sha256::digest(bytes)),"sizeBytes":bytes.len()})).collect();
    prepared.manifest["outputs"] = json!(outputs);
    prepared.manifest["copiedDocuments"] = json!(copied);
    prepared
        .files
        .insert("manifest.json".into(), json_bytes(&prepared.manifest)?);
    let files = prepared
        .files
        .into_iter()
        .map(|(path, bytes)| (format!("{}/{path}", prepared.root), bytes))
        .collect();
    let bytes = tar(files)?;
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('accountant_handoff_export','company','1',?1,?2)",params![format!("Exported accountant_handoff_export for {}..{} to {}",prepared.manifest["periodStart"].as_str().unwrap(),prepared.manifest["periodEnd"].as_str().unwrap(),prepared.root),format!("user:{actor}")]).map_err(invalid)?;
    Ok(Download {
        bytes,
        journal_entry_count: prepared.manifest["counts"]["journalEntries"]
            .as_u64()
            .unwrap() as usize,
        document_count: prepared.manifest["counts"]["documents"].as_u64().unwrap() as usize,
        bank_transaction_count: prepared.manifest["counts"]["bankTransactions"]
            .as_u64()
            .unwrap() as usize,
    })
}
/// Deterministic regular-file USTAR, identical metadata conventions to the existing tar contract.
pub fn tar(files: BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, Error> {
    fn octal(target: &mut [u8], value: usize) -> Result<(), Error> {
        target.fill(0);
        let text = format!("{value:0width$o}", width = target.len() - 1);
        if text.len() >= target.len() {
            return Err(refused("archive field exceeds USTAR limit"));
        }
        target[..text.len()].copy_from_slice(text.as_bytes());
        Ok(())
    }
    let mut output = vec![];
    for (path, bytes) in files {
        if path.starts_with('/')
            || path.contains('\0')
            || path
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
        {
            return Err(refused("unsafe archive path"));
        }
        let (prefix, name) = if path.len() <= 100 {
            ("", path.as_str())
        } else {
            path.match_indices('/')
                .find_map(|(i, _)| {
                    if i <= 155 && path.len() - i - 1 <= 100 {
                        Some((&path[..i], &path[i + 1..]))
                    } else {
                        None
                    }
                })
                .ok_or_else(|| refused("archive path exceeds USTAR limit"))?
        };
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        header[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
        octal(&mut header[100..108], 0o644)?;
        octal(&mut header[108..116], 0)?;
        octal(&mut header[116..124], 0)?;
        octal(&mut header[124..136], bytes.len())?;
        octal(&mut header[136..148], 0)?;
        header[148..156].fill(b' ');
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        octal(&mut header[329..337], 0)?;
        octal(&mut header[337..345], 0)?;
        let checksum: usize = header.iter().map(|b| *b as usize).sum();
        octal(&mut header[148..155], checksum)?;
        header[155] = b' ';
        if output
            .len()
            .saturating_add(bytes.len())
            .saturating_add(1536)
            > 256 * 1024 * 1024
        {
            return Err(refused("accountant archive exceeds 256 MiB limit"));
        }
        output.extend_from_slice(&header);
        output.extend_from_slice(&bytes);
        output.resize(output.len().div_ceil(512) * 512, 0);
    }
    output.resize(output.len() + 1024, 0);
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (rusqlite::Connection, rusqlite::Connection, Vec<u8>) {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        let mut l = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Accountant").unwrap();
        let original = b"Synthetic original receipt\n".to_vec();
        let hash = format!("{:x}", Sha256::digest(&original));
        l.execute_batch("INSERT INTO accounts(id,account_no,name,type,normal_balance) VALUES(1,'A','Synthetic bank','asset','debit'),(2,'X','Synthetic expense','expense','debit');
            INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,entry_hash,created_by) VALUES(1,'SYN-1','2026-07-01','Synthetic, quoted \"expense\"','fixture','synthetic-hash','agent:synthetic');
            INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(1,2,100.01,0),(1,1,0,100.01);
            INSERT INTO bank_transactions(transaction_date,text,amount,currency,reference) VALUES('2026-07-01','Synthetic bank transaction',-100.01,'DKK','synthetic-reference');
            INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at) VALUES('synthetic_before_export','company','1','Synthetic original audit','agent:synthetic','2026-07-01 12:00:00');").unwrap();
        l.execute("INSERT INTO documents(id,document_no,document_type,source,sha256_hash,original_filename,stored_path,mime_type,invoice_date,amount_inc_vat,vat_amount) VALUES(1,'DOC-2026-000001','cash_register_receipt','synthetic',?1,'synthetic.txt',?2,'text/plain','2026-06-30',100.01,0)",params![hash,format!("documents/originals/{hash}.txt")]).unwrap();
        // Linked original lies outside invoice-date filter but belongs to the exported journal.
        l.execute_batch("INSERT INTO journal_entries(id,entry_no,transaction_date,text,document_id,rule_version,entry_hash,created_by) VALUES(2,'SYN-2','2026-07-02','Synthetic linked original outside period',1,'fixture','synthetic-hash-2','agent:synthetic'); INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) VALUES(2,2,1,0),(2,1,0,1);").unwrap();
        (c, l, original)
    }
    pub(super) fn entries(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
        let mut result = BTreeMap::new();
        let mut cursor = 0;
        while bytes[cursor..cursor + 512].iter().any(|b| *b != 0) {
            let h = &bytes[cursor..cursor + 512];
            let read = |s: &[u8]| {
                String::from_utf8(s.iter().copied().take_while(|b| *b != 0).collect()).unwrap()
            };
            let name = read(&h[..100]);
            let prefix = read(&h[345..500]);
            let path = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let size = usize::from_str_radix(read(&h[124..136]).trim(), 8).unwrap();
            let checksum = usize::from_str_radix(read(&h[148..154]).trim(), 8).unwrap();
            let expected: usize = h
                .iter()
                .enumerate()
                .map(|(i, b)| {
                    if (148..156).contains(&i) {
                        32
                    } else {
                        *b as usize
                    }
                })
                .sum();
            assert_eq!(checksum, expected);
            assert_eq!(&h[257..263], b"ustar\0");
            result.insert(path, bytes[cursor + 512..cursor + 512 + size].to_vec());
            cursor += 512 + size.div_ceil(512) * 512;
        }
        assert!(bytes[cursor..].iter().all(|b| *b == 0));
        result
    }
    #[test]
    fn complete_archive_hashes_csv_original_and_audit_without_journal_effects() {
        let (_c, mut l, original) = fixture();
        // Invoice date includes the selected original; unrelated company records cannot enter this DB.
        let input = Input {
            period_start: "2026-07-01".into(),
            period_end: "2026-07-31".into(),
            confirm: true,
        };
        let tx = l
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let prepared = prepare(&tx, "synthetic", &input).unwrap();
        assert_eq!(prepared.originals.len(), 1);
        let root = prepared.root.clone();
        let first = finish(&tx, prepared, vec![original.clone()], "synthetic-exporter").unwrap();
        assert_eq!(first.journal_entry_count, 2);
        assert_eq!(first.document_count, 1);
        assert_eq!(first.bank_transaction_count, 1);
        let files = entries(&first.bytes);
        let manifest: Value =
            serde_json::from_slice(&files[&format!("{root}/manifest.json")]).unwrap();
        assert_eq!(manifest["manifestVersion"], 2);
        assert_eq!(manifest["packageType"], "accountant_handoff_export");
        assert!(manifest["provenance"].is_null());
        for output in manifest["outputs"].as_array().unwrap() {
            let path = output["path"].as_str().unwrap();
            let bytes = &files[&format!("{root}/{path}")];
            assert_eq!(output["sizeBytes"].as_u64().unwrap(), bytes.len() as u64);
            assert_eq!(output["sha256"], format!("{:x}", Sha256::digest(bytes)));
        }
        let copied = &manifest["copiedDocuments"][0];
        assert_eq!(
            files[&format!("{root}/{}", copied["exportedPath"].as_str().unwrap())],
            original
        );
        let journal_csv = String::from_utf8(
            files[&format!("{root}/machine-readable/journal-entries.csv")].clone(),
        )
        .unwrap();
        assert!(journal_csv.contains("\"Synthetic, quoted \"\"expense\"\"\""));
        assert!(journal_csv.contains("100.01"));
        let audits: Value =
            serde_json::from_slice(&files[&format!("{root}/machine-readable/audit-log.json")])
                .unwrap();
        assert_eq!(audits[0]["message"], "Synthetic original audit");
        assert_eq!(
            tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(tx.query_row("SELECT count(*) FROM audit_log WHERE event_type='accountant_handoff_export' AND actor='user:synthetic-exporter'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        assert_eq!(
            tx.query_row("SELECT count(*) FROM sequences", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        tx.commit().unwrap();
    }
    #[test]
    fn failure_rolls_back_audit_and_ustar_is_deterministic_safe_and_compatible() {
        let (_c, mut l, _original) = fixture();
        let input = Input {
            period_start: "2026-07-01".into(),
            period_end: "2026-07-31".into(),
            confirm: true,
        };
        let tx = l
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let p = prepare(&tx, "synthetic", &input).unwrap();
        assert!(finish(&tx, p, vec![b"tampered".to_vec()], "synthetic").is_err());
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM audit_log WHERE event_type='accountant_handoff_export'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        let mut bad = Input {
            period_start: "2026-02-30".into(),
            period_end: "2026-07-31".into(),
            confirm: true,
        };
        assert!(prepare(&tx, "synthetic", &bad).is_err());
        bad.period_start = "2026-07-01".into();
        bad.confirm = false;
        assert!(prepare(&tx, "synthetic", &bad).is_err());
        let files = BTreeMap::from([
            ("root/subdirectory/a.txt".into(), b"abc".to_vec()),
            ("root/z.bin".into(), vec![0, 1, 255]),
        ]);
        let bytes = tar(files.clone()).unwrap();
        assert_eq!(bytes, tar(files.clone()).unwrap());
        assert_eq!(entries(&bytes), files);
        for path in ["../escape", "root/../escape", "/absolute", "root//empty"] {
            assert!(tar(BTreeMap::from([(path.into(), vec![])])).is_err());
        }
    }
}
