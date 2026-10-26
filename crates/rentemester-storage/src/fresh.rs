//! Empty native construction on caller-owned connections. No path opens/adoption.
use crate::migration::{integrity, schema};
use rentemester_engine::Error;
use rusqlite::{Connection, MAIN_DB};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize)]
pub struct Initialized {
    pub control_version: u32,
    pub company_version: u32,
    pub control_schema_sha256: String,
    pub company_schema_sha256: String,
    pub ledger_uuid: String,
}
fn refused() -> Error {
    Error::new(
        "native_initialization_refused",
        "native initialization requires two newly created empty databases",
    )
}
fn empty(db: &Connection) -> Result<(), Error> {
    if !db.is_autocommit() || db.is_readonly(MAIN_DB).map_err(|_| refused())? {
        return Err(refused());
    }
    for sql in [
        "SELECT count(*) FROM sqlite_schema",
        "SELECT count(*) FROM sqlite_temp_schema",
        "SELECT count(*) FROM pragma_database_list WHERE name NOT IN ('main','temp')",
        "PRAGMA user_version",
        "PRAGMA application_id",
        "PRAGMA query_only",
    ] {
        if db
            .query_row(sql, [], |r| r.get::<_, i64>(0))
            .map_err(|_| refused())?
            != 0
        {
            return Err(refused());
        }
    }
    Ok(())
}
fn fingerprint(db: &Connection) -> Result<String, Error> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&schema(db)?).map_err(|_| refused())?)
    ))
}
/// Caller must establish fresh-file provenance and hold the complete physical
/// lifetime before calling. BOTH preflight before any persistent SQL. The two
/// independent commits are not atomic together: on error discard/retire the
/// owning initializer and retain the private partial namespace; never retry on it.
/// No accounts, credentials, sessions, memberships or first-user authority seeded.
pub fn initialize_empty_pair(
    control: &mut Connection,
    company: &mut Connection,
    company_name: &str,
) -> Result<Initialized, Error> {
    if company_name.trim().is_empty()
        || company_name.len() > 1024
        || company_name.chars().any(char::is_control)
    {
        return Err(refused());
    }
    empty(control)?;
    empty(company)?;
    for db in [&*control, &*company] {
        db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF; PRAGMA synchronous=FULL;",
        )
        .map_err(|_| refused())?;
    }
    crate::control_migration::initialize_empty(control).map_err(|_| refused())?;
    crate::company_baseline::initialize_empty(company, company_name).map_err(|_| refused())?;
    integrity(control).map_err(|_| refused())?;
    integrity(company).map_err(|_| refused())?;
    Ok(Initialized {
        control_version: 32,
        company_version: 36,
        control_schema_sha256: fingerprint(control).map_err(|_| refused())?,
        company_schema_sha256: fingerprint(company).map_err(|_| refused())?,
        ledger_uuid: company
            .query_row(
                "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
                [],
                |r| r.get(0),
            )
            .map_err(|_| refused())?,
    })
}

#[cfg(test)]
mod tests;

/// Explicit workspace configuration, never a product-specific default chart.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InitialAccount {
    pub account_no: rentemester_engine::identifiers::AccountNo,
    pub name: String,
    pub r#type: String,
    pub normal_balance: String,
}
/// Fresh-owner-only chart seed. Refuses any existing chart or journal and commits once.
pub fn initialize_accounts(
    company: &mut Connection,
    accounts: &[InitialAccount],
) -> Result<(), Error> {
    if accounts.len() > 10_000 {
        return Err(refused());
    }
    let mut seen = std::collections::BTreeSet::new();
    for account in accounts {
        if account.name.trim().is_empty()
            || account.name.len() > 512
            || account.name.chars().any(char::is_control)
            || !["asset", "liability", "equity", "income", "expense", "vat"]
                .contains(&account.r#type.as_str())
            || !["debit", "credit"].contains(&account.normal_balance.as_str())
            || !seen.insert(account.account_no.as_str())
        {
            return Err(refused());
        }
    }
    let tx = company
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| refused())?;
    crate::validate(&tx)?;
    for table in ["accounts", "journal_entries"] {
        if tx
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .map_err(|_| refused())?
            != 0
        {
            return Err(refused());
        }
    }
    for account in accounts {
        tx.execute(
            "INSERT INTO accounts(account_no,name,type,normal_balance) VALUES(?1,?2,?3,?4)",
            [
                account.account_no.as_str(),
                &account.name,
                &account.r#type,
                &account.normal_balance,
            ],
        )
        .map_err(|_| refused())?;
    }
    tx.commit().map_err(|_| refused())
}

/// Newly created company file under the existing workspace owner. No control
/// initialization or identity bootstrap; caller proves exclusive fresh provenance.
pub fn initialize_company(company:&mut Connection,name:&str)->Result<(),Error>{
 if name.trim().is_empty() || name.len()>1024 || name.chars().any(char::is_control){return Err(refused());}
 empty(company)?;
 company.execute_batch("PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF; PRAGMA synchronous=FULL;").map_err(|_|refused())?;
 crate::company_baseline::initialize_empty(company,name).map_err(|_|refused())?;
 integrity(company).map_err(|_|refused())
}
