//! Board vocabulary is visible to members; structural consequences require current task authority.
use super::*;
use rentemester_engine::tasks::boards as engine;
fn input(value: &Value) -> Result<engine::Draft, Error> {
    let d = engine::validate(value)?;
    if d.columns
        .iter()
        .any(|c| c.name.encode_utf16().count() > 100)
    {
        return Err(error("invalid_input"));
    }
    Ok(d)
}
fn admit(scope: &Scope, structural: bool, catalog: &Catalog, access: &Access) -> Result<(), Error> {
    let slugs = scope.companies();
    if !(if slugs.is_empty() {
        access.owner
    } else {
        slugs.iter().all(|s| access.manage.contains(*s))
    }) {
        return Err(denied());
    }
    if structural
        && matches!(scope, Scope::Workspace { .. })
        && catalog
            .companies
            .iter()
            .any(|c| !access.write.contains(&c.slug))
    {
        return Err(denied());
    }
    Ok(())
}
pub(super) fn preview(
    tx: &Transaction<'_>,
    value: &Value,
    catalog: &Catalog,
    access: &Access,
) -> Result<engine::Changes, Error> {
    input(value)?;
    store::boards::preview(
        tx,
        value,
        |scope, structural| admit(scope, structural, catalog, access),
        |task| authorize_task(tx, catalog, access, task, true),
    )
}
pub(super) fn preadmit(
    tx: &Transaction<'_>,
    action: &Action<'_>,
    catalog: &Catalog,
    access: &Access,
    expected: Option<u64>,
) -> Result<(), Error> {
    match action {
        Action::BoardsSave(v) => {
            if expected.is_none_or(|n| n > 9_007_199_254_740_991) {
                return Err(error("version_required"));
            }
            input(v)?;
            store::boards::admit_request(tx, v, |scope, structural| {
                admit(scope, structural, catalog, access)
            })?;
        }
        Action::BoardsPreview(v) => {
            preview(tx, v, catalog, access)?;
        }
        _ => {}
    }
    Ok(())
}
pub(super) fn save(
    tx: &Transaction<'_>,
    value: &Value,
    ctx: &store::Context<'_>,
    catalog: &Catalog,
    access: &Access,
) -> Result<Value, Error> {
    let board = store::boards::save_in_tx(
        tx,
        value,
        ctx,
        |scope, structural| admit(scope, structural, catalog, access),
        |task| authorize_task(tx, catalog, access, task, true),
    )?;
    Ok(json!({"board":board}))
}
pub(super) fn list(tx: &Transaction<'_>, input: &Value, access: &Access) -> Result<Value, Error> {
    if input.as_object().is_none_or(|m| {
        m.keys()
            .any(|k| !["companySlugs", "includeArchived"].contains(&k.as_str()))
    }) || input
        .get("includeArchived")
        .is_some_and(|v| !v.is_boolean())
    {
        return Err(error("invalid_input"));
    }
    if let Some(slugs) = input.get("companySlugs") {
        if slugs.as_array().is_none_or(|a| {
            a.len() > 200
                || a.iter()
                    .any(|s| s.as_str().is_none_or(|s| !access.read.contains(s)))
        }) {
            return Err(denied());
        }
    }
    // The source contract lists all visible board vocabularies independently of workload filters.
    let stored = store::boards(tx)?;
    let boards: Vec<_> = access
        .read
        .iter()
        .map(|s| Scope::Company {
            company_slug: s.clone(),
        })
        .chain(std::iter::once(Scope::Workspace {
            company_slugs: vec![],
        }))
        .map(|scope| {
            stored
                .iter()
                .find(|b| b.board_id == scope.board_id())
                .cloned()
                .unwrap_or_else(|| engine::default_board(&scope))
        })
        .collect();
    Ok(json!({"boards":boards}))
}
pub(super) fn receipt(
    tx: &Transaction<'_>,
    principal: &str,
    key: &str,
    catalog: &Catalog,
    access: &Access,
) -> Result<Option<Value>, Error> {
    domain::id(&json!(key))?;
    let row:Option<(String,String)>=tx.query_row("SELECT operation,result_json FROM rm_task_receipts WHERE principal=?1 AND idempotency_key=?2",rusqlite::params![principal,key],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|_|error("invalid_control"))?;
    if let Some((operation, raw)) = &row {
        if operation == "runtime-run" {
            let result: store::reminders::MaintenanceResult =
                serde_json::from_str(raw).map_err(|_| error("invalid_control"))?;
            reminders::admit_result(tx, &result, catalog, access)?;
            return Ok(Some(json!({"operation":operation})));
        }
        if operation == "reminder-set" {
            let t: Task = serde_json::from_str(raw).map_err(|_| error("invalid_control"))?;
            get_current(tx, &t.task_id, catalog, access, false)?;
            authorize_task(tx, catalog, access, &t, false)?;
            return Ok(Some(json!({"operation":operation,"version":t.version})));
        }
        if operation == "sources-sync" {
            let result: store::sources::SyncResult =
                serde_json::from_str(raw).map_err(|_| error("invalid_control"))?;
            if !scope_allowed(access, &result.scope, false) || result.includes_workspace {
                return Err(denied());
            }
            return Ok(Some(json!({"operation":operation})));
        }
        if operation == "task-board-save" {
            let b: domain::Board =
                serde_json::from_str(raw).map_err(|_| error("invalid_control"))?;
            let live = store::boards::get(tx, &b.board_id)?.ok_or_else(|| error("not_found"))?;
            if !scope_allowed(access, &b.scope, false) || !scope_allowed(access, &live.scope, false)
            {
                return Err(denied());
            }
            return Ok(Some(json!({"operation":operation,"version":b.version})));
        }
    }
    store::series::receipt(
        tx,
        principal,
        key,
        |scope| {
            if scope_allowed(access, scope, false) {
                Ok(())
            } else {
                Err(denied())
            }
        },
        |task| authorize_task(tx, catalog, access, task, false),
    )
}
use rusqlite::OptionalExtension;
