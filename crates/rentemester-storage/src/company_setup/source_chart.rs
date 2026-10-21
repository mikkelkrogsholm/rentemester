//! Reconcile classified migration definitions without changing account policy.
use super::*;
use serde::{Deserialize, Serialize};

/// Absent classification preserves the live field. VAT Some(None) explicitly clears it.
#[derive(Clone, Debug)]
pub struct SourceAccount {
    pub account_no: String,
    pub name: String,
    pub kind: Option<String>,
    pub normal_balance: Option<String>,
    pub default_vat_code: Option<Option<String>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Definition {
    pub name: String,
    pub kind: String,
    pub normal_balance: String,
    pub default_vat_code: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Change {
    pub account_no: String,
    pub before: Option<Definition>,
    pub after: Definition,
    pub journal_lines: i64,
    pub action: String,
    pub conflict: Option<String>,
}
fn definition(row: &Value) -> Result<Definition, Error> {
    Ok(Definition {
        name: row[1]
            .as_str()
            .ok_or_else(|| fail("invalid chart context"))?
            .into(),
        kind: row[2]
            .as_str()
            .ok_or_else(|| fail("invalid chart context"))?
            .into(),
        normal_balance: row[3]
            .as_str()
            .ok_or_else(|| fail("invalid chart context"))?
            .into(),
        default_vat_code: match &row[4] {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            _ => return Err(fail("invalid chart context")),
        },
    })
}
fn resolve(source: &SourceAccount, row: Option<&Value>, count: i64) -> Result<Change, Error> {
    let before = row.map(definition).transpose()?;
    let after = Definition {
        name: source.name.trim().to_owned(),
        kind: source
            .kind
            .clone()
            .or_else(|| before.as_ref().map(|d| d.kind.clone()))
            .ok_or_else(|| fail("source account classification required"))?,
        normal_balance: source
            .normal_balance
            .clone()
            .or_else(|| before.as_ref().map(|d| d.normal_balance.clone()))
            .ok_or_else(|| fail("source normal balance required"))?,
        default_vat_code: source
            .default_vat_code
            .clone()
            .unwrap_or_else(|| before.as_ref().and_then(|d| d.default_vat_code.clone())),
    };
    let after = Definition {
        name: if after.name.is_empty() {
            before.as_ref().map(|d| d.name.clone()).unwrap_or_default()
        } else {
            after.name
        },
        ..after
    };
    let no = source.account_no.as_str();
    if no.trim() != no
        || no.is_empty()
        || no.len() > 64
        || after.name.is_empty()
        || after.name.len() > 256
        || no.chars().chain(after.name.chars()).any(char::is_control)
        || !["asset", "liability", "equity", "income", "expense", "vat"]
            .contains(&after.kind.as_str())
        || !["debit", "credit"].contains(&after.normal_balance.as_str())
        || after
            .default_vat_code
            .as_deref()
            .is_some_and(|v| !VAT_CODES.contains(&v))
    {
        return Err(fail("valid classified source definition required"));
    }
    let conflict = if row.is_some_and(|r| r[5] != 1 || r[6] != 1) {
        Some("account is inactive or disallows direct posting".to_owned())
    } else if before.as_ref().is_some_and(|d| d != &after) && count > 0 {
        Some("account already has journal lines; kept live, not reclassified".to_owned())
    } else {
        None
    };
    let action = if conflict.is_some() {
        "conflict"
    } else if before.is_none() {
        "create"
    } else if before.as_ref() == Some(&after) {
        "unchanged"
    } else {
        "update"
    };
    Ok(Change {
        account_no: source.account_no.clone(),
        before,
        after,
        journal_lines: count,
        action: action.into(),
        conflict,
    })
}
/// Reconstruct a review from its retained chart and ledger prefix, never today's definitions.
pub(crate) fn at_context(
    db: &rusqlite::Connection,
    sources: &[SourceAccount],
    rows: &[Value],
    head: i64,
) -> Result<Vec<Change>, Error> {
    let mut seen = std::collections::BTreeSet::new();
    sources.iter().map(|s| {
        if !seen.insert(&s.account_no) { return Err(fail("duplicate source account")); }
        let row = rows.iter().find(|r| r[0] == s.account_no);
        let count = db.query_row("SELECT COUNT(*) FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE a.account_no=?1 AND l.journal_entry_id<=?2", params![s.account_no,head], |r| r.get(0)).map_err(sql)?;
        resolve(s, row, count)
    }).collect()
}
pub fn prepare(tx: &Transaction<'_>, sources: &[SourceAccount]) -> Result<Vec<Change>, Error> {
    let mut q=tx.prepare("SELECT account_no,name,type,normal_balance,default_vat_code,active,allow_direct_posting FROM accounts ORDER BY account_no").map_err(sql)?;
    let rows = q
        .query_map([], |r| {
            Ok(json!([
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?
            ]))
        })
        .map_err(sql)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql)?;
    at_context(tx, sources, &rows, i64::MAX)
}
/// Caller owns the transaction and must roll back all effects on any error.
pub fn apply(
    tx: &Transaction<'_>,
    sources: &[SourceAccount],
    reviewed: &[Change],
    actor: &str,
    at: &str,
) -> Result<(), Error> {
    crate::journal_post::require_current(tx)?;
    if prepare(tx, sources)? != reviewed || reviewed.iter().any(|c| c.conflict.is_some()) {
        return Err(fail("source chart review stale or conflicted"));
    }
    for change in reviewed {
        let d = &change.after;
        match change.action.as_str() {
            "create" => {
                create_account(
                    tx,
                    &Account {
                        account_no: change.account_no.clone(),
                        name: d.name.clone(),
                        r#type: d.kind.clone(),
                        normal_balance: d.normal_balance.clone(),
                        default_vat_code: d.default_vat_code.clone(),
                    },
                    actor,
                )?;
            }
            "update" => {
                let updated = tx.execute("UPDATE accounts SET name=?1,type=?2,normal_balance=?3,default_vat_code=?4 WHERE account_no=?5 AND NOT EXISTS(SELECT 1 FROM journal_lines WHERE account_id=accounts.id)",params![d.name,d.kind,d.normal_balance,d.default_vat_code,change.account_no]).map_err(sql)?;
                if updated != 1 {
                    return Err(fail("source account changed before reclassification"));
                }
                tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at)VALUES('account_source_reclassified','account',?1,?2,?3,?4)",params![change.account_no,serde_json::to_string(change).map_err(|_|fail("chart audit serialization failed"))?,format!("user:{actor}"),at]).map_err(sql)?;
            }
            "unchanged" => {}
            _ => return Err(fail("source chart conflict")),
        }
    }
    Ok(())
}
