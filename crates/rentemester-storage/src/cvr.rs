//! Shared public CVR snapshots and a narrowly scoped company identity sync.
use crate::{
    contact_import::{LookupOutcome, cache},
    invalid,
};
use rentemester_engine::{Error, contact_import::CvrCompany};
use rusqlite::{Transaction, params};
use serde_json::{Value, json};
pub struct Prepared {
    pub cvr: Option<String>,
    pub own_cvr: Option<String>,
    pub needs_lookup: bool,
    sync: bool,
    cached: Option<cache::Cached>,
}
pub fn normalize(value: &str) -> Option<String> {
    let compact = value
        .trim()
        .to_uppercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    let digits = compact.strip_prefix("DK").unwrap_or(&compact);
    (digits.len() == 8 && digits.bytes().all(|v| v.is_ascii_digit())).then(|| digits.to_owned())
}
fn failed(sync: bool, cvr: Option<&str>, message: &str) -> Value {
    if sync {
        let mut value = json!({"ok":false,"errors":[message]});
        if let Some(cvr) = cvr {
            value["cvr"] = json!(cvr);
        }
        value
    } else {
        json!({"ok":false,"cached":false,"company":null,"errors":[message]})
    }
}
pub fn prepare(
    tx: &Transaction<'_>,
    cvr: Option<&str>,
    sync: bool,
    now: &str,
) -> Result<Prepared, Error> {
    crate::journal_preview::require_current(tx)?;
    let own_cvr = if sync {
        tx.query_row("SELECT cvr FROM companies WHERE id=1", [], |r| {
            r.get::<_, Option<String>>(0)
        })
        .map_err(invalid)?
    } else {
        None
    };
    let cvr = if sync {
        own_cvr.as_deref().and_then(normalize)
    } else {
        cvr.and_then(normalize)
    };
    let cached = cvr
        .as_ref()
        .map(|cvr| cache::read(tx, cvr))
        .transpose()?
        .flatten();
    let now = rentemester_engine::company_knowledge::iso(now)?;
    let needs_lookup = cvr.is_some() && cached.as_ref().is_none_or(|entry| !entry.fresh(&now));
    Ok(Prepared {
        cvr,
        own_cvr,
        needs_lookup,
        sync,
        cached,
    })
}
pub fn finish(
    tx: &Transaction<'_>,
    prepared: &Prepared,
    outcome: Option<&LookupOutcome>,
    actor: &str,
    now: &str,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let now = rentemester_engine::company_knowledge::iso(now)?;
    if prepared.sync {
        let current = tx
            .query_row("SELECT cvr FROM companies WHERE id=1", [], |r| {
                r.get::<_, Option<String>>(0)
            })
            .map_err(invalid)?;
        if current != prepared.own_cvr {
            return Err(Error::new(
                "cvr_identity_changed",
                "Company CVR changed during lookup",
            ));
        }
    }
    let Some(cvr) = prepared.cvr.as_deref() else {
        return Ok(failed(
            prepared.sync,
            None,
            if prepared.sync {
                "virksomheden har intet gyldigt CVR-nummer registreret — sæt det via 'init --cvr' / 'company add --cvr'"
            } else {
                "cvr skal være et 8-cifret CVR-nummer (evt. med DK-præfiks)"
            },
        ));
    };
    let (company, cached, fetched) = if !prepared.needs_lookup {
        let entry = prepared
            .cached
            .as_ref()
            .ok_or_else(|| Error::new("cvr_state_invalid", "Prepared CVR cache missing"))?;
        (entry.company.clone(), true, entry.fetched_at.clone())
    } else {
        match outcome {
            Some(LookupOutcome::Found(entity)) => {
                let company = match CvrCompany::map(entity, cvr) {
                    Ok(value) => value,
                    Err(error) => return Ok(failed(prepared.sync, Some(cvr), &error.message)),
                };
                cache::store(tx, &company, entity, &now)?;
                (company, false, now.clone())
            }
            Some(LookupOutcome::Failed(error)) => match &prepared.cached {
                Some(entry) => (entry.company.clone(), true, entry.fetched_at.clone()),
                None => return Ok(failed(prepared.sync, Some(cvr), error)),
            },
            None => {
                return Err(Error::new(
                    "cvr_state_invalid",
                    "Prepared CVR provider result missing",
                ));
            }
        }
    };
    if !prepared.sync {
        return Ok(
            json!({"ok":true,"company":company,"cached":cached,"fetchedAt":fetched,"errors":[]}),
        );
    }
    let before = crate::company_setup::profile(tx)?;
    tx.execute("UPDATE companies SET name=?1,address=?2,postal_code=?3,city=?4,company_form=?5,industry_code=?6,industry_text=?7,cvr_status=?8,audit_waived=?9,cvr_synced_at=?10 WHERE id=1",params![company.name,company.address,company.postal_code,company.city,company.company_form_short,company.industry_code,company.industry_text,company.status,company.audit_waived,now]).map_err(invalid)?;
    let after = crate::company_setup::profile(tx)?;
    let fields = [
        "name",
        "address",
        "postalCode",
        "city",
        "companyForm",
        "industryCode",
        "industryText",
        "cvrStatus",
        "auditWaived",
    ]
    .into_iter()
    .filter(|field| before[*field] != after[*field])
    .collect::<Vec<_>>();
    let message = if fields.is_empty() {
        format!("Synced company stamdata from CVR {cvr} (no changes)")
    } else {
        format!(
            "Synced company stamdata from CVR {cvr}: {}",
            fields.join(", ")
        )
    };
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('company_cvr_sync','company','1',?1,?2)",params![message,format!("user:{actor}")]).map_err(invalid)?;
    let month = before["fiscalYearStartMonth"].as_u64();
    Ok(
        json!({"ok":true,"cvr":cvr,"company":company,"cached":cached,"updatedFields":fields,"fiscalYearStartMonth":{"current":month,"cvr":company.fiscal_year_start_month,"matches":company.fiscal_year_start_month.is_none_or(|value|Some(u64::from(value))==month)},"errors":[]}),
    )
}
#[cfg(test)]
mod tests;
