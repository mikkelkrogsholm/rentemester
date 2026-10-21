//! Original account lifecycle on the admitted transaction; browser updates share
//! these exact identity/history rules. Ordinary refusals precede every write.
use crate::{bank, invalid};
use rentemester_engine::{Error, bank::original_account_slug, currency::Currency};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};

#[derive(Debug, Default)]
pub struct Patch {
    pub name: Option<String>,
    pub bank_name: Option<String>,
    pub registration_no: Option<String>,
    pub account_no: Option<String>,
    pub iban: Option<String>,
    pub bic: Option<String>,
    pub account_owner: Option<String>,
    pub customer_no: Option<String>,
    pub currency: Option<String>,
    pub ledger_account_no: Option<String>,
    pub active: Option<bool>,
}
#[derive(Debug)]
pub enum Outcome {
    Account(Value),
    Refused {
        code: &'static str,
        errors: Vec<String>,
    },
}
impl Outcome {
    pub fn original(self) -> Value {
        match self {
            Self::Account(account) => json!({"ok":true,"data":{"account":account},"errors":[]}),
            Self::Refused { code, errors } => {
                let mut value = json!({"ok":false,"errors":errors});
                if code == "bank_account_not_found" {
                    value["code"] = json!("NOT_FOUND");
                }
                value
            }
        }
    }
    pub fn browser(self) -> Result<Value, Error> {
        match self {
            Self::Account(account) => Ok(account),
            Self::Refused { code, errors } => Err(Error::new(code, errors.join("; "))),
        }
    }
}
fn refused(code: &'static str, error: impl Into<String>) -> Outcome {
    Outcome::Refused {
        code,
        errors: vec![error.into()],
    }
}
fn clean(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
fn currency(value: Option<&str>) -> String {
    clean(value).unwrap_or_else(|| "DKK".into()).to_uppercase()
}
fn account(tx: &Transaction<'_>, id: i64) -> Result<Value, Error> {
    tx.query_row("SELECT id,slug,name,bank_name,registration_no,account_no,iban,bic,account_owner,customer_no,currency,ledger_account_no,active,created_at FROM bank_accounts WHERE id=?1",[id],bank::account_row).map_err(invalid)
}
fn resolve(tx: &Transaction<'_>, selector: &str) -> Result<Option<Value>, Error> {
    let key = selector.trim();
    let id = key.parse::<i64>().ok().filter(|n| *n > 0);
    let id = tx
        .query_row(
            "SELECT id FROM bank_accounts WHERE id=?1 OR slug=?2 ORDER BY (id=?1) DESC LIMIT 1",
            params![id, key],
            |r| r.get::<_, i64>(0),
        )
        .optional()
        .map_err(invalid)?;
    id.map(|id| account(tx, id)).transpose()
}
fn ledger_error(tx: &Transaction<'_>, ledger: Option<&str>) -> Result<Option<Outcome>, Error> {
    let Some(no) = ledger else {
        return Ok(None);
    };
    let row = tx
        .query_row(
            "SELECT type,active FROM accounts WHERE account_no=?1",
            [no],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(invalid)?;
    Ok(match row {
        None => Some(refused(
            "bank_account_not_found",
            format!("ledger account {no} does not exist"),
        )),
        Some((_, active)) if active != 1 => Some(refused(
            "bank_account_validation",
            format!("ledger account {no} must be active"),
        )),
        Some((kind, _)) if kind != "asset" => Some(refused(
            "bank_account_validation",
            format!("ledger account {no} must be an asset account"),
        )),
        _ => None,
    })
}
fn audit(
    tx: &Transaction<'_>,
    event: &str,
    id: i64,
    message: &str,
    actor: &str,
    principal: crate::idempotency::PrincipalKind,
) -> Result<(), Error> {
    let kind = match principal {
        crate::idempotency::PrincipalKind::User => "user",
        crate::idempotency::PrincipalKind::ServiceAccount => "service_account",
    };
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES(?1,'bank_account',?2,?3,?4)",params![event,id.to_string(),message,format!("{kind}:{actor}")]).map_err(invalid)?;
    Ok(())
}
pub fn create(
    tx: &Transaction<'_>,
    input: &bank::AccountInput,
    actor: &str,
    principal: crate::idempotency::PrincipalKind,
) -> Result<Outcome, Error> {
    crate::journal_post::require_current(tx)?;
    let name = input.name.trim();
    let slug = original_account_slug(
        input
            .slug
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(name),
    );
    let currency = currency(input.currency.as_deref());
    let mut errors = vec![];
    if name.is_empty() {
        errors.push("name is required".into());
    }
    if slug.is_empty() {
        errors.push("slug could not be derived; pass an explicit --slug".into());
    }
    if Currency::parse(&currency).is_err() {
        errors.push("currency must be a 3-letter ISO currency code".into());
    }
    if !errors.is_empty() {
        return Ok(Outcome::Refused {
            code: "bank_account_validation",
            errors,
        });
    }
    let exists = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM bank_accounts WHERE slug=?1)",
            [&slug],
            |r| r.get::<_, bool>(0),
        )
        .map_err(invalid)?;
    if exists {
        return Ok(refused(
            "bank_account_conflict",
            format!("a bank account with slug '{slug}' already exists"),
        ));
    }
    let ledger = clean(input.ledger_account_no.as_deref());
    if let Some(error) = ledger_error(tx, ledger.as_deref())? {
        return Ok(error);
    }
    let id=tx.query_row("INSERT INTO bank_accounts(slug,name,bank_name,registration_no,account_no,iban,bic,account_owner,customer_no,currency,ledger_account_no)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)RETURNING id",params![slug,name,clean(input.bank_name.as_deref()),clean(input.registration_no.as_deref()),clean(input.account_no.as_deref()),clean(input.iban.as_deref()),clean(input.bic.as_deref()),clean(input.account_owner.as_deref()),clean(input.customer_no.as_deref()),currency,ledger],|r|r.get::<_,i64>(0)).map_err(invalid)?;
    audit(
        tx,
        "bank_account_add",
        id,
        &format!("Added bank account '{slug}' ({name})"),
        actor,
        principal,
    )?;
    Ok(Outcome::Account(account(tx, id)?))
}
pub fn update(
    tx: &Transaction<'_>,
    selector: &str,
    patch: &Patch,
    actor: &str,
    principal: crate::idempotency::PrincipalKind,
) -> Result<Outcome, Error> {
    crate::journal_post::require_current(tx)?;
    let Some(existing) = resolve(tx, selector)? else {
        return Ok(refused(
            "bank_account_not_found",
            format!("bank account '{selector}' does not exist"),
        ));
    };
    let id = existing["id"]
        .as_i64()
        .ok_or_else(|| invalid("bank account id missing"))?;
    let next_currency = patch
        .currency
        .as_deref()
        .map(|s| currency(Some(s)))
        .unwrap_or_else(|| existing["currency"].as_str().unwrap_or("").to_owned());
    if Currency::parse(&next_currency).is_err() {
        return Ok(refused(
            "bank_account_validation",
            "currency must be a 3-letter ISO currency code",
        ));
    }
    let ledger = match &patch.ledger_account_no {
        None => existing["ledgerAccountNo"].as_str().map(str::to_owned),
        Some(value) => clean(Some(value)),
    };
    if let Some(error) = ledger_error(tx, ledger.as_deref())? {
        return Ok(error);
    }
    let used = tx
        .query_row(
            "SELECT COUNT(*) FROM bank_transactions WHERE bank_account_id=?1",
            [id],
            |r| r.get::<_, i64>(0),
        )
        .map_err(invalid)?;
    if ledger.as_deref() != existing["ledgerAccountNo"].as_str() && used > 0 {
        return Ok(refused(
            "bank_account_conflict",
            format!(
                "unsafe ledger remapping refused: bank account '{}' has {used} preserved transaction(s); create a new bank account instead",
                existing["slug"].as_str().unwrap_or("")
            ),
        ));
    }
    let incompatible=tx.query_row("SELECT currency FROM bank_transactions WHERE bank_account_id=?1 AND UPPER(currency)!=?2 LIMIT 1",params![id,next_currency],|r|r.get::<_,String>(0)).optional().map_err(invalid)?;
    if let Some(currency) = incompatible {
        return Ok(refused(
            "bank_account_conflict",
            format!(
                "currency {next_currency} is incompatible with preserved bank transaction currency {currency}"
            ),
        ));
    }
    let name = patch
        .name
        .as_deref()
        .map(str::trim)
        .unwrap_or_else(|| existing["name"].as_str().unwrap_or(""));
    if name.is_empty() {
        return Ok(refused("bank_account_validation", "name must not be empty"));
    }
    let next = |value: &Option<String>, key: &str| {
        value
            .as_deref()
            .map(|s| clean(Some(s)))
            .unwrap_or_else(|| existing[key].as_str().map(str::to_owned))
    };
    let active = patch.active.unwrap_or(existing["active"] == true);
    tx.execute("UPDATE bank_accounts SET name=?1,bank_name=?2,registration_no=?3,account_no=?4,iban=?5,bic=?6,account_owner=?7,customer_no=?8,currency=?9,ledger_account_no=?10,active=?11 WHERE id=?12",params![name,next(&patch.bank_name,"bankName"),next(&patch.registration_no,"registrationNo"),next(&patch.account_no,"accountNo"),next(&patch.iban,"iban"),next(&patch.bic,"bic"),next(&patch.account_owner,"accountOwner"),next(&patch.customer_no,"customerNo"),next_currency,ledger,active,id]).map_err(invalid)?;
    audit(
        tx,
        "bank_account_update",
        id,
        &format!(
            "Updated bank account '{}': payment profile fields and active status; ledger mapping {} -> {}",
            existing["slug"].as_str().unwrap_or(""),
            existing["ledgerAccountNo"].as_str().unwrap_or("none"),
            ledger.as_deref().unwrap_or("none")
        ),
        actor,
        principal,
    )?;
    Ok(Outcome::Account(account(tx, id)?))
}
#[cfg(test)]
mod tests;
