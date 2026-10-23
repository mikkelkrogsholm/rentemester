//! Frozen source accounts ABI over the native chart and required-role writers.
//! Reads have no effects. `allowDirectPosting` preserves the existing targeted
//! guards; it does not introduce a universal manual-journal posting policy.
use crate::{company_setup as setup, invalid};
use rentemester_engine::Error;
use rusqlite::{OptionalExtension, Transaction};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Copy)]
pub enum Operation {
    List,
    Add,
    RolesStatus,
    RoleConfirm,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Add {
    input: Fields,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Fields {
    account_no: String,
    name: String,
    r#type: String,
    normal_balance: Option<String>,
    default_vat_code: Option<String>,
    #[serde(default = "yes")]
    allow_direct_posting: bool,
}
fn yes() -> bool {
    true
}
fn refused(message: impl Into<String>) -> Error {
    Error::new("accounts_original_refused", message)
}
fn normalized(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
fn parse<T: serde::de::DeserializeOwned>(input: &Value) -> Result<T, Error> {
    serde_json::from_value(input.clone()).map_err(|e| refused(e.to_string()))
}
fn typed_actor(actor: &str) -> bool {
    actor
        .strip_prefix("user:")
        .or_else(|| actor.strip_prefix("service_account:"))
        .is_some_and(|s| !s.is_empty() && !s.chars().any(|c| c.is_control() || c.is_whitespace()))
}

pub fn execute(
    tx: &Transaction<'_>,
    op: Operation,
    input: &Value,
    actor: &str,
) -> Result<Value, Error> {
    crate::validate(tx)?;
    if matches!(op, Operation::List | Operation::RolesStatus) {
        if input.as_object().is_none_or(|v| !v.is_empty()) {
            return Err(refused("read input must be an empty object"));
        }
        if matches!(op, Operation::List) {
            let mut q = tx.prepare("SELECT account_no,name,type,default_vat_code FROM accounts ORDER BY account_no ASC").map_err(invalid)?;
            let rows = q.query_map([], |r| Ok(json!({"account_no":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"type":r.get::<_,String>(2)?,"default_vat_code":r.get::<_,Option<String>>(3)?})))
                .map_err(invalid)?.collect::<Result<Vec<_>,_>>().map_err(invalid)?;
            return Ok(json!({"ok":true,"count":rows.len(),"rows":rows}));
        }
        let mut status = source_status(tx)?;
        status["ok"] = json!(true);
        return Ok(status);
    }
    crate::journal_post::require_current(tx)?;
    // Even a caller retaining its outer transaction cannot commit partial effects
    // from a refused writer (including an audit insert failure).
    tx.execute_batch("SAVEPOINT original_accounts_write")
        .map_err(invalid)?;
    let result = (|| {
        if !typed_actor(actor) {
            return Err(refused("canonical current typed principal required"));
        }
        match op {
            Operation::Add => {
                let add: Add = parse(input)?;
                if !add.confirm {
                    return Err(refused("explicit account creation confirmation required"));
                }
                let fields = add.input;
                let kind = fields.r#type.trim().to_string();
                let account = setup::Account {
                    account_no: fields.account_no.trim().into(),
                    name: fields.name.trim().into(),
                    normal_balance: normalized(fields.normal_balance).unwrap_or_else(|| {
                        if matches!(kind.as_str(), "asset" | "expense") {
                            "debit"
                        } else {
                            "credit"
                        }
                        .into()
                    }),
                    r#type: kind,
                    default_vat_code: normalized(fields.default_vat_code),
                };
                let created =
                    setup::create_account_typed(tx, &account, fields.allow_direct_posting, actor)?;
                Ok(
                    json!({"ok":true,"accountNo":created["accountNo"],"appliedRules":["DK-CHART-OF-ACCOUNTS-ADD-001"],"errors":[]}),
                )
            }
            Operation::RoleConfirm => {
                let mut role: setup::Role = parse(input)?;
                role.role = role.role.trim().into();
                role.account_no = role.account_no.trim().into();
                let resolution = setup::confirm_source_role(tx, &role, actor)?;
                let mut status = source_status(tx)?;
                status["ok"] = json!(true);
                status["resolution"] = resolution;
                Ok(status)
            }
            _ => unreachable!(),
        }
    })();
    match result {
        Ok(value) => {
            tx.execute_batch("RELEASE original_accounts_write")
                .map_err(invalid)?;
            Ok(value)
        }
        Err(error) => {
            tx.execute_batch(
                "ROLLBACK TO original_accounts_write; RELEASE original_accounts_write",
            )
            .map_err(invalid)?;
            let mut value = if matches!(op, Operation::RoleConfirm) {
                source_status(tx)?
            } else {
                json!({})
            };
            value["ok"] = json!(false);
            value["errors"] = json!([error.message]);
            Ok(value)
        }
    }
}

fn source_resolution(tx: &Transaction<'_>, role: &str) -> Result<Value, Error> {
    let row: Option<(String,i64,String,String,String,String,i64)> = tx.query_row(
        "SELECT m.account_no,m.version,m.confirmed_by,m.confirmation_source,a.type,a.normal_balance,a.active FROM account_role_mappings m JOIN accounts a ON a.account_no=m.account_no WHERE m.role=?1 AND m.status='confirmed'", [role],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(invalid)?;
    let error = match row {
        None => format!("account role '{role}' has no active confirmed mapping"),
        Some((account, version, actor, source, kind, balance, active)) => {
            if active != 1 {
                format!("account role '{role}' maps to inactive account {account}")
            } else if !setup::ROLES
                .iter()
                .any(|r| r.0 == role && r.1 == kind && r.2 == balance)
            {
                format!("account role '{role}' maps to semantically incompatible account {account}")
            } else {
                return Ok(
                    json!({"ok":true,"role":role,"accountNo":account,"version":version,"confirmedBy":actor,"confirmationSource":source}),
                );
            }
        }
    };
    Ok(json!({"ok":false,"role":role,"error":error}))
}
fn source_status(tx: &Transaction<'_>) -> Result<Value, Error> {
    let mut q = tx.prepare("SELECT role,account_no,source,proposed_at FROM account_role_proposals WHERE status='proposed' ORDER BY role,account_no,source").map_err(invalid)?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut proposals = Vec::new();
    for (role, account, source, at) in rows {
        if !setup::ROLES.iter().any(|r| r.0 == role) {
            continue;
        }
        let valid = setup::compatible(tx, &role, &account);
        let mut proposal = json!({"role":role,"accountNo":account,"source":source,"proposedAt":at,"compatible":valid.is_ok()});
        if let Err(e) = valid {
            proposal["reason"] = json!(e.message);
        }
        proposals.push(proposal);
    }
    let (mut missing, mut ambiguous, mut reasons, mut roles) = (vec![], vec![], vec![], vec![]);
    for (role, _, _) in setup::ROLES {
        let resolution = source_resolution(tx, role)?;
        if resolution["ok"] != true {
            let count = proposals
                .iter()
                .filter(|p| p["role"] == role && p["compatible"] == true)
                .map(|p| p["accountNo"].as_str().unwrap())
                .collect::<std::collections::BTreeSet<_>>()
                .len();
            if count > 1 {
                ambiguous.push(role);
            } else {
                missing.push(role);
            }
            let suffix = if count == 0 {
                String::new()
            } else {
                format!(
                    "; {count} unconfirmed proposal{}",
                    if count == 1 { "" } else { "s" }
                )
            };
            reasons.push(json!({"role":role,"reason":format!("{}{suffix}",resolution["error"].as_str().unwrap())}));
        }
        roles.push(resolution);
    }
    Ok(
        json!({"status":if !ambiguous.is_empty(){"ambiguous"}else if !missing.is_empty(){"incomplete"}else{"complete"},"missing":missing,"ambiguous":ambiguous,"proposals":proposals,"candidates":proposals,"reasons":reasons,"roles":roles}),
    )
}

#[cfg(test)]
mod tests;
