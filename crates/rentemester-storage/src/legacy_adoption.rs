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
    let (bytes, report, inventory) = copy.with_writable_copy(convert_owned)?;
    Ok(ConvertedCompany {
        bytes,
        report,
        source_files,
        inventory,
    })
}
fn convert_owned(
    db: &mut Connection,
) -> Result<(Vec<u8>, CompanyConversionReport, CompanyInventory), Error> {
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
    let (kind, mut conversion, canonical) = if history == 0 {
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
    let converted = canonical.as_ref().unwrap_or(db);
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
