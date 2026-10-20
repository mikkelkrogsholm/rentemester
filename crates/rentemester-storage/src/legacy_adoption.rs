//! Retained conversion of one captured legacy company. This proves a converted
//! SQLite image, never writer authority, source retirement, or permission to
//! publish. The workspace owner must separately fence the original writer,
//! capture originals, review provenance, and admit a protected destination.
use crate::{Snapshot, company_baseline, company_physical, invalid};
use rentemester_engine::Error;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct SourceFileDigest {
    path: String,
    bytes: u64,
    sha256: String,
}
impl SourceFileDigest {
    pub(super) fn new(suffix: &str, bytes: &[u8]) -> Self {
        Self {
            path: format!("data/ledger.sqlite{suffix}"),
            bytes: bytes.len() as u64,
            sha256: sha(bytes),
        }
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}
#[derive(Debug, Serialize)]
pub struct CompanyInventory {
    ledger_uuid: String,
    originals: Vec<(String, String)>,
    bank_hashes: Vec<String>,
}
impl CompanyInventory {
    pub fn ledger_uuid(&self) -> &str {
        &self.ledger_uuid
    }
    pub fn originals(&self) -> &[(String, String)] {
        &self.originals
    }
    pub fn bank_hashes(&self) -> &[String] {
        &self.bank_hashes
    }
}
#[derive(Debug, Serialize)]
pub struct CompanyConversionReport {
    source_logical_digest: String,
    path_mappings: Vec<DocumentPathMapping>,
    retirement_binding: Option<Value>,
    source_kind: &'static str,
    catalogue_sha256: String,
    schema_images_sha256: String,
    physical_catalogue_sha256: Option<String>,
    conversion: Value,
    converted_sqlite_sha256: String,
    writer_authority: bool,
    source_retired: bool,
}
/// Deliberately non-Clone: a retained, verified image is distinct from a
/// disposable rehearsal. Possession does not authorize publication or writes.
pub struct ConvertedCompany {
    bytes: Vec<u8>,
    report: CompanyConversionReport,
    source_files: Vec<SourceFileDigest>,
    inventory: CompanyInventory,
}
impl ConvertedCompany {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn report(&self) -> &CompanyConversionReport {
        &self.report
    }
    pub fn source_files(&self) -> &[SourceFileDigest] {
        &self.source_files
    }
    pub fn inventory(&self) -> &CompanyInventory {
        &self.inventory
    }
    pub fn path_mappings(&self) -> &[DocumentPathMapping] {
        &self.report.path_mappings
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Capture with the existing stable NOFOLLOW reader and convert ONLY its private
/// copy. Only complete normalized prefixes 1–36 or the fixed recognized
/// unversioned physical baseline qualify. Empty and suffix-only databases do not.
pub fn convert_company_for_adoption(company: &Path) -> Result<ConvertedCompany, Error> {
    let (copy, source_files) = Snapshot::capture_with_inventory(company, "data", "ledger.sqlite")?;
    let (bytes, report, inventory) = copy.with_writable_copy(|db| convert_owned(db, company))?;
    Ok(ConvertedCompany {
        bytes,
        report,
        source_files,
        inventory,
    })
}
fn convert_owned(
    db: &mut Connection,
    company: &Path,
) -> Result<(Vec<u8>, CompanyConversionReport, CompanyInventory), Error> {
    let source_logical_digest = crate::legacy_retirement::source_digest(db)?;
    let retirement_binding = crate::legacy_retirement::strip_verified_overlay(db)?
        .map(serde_json::to_value)
        .transpose()
        .map_err(invalid)?;
    let objects = crate::migration::schema(db)?;
    if objects.is_empty() {
        return Err(Error::new(
            "unsupported_schema",
            "adoption requires an existing complete company",
        ));
    }
    let history: i64 = db
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .map_err(invalid)?;
    let (kind, mut conversion, mut canonical) = if history == 0 {
        let report = company_physical::rehearse(db)?;
        let canonical = company_physical::canonicalize(db, &report)?;
        (
            "recognized-unversioned-physical",
            serde_json::to_value(report).map_err(invalid)?,
            Some(canonical),
        )
    } else {
        let report = company_baseline::rehearse(db)?;
        (
            "full-normalized-company-prefix",
            serde_json::to_value(report).map_err(invalid)?,
            None,
        )
    };
    // The conversion image is retained, while its working files are discarded.
    // Keep the public rehearsal API/report unchanged.
    if let Some(object) = conversion.as_object_mut() {
        object.insert("private_copy_discarded".into(), Value::Bool(false));
        if let Some(migration) = object.get_mut("migration").and_then(Value::as_object_mut) {
            migration.insert("private_copy_discarded".into(), Value::Bool(false));
        }
    }
    let converted = canonical.as_mut().unwrap_or(db);
    let path_mappings = normalize_document_paths(converted, company)?;
    let native = crate::portable_backup::company(converted)?;
    let inventory = CompanyInventory {
        ledger_uuid: native.uuid,
        originals: native.originals,
        bank_hashes: native.bank_hashes,
    };
    let output = tempfile::tempdir().map_err(invalid)?;
    let path = output.path().join("converted.sqlite");
    converted
        .execute(
            "VACUUM main INTO ?1",
            [path
                .to_str()
                .ok_or_else(|| invalid("private output path"))?],
        )
        .map_err(invalid)?;
    // VACUUM INTO produces a standalone image. Reopen and prove the serialized
    // image itself, not merely the preceding working connection.
    let image = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(invalid)?;
    crate::portable_backup::company(&image)?;
    image.close().map_err(|(_, e)| invalid(e))?;
    let bytes = std::fs::read(&path).map_err(invalid)?;
    output.close().map_err(invalid)?;
    let report = CompanyConversionReport {
        source_logical_digest,
        path_mappings,
        retirement_binding,
        source_kind: kind,
        catalogue_sha256: sha(include_bytes!("legacy-schema.json")),
        schema_images_sha256: sha(include_bytes!("legacy-company-schemas.json")),
        physical_catalogue_sha256: (history == 0)
            .then(|| sha(include_bytes!("legacy-company-physical.json"))),
        conversion,
        converted_sqlite_sha256: sha(&bytes),
        writer_authority: false,
        source_retired: false,
    };
    Ok((bytes, report, inventory))
}

/// Exact original-path relocation. Bytes, document identity and accounting
/// evidence are unchanged; the host must capture source and publish target.
#[derive(Debug, Serialize)]
pub struct DocumentPathMapping {
    document_id: i64,
    original_stored_path: String,
    source_relative_path: String,
    target_relative_path: String,
    sha256: String,
}
impl DocumentPathMapping {
    pub fn document_id(&self) -> i64 {
        self.document_id
    }
    pub fn original_stored_path(&self) -> &str {
        &self.original_stored_path
    }
    pub fn source_relative_path(&self) -> &str {
        &self.source_relative_path
    }
    pub fn target_relative_path(&self) -> &str {
        &self.target_relative_path
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}
fn path_refused(error: impl std::fmt::Display) -> Error {
    Error::new("legacy_document_path_refused", error.to_string())
}
#[cfg(unix)]
fn verify_document_source(
    company: &Path,
    stored: &str,
    hash: &str,
    kind: &str,
    issued_binding: Option<&(String, String)>,
) -> Result<(String, String), Error> {
    use rustix::fs::{Mode, OFlags, open, openat};
    use std::{fs::File, io::Read, os::unix::fs::MetadataExt, path::Component};
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW;
    let selected = File::from(open(company, flags, Mode::empty()).map_err(path_refused)?);
    let path = Path::new(stored);
    if stored.contains('\\')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::RootDir))
        || stored.split('/').any(|part| part == "." || part == "..")
        || stored.contains("//")
    {
        return Err(path_refused("noncanonical registered document path"));
    }
    let (root, relative) = if path.is_absolute() {
        let parts = path.components().collect::<Vec<_>>();
        if parts.len() < 5 {
            return Err(path_refused("document root unavailable"));
        }
        let prefix = parts[..parts.len() - 3]
            .iter()
            .collect::<std::path::PathBuf>();
        let candidate = File::from(open(&prefix, flags, Mode::empty()).map_err(path_refused)?);
        let a = selected.metadata().map_err(path_refused)?;
        let b = candidate.metadata().map_err(path_refused)?;
        if (a.dev(), a.ino()) != (b.dev(), b.ino()) {
            return Err(path_refused(
                "registered document is outside selected company",
            ));
        }
        let relative = parts[parts.len() - 3..]
            .iter()
            .collect::<std::path::PathBuf>();
        (
            candidate,
            relative
                .to_str()
                .ok_or_else(|| path_refused("non-UTF8 registered path"))?
                .to_owned(),
        )
    } else {
        (selected, stored.to_owned())
    };
    let parts = relative.split('/').collect::<Vec<_>>();
    if parts.len() != 3
        || hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(path_refused("document path or SHA256 identity differs"));
    }
    let filename = parts[2];
    let numbered = issued_binding.filter(|(expected, _)| filename == expected);
    let extension = if numbered.is_some() {
        if kind == "issued_invoice" {
            "json"
        } else {
            "pdf"
        }
    } else {
        filename
            .strip_prefix(hash)
            .and_then(|v| v.strip_prefix('.'))
            .filter(|v| ["pdf", "png", "jpg", "jpeg", "txt", "json", "xml"].contains(v))
            .ok_or_else(|| {
                path_refused(
                    "document filename is not exact SHA256 or bound issued invoice identity",
                )
            })?
    };
    let prefix = (parts[0], parts[1]);
    let issued_store = prefix == ("invoices", "issued")
        && (numbered.is_some() || (extension == "pdf" && kind == "issued_invoice_pdf"));
    if (numbered.is_some() || prefix != ("documents", "originals")) && !issued_store {
        return Err(path_refused("unrecognized legacy document store"));
    }
    let first = openat(&root, parts[0], flags, Mode::empty()).map_err(path_refused)?;
    let second = openat(&first, parts[1], flags, Mode::empty()).map_err(path_refused)?;
    let fd = openat(
        &second,
        filename,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(path_refused)?;
    let file = File::from(fd);
    let meta = file.metadata().map_err(path_refused)?;
    const MAX: u64 = 32 * 1024 * 1024;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX {
        return Err(path_refused("document original type or bounds differ"));
    }
    let mut bytes = Vec::new();
    file.take(MAX + 1)
        .read_to_end(&mut bytes)
        .map_err(path_refused)?;
    if bytes.len() as u64 != meta.len() || sha(&bytes) != hash {
        return Err(path_refused(
            "document original bytes differ from registered SHA256",
        ));
    }
    if let Some((_, payload)) = numbered
        && ((kind == "issued_invoice" && bytes != payload.as_bytes())
            || (kind == "issued_invoice_pdf" && !bytes.starts_with(b"%PDF-")))
    {
        return Err(path_refused(
            "issued original differs from immutable snapshot or PDF format",
        ));
    }
    let target = format!("documents/originals/{hash}.{extension}");
    Ok((relative, target))
}
#[cfg(not(unix))]
fn verify_document_source(
    _: &Path,
    _: &str,
    _: &str,
    _: &str,
    _: Option<&(String, String)>,
) -> Result<(String, String), Error> {
    Err(path_refused(
        "descriptor-safe legacy document capture is not qualified on this platform",
    ))
}
// The TS issueInvoice writer and legacy renderIssuedInvoicePdf both bind the
// numbered originals to the same immutable logical snapshot and invoice number.
fn issued_path_binding(
    db: &Connection,
    id: i64,
    kind: &str,
) -> Result<Option<(String, String)>, Error> {
    if !["issued_invoice", "issued_invoice_pdf"].contains(&kind) {
        return Ok(None);
    }
    type Metadata = (
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        Option<String>,
    );
    let (number,document_no,original_filename,mime,source,status,payload):Metadata=db.query_row(
        "SELECT invoice_no,document_no,original_filename,mime_type,source,status,payload_json FROM documents WHERE id=?1",[id],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).map_err(path_refused)?;
    let (Some(number), Some(payload)) = (number, payload) else {
        return Ok(None);
    };
    if number.is_empty()
        || number.len() > 128
        || !number.bytes().all(|b| b.is_ascii_digit() || b == b'-')
        || source != "rentemester"
        || status != "issued"
    {
        return Ok(None);
    }
    let extension = if kind == "issued_invoice" {
        "json"
    } else {
        "pdf"
    };
    let filename = format!("{number}.{extension}");
    let expected_no = if kind == "issued_invoice" {
        number.clone()
    } else {
        format!("{number}-pdf")
    };
    if document_no.as_deref() != Some(expected_no.as_str())
        || original_filename.as_deref() != Some(filename.as_str())
        || mime.as_deref()
            != Some(if extension == "json" {
                "application/json"
            } else {
                "application/pdf"
            })
    {
        return Ok(None);
    }
    let logical=db.prepare("SELECT id,payload_json,sha256_hash FROM documents WHERE document_type='issued_invoice' AND invoice_no=?1 ORDER BY id").map_err(path_refused)?
        .query_map([&number],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,String>(2)?))).map_err(path_refused)?.collect::<Result<Vec<_>,_>>().map_err(path_refused)?;
    let [(logical_id, Some(logical_payload), logical_hash)] = logical.as_slice() else {
        return Ok(None);
    };
    let parsed: Value = serde_json::from_str(logical_payload).map_err(path_refused)?;
    if payload != *logical_payload
        || sha(logical_payload.as_bytes()) != *logical_hash
        || parsed.get("invoiceNumber").and_then(Value::as_str) != Some(number.as_str())
        || parsed.get("status").and_then(Value::as_str) != Some("issued")
        || (kind == "issued_invoice" && *logical_id != id)
    {
        return Ok(None);
    }
    if kind == "issued_invoice_pdf" {
        let count:i64=db.query_row("SELECT count(*) FROM documents WHERE document_type='issued_invoice_pdf' AND invoice_no=?1",[&number],|r|r.get(0)).map_err(path_refused)?;
        if count != 1 {
            return Ok(None);
        }
    }
    Ok(Some((filename, payload)))
}
fn normalize_document_paths(
    db: &mut Connection,
    company: &Path,
) -> Result<Vec<DocumentPathMapping>, Error> {
    use crate::migration::{integrity, project_digest, projection, quote, schema};
    let rows = db
        .prepare("SELECT id,stored_path,sha256_hash,document_type FROM documents ORDER BY id")
        .map_err(path_refused)?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(path_refused)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(path_refused)?;
    let mut mappings = Vec::new();
    for (id, path, hash, kind) in &rows {
        let path = path
            .as_deref()
            .ok_or_else(|| path_refused("registered document original unavailable"))?;
        let issued_binding = issued_path_binding(db, *id, kind)?;
        let (source, target) =
            verify_document_source(company, path, hash, kind, issued_binding.as_ref())?;
        mappings.push(DocumentPathMapping {
            document_id: *id,
            original_stored_path: path.into(),
            source_relative_path: source,
            target_relative_path: target,
            sha256: hash.clone(),
        });
    }
    if mappings
        .iter()
        .all(|m| m.original_stored_path == m.target_relative_path)
    {
        return Ok(mappings);
    }
    let objects = schema(db)?;
    let tables = objects
        .iter()
        .filter(|o| o.kind == "table")
        .map(|o| o.name.clone())
        .collect::<Vec<_>>();
    let mut columns = projection(db, &tables)?;
    for (name, cols) in &mut columns {
        if name == "documents" {
            cols.retain(|c| c != "stored_path");
        }
    }
    let before = project_digest(db, &columns)?;
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(path_refused)?;
    // The caller already proved the complete known schema. Only this private
    // conversion transaction removes exact admitted document UPDATE guards;
    // every original guard is restored before checking schema and data proof.
    let guards = objects
        .iter()
        .filter(|o| {
            o.kind == "trigger"
                && o.table == "documents"
                && o.sql.to_ascii_uppercase().contains("BEFORE UPDATE")
        })
        .collect::<Vec<_>>();
    for guard in &guards {
        tx.execute_batch(&format!("DROP TRIGGER {}", quote(&guard.name)))
            .map_err(path_refused)?;
    }
    for m in &mappings {
        let n = tx.execute("UPDATE documents SET stored_path=?1 WHERE id=?2 AND stored_path=?3 AND sha256_hash=?4",rusqlite::params![m.target_relative_path,m.document_id,m.original_stored_path,m.sha256]).map_err(path_refused)?;
        if n != 1 {
            return Err(path_refused("document path mapping row changed"));
        }
    }
    for guard in guards {
        tx.execute_batch(&guard.sql).map_err(path_refused)?;
    }
    let after_rows = tx
        .prepare("SELECT id,stored_path,sha256_hash,document_type FROM documents ORDER BY id")
        .map_err(path_refused)?
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(path_refused)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(path_refused)?;
    let expected = rows
        .into_iter()
        .zip(&mappings)
        .map(|((id, _, hash, kind), m)| (id, m.target_relative_path.clone(), hash, kind))
        .collect::<Vec<_>>();
    if schema(&tx)? != objects || project_digest(&tx, &columns)? != before || after_rows != expected
    {
        return Err(path_refused(
            "document relocation changed unrelated typed cells or paths",
        ));
    }
    integrity(&tx)?;
    tx.commit().map_err(path_refused)?;
    Ok(mappings)
}

