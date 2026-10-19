//! Source-read-only reports/rehearsals and caller-owned transactional primitives.
use rentemester_engine::{
    Error,
    money::Money,
    trial_balance::{self, Line, Period, Report},
};
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde::Deserialize;
use std::{fs::OpenOptions, io::Write, path::Path, time::Duration};
use tempfile::TempDir;
mod company_baseline;
mod company_normalization;
mod company_physical;
mod company_vat;
#[cfg(test)]
mod test_support;
pub use company_physical::{PhysicalRehearsalReport, rehearse_company_baseline};
pub mod control_access;
mod control_migration;
pub mod cookie_session;
pub mod email_verification;
mod journal_hash;
pub mod journal_preview;
pub mod mfa_login;
pub mod password_change;
pub mod password_credential;
pub mod password_reset;
pub mod reset_request;
pub mod session_control;
pub mod session_view;
pub use journal_preview::{
    PreviewInput, PreviewReport, manual_journal_rule_version, preview_manual_journal,
};
mod migration;
pub mod sequences;
pub use control_migration::{ControlRehearsalReport, rehearse_control_migration};
pub use migration::{RehearsalReport, rehearse_company_migration};
#[cfg(unix)]
mod snapshot;
pub use journal_hash::Report as JournalHashReport;

/// Journal canonical hashes only; not a complete audit/accounting integrity check.
pub fn verify_journal_hashes(company: &Path) -> Result<JournalHashReport, Error> {
    let copy = Snapshot::capture(company)?;
    journal_hash::verify(&copy.conn)
}

#[derive(Deserialize)]
struct Migration {
    id: i64,
    name: String,
    checksum: String,
}
fn invalid(error: impl std::fmt::Display) -> Error {
    Error::new("invalid_ledger", error.to_string())
}

