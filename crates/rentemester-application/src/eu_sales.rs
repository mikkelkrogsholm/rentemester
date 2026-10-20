//! EU sales preparation and explicit VIES lookup under current company authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::{Error, fiscal_year::Date};
use rentemester_runtime::namespace::NamespaceGuard;
use rusqlite::{Connection, Transaction};
use serde_json::{Value, json};

/// One-use server admission; clients cannot construct or rebind a provider result.
pub struct Admission {
    workspace: String,
    slug: String,
    subject: String,
    ledger: String,
    vat: String,
}
impl Admission {
    pub fn vat(&self) -> &str {
        &self.vat
    }
}
fn refused() -> Error {
    Error::new(
        "vies_admission_changed",
        "Current VIES company, subject or result identity differs",
    )
}
fn ledger(tx: &Transaction<'_>) -> Result<String, Error> {
    tx.query_row(
        "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
        [],
        |row| row.get(0),
    )
    .map_err(|_| refused())
}
pub fn start(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    vat: &str,
) -> Result<Committed<Admission>, Failure> {
    let workspace = context.selection.workspace.to_owned();
    let slug = context.selection.slug.to_owned();
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Fresh,
        },
        context.selection,
        Permission::CompanyExternalLookup,
        |tx, subject| {
            Ok(Admission {
                workspace,
                slug,
                subject: subject.to_owned(),
                ledger: ledger(tx)?,
                vat: rentemester_engine::vies::foreign(vat)?.normalized,
            })
        },
    )
}
pub fn finish(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    admission: Admission,
    observation: &Value,
) -> Result<Committed<Value>, Failure> {
    let workspace = context.selection.workspace;
    let slug = context.selection.slug;
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Fresh,
        },
        context.selection,
        Permission::CompanyExternalLookup,
        |tx, subject| {
            if admission.workspace != workspace
                || admission.slug != slug
                || admission.subject != subject
                || admission.ledger != ledger(tx)?
                || observation["normalized"].as_str() != Some(admission.vat())
            {
                return Err(refused());
            }
            let now = rentemester_engine::company_knowledge::timestamp(
                context.clock.now_unix_ms().map_err(|_| refused())?,
            )?;
            let record = rentemester_engine::vies::record(observation, &now)?;
            rentemester_storage::vies::store(tx, &record, &format!("user:{subject}"))?;
            Ok(json!({"ok":true,"validation":record}))
        },
    )
}
pub fn report(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    from: Date,
    to: Date,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyRead,
        true,
        |tx, _| {
            if !crate::periods::verify_sources(tx, guard)?.is_empty() {
                return Err(Error::new(
                    "eu_sales_source_unavailable",
                    "EU sales source assurance differs",
                ));
            }
            crate::archive_import::verify_physical(tx, guard)?;
            rentemester_storage::eu_sales::read(tx, from, to)
        },
    )
}
