//! Fixed ordinary archive reports over complete physical original proofs in the admitted transaction.
use crate::archive_import::{self, VerifiedArchive};
pub use policy::Resource;
use rentemester_engine::{
    Error,
    archive_reports::{self as policy, Classification, Context, Year},
};
use rusqlite::Transaction;
use serde_json::{Value, json};
#[cfg(test)]
mod tests;
pub fn read(
    tx: &Transaction<'_>,
    verified: &[VerifiedArchive],
    slug: &str,
    year: u16,
    account: Option<&str>,
    resource: Resource,
) -> Result<Value, Error> {
    // Both calls validate current historical rows and full original proof coverage.
    let detail = archive_import::read(tx, verified, Some(year))?;
    let inventory = archive_import::read(tx, verified, None)?;
    let source: Year =
        serde_json::from_value(detail["years"][0].clone()).map_err(crate::invalid)?;
    let company=tx.query_row("SELECT name,cvr,country,currency,fiscal_year_start_month,fiscal_year_label_strategy FROM companies WHERE id=1",[],|r|Ok(json!({"name":r.get::<_,String>(0)?,"cvr":r.get::<_,Option<String>>(1)?,"country":r.get::<_,String>(2)?,"currency":r.get::<_,String>(3)?,"fiscalYearStartMonth":r.get::<_,i32>(4)?,"fiscalYearLabelStrategy":r.get::<_,String>(5)?}))).map_err(crate::invalid)?;
    let ledger_uuid = tx
        .query_row(
            "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(crate::invalid)?;
    let mut q = tx
        .prepare("SELECT account_no,type,normal_balance FROM accounts ORDER BY account_no")
        .map_err(crate::invalid)?;
    let chart = q
        .query_map([], |r| {
            Ok(Classification {
                account_no: r.get(0)?,
                kind: r.get(1)?,
                normal_balance: r.get(2)?,
            })
        })
        .map_err(crate::invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(crate::invalid)?;
    let years:Vec<_>=inventory["years"].as_array().ok_or_else(||crate::invalid("archive years unavailable"))?.iter().map(|y|json!({"label":format!("{:04}",y["fiscalYear"].as_u64().expect("verified fiscal year")),"start":null,"end":null,"source":"archive","selectable":true})).collect();
    policy::render(
        &Context {
            slug: slug.into(),
            ledger_uuid,
            company,
            fiscal_years: json!(years),
        },
        &source,
        &chart,
        account,
        resource,
    )
}