/// Copy, validate and report one explicitly selected local company. No migration.
pub fn trial_balance(company: &Path, period: Period) -> Result<Report, Error> {
    let period = Period::new(&period.start, &period.end)?;
    let copy = Snapshot::capture(company)?;
    trial_balance_connection(&copy.conn, period)
}
/// Read an already owned transaction without snapshotting/reopening any inode.
/// Caller must hold workspace lifetime and current company-read authorization.
pub fn trial_balance_owned(
    company: &rusqlite::Transaction<'_>,
    period: Period,
) -> Result<Report, Error> {
    if company
        .transaction_state(Some(rusqlite::MAIN_DB))
        .map_err(invalid)?
        == rusqlite::TransactionState::None
    {
        return Err(invalid("report requires held transaction"));
    }
    validate(company)?;
    trial_balance_connection(company, Period::new(&period.start, &period.end)?)
}
fn trial_balance_connection(company: &Connection, period: Period) -> Result<Report, Error> {
    let mut statement=company.prepare("SELECT a.account_no,a.name,a.type,a.normal_balance,je.id,jl.debit_amount,jl.credit_amount FROM journal_entries je JOIN journal_lines jl ON jl.journal_entry_id=je.id JOIN accounts a ON a.id=jl.account_id WHERE je.transaction_date>=?1 AND je.transaction_date<=?2 ORDER BY a.account_no,je.id,jl.id").map_err(invalid)?;
    let mut rows = statement
        .query([&period.start, &period.end])
        .map_err(invalid)?;
    let mut lines = Vec::new();
    while let Some(row) = rows.next().map_err(invalid)? {
        let kind: String = row.get(2).map_err(invalid)?;
        let normal: String = row.get(3).map_err(invalid)?;
        if !["asset", "liability", "equity", "income", "expense", "vat"].contains(&kind.as_str())
            || !["debit", "credit"].contains(&normal.as_str())
        {
            return Err(invalid("invalid account classification"));
        }
        lines.push(Line {
            account_no: row.get(0).map_err(invalid)?,
            name: row.get(1).map_err(invalid)?,
            kind,
            normal_balance: normal,
            entry_id: safe_entry_id(row.get(4).map_err(invalid)?)?,
            debit: legacy_money(row.get_ref(5).map_err(invalid)?)?,
            credit: legacy_money(row.get_ref(6).map_err(invalid)?)?,
        });
    }
    Ok(trial_balance::build(period, lines))
}
struct Snapshot {
    conn: Connection,
    _dir: TempDir,
}
impl Snapshot {
    fn capture(company: &Path) -> Result<Self, Error> {
        let copy = Self::capture_unvalidated(company)?;
        validate(&copy.conn)?;
        Ok(copy)
    }
    fn capture_unvalidated(company: &Path) -> Result<Self, Error> {
        Self::capture_at(company, "data", "ledger.sqlite")
    }
    fn capture_control(workspace: &Path) -> Result<Self, Error> {
        Self::capture_at(workspace, ".rentemester", "workspace-control.sqlite")
    }
    fn capture_at(root: &Path, directory: &str, filename: &str) -> Result<Self, Error> {
        #[cfg(not(unix))]
        {
            let _ = (root, directory, filename);
            Err(Error::new(
                "source_unavailable",
                "this reader has not yet qualified source capture on this platform",
            ))
        }
        #[cfg(unix)]
        {
            let captured = snapshot::capture_at(root, directory, filename)?;
            let dir =
                tempfile::tempdir().map_err(|e| Error::new("storage_failure", e.to_string()))?;
            let path = dir.path().join("snapshot.sqlite");
            let result = (|| {
                for source in captured {
                    if source.suffix == "-shm" {
                        continue;
                    }
                    let mut options = OpenOptions::new();
                    options.write(true).create_new(true);
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                    let mut file = options
                        .open(format!("{}{}", path.display(), source.suffix))
                        .map_err(|e| Error::new("storage_failure", e.to_string()))?;
                    file.write_all(&source.bytes)
                        .map_err(|e| Error::new("storage_failure", e.to_string()))?;
                }
                Self::open(&path)
            })();
            match result {
                Ok(conn) => Ok(Self { conn, _dir: dir }),
                Err(error) => {
                    dir.close()
                        .map_err(|e| Error::new("storage_failure", e.to_string()))?;
                    Err(error)
                }
            }
        }
    }
    /// Private copy ownership and checked cleanup shared by both rehearsals.
    fn with_writable_copy<T>(
        self,
        operation: impl FnOnce(&mut Connection) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let Self { conn, _dir } = self;
        let result = (|| {
            conn.close()
                .map_err(|(_, error)| Error::new("storage_failure", error.to_string()))?;
            let mut db = Connection::open_with_flags(
                _dir.path().join("snapshot.sqlite"),
                OpenFlags::SQLITE_OPEN_READ_WRITE,
            )
            .map_err(invalid)?;
            let result = operation(&mut db);
            db.close()
                .map_err(|(_, error)| Error::new("storage_failure", error.to_string()))?;
            result
        })();
        _dir.close()
            .map_err(|error| Error::new("storage_failure", error.to_string()))?;
        result
    }
    fn open(path: &Path) -> Result<Connection, Error> {
        // Recovery/checkpoint happens only on private, disposable files.
        let recovery = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(invalid)?;
        recovery
            .busy_timeout(Duration::from_secs(1))
            .map_err(invalid)?;
        recovery
            .query_row("SELECT count(*) FROM sqlite_schema", [], |r| {
                r.get::<_, i64>(0)
            })
            .map_err(invalid)?;
        let busy: i64 = recovery
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
            .map_err(invalid)?;
        if busy != 0 {
            return Err(invalid("private snapshot recovery is busy"));
        }
        drop(recovery);
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(invalid)?;
        conn.execute_batch(
            "PRAGMA query_only=ON; PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF;",
        )
        .map_err(invalid)?;
        Ok(conn)
    }
}
fn validate(db: &Connection) -> Result<(), Error> {
    let expected: Vec<Migration> =
        serde_json::from_str(include_str!("legacy-schema.json")).map_err(invalid)?;
    let mut statement = db
        .prepare("SELECT id,name,checksum FROM schema_migrations ORDER BY id")
        .map_err(|_| {
            Error::new(
                "unsupported_schema",
                "current checksummed schema history is required",
            )
        })?;
    let rows = statement
        .query_map([], |r| {
            Ok(Migration {
                id: r.get(0)?,
                name: r.get(1)?,
                checksum: r.get(2)?,
            })
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if rows.len() != expected.len()
        || rows
            .iter()
            .zip(&expected)
            .any(|(a, b)| a.id != b.id || a.name != b.name || a.checksum != b.checksum)
    {
        return Err(Error::new(
            "unsupported_schema",
            "pending, newer or altered schema history; no migration performed",
        ));
    }
    let check: String = db
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(invalid)?;
    if check != "ok" {
        return Err(invalid("SQLite integrity check failed"));
    }
    let mut foreign = db.prepare("PRAGMA foreign_key_check").map_err(invalid)?;
    if foreign
        .query([])
        .map_err(invalid)?
        .next()
        .map_err(invalid)?
        .is_some()
    {
        return Err(invalid("ledger contains foreign-key violations"));
    }
    // History provenance is not physical-schema proof. Validate the direct read contract.
    for (table, required) in [
        (
            "accounts",
            &[
                ("id", "INTEGER", 0, 1),
                ("account_no", "TEXT", 1, 0),
                ("name", "TEXT", 1, 0),
                ("type", "TEXT", 1, 0),
                ("normal_balance", "TEXT", 1, 0),
            ][..],
        ),
        (
            "journal_entries",
            &[("id", "INTEGER", 0, 1), ("transaction_date", "TEXT", 1, 0)][..],
        ),
        (
            "journal_lines",
            &[
                ("id", "INTEGER", 0, 1),
                ("journal_entry_id", "INTEGER", 1, 0),
                ("account_id", "INTEGER", 1, 0),
                ("debit_amount", "NUMERIC", 1, 0),
                ("credit_amount", "NUMERIC", 1, 0),
            ][..],
        ),
    ] {
        require_columns(db, table, required)?;
    }
    for (table, required) in [
        (
            "journal_lines",
            &[
                ("journal_entry_id", "journal_entries", "id"),
                ("account_id", "accounts", "id"),
            ][..],
        ),
        (
            "journal_entries",
            &[
                ("document_id", "documents", "id"),
                ("source_bank_transaction_id", "bank_transactions", "id"),
            ][..],
        ),
    ] {
        let mut q = db
            .prepare(&format!("PRAGMA foreign_key_list({table})"))
            .map_err(invalid)?;
        let refs = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .map_err(invalid)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(invalid)?;
        if required.iter().any(|(from, to, col)| {
            !refs
                .iter()
                .any(|r| r.0 == *from && r.1 == *to && r.2 == *col)
        }) {
            return Err(Error::new(
                "unsupported_schema",
                "required report foreign-key metadata differs",
            ));
        }
    }
    let mut q = db
        .prepare("SELECT name FROM pragma_index_list('accounts') WHERE [unique]=1 AND partial=0")
        .map_err(invalid)?;
    let indexes = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut unique_account = false;
    for name in indexes {
        let mut q = db
            .prepare("SELECT name FROM pragma_index_info(?1) ORDER BY seqno")
            .map_err(invalid)?;
        let cols = q
            .query_map([name], |r| r.get::<_, String>(0))
            .map_err(invalid)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(invalid)?;
        if cols == ["account_no"] {
            unique_account = true;
        }
    }
    if !unique_account {
        return Err(Error::new(
            "unsupported_schema",
            "unique account-number constraint is required",
        ));
    }

    Ok(())
}
fn require_columns(
    db: &Connection,
    table: &str,
    required: &[(&str, &str, i64, i64)],
) -> Result<(), Error> {
    let kind: String = db
        .query_row(
            "SELECT type FROM sqlite_schema WHERE name=?1",
            [table],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if kind != "table" {
        return Err(Error::new(
            "unsupported_schema",
            "required report source must be a table",
        ));
    }
    let mut q = db
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(invalid)?;
    let cols = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?.to_ascii_uppercase(),
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(5)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if required.iter().any(|(name, kind, notnull, pk)| {
        !cols
            .iter()
            .any(|c| c.0 == *name && c.1 == *kind && c.2 == *notnull && c.3 == *pk)
    }) {
        return Err(Error::new(
            "unsupported_schema",
            "required report column metadata differs",
        ));
    }
    Ok(())
}

fn safe_entry_id(id: i64) -> Result<i64, Error> {
    if id.unsigned_abs() > 9_007_199_254_740_991 {
        return Err(invalid(
            "unsupported legacy journal entry identifier precision",
        ));
    }
    Ok(id)
}
fn legacy_money(value: ValueRef<'_>) -> Result<Money, Error> {
    let value = match value {
        ValueRef::Integer(n) if n.unsigned_abs() <= 9_007_199_254_740_991 => n as f64,
        ValueRef::Real(n) if n.is_finite() => n,
        _ => return Err(invalid("unsupported legacy amount storage or precision")),
    };
    if value < 0.0 {
        return Err(invalid("negative ledger debit/credit amount"));
    }
    Money::parse(ryu_js::Buffer::new().format_finite(value)).map_err(invalid)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_schema_and_safe_identity_are_required() {
        assert!(safe_entry_id(9_007_199_254_740_993).is_err());
        for drift in ["wrong-type", "missing-foreign-key"] {
            let db = Connection::open_in_memory().unwrap();
            db.execute_batch("CREATE TABLE schema_migrations(id INTEGER,name TEXT,checksum TEXT); CREATE TABLE accounts(id INTEGER PRIMARY KEY,account_no TEXT NOT NULL UNIQUE,name TEXT NOT NULL,type TEXT NOT NULL,normal_balance TEXT NOT NULL); CREATE TABLE journal_entries(id INTEGER PRIMARY KEY,transaction_date TEXT NOT NULL); CREATE TABLE journal_lines(id INTEGER PRIMARY KEY,journal_entry_id INTEGER NOT NULL,account_id INTEGER NOT NULL,debit_amount NUMERIC NOT NULL,credit_amount NUMERIC NOT NULL);").unwrap();
            let expected: Vec<Migration> =
                serde_json::from_str(include_str!("legacy-schema.json")).unwrap();
            for m in expected {
                db.execute(
                    "INSERT INTO schema_migrations VALUES (?1,?2,?3)",
                    rusqlite::params![m.id, m.name, m.checksum],
                )
                .unwrap();
            }
            if drift == "wrong-type" {
                db.execute_batch("DROP TABLE journal_lines;CREATE TABLE journal_lines(id TEXT,journal_entry_id TEXT,account_id TEXT,debit_amount TEXT,credit_amount TEXT);").unwrap();
            }
            db.execute("INSERT INTO journal_lines VALUES (1,1,999,1,0)", [])
                .unwrap();
            assert_eq!(
                validate(&db).unwrap_err().code,
                "unsupported_schema",
                "{drift}"
            );
        }
    }
    #[test]
    fn sql_number_conversion_does_not_round_twice() {
        for (n, amount) in [
            (1.0049999999999997, "1.00"),
            (0.004999999999999999, "0.00"),
            (999999999999.9949, "999999999999.99"),
            (1.005, "1.01"),
        ] {
            assert_eq!(legacy_money(ValueRef::Real(n)).unwrap().decimal(), amount);
        }
        for value in [
            ValueRef::Real(f64::INFINITY),
            ValueRef::Real(f64::NAN),
            ValueRef::Real(-1.0),
            ValueRef::Integer(9_007_199_254_740_993),
            ValueRef::Text(b"1.00"),
            ValueRef::Null,
        ] {
            assert!(legacy_money(value).is_err());
        }
    }
}

pub mod workspace_context;

pub mod service_context;

pub mod service_lifecycle;

pub mod idempotency;

pub mod journal_post;
pub mod journal_reversal;

pub mod fresh;

pub mod bootstrap;

pub mod cockpit;

pub mod documents;

pub mod bank;

pub mod expense;
pub mod payables;
pub mod periods;

pub mod company_setup;

pub mod vat_report;

pub mod sales;
pub mod vendors;

pub mod invoice_settlement;

pub mod credit_note;

pub mod assets;

pub mod accruals;
pub mod budget;
pub mod mileage;

pub mod recurring_invoices;

pub mod dashboard;

pub mod accountant;

pub mod exceptions;
pub mod integrity;

pub mod multi_year;
