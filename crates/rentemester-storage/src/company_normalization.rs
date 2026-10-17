//! Baseline data effects on an admitted private canonical company database.
use crate::{
    invalid,
    migration::{project_digest, project_digest_excluding_ids, projection, quote, schema},
};
use rentemester_engine::{
    Error,
    calendar::{retention_deadline, valid_iso_date},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params, types::Value};
use serde::Serialize;
use std::collections::BTreeMap;
#[derive(Debug, Serialize)]
pub(crate) struct Effects {
    original_business_data_digest: String,
    vat_rows_relabeled: usize,
    role_mappings_inserted: usize,
    audit_rows_inserted: usize,
    document_deadlines_filled: usize,
    bank_deadlines_filled: usize,
}
const ROLES: [(&str, &str, &str, &str); 8] = [
    ("bank", "2000", "asset", "debit"),
    ("debtors", "1100", "asset", "debit"),
    ("creditors", "7000", "liability", "credit"),
    ("output_vat", "1200", "vat", "credit"),
    ("input_vat", "4000", "vat", "debit"),
    ("reverse_charge_vat", "1200", "vat", "credit"),
    ("vat_settlement", "4500", "liability", "credit"),
    ("operational_default", "3000", "expense", "debit"),
];
fn failed(e: impl std::fmt::Display) -> Error {
    Error::new("migration_failed", e.to_string())
}
// Match ECMAScript String.trim for stored date/CVR input (Rust trim additionally
// includes U+0085, while ECMAScript additionally includes U+FEFF).
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c != '\u{0085}' && (c.is_whitespace() || c == '\u{FEFF}'))
}
fn fiscal_month(db: &Connection) -> Result<u32, Error> {
    let row = db
        .query_row(
            "SELECT fiscal_year_start_month,cvr FROM companies ORDER BY id LIMIT 1",
            [],
            |r| Ok((r.get::<_, Value>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(invalid)?;
    let Some((month, cvr)) = row else {
        return Ok(1);
    };
    if let Some(cvr) = cvr {
        let cvr = js_trim(&cvr).to_uppercase();
        let digits = cvr.strip_prefix("DK").unwrap_or(&cvr);
        if !cvr.is_empty() && (digits.len() != 8 || !digits.bytes().all(|b| b.is_ascii_digit())) {
            return Err(invalid("company CVR is invalid for retention backfill"));
        }
    }
    let month = match month {
        Value::Integer(n) => n as f64,
        Value::Real(n) => n,
        Value::Text(s) => js_trim(&s).parse().unwrap_or(f64::NAN),
        _ => f64::NAN,
    };
    Ok(if (1.0..=12.0).contains(&month) && month.fract() == 0.0 {
        month as u32
    } else {
        1
    })
}
struct Deadline {
    id: i64,
    original: Value,
    expected: Value,
}
fn deadlines(db: &Connection, table: &str, basis: &str) -> Result<Vec<Deadline>, Error> {
    let mut stmt = db
        .prepare(&format!(
            "SELECT id,retain_until,{basis} FROM {table} ORDER BY id"
        ))
        .map_err(invalid)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Value>(1)?,
                r.get::<_, Value>(2)?,
            ))
        })
        .map_err(invalid)?;
    let mut result = Vec::new();
    let mut month = None;
    for row in rows {
        let (id, original, basis) = row.map_err(invalid)?;
        let mut expected = original.clone();
        if original == Value::Null
            && let Value::Text(basis) = basis
        {
            let basis = js_trim(&basis);
            if valid_iso_date(basis) {
                let m = match month {
                    Some(m) => m,
                    None => {
                        let m = fiscal_month(db)?;
                        month = Some(m);
                        m
                    }
                };
                expected = Value::Text(retention_deadline(basis, m)?);
            }
        }
        result.push(Deadline {
            id,
            original,
            expected,
        });
    }
    Ok(result)
}
pub(super) fn normalize(db: &mut Connection) -> Result<Effects, Error> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(failed)?;
    let effects = normalize_in_transaction(&tx)?;
    tx.commit().map_err(failed)?;
    Ok(effects)
}
/// Caller owns the existing immediate transaction and its commit/rollback.
pub(super) fn normalize_in_transaction(tx: &Connection) -> Result<Effects, Error> {
    let tables: Vec<_> = schema(tx)?
        .into_iter()
        .filter(|o| o.kind == "table" && o.name != "schema_migrations")
        .map(|o| o.name)
        .collect();
    let all = projection(tx, &tables)?;
    let original_business_data_digest = project_digest(tx, &all)?;
    let vat_rows_relabeled = crate::company_vat::normalize_in_transaction(tx)?;
    let stable: Vec<_> = all
        .iter()
        .map(|(t, c)| {
            (
                t.clone(),
                c.iter()
                    .filter(|c| {
                        !(matches!(t.as_str(), "documents" | "bank_transactions")
                            && c.as_str() == "retain_until")
                    })
                    .cloned()
                    .collect(),
            )
        })
        .collect();
    let mut original_counts = BTreeMap::new();
    for table in ["account_role_mappings", "audit_log"] {
        let count: i64 = tx
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .map_err(invalid)?;
        original_counts.insert(table.to_string(), count);
    }
    let before = project_digest(tx, &stable)?;
    let documents = deadlines(
        tx,
        "documents",
        "COALESCE(invoice_date,substr(upload_datetime,1,10))",
    )?;
    let bank = deadlines(
        tx,
        "bank_transactions",
        "COALESCE(booking_date,transaction_date)",
    )?;
    let mut inserted = Vec::new();
    for (role, account, kind, balance) in ROLES {
        let eligible:bool=tx.query_row("SELECT NOT EXISTS(SELECT 1 FROM account_role_mappings WHERE role=?1) AND NOT EXISTS(SELECT 1 FROM account_role_proposals WHERE role=?1) AND EXISTS(SELECT 1 FROM accounts WHERE account_no=?2 AND type=?3 AND normal_balance=?4 AND active=1)",params![role,account,kind,balance],|r|r.get(0)).map_err(invalid)?;
        if !eligible {
            continue;
        }
        tx.execute("INSERT INTO account_role_mappings(role,account_no,version,confirmed_by,confirmation_source) VALUES(?1,?2,1,'system','native_seed')",params![role,account]).map_err(failed)?;
        let mapping_id = tx.last_insert_rowid();
        let message = format!("Confirmed {role} -> {account} (version 1)");
        tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('account_role_confirmed','account_role',?1,?2,'system via native-account-role-seed')",params![role,message]).map_err(failed)?;
        inserted.push((mapping_id, tx.last_insert_rowid(), role, account, message));
    }
    #[cfg(test)]
    checkpoint("roles");
    let mut counts = [0, 0];
    for (index, (table, rows)) in [("documents", &documents), ("bank_transactions", &bank)]
        .iter()
        .enumerate()
    {
        for row in *rows {
            if row.expected != row.original {
                let n=tx.execute(&format!("UPDATE {table} SET retain_until=?1 WHERE id=?2 AND retain_until IS NULL"),params![row.expected,row.id]).map_err(failed)?;
                if n != 1 {
                    return Err(invalid("normalization deadline target changed"));
                }
                counts[index] += 1;
            }
        }
    }
    #[cfg(test)]
    checkpoint("retention");
    let inserted_ids = BTreeMap::from([
        (
            "account_role_mappings".to_string(),
            inserted.iter().map(|r| r.0).collect(),
        ),
        (
            "audit_log".to_string(),
            inserted.iter().map(|r| r.1).collect(),
        ),
    ]);
    if before != project_digest_excluding_ids(tx, &stable, &inserted_ids)? {
        return Err(invalid("normalization changed unrelated original cells"));
    }
    for (table, rows) in [("documents", &documents), ("bank_transactions", &bank)] {
        for row in rows {
            let actual: Value = tx
                .query_row(
                    &format!("SELECT retain_until FROM {table} WHERE id=?1"),
                    [row.id],
                    |r| r.get(0),
                )
                .map_err(invalid)?;
            if actual != row.expected {
                return Err(invalid("normalization retention effect differs"));
            }
        }
    }
    for (mapping, audit, role, account, message) in &inserted {
        let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM account_role_mappings WHERE id=?1 AND role=?2 AND account_no=?3 AND status='confirmed' AND version=1 AND confirmed_by='system' AND confirmation_source='native_seed' AND typeof(confirmed_at)='text' AND length(confirmed_at)=19) AND EXISTS(SELECT 1 FROM audit_log WHERE id=?4 AND event_type='account_role_confirmed' AND entity_type='account_role' AND entity_id=?2 AND message=?5 AND actor='system via native-account-role-seed' AND typeof(created_at)='text' AND length(created_at)=19)",params![mapping,role,account,audit,message],|r|r.get(0)).map_err(invalid)?;
        if !valid {
            return Err(invalid("normalization seed/audit effect differs"));
        }
    }
    for (table, original) in original_counts {
        let count: i64 = tx
            .query_row(
                &format!("SELECT COUNT(*) FROM {}", quote(&table)),
                [],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        if count != original + inserted.len() as i64 {
            return Err(invalid("normalization appended row count differs"));
        }
    }
    Ok(Effects {
        original_business_data_digest,
        vat_rows_relabeled,
        role_mappings_inserted: inserted.len(),
        audit_rows_inserted: inserted.len(),
        document_deadlines_filled: counts[0],
        bank_deadlines_filled: counts[1],
    })
}
#[cfg(test)]
fn checkpoint(phase: &str) {
    if std::env::var("RM_NORMALIZATION_PHASE").as_deref() == Ok(phase) {
        std::fs::write(std::env::var("RM_NORMALIZATION_MARKER").unwrap(), phase).unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}