#[cfg(test)]
pub(super) fn seed_test_journal(db: &Connection) {
    use crate::journal_hash::{Entry, Line, canonical_hash, json_number};
    db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('2990','Synthetic capital','equity','credit')").unwrap();
    let entry = Entry {
        id: 1,
        entry_no: "SYNTHETIC-ADOPT-1".into(),
        transaction_date: "2027-01-01".into(),
        text: "Synthetic adoption capital".into(),
        source_bank_transaction_id: None,
        document_id: None,
        currency: "DKK".into(),
        amount_foreign: None,
        amount_dkk: None,
        fx_rate_to_dkk: None,
        rule_version: "synthetic-adoption/1".into(),
        created_by: "agent:synthetic".into(),
        created_by_program: "synthetic-fixture".into(),
        status: "posted".into(),
        reversal_of_entry_id: None,
        lines: vec![
            Line {
                ordinal: 0,
                account_no: "1890".into(),
                debit_amount: json_number(175.25).unwrap(),
                credit_amount: json_number(0.0).unwrap(),
                vat_code: None,
                text: None,
            },
            Line {
                ordinal: 1,
                account_no: "2990".into(),
                debit_amount: json_number(0.0).unwrap(),
                credit_amount: json_number(175.25).unwrap(),
                vat_code: None,
                text: None,
            },
        ],
    };
    let hash = canonical_hash(&entry, "GENESIS").unwrap();
    db.execute("INSERT INTO journal_entries(id,entry_no,transaction_date,text,entry_hash,previous_hash,rule_version,created_by,created_by_program,status) VALUES(1,?1,?2,?3,?4,'GENESIS',?5,?6,?7,'posted')", rusqlite::params![entry.entry_no,entry.transaction_date,entry.text,hash,entry.rule_version,entry.created_by,entry.created_by_program]).unwrap();
    db.execute_batch("INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) SELECT 1,id,175.25,0 FROM accounts WHERE account_no='1890';INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount) SELECT 1,id,0,175.25 FROM accounts WHERE account_no='2990'").unwrap();
    assert!(crate::journal_hash::verify(db).unwrap().ok);
}

