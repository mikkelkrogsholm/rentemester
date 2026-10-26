//! Versioned task business codec. Historical actors/assignees are evidence, never restored grants.
//! Runtime lease/tick/error are deliberately excluded and remain freshly initialized.
use super::*;
use rentemester_engine::tasks::{self as domain, Board, Task};
use serde_json::Value;
use std::collections::BTreeSet;

mod boards;
mod reminders;
const FORMAT5: &str = "rentemester-native-tasks/5";
mod sources;
const FORMAT4: &str = "rentemester-native-tasks/4";
mod series;
const FORMAT3: &str = "rentemester-native-tasks/3";
const FORMAT2: &str = "rentemester-native-tasks/2";
const FORMAT: &str = "rentemester-native-tasks/1";
const TABLES: [&str; 5] = [
    "rm_task_events",
    "rm_task_board_events",
    "rm_task_series_events",
    "rm_task_receipts",
    "rm_task_notifications",
];
fn shape(db: &Connection) -> Result<(), Error> {
    let reference = Connection::open_in_memory().map_err(sql)?;
    let artifact: Value = serde_json::from_str(include_str!(
        "../native-workspace-migrations/0033-workspace-tasks-v33.json"
    ))
    .map_err(sql)?;
    reference
        .execute_batch(
            artifact["sql"]
                .as_str()
                .ok_or_else(|| refused("task artifact invalid"))?,
        )
        .map_err(sql)?;
    for table in TABLES {
        if columns(db, table)? != columns(&reference, table)? {
            return Err(refused("task schema differs from codec v1"));
        }
    }
    Ok(())
}
fn json_refs(value: &Value, registered: &BTreeSet<&str>) -> Result<(), Error> {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                match key.as_str() {
                    "companySlug" if !value.is_null() => {
                        if value.as_str().is_none_or(|s| !registered.contains(s)) {
                            return Err(refused("task refers to an unregistered company"));
                        }
                    }
                    "companySlugs" => {
                        if value.as_array().is_none_or(|a| {
                            a.iter()
                                .any(|v| v.as_str().is_none_or(|s| !registered.contains(s)))
                        }) {
                            return Err(refused("task companies differ from complete manifest"));
                        }
                    }
                    _ => {}
                }
                json_refs(value, registered)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                json_refs(value, registered)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn decoded(raw: &str) -> Result<Value, Error> {
    if raw.len() > 2_000_000 {
        return Err(refused("task JSON bound"));
    }
    serde_json::from_str(raw).map_err(sql)
}
fn task_snapshot(raw: &str) -> Result<Task, Error> {
    let task: Task = serde_json::from_str(raw).map_err(sql)?;
    let mut editable = serde_json::to_value(&task).map_err(sql)?;
    for key in ["version", "completion", "createdAt", "updatedAt"] {
        editable.as_object_mut().unwrap().remove(key);
    }
    // A completed snapshot is not a new create command. Validate its editable
    // content with the common domain validator, then validate completion separately.
    editable["status"] = serde_json::json!("open");
    domain::draft(&editable)?;
    domain::id(&serde_json::json!(task.task_id))?;
    domain::scope(&serde_json::json!(task.scope))?;
    if task.version == 0
        || task.version > 9_007_199_254_740_991
        || !domain::STATUSES.contains(&task.status.as_str())
        || (task.status == "done") != task.completion.is_some()
    {
        return Err(refused("task snapshot state differs"));
    }
    domain::instant(&task.created_at)?;
    domain::instant(&task.updated_at)?;
    domain::references(&serde_json::json!(task.references), &task.scope)?;
    if let Some(c) = &task.completion {
        domain::text(&serde_json::json!(c.note), "completion note", 8000, true)?;
        domain::draft(
            &serde_json::json!({"title":"Historical completion","scope":task.scope,"description":c.note}),
        )?;
        if !["completed", "not_relevant", "cancelled", "exception"].contains(&c.outcome.as_str())
            || !["user_reported", "product_verified"].contains(&c.assurance.as_str())
        {
            return Err(refused("task completion differs"));
        }
        if c.references.len() > 200 {
            return Err(refused("task completion reference bound"));
        }
        for chunk in c.references.chunks(100) {
            domain::references(&serde_json::json!(chunk), &task.scope)?;
        }
        domain::instant(&c.at)?;
        if c.assurance == "product_verified"
            && (c.outcome != "completed"
                || c.references.is_empty()
                || task.source.as_ref().is_none_or(|s| s.state != "resolved"))
        {
            return Err(refused("task verification differs"));
        }
    }
    Ok(task)
}
fn facts(db: &Connection, companies: Option<&[String]>, codec: u8) -> Result<(), Error> {
    if reminders::needed(db)? && codec < 5 {
        return Err(refused("Reminder/notification operations require codec5"));
    }
    if sources::needed(db)? && codec < 4 {
        return Err(refused("Source task operations require codec4"));
    }
    for (table, key) in [
        (TABLES[0], "task_id"),
        (TABLES[1], "board_id"),
        (TABLES[2], "series_id"),
    ] {
        let bad:i64=db.query_row(&format!("SELECT count(*) FROM (SELECT {key},min(version) lo,max(version) hi,count(*) n FROM {table} GROUP BY {key} HAVING lo<>1 OR hi<>n)"),[],|r|r.get(0)).map_err(sql)?;
        if bad != 0 {
            return Err(refused("task event versions are not a complete prefix"));
        }
        let bad:i64=db.query_row(&format!("SELECT count(*) FROM (SELECT version,lag(version) OVER (PARTITION BY {key} ORDER BY id) prior FROM {table}) WHERE prior IS NOT NULL AND version<>prior+1"),[],|r|r.get(0)).map_err(sql)?;
        if bad != 0 {
            return Err(refused("task event chronology differs"));
        }
        let bad: i64 = db
            .query_row(
                &format!("SELECT count(*) FROM {table} WHERE id<=0"),
                [],
                |r| r.get(0),
            )
            .map_err(sql)?;
        if bad != 0 {
            return Err(refused("task event id invalid"));
        }
    }
    let registered: Option<BTreeSet<&str>> =
        companies.map(|c| c.iter().map(String::as_str).collect());
    for table in TABLES {
        let json_column = if table == "rm_task_receipts" {
            "result_json"
        } else {
            "payload_json"
        };
        let mut q = db
            .prepare(&format!("SELECT {json_column} FROM {table}"))
            .map_err(sql)?;
        for raw in q.query_map([], |r| r.get::<_, String>(0)).map_err(sql)? {
            let raw = raw.map_err(sql)?;
            let value = decoded(&raw)?;
            if let Some(registered) = &registered {
                json_refs(&value, registered)?;
            }
            if table == "rm_task_events" {
                task_snapshot(&raw)?;
            }
            if table == "rm_task_board_events" {
                let board: Board = serde_json::from_str(&raw).map_err(sql)?;
                domain::scope(&serde_json::json!(board.scope))?;
                if board.board_id != board.scope.board_id()
                    || board.columns.is_empty()
                    || board.columns.len() > 40
                {
                    return Err(refused("task board differs"));
                }
            }
            if table == "rm_task_series_events" {
                domain::scope(&value["scope"])?;
                domain::draft(&value["template"])?;
                if !["month", "quarter", "year", "custom"]
                    .contains(&value["cadence"].as_str().unwrap_or(""))
                {
                    return Err(refused("task series cadence differs"));
                }
            }
        }
    }
    let bad:i64=db.query_row("SELECT count(*) FROM rm_task_events WHERE json_extract(payload_json,'$.updatedAt') IS NOT created_at",[],|r|r.get(0)).map_err(sql)?;
    if bad != 0 {
        return Err(refused("task event timestamp differs"));
    }
    let bad:i64=db.query_row("SELECT count(*) FROM rm_task_notifications n WHERE NOT EXISTS(SELECT 1 FROM rm_task_events e WHERE e.task_id=n.task_id)",[],|r|r.get(0)).map_err(sql)?;
    if bad != 0 {
        return Err(refused("task notification refers to missing history"));
    }
    if codec >= 2 {
        series::events(db)?;
    }
    if codec >= 3 {
        boards::events(db)?;
    }
    if codec >= 4 {
        sources::events(db)?;
    }
    if codec >= 5 {
        reminders::notifications(db)?;
    }
    let mut q = db
        .prepare("SELECT principal,operation,result_json,created_at FROM rm_task_receipts")
        .map_err(sql)?;
    for row in q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(sql)?
    {
        let (principal, operation, raw, at) = row.map_err(sql)?;
        if codec >= 5 && reminders::receipt(db, &principal, &operation, &raw, &at)? {
            continue;
        }
        if codec >= 4 && sources::receipt(db, &operation, &raw, &at)? {
            continue;
        }
        if codec >= 3 && boards::receipt(db, &principal, &operation, &raw, &at)? {
            continue;
        }
        if codec >= 2 && series::receipt(db, &principal, &operation, &raw, &at)? {
            continue;
        }
        let event = match operation.as_str() {
            "task-create" => "created",
            "task-update" => "updated",
            "task-move" => "moved",
            "task-complete" => "completed",
            "task-reopen" => "reopened",
            _ => return Err(refused("task receipt operation requires an explicit codec")),
        };
        let task = task_snapshot(&raw)?;
        let matches:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM rm_task_events WHERE task_id=?1 AND version=?2 AND operation=?3 AND principal=?4 AND payload_json=?5 AND created_at=?6)",params![task.task_id,i64::try_from(task.version).map_err(sql)?,event,principal,raw,at],|r|r.get(0)).map_err(sql)?;
        if !matches {
            return Err(refused(
                "task receipt has no exact historical result/principal/operation",
            ));
        }
        // Original request bytes are not stored here: do not invent a request-hash recomputation.
    }
    Ok(())
}
pub(super) fn export(db: &Connection) -> Result<Vec<u8>, Error> {
    shape(db)?;
    let codec = if reminders::needed(db)? {
        5
    } else if sources::needed(db)? {
        4
    } else if boards::needed(db)? {
        3
    } else if series::needed(db)? {
        2
    } else {
        1
    };
    facts(db, None, codec)?;
    export_tables(
        db,
        &TABLES,
        match codec {
            5 => FORMAT5,
            4 => FORMAT4,
            3 => FORMAT3,
            2 => FORMAT2,
            _ => FORMAT,
        },
    )
}
pub fn validate_companies(db: &Connection, companies: &[String]) -> Result<(), Error> {
    shape(db)?;
    facts(
        db,
        Some(companies),
        if reminders::needed(db)? {
            5
        } else if sources::needed(db)? {
            4
        } else if boards::needed(db)? {
            3
        } else if series::needed(db)? {
            2
        } else {
            1
        },
    )
}
pub fn initialize(db: &mut Connection, bytes: &[u8], companies: &[String]) -> Result<(), Error> {
    crate::control_migration::validate_current_control(db)?;
    shape(db)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(refused("task codec byte bound"));
    }
    let b: Business = serde_json::from_slice(bytes).map_err(sql)?;
    if ![FORMAT, FORMAT2, FORMAT3, FORMAT4, FORMAT5].contains(&b.format.as_str())
        || b.tables.len() != TABLES.len()
    {
        return Err(refused("unsupported task codec"));
    }
    let mut total = 0usize;
    for table in TABLES {
        let data = b
            .tables
            .get(table)
            .ok_or_else(|| refused("task table missing"))?;
        if data.columns != columns(db, table)? || data.rows.len() > 100000 {
            return Err(refused("task table shape differs"));
        }
        for row in &data.rows {
            if row.len() != data.columns.len() {
                return Err(refused("task row shape differs"));
            }
            for cell in row {
                if let Cell::Text(s) = cell {
                    total = total
                        .checked_add(s.len())
                        .ok_or_else(|| refused("task codec byte bound"))?;
                }
            }
        }
    }
    if total > 16 * 1024 * 1024 {
        return Err(refused("task codec byte bound"));
    }
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(sql)?;
    for table in TABLES {
        if tx
            .query_row::<i64, _, _>(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .map_err(sql)?
            != 0
        {
            return Err(refused("fresh task target required"));
        }
    }
    insert_tables(&tx, &b, &TABLES)?;
    facts(
        &tx,
        Some(companies),
        if b.format == FORMAT5 {
            5
        } else if b.format == FORMAT4 {
            4
        } else if b.format == FORMAT3 {
            3
        } else if b.format == FORMAT2 {
            2
        } else {
            1
        },
    )?;
    // No runtime state, authority, credentials or grants are imported by this codec.
    tx.execute("UPDATE rm_task_runtime SET lease_owner=NULL,lease_until=NULL,last_tick_at=NULL,last_error=NULL WHERE id=1",[]).map_err(sql)?;
    tx.commit().map_err(sql)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{self as writes, Command, Context};
    use serde_json::json;
    fn db() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        crate::control_migration::initialize_empty(&mut db).unwrap();
        db
    }
    pub(super) fn populated() -> Connection {
        let mut db = db();
        let v = json!({"taskId":"task-synthetic","title":"Kontrollér perioden","scope":{"kind":"company","companySlug":"synthetic"}});
        let ctx = Context {
            actor: "user:synthetic",
            principal: "synthetic-member",
            idempotency_key: "create",
            expected_version: None,
            now: "2026-10-26T11:00:00Z",
            source_check: None,
        };
        writes::mutate(
            &mut db,
            Command::Create {
                input: &v,
                generated_id: "unused-id",
            },
            ctx,
            |_, _| Ok(()),
        )
        .unwrap();
        db.execute("UPDATE rm_task_runtime SET lease_owner='old-owner',lease_until='2099-01-01T00:00:00Z',last_tick_at='2026-10-26T11:00:00Z'",[]).unwrap();
        db
    }
    #[test]
    fn history_and_receipts_roundtrip_without_runtime_or_authority() {
        let mut source = populated();
        let tx = source
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert!(
            crate::original716::transfer::require_source_export_coverage(&tx).is_err(),
            "the old source profile cannot silently omit task history"
        );
        tx.rollback().unwrap();
        let bytes = export(&source).unwrap();
        let mut target = db();
        initialize(&mut target, &bytes, &["synthetic".into()]).unwrap();
        assert_eq!(export(&target).unwrap(), bytes);
        assert_eq!(
            target
                .query_row::<i64, _, _>("SELECT count(*) FROM user", [], |r| r.get(0))
                .unwrap(),
            0
        );
        assert_eq!(
            target
                .query_row::<i64, _, _>(
                    "SELECT count(*) FROM rm_company_membership_events",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(
            target
                .query_row::<Option<String>, _, _>(
                    "SELECT lease_owner FROM rm_task_runtime",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
            None
        );
        assert_eq!(
            target
                .query_row::<i64, _, _>("SELECT count(*) FROM rm_task_receipts", [], |r| r.get(0))
                .unwrap(),
            1
        );
        assert!(initialize(&mut target, &bytes, &["synthetic".into()]).is_err());
    }
    #[test]
    fn unknown_company_wrong_codec_and_missing_table_refuse_atomically() {
        let bytes = export(&populated()).unwrap();
        let mut target = db();
        assert!(initialize(&mut target, &bytes, &["other".into()]).is_err());
        assert_eq!(
            target
                .query_row::<i64, _, _>("SELECT count(*) FROM rm_task_events", [], |r| r.get(0))
                .unwrap(),
            0
        );
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["format"] = json!("unknown");
        assert!(
            initialize(
                &mut target,
                &serde_json::to_vec(&value).unwrap(),
                &["synthetic".into()]
            )
            .is_err()
        );
        value["format"] = json!(FORMAT);
        value["tables"]
            .as_object_mut()
            .unwrap()
            .remove("rm_task_receipts");
        assert!(
            initialize(
                &mut target,
                &serde_json::to_vec(&value).unwrap(),
                &["synthetic".into()]
            )
            .is_err()
        );
    }
    #[test]
    fn malformed_cross_principal_and_wrong_operation_receipts_refuse_without_partial_restore() {
        let bytes = export(&populated()).unwrap();
        let original: Value = serde_json::from_slice(&bytes).unwrap();
        for (column, value) in [
            ("result_json", "{}"),
            ("principal", "another-member"),
            ("operation", "task-complete"),
            ("operation", "unknown-future-operation"),
        ] {
            let mut changed = original.clone();
            let table = &mut changed["tables"]["rm_task_receipts"];
            let index = table["columns"]
                .as_array()
                .unwrap()
                .iter()
                .position(|v| v == column)
                .unwrap();
            table["rows"][0][index]["value"] = serde_json::json!(value);
            let mut target = db();
            assert!(
                initialize(
                    &mut target,
                    &serde_json::to_vec(&changed).unwrap(),
                    &["synthetic".into()]
                )
                .is_err(),
                "{column}/{value}"
            );
            assert_eq!(
                target
                    .query_row::<i64, _, _>("SELECT count(*) FROM rm_task_events", [], |r| r.get(0))
                    .unwrap(),
                0
            );
            assert_eq!(
                target
                    .query_row::<i64, _, _>("SELECT count(*) FROM rm_task_receipts", [], |r| r
                        .get(0))
                    .unwrap(),
                0
            );
        }
    }
}
#[cfg(test)]
mod routine_tests;
