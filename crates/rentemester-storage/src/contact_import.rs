//! Prepared contact-only import, using current writers and retained public CVR snapshots.
pub(crate) mod cache;
#[cfg(test)]
mod tests;
use rentemester_engine::{
    Error,
    contact_import::{Contact, CvrCompany, Input, Parsed, Role},
};
use rusqlite::{Transaction, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug)]
pub struct LookupNeed {
    pub cvr: String,
}
#[derive(Clone, Debug)]
pub enum LookupOutcome {
    Found(Value),
    Failed(String),
}
#[derive(Clone, Debug)]
pub struct Lookup {
    pub cvr: String,
    pub outcome: LookupOutcome,
}
#[derive(Clone, Debug)]
pub struct Prepared {
    parsed: Parsed,
    enrich: bool,
    default_role: Role,
    pub lookup_needs: Vec<LookupNeed>,
}
fn refused(message: &str) -> Error {
    Error::new("contact_import_refused", message)
}
fn sql(_: impl std::fmt::Display) -> Error {
    Error::new(
        "contact_import_storage_unavailable",
        "contact import storage unavailable; no import committed",
    )
}
fn known(e: &Error) -> bool {
    [
        "invoice_refused",
        "gdpr_refused",
        "invalid_document_metadata",
    ]
    .contains(&e.code)
}
fn exists(tx: &Transaction<'_>, role: Role, c: &Contact) -> Result<bool, Error> {
    tx.query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE name=?1 AND vat_or_cvr IS ?2)",
            role.table()
        ),
        params![c.name, c.vat_or_cvr],
        |r| r.get(0),
    )
    .map_err(sql)
}
pub fn prepare(tx: &Transaction<'_>, input: &Input, now: &str) -> Result<Prepared, Error> {
    crate::journal_preview::require_current(tx)?;
    if !input.confirm {
        return Err(refused("confirmation required"));
    }
    if input.file_name.trim().is_empty() {
        return Err(Error::new("contact_import_invalid", "fileName required"));
    }
    let parsed = if input.default_role.is_some() {
        rentemester_engine::contact_import::parse_original(&input.content)?
    } else {
        rentemester_engine::contact_import::parse(&input.content)?
    };
    let default_role = input.default_role.unwrap_or(Role::Vendor);
    let now = rentemester_engine::company_knowledge::iso(now)?;
    let mut needs = BTreeSet::new();
    if input.enrich_cvr {
        for c in &parsed.contacts {
            for role in c.roles_with_default(default_role) {
                if exists(tx, role, c)? {
                    continue;
                }
                if let Err(e) = crate::gdpr::require_contact_identity(
                    tx,
                    role.table(),
                    None,
                    &c.name,
                    c.vat_or_cvr.as_deref(),
                ) {
                    if known(&e) {
                        continue;
                    }
                    return Err(e);
                }
                if let Some(cvr) = &c.danish_cvr
                    && cache::read(tx, cvr)?.is_none_or(|v| !v.fresh(&now))
                {
                    needs.insert(cvr.clone());
                }
            }
        }
    }
    Ok(Prepared {
        parsed,
        enrich: input.enrich_cvr,
        default_role,
        lookup_needs: needs.into_iter().map(|cvr| LookupNeed { cvr }).collect(),
    })
}
fn create(
    tx: &Transaction<'_>,
    role: Role,
    c: &Contact,
    company: Option<&CvrCompany>,
    actor: &str,
) -> Result<(), Error> {
    let address = c
        .address
        .clone()
        .or_else(|| company.and_then(CvrCompany::full_address));
    let email = c
        .email
        .clone()
        .or_else(|| company.and_then(|v| v.email.clone()));
    let phone = c
        .phone
        .clone()
        .or_else(|| company.and_then(|v| v.phone.clone()));
    let website = c
        .website
        .clone()
        .or_else(|| company.and_then(|v| v.website.clone()));
    let notes = c.attention_person.as_ref().map(|v| format!("Att.: {v}"));
    match role {
        Role::Customer => {
            crate::sales::create_customer(
                tx,
                &crate::sales::Customer {
                    name: c.name.clone(),
                    address,
                    vat_or_cvr: c.vat_or_cvr.clone(),
                    email,
                    phone,
                    website,
                    ean_number: c.ean_number.clone(),
                    payment_terms_days: c.payment_terms_days,
                    default_currency: None,
                    notes,
                },
                actor,
            )?;
        }
        Role::Vendor => {
            crate::vendors::create(
                tx,
                &crate::vendors::Vendor {
                    name: c.name.clone(),
                    address,
                    vat_or_cvr: c.vat_or_cvr.clone(),
                    country_code: None,
                    identifier_kind: None,
                    email,
                    phone,
                    website,
                    default_expense_account: None,
                    default_vat_treatment: None,
                    notes,
                },
                actor,
            )?;
        }
    }
    Ok(())
}
pub fn execute(
    tx: &Transaction<'_>,
    prepared: &Prepared,
    lookups: &[Lookup],
    actor: &str,
    now: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let now = rentemester_engine::company_knowledge::iso(now)?;
    let allowed: BTreeSet<_> = prepared
        .lookup_needs
        .iter()
        .map(|n| n.cvr.as_str())
        .collect();
    let mut outcomes = BTreeMap::new();
    for lookup in lookups {
        if !allowed.contains(lookup.cvr.as_str())
            || outcomes
                .insert(lookup.cvr.as_str(), &lookup.outcome)
                .is_some()
        {
            return Err(refused("lookup identity differs from prepared import"));
        }
    }
    let mut errors = prepared.parsed.errors.clone();
    let mut customers = 0;
    let mut vendors = 0;
    let mut skipped = 0;
    let mut enriched = 0;
    let mut failures = 0;
    let mut successful: BTreeMap<String, CvrCompany> = BTreeMap::new();
    for c in &prepared.parsed.contacts {
        let mut contact_enriched = false;
        for role in c.roles_with_default(prepared.default_role) {
            if exists(tx, role, c)? {
                skipped += 1;
                continue;
            }
            // Erasure is authoritative before any cache write and again in the reused writer.
            if let Err(e) = crate::gdpr::require_contact_identity(
                tx,
                role.table(),
                None,
                &c.name,
                c.vat_or_cvr.as_deref(),
            ) {
                if !known(&e) {
                    return Err(e);
                }
                errors.push(format!(
                    "Kontakt '{}' kunne ikke oprettes: {}",
                    c.name, e.message
                ));
                continue;
            }
            let mut company = None;
            if prepared.enrich
                && let Some(cvr) = &c.danish_cvr
            {
                let cached = cache::read(tx, cvr)?;
                if let Some(snapshot) = cached.as_ref().filter(|s| s.fresh(&now)) {
                    company = Some(snapshot.company.clone());
                } else if let Some(snapshot) = successful.get(cvr) {
                    company = Some(snapshot.clone());
                } else {
                    let result = match outcomes.get(cvr.as_str()) {
                        Some(LookupOutcome::Found(entity)) => {
                            CvrCompany::map(entity, cvr).map(|v| (v, entity))
                        }
                        Some(LookupOutcome::Failed(message)) => Err(refused(message)),
                        None => Err(refused("CVR-opslag har intet aktuelt resultat")),
                    };
                    match result {
                        Ok((snapshot, entity)) => {
                            cache::store(tx, &snapshot, entity, &now)?;
                            successful.insert(cvr.clone(), snapshot.clone());
                            company = Some(snapshot);
                        }
                        Err(e) => {
                            if let Some(stale) = cached {
                                company = Some(stale.company);
                            } else {
                                failures += 1;
                                errors.push(format!(
                                    "CVR-berigelse af '{}' fejlede: {}",
                                    c.name, e.message
                                ));
                            }
                        }
                    }
                }
                if company.is_some() {
                    contact_enriched = true;
                }
            }
            tx.execute_batch("SAVEPOINT contact_import_role")
                .map_err(sql)?;
            match create(tx, role, c, company.as_ref(), actor) {
                Ok(()) => {
                    tx.execute_batch("RELEASE contact_import_role")
                        .map_err(sql)?;
                    match role {
                        Role::Customer => customers += 1,
                        Role::Vendor => vendors += 1,
                    }
                }
                Err(e) => {
                    tx.execute_batch(
                        "ROLLBACK TO contact_import_role; RELEASE contact_import_role",
                    )
                    .map_err(sql)?;
                    if !known(&e) {
                        return Err(e);
                    }
                    errors.push(format!(
                        "Kontakt '{}' kunne ikke oprettes: {}",
                        c.name, e.message
                    ));
                }
            }
        }
        if contact_enriched {
            enriched += 1;
        }
    }
    Ok(
        json!({"ok":true,"import":{"detected":{"id":"dinero-contacts","label":"Dinero — Kontakter (kunder og leverandører)","system":"Dinero","dataType":"contacts"},"summary":{"parsed":prepared.parsed.contacts.len(),"customersCreated":customers,"vendorsCreated":vendors,"skipped":skipped,"enriched":enriched,"enrichmentFailures":failures},"errors":errors}}),
    )
}
