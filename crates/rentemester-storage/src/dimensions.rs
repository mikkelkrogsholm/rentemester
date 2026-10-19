//! Classifications beside immutable journal lines. All revisions append evidence; none post money.
use crate::{invalid, legacy_money};
use rentemester_engine::{Error, fiscal_year::Date};
use rusqlite::{Transaction, params, types::ValueRef};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
const MAX: i64 = 9_007_199_254_740_991;
fn fail(code: &str) -> Error {
    Error::new("dimension_refused", code)
}
fn text(v: &str, max: usize) -> Result<String, Error> {
    let s = v.trim();
    if s.is_empty() || s.chars().count() > max {
        return Err(fail("INVALID_TEXT"));
    }
    Ok(s.to_owned())
}
fn id(v: i64) -> Result<i64, Error> {
    if v <= 0 || v > MAX {
        Err(fail("INVALID_ID"))
    } else {
        Ok(v)
    }
}
fn now(tx: &Transaction<'_>) -> Result<String, Error> {
    tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ','now')", [], |r| {
        r.get(0)
    })
    .map_err(invalid)
}
fn rows(tx: &Transaction<'_>, sql: &str, p: impl rusqlite::Params) -> Result<Vec<Value>, Error> {
    let mut q = tx.prepare(sql).map_err(invalid)?;
    let names = q
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut query = q.query(p).map_err(invalid)?;
    let mut out = vec![];
    while let Some(r) = query.next().map_err(invalid)? {
        let mut value = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            let v = match r.get_ref(i).map_err(invalid)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(v) if v.unsigned_abs() <= MAX as u64 => json!(v),
                ValueRef::Text(v) => json!(std::str::from_utf8(v).map_err(invalid)?),
                _ => return Err(invalid("dimension evidence storage type")),
            };
            value.insert(name.clone(), v);
        }
        out.push(Value::Object(value));
    }
    Ok(out)
}
fn definition(tx: &Transaction<'_>, dimension: &str) -> Result<Option<Value>, Error> {
    Ok(rows(
        tx,
        "SELECT * FROM current_accounting_dimension_definitions WHERE dimension_id=?1",
        [dimension],
    )?
    .into_iter()
    .next())
}
fn member(tx: &Transaction<'_>, dimension: &str, member: &str) -> Result<Option<Value>, Error> {
    Ok(rows(
        tx,
        "SELECT * FROM current_accounting_dimension_members WHERE dimension_id=?1 AND member_id=?2",
        [dimension, member],
    )?
    .into_iter()
    .next())
}
pub fn definitions(tx: &Transaction<'_>) -> Result<Value, Error> {
    crate::validate(tx)?;
    Ok(
        json!({"ok":true,"definitions":rows(tx,"SELECT * FROM accounting_dimension_definition_events ORDER BY dimension_id,id",[])?}),
    )
}
pub fn members(tx: &Transaction<'_>, dimension: Option<&str>) -> Result<Value, Error> {
    crate::validate(tx)?;
    Ok(
        json!({"ok":true,"members":rows(tx,"SELECT * FROM accounting_dimension_member_events WHERE ?1 IS NULL OR dimension_id=?1 ORDER BY dimension_id,member_id,id",[dimension])?}),
    )
}
pub fn assignments(tx: &Transaction<'_>, line: i64) -> Result<Value, Error> {
    crate::validate(tx)?;
    id(line)?;
    Ok(
        json!({"ok":true,"assignments":rows(tx,"SELECT * FROM accounting_dimension_assignment_events WHERE journal_line_id=?1 ORDER BY id",[line])?}),
    )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MasterInput {
    pub confirm: bool,
    pub dimension_id: String,
    pub kind: Option<String>,
    pub name: Option<String>,
    pub member_id: Option<String>,
    pub status: Option<String>,
    pub effective_from: Option<String>,
    pub effective_to: Option<String>,
    pub action: Option<String>,
    pub supersedes_id: Option<String>,
}
/// Definition and member lifecycle mirrors source stable IDs and idempotent repeated events.
pub fn master(
    tx: &Transaction<'_>,
    operation: &str,
    input: &MasterInput,
    actor: &str,
) -> Result<Value, Error> {
    crate::validate(tx)?;
    if ![
        "define",
        "member",
        "definition-lifecycle",
        "member-lifecycle",
    ]
    .contains(&operation)
    {
        return Err(fail("INVALID_DIMENSION_ACTION"));
    }
    if !input.confirm {
        return Err(fail("CONFIRMATION_REQUIRED"));
    }
    let dimension = text(&input.dimension_id, 64)?;
    let by = text(actor, 160)?;
    let created = now(tx)?;
    let is_member = operation == "member" || operation == "member-lifecycle";
    let member_id = if is_member {
        Some(text(input.member_id.as_deref().unwrap_or(""), 64)?)
    } else {
        None
    };
    let old = if let Some(m) = &member_id {
        member(tx, &dimension, m)?
    } else {
        definition(tx, &dimension)?
    };
    let lifecycle = operation.ends_with("lifecycle");
    let action = if lifecycle {
        input.action.as_deref().unwrap_or("")
    } else {
        "define"
    };
    if !["define", "activate", "deactivate", "rename", "supersede"].contains(&action) {
        return Err(fail("INVALID_LIFECYCLE_ACTION"));
    }
    let (name, status, event, kind, from, to, target) = if action == "define" {
        let name = text(input.name.as_deref().unwrap_or(""), 160)?;
        if is_member {
            if definition(tx, &dimension)?.is_none_or(|d| d["status"] != "active") {
                return Err(fail("INVALID_MEMBER"));
            }
        } else if !dimension.bytes().enumerate().all(|(i, b)| {
            if i == 0 {
                b.is_ascii_lowercase()
            } else {
                b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-'
            }
        }) {
            return Err(fail("INVALID_DIMENSION"));
        }
        let kind = if is_member {
            None
        } else {
            Some(text(input.kind.as_deref().unwrap_or(""), 64)?)
        };
        let status = if is_member {
            input.status.as_deref().unwrap_or("active")
        } else {
            "active"
        };
        if !["active", "inactive"].contains(&status) {
            return Err(fail("INVALID_MEMBER"));
        }
        for date in [&input.effective_from, &input.effective_to]
            .into_iter()
            .flatten()
        {
            Date::parse(date).map_err(|_| fail("INVALID_EFFECTIVE_DATES"))?;
        }
        if input
            .effective_from
            .as_ref()
            .zip(input.effective_to.as_ref())
            .is_some_and(|(a, b)| a > b)
        {
            return Err(fail("INVALID_EFFECTIVE_DATES"));
        }
        if let Some(old) = &old {
            let same = old["name"] == name
                && if is_member {
                    old["status"] == status
                        && old["effective_from"] == json!(input.effective_from)
                        && old["effective_to"] == json!(input.effective_to)
                } else {
                    old["kind"] == json!(kind)
                };
            return if same {
                Ok(json!({"ok":true,"id":old["id"],"idempotent":true}))
            } else {
                Err(fail(if is_member {
                    "MEMBER_CONFLICT"
                } else {
                    "DIMENSION_CONFLICT"
                }))
            };
        }
        (
            name,
            status.to_string(),
            "defined",
            kind,
            input.effective_from.clone(),
            input.effective_to.clone(),
            None,
        )
    } else {
        let old = old.ok_or_else(|| {
            fail(if is_member {
                "MEMBER_NOT_FOUND"
            } else {
                "DIMENSION_NOT_FOUND"
            })
        })?;
        let name = if action == "rename" {
            text(input.name.as_deref().unwrap_or(""), 160)?
        } else {
            old["name"]
                .as_str()
                .ok_or_else(|| invalid("dimension name"))?
                .to_owned()
        };
        let status = match action {
            "activate" => "active",
            "deactivate" => "inactive",
            "supersede" => "superseded",
            _ => old["status"]
                .as_str()
                .ok_or_else(|| invalid("dimension status"))?,
        }
        .to_owned();
        if action != "supersede" && old["name"] == name && old["status"] == status {
            return Ok(json!({"ok":true,"id":old["id"],"idempotent":true}));
        }
        let target = if action == "supersede" {
            let target = text(input.supersedes_id.as_deref().unwrap_or(""), 64)?;
            let found = if is_member {
                member(tx, &dimension, &target)?
            } else {
                definition(tx, &target)?
            };
            if found.is_none_or(|v| v["status"] != "active") {
                return Err(fail(if is_member {
                    "SUPERSEDING_MEMBER_NOT_FOUND"
                } else {
                    "SUPERSEDING_DIMENSION_NOT_FOUND"
                }));
            }
            Some(target)
        } else {
            None
        };
        (
            name,
            status,
            match action {
                "activate" => "activated",
                "deactivate" => "deactivated",
                "rename" => "renamed",
                _ => "superseded",
            },
            old["kind"].as_str().map(str::to_owned),
            old["effective_from"].as_str().map(str::to_owned),
            old["effective_to"].as_str().map(str::to_owned),
            target,
        )
    };
    if let Some(m) = member_id {
        tx.execute("INSERT INTO accounting_dimension_member_events(dimension_id,member_id,name,status,effective_from,effective_to,event_type,supersedes_member_id,actor,principal,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9,?10)",params![dimension,m,name,status,from,to,event,target,by,created]).map_err(invalid)?;
    } else {
        tx.execute("INSERT INTO accounting_dimension_definition_events(dimension_id,kind,name,status,event_type,supersedes_dimension_id,actor,principal,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?7,?8)",params![dimension,kind,name,status,event,target,by,created]).map_err(invalid)?;
    }
    Ok(json!({"ok":true,"id":tx.last_insert_rowid(),"idempotent":false}))
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Allocation {
    pub dimension_id: String,
    pub member_id: String,
    pub amount_minor: i64,
    pub currency: String,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssignmentInput {
    pub journal_line_id: i64,
    pub allocations: Vec<Allocation>,
    pub source: Option<String>,
    pub reviewed_import: Option<bool>,
    pub source_ref: Option<String>,
    pub confirm: Option<bool>,
    pub plan_hash: Option<String>,
    pub idempotency_key: Option<String>,
    pub expected_assignment_id: Option<i64>,
    pub reason: Option<String>,
}
/// Canonical sorted-key JSON matches source hashing; all amounts are integer minor units.
fn canonical(value: &Value) -> String {
    match value {
        Value::Object(o) => {
            let mut keys = o.keys().collect::<Vec<_>>();
            keys.sort();
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|k| format!("{}:{}", json!(k), canonical(&o[k])))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        _ => value.to_string(),
    }
}
pub fn plan(tx: &Transaction<'_>, input: &AssignmentInput) -> Result<Value, Error> {
    crate::validate(tx)?;
    id(input.journal_line_id)?;
    let mut q=tx.prepare("SELECT jl.debit_amount,jl.credit_amount,jl.currency,je.id,je.entry_hash,je.transaction_date FROM journal_lines jl JOIN journal_entries je ON je.id=jl.journal_entry_id WHERE jl.id=?1").map_err(invalid)?;
    let mut query = q.query([input.journal_line_id]).map_err(invalid)?;
    let line = query
        .next()
        .map_err(invalid)?
        .ok_or_else(|| fail("JOURNAL_LINE_NOT_FOUND"))?;
    let amount = legacy_money(line.get_ref(0).map_err(invalid)?)?
        .subtract(&legacy_money(line.get_ref(1).map_err(invalid)?)?);
    let minor = amount
        .decimal()
        .trim_start_matches('-')
        .replace('.', "")
        .parse::<i64>()
        .map_err(|_| fail("LINE_AMOUNT_INVALID"))?;
    if minor > MAX {
        return Err(fail("LINE_AMOUNT_INVALID"));
    }
    let currency: String = line.get(2).map_err(invalid)?;
    let entry: i64 = line.get(3).map_err(invalid)?;
    let hash: String = line.get(4).map_err(invalid)?;
    let date: String = line.get(5).map_err(invalid)?;
    if input.allocations.is_empty() {
        return Err(fail("ALLOCATIONS_REQUIRED"));
    }
    let mut groups = std::collections::BTreeMap::<String, i64>::new();
    let mut allocations = vec![];
    for a in &input.allocations {
        let dimension = text(&a.dimension_id, 64)?;
        let member_id = text(&a.member_id, 64)?;
        let curr = text(&a.currency, 3)?.to_uppercase();
        if a.amount_minor <= 0 || a.amount_minor > MAX || curr != currency {
            return Err(fail("INVALID_ALLOCATION"));
        }
        let total = groups.entry(dimension.clone()).or_default();
        *total = total
            .checked_add(a.amount_minor)
            .filter(|v| *v <= MAX)
            .ok_or_else(|| fail("ALLOCATION_DOES_NOT_RECONCILE"))?;
        if definition(tx, &dimension)?.is_none_or(|d| d["status"] != "active") {
            return Err(fail("DIMENSION_NOT_ACTIVE"));
        }
        let member = member(tx, &dimension, &member_id)?.ok_or_else(|| fail("MEMBER_NOT_FOUND"))?;
        if member["status"] != "active" {
            return Err(fail("INACTIVE_MEMBER_REQUIRES_REVIEWED_OVERRIDE"));
        }
        if member["effective_from"]
            .as_str()
            .is_some_and(|v| date.as_str() < v)
            || member["effective_to"]
                .as_str()
                .is_some_and(|v| date.as_str() > v)
        {
            return Err(fail("MEMBER_NOT_EFFECTIVE_ON_JOURNAL_DATE"));
        }
        allocations.push(json!({"dimensionId":dimension,"memberId":member_id,"amountMinor":a.amount_minor,"currency":curr}));
    }
    if groups.values().any(|n| *n != minor) {
        return Err(fail("ALLOCATION_DOES_NOT_RECONCILE"));
    }
    allocations.sort_by_key(canonical);
    let source = input.source.as_deref().unwrap_or("reviewed");
    if !["reviewed", "imported"].contains(&source) {
        return Err(fail("INVALID_SOURCE"));
    }
    let source_ref = if source == "imported" {
        if input.reviewed_import != Some(true) {
            return Err(fail("IMPORTED_PROVENANCE_REQUIRES_REVIEW"));
        }
        Some(
            text(input.source_ref.as_deref().unwrap_or(""), 500)
                .map_err(|_| fail("IMPORTED_PROVENANCE_REQUIRES_REVIEW"))?,
        )
    } else {
        None
    };
    let mut plan = json!({"schemaVersion":"rentemester-accounting-dimensions-v1","journalLineId":input.journal_line_id,"journalEntryId":entry,"journalEntryHash":hash,"lineCurrency":currency,"lineAmountMinor":minor,"allocations":allocations,"source":source,"sourceRef":source_ref});
    plan["planHash"] = json!(format!("{:x}", Sha256::digest(canonical(&plan).as_bytes())));
    Ok(json!({"ok":true,"plan":plan}))
}
fn insert_assigned(
    tx: &Transaction<'_>,
    plan: &Value,
    key: Option<&str>,
    actor: &str,
    created: &str,
) -> Result<i64, Error> {
    tx.execute("INSERT INTO accounting_dimension_assignment_events(journal_line_id,journal_entry_id,journal_entry_hash,line_currency,line_amount_minor,allocations_json,source,source_ref,plan_hash,event_type,idempotency_key,actor,principal,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'assigned',?10,?11,?11,?12)",params![plan["journalLineId"].as_i64(),plan["journalEntryId"].as_i64(),plan["journalEntryHash"].as_str(),plan["lineCurrency"].as_str(),plan["lineAmountMinor"].as_i64(),canonical(&plan["allocations"]),plan["source"].as_str(),plan["sourceRef"].as_str(),plan["planHash"].as_str(),key,actor,created]).map_err(invalid)?;
    Ok(tx.last_insert_rowid())
}
fn retire(
    tx: &Transaction<'_>,
    old: &Value,
    reason: &str,
    actor: &str,
    created: &str,
) -> Result<i64, Error> {
    tx.execute("INSERT INTO accounting_dimension_assignment_events(journal_line_id,journal_entry_id,journal_entry_hash,line_currency,line_amount_minor,allocations_json,source,source_ref,plan_hash,event_type,supersedes_assignment_id,reason,actor,principal,created_at)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'superseded',?10,?11,?12,?12,?13)",params![old["journal_line_id"].as_i64(),old["journal_entry_id"].as_i64(),old["journal_entry_hash"].as_str(),old["line_currency"].as_str(),old["line_amount_minor"].as_i64(),old["allocations_json"].as_str(),old["source"].as_str(),old["source_ref"].as_str(),old["plan_hash"].as_str(),old["id"].as_i64(),reason,actor,created]).map_err(invalid)?;
    Ok(tx.last_insert_rowid())
}
pub fn apply(
    tx: &Transaction<'_>,
    input: &AssignmentInput,
    replace: bool,
    actor: &str,
) -> Result<Value, Error> {
    if input.confirm != Some(true) {
        return Err(fail("CONFIRMATION_REQUIRED"));
    }
    let by = text(actor, 160)?;
    let reason = if replace {
        Some(text(input.reason.as_deref().unwrap_or(""), 1000)?)
    } else {
        None
    };
    let planned = plan(tx, input)?;
    let plan = &planned["plan"];
    let hash = input.plan_hash.as_deref().unwrap_or("");
    if plan["planHash"] != hash {
        return Err(fail("PLAN_HASH_MISMATCH"));
    }
    let key = if replace {
        input
            .idempotency_key
            .as_deref()
            .map(|s| text(s, 200))
            .transpose()?
    } else {
        input.idempotency_key.clone()
    };
    if let Some(key) = &key {
        if let Some(prior)=rows(tx,"SELECT id,plan_hash FROM accounting_dimension_assignment_events WHERE idempotency_key=?1",[key])?.first(){return if prior["plan_hash"]==hash{Ok(json!({"ok":true,"id":prior["id"],"idempotent":true,"planHash":hash}))}else{Err(fail("IDEMPOTENCY_CONFLICT"))}}
    }
    let current = rows(
        tx,
        "SELECT * FROM current_accounting_dimension_assignments WHERE journal_line_id=?1 ORDER BY id",
        [input.journal_line_id],
    )?;
    if !replace && !current.is_empty() {
        return if current.iter().all(|v| v["plan_hash"] == hash) {
            Ok(json!({"ok":true,"id":current[0]["id"],"idempotent":true,"planHash":hash}))
        } else {
            Err(fail("CURRENT_ASSIGNMENT_REQUIRES_SUPERSESSION"))
        };
    }
    let created = now(tx)?;
    let retired = if replace {
        if current.len() != 1 {
            return Err(fail("CURRENT_ASSIGNMENT_NOT_FOUND"));
        }
        let old = &current[0];
        if old["id"].as_i64() != input.expected_assignment_id {
            return Err(fail("CURRENT_ASSIGNMENT_CONFLICT"));
        }
        if old["plan_hash"] == hash {
            return Ok(json!({"ok":true,"id":old["id"],"idempotent":true,"planHash":hash}));
        }
        Some(retire(tx, old, reason.as_deref().unwrap(), &by, &created)?)
    } else {
        None
    };
    let assigned = insert_assigned(tx, plan, key.as_deref(), &by, &created)?;
    Ok(json!({"ok":true,"id":assigned,"supersededId":retired,"idempotent":false,"planHash":hash}))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Supersede {
    pub confirm: bool,
    pub assignment_id: i64,
    pub reason: String,
}
pub fn supersede(tx: &Transaction<'_>, input: &Supersede, actor: &str) -> Result<Value, Error> {
    crate::validate(tx)?;
    if !input.confirm {
        return Err(fail("CONFIRMATION_REQUIRED"));
    }
    id(input.assignment_id)?;
    let reason = text(&input.reason, 1000)?;
    let by = text(actor, 160)?;
    let old = rows(
        tx,
        "SELECT * FROM current_accounting_dimension_assignments WHERE id=?1",
        [input.assignment_id],
    )?
    .into_iter()
    .next()
    .ok_or_else(|| fail("ASSIGNMENT_NOT_FOUND"))?;
    let id = retire(tx, &old, &reason, &by, &now(tx)?)?;
    Ok(json!({"ok":true,"id":id,"idempotent":false}))
}
#[cfg(test)]
#[path = "dimensions_tests.rs"]
mod tests;