#[cfg(all(test, unix))]
mod path_tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("documents/originals")).unwrap();
        std::fs::create_dir_all(dir.path().join("invoices/issued")).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_company(&mut db, "Synthetic paths").unwrap();
        (dir, db)
    }
    fn document(db: &Connection, id: i64, path: &str, hash: &str, kind: &str) {
        // TS ingest persists retention at insertion (documents.ts); immutable
        // issued evidence must not rely on a later retention UPDATE.
        db.execute("INSERT INTO documents(id,source,stored_path,sha256_hash,document_type,retain_until) VALUES(?1,'synthetic',?2,?3,?4,'2031-12-31')",rusqlite::params![id,path,hash,kind]).unwrap();
    }
    #[test]
    fn exact_legacy_paths_relocate_private_rows_and_preserve_journal_and_originals() {
        let (dir, mut db) = fixture();
        db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('1890','Synthetic bank','asset','debit')").unwrap();
        seed_test_journal(&db);
        let pdf = b"%PDF-1.4 synthetic original";
        let json = b"{\"synthetic\":true}";
        let pdfhash = sha(pdf);
        let jsonhash = sha(json);
        let pdfrelative = format!("invoices/issued/{pdfhash}.pdf");
        let jsonrelative = format!("documents/originals/{jsonhash}.json");
        std::fs::write(dir.path().join(&pdfrelative), pdf).unwrap();
        std::fs::write(dir.path().join(&jsonrelative), json).unwrap();
        // Real TS storage uses an absolute lexical root; selected root can use
        // the OS's equivalent canonical alias, proved by directory identity.
        let selected = std::fs::canonicalize(dir.path()).unwrap();
        document(
            &db,
            1,
            dir.path().join(&pdfrelative).to_str().unwrap(),
            &pdfhash,
            "issued_invoice_pdf",
        );
        document(
            &db,
            2,
            dir.path().join(&jsonrelative).to_str().unwrap(),
            &jsonhash,
            "issued_invoice",
        );
        std::fs::create_dir(dir.path().join("data")).unwrap();
        let source_ledger = dir.path().join("data/ledger.sqlite");
        db.execute("VACUUM main INTO ?1", [source_ledger.to_str().unwrap()])
            .unwrap();
        let original_ledger_bytes = std::fs::read(&source_ledger).unwrap();
        let retained = convert_company_for_adoption(&selected).unwrap();
        assert_eq!(retained.path_mappings().len(), 2);
        assert_eq!(
            retained.path_mappings()[0].source_relative_path(),
            pdfrelative
        );
        assert_eq!(
            std::fs::read(&source_ledger).unwrap(),
            original_ledger_bytes
        );
        assert_eq!(
            retained.inventory().originals()[0].0,
            format!("documents/originals/{pdfhash}.pdf")
        );
        let original_schema = crate::migration::schema(&db).unwrap();
        let mappings = normalize_document_paths(&mut db, &selected).unwrap();
        assert_eq!(mappings.len(), 2);
        assert_eq!(mappings[0].source_relative_path(), pdfrelative);
        assert_eq!(
            mappings[0].target_relative_path(),
            format!("documents/originals/{pdfhash}.pdf")
        );
        assert_eq!(mappings[1].source_relative_path(), jsonrelative);
        assert_eq!(crate::migration::schema(&db).unwrap(), original_schema);
        assert!(crate::journal_hash::verify(&db).unwrap().ok);
        assert_eq!(std::fs::read(dir.path().join(pdfrelative)).unwrap(), pdf);
        assert_eq!(std::fs::read(dir.path().join(jsonrelative)).unwrap(), json);
        crate::portable_backup::company(&db).unwrap();
        assert!(
            db.execute("UPDATE documents SET source='changed' WHERE id=2", [])
                .is_err()
        );
    }
    fn numbered_documents(db: &Connection, dir: &Path, bad_pair: bool) -> (String, String) {
        let payload = r#"{"invoiceNumber":"2026-0001","status":"issued","issuedAt":"2026-10-20T12:00:00.000Z"}"#;
        let pdf = b"%PDF-1.4 synthetic numbered invoice";
        let jsonhash = sha(payload.as_bytes());
        let pdfhash = sha(pdf);
        std::fs::write(dir.join("invoices/issued/2026-0001.json"), payload).unwrap();
        std::fs::write(dir.join("invoices/issued/2026-0001.pdf"), pdf).unwrap();
        for (id, kind, mime, doc_no, ext, hash, snapshot) in [
            (
                1,
                "issued_invoice",
                "application/json",
                "2026-0001",
                "json",
                jsonhash.as_str(),
                payload,
            ),
            (
                2,
                "issued_invoice_pdf",
                "application/pdf",
                "2026-0001-pdf",
                "pdf",
                pdfhash.as_str(),
                if bad_pair { "{}" } else { payload },
            ),
        ] {
            let path = dir.join(format!("invoices/issued/2026-0001.{ext}"));
            db.execute("INSERT INTO documents(id,source,stored_path,sha256_hash,document_type,retain_until,invoice_no,document_no,original_filename,mime_type,status,payload_json) VALUES(?1,'rentemester',?2,?3,?4,'2031-12-31','2026-0001',?5,?6,?7,'issued',?8)",rusqlite::params![id,path.to_str().unwrap(),hash,kind,doc_no,format!("2026-0001.{ext}"),mime,snapshot]).unwrap();
        }
        (jsonhash, pdfhash)
    }
    #[test]
    fn actual_ts_numbered_invoice_originals_convert_without_rewriting_source_or_snapshot() {
        let (dir, db) = fixture();
        let (jsonhash, pdfhash) = numbered_documents(&db, dir.path(), false);
        std::fs::create_dir(dir.path().join("data")).unwrap();
        let path = dir.path().join("data/ledger.sqlite");
        db.execute("VACUUM main INTO ?1", [path.to_str().unwrap()])
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        let converted = convert_company_for_adoption(dir.path()).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), before);
        assert_eq!(
            converted.path_mappings()[0].source_relative_path(),
            "invoices/issued/2026-0001.json"
        );
        assert_eq!(
            converted.path_mappings()[0].target_relative_path(),
            format!("documents/originals/{jsonhash}.json")
        );
        assert_eq!(
            converted.path_mappings()[1].source_relative_path(),
            "invoices/issued/2026-0001.pdf"
        );
        assert_eq!(
            converted.path_mappings()[1].target_relative_path(),
            format!("documents/originals/{pdfhash}.pdf")
        );
        let result_path = dir.path().join("converted.sqlite");
        std::fs::write(&result_path, converted.bytes()).unwrap();
        let result = Connection::open(result_path).unwrap();
        assert_eq!(
            result
                .query_row(
                    "SELECT count(DISTINCT payload_json) FROM documents",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert!(
            result
                .execute("UPDATE documents SET payload_json='changed' WHERE id=1", [])
                .is_err()
        );
        assert!(
            result
                .execute("UPDATE documents SET stored_path='changed' WHERE id=2", [])
                .is_err()
        );
    }
    #[test]
    fn numbered_pdf_without_matching_immutable_snapshot_refuses() {
        let (dir, mut db) = fixture();
        numbered_documents(&db, dir.path(), true);
        let before = crate::test_support::typed_tables(&db);
        assert!(normalize_document_paths(&mut db, dir.path()).is_err());
        assert_eq!(crate::test_support::typed_tables(&db), before);
    }
    #[test]
    fn canonical_native_path_is_reported_without_relocation() {
        let (dir, mut db) = fixture();
        let bytes = b"synthetic canonical";
        let hash = sha(bytes);
        let relative = format!("documents/originals/{hash}.txt");
        std::fs::write(dir.path().join(&relative), bytes).unwrap();
        document(&db, 1, &relative, &hash, "cash_register_receipt");
        let mappings = normalize_document_paths(&mut db, dir.path()).unwrap();
        assert_eq!(mappings[0].original_stored_path(), relative);
        assert_eq!(mappings[0].target_relative_path(), relative);
    }
    #[test]
    fn outside_unknown_traversal_missing_and_tampered_paths_refuse_without_row_changes() {
        for variant in 0..8 {
            let (dir, mut db) = fixture();
            let bytes = b"synthetic refusal";
            let hash = sha(bytes);
            let relative = format!("documents/originals/{hash}.pdf");
            std::fs::write(dir.path().join(&relative), bytes).unwrap();
            let outside = tempfile::tempdir().unwrap();
            let path = match variant {
                0 => outside
                    .path()
                    .join(&relative)
                    .to_string_lossy()
                    .into_owned(),
                1 => format!("documents/unknown/{hash}.pdf"),
                2 => format!("documents/../originals/{hash}.pdf"),
                3 => format!("documents/originals/{hash}.exe"),
                4 => {
                    std::fs::remove_file(dir.path().join(&relative)).unwrap();
                    relative.clone()
                }
                5 => {
                    std::fs::write(dir.path().join(&relative), b"changed").unwrap();
                    relative.clone()
                }
                6 => {
                    std::fs::remove_dir_all(dir.path().join("documents/originals")).unwrap();
                    std::os::unix::fs::symlink(
                        outside.path(),
                        dir.path().join("documents/originals"),
                    )
                    .unwrap();
                    relative.clone()
                }
                _ => format!("invoices/issued/{hash}.pdf"),
            };
            document(&db, 1, &path, &hash, "cash_register_receipt");
            let before = crate::test_support::typed_tables(&db);
            assert!(
                normalize_document_paths(&mut db, dir.path()).is_err(),
                "variant{variant}"
            );
            assert_eq!(crate::test_support::typed_tables(&db), before);
        }
    }
}
