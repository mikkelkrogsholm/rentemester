//! Native in-product reminders: verified current identities, permissions and one business transaction.
use super::*;
use store::reminders::{MaintenanceResult, MaintenanceSummary};
fn allowed(access: &Access, task: &Task) -> bool {
    scope_allowed(access, &task.scope, false)
}
fn recipient(
    tx: &Transaction<'_>,
    catalog: &Catalog,
    id: &str,
    task: &Task,
) -> Result<bool, Error> {
    if id == "local" {
        return Ok(false);
    }
    match recipient_access(tx, id, catalog) {
        Ok(a) => Ok(allowed(&a, task)),
        Err(e) if ["invalid_assignee", "access_denied"].contains(&e.code) => Ok(false),
        Err(e) => Err(e),
    }
}
pub(super) fn notifications(
    tx: &Transaction<'_>,
    catalog: &Catalog,
    identity: &Identity,
    access: &Access,
) -> Result<Vec<domain::reminders::Notification>, Error> {
    store::reminders::visible(tx, &identity.subject, |task| {
        if !allowed(access, task) {
            return Ok(false);
        }
        authorize_task(tx, catalog, access, task, false)?;
        recipient(tx, catalog, &identity.subject, task)
    })
}
pub(super) fn preadmit(
    action: &Action<'_>,
    access: &Access,
    expected: Option<u64>,
) -> Result<(), Error> {
    match action {
        Action::Reminder { reminder, .. } => {
            if expected.is_none_or(|n| n == 0 || n > 9_007_199_254_740_991) {
                return Err(error("version_required"));
            }
            domain::reminders::normalize(reminder)?;
        }
        Action::Run(v) => {
            if !access.owner {
                return Err(denied());
            }
            sources::preadmit(v, access, expected)?;
        }
        Action::Maintain { owner } | Action::ClaimRuntime { owner } => {
            domain::id(&json!(owner))?;
            if expected.is_some() || !access.owner && access.write.is_empty() {
                return Err(denied());
            }
        }
        _ => {}
    }
    Ok(())
}
pub(super) fn configure(
    tx: &Transaction<'_>,
    id: &str,
    value: &Value,
    ctx: &store::Context<'_>,
    catalog: &Catalog,
    access: &Access,
) -> Result<Value, Error> {
    let task = store::reminders::configure_in_tx(
        tx,
        id,
        value,
        ctx,
        |t| authorize_task(tx, catalog, access, t, true),
        |id, t| {
            if recipient(tx, catalog, id, t)? {
                Ok(())
            } else {
                Err(error("invalid_reminder"))
            }
        },
    )?;
    Ok(json!({"task":task}))
}
pub(super) fn admit_result(
    tx: &Transaction<'_>,
    r: &MaintenanceResult,
    catalog: &Catalog,
    access: &Access,
) -> Result<(), Error> {
    if !scope_allowed(access, &r.scope, true)
        || r.includes_workspace && !access.owner
        || !scope_allowed(access, &r.notification_scope, false)
        || r.notifications_include_workspace && !access.owner
    {
        return Err(denied());
    }
    for id in &r.run.materialized.task_ids {
        let t = get_current(tx, id, catalog, access, true)?;
        series::manage(access, &t.scope)?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn maintain(
    tx: &Transaction<'_>,
    action: &Action<'_>,
    ctx: &store::Context<'_>,
    catalog: &Catalog,
    identity: &Identity,
    access: &Access,
    now: i64,
    reader: &mut Option<&mut dyn SourceReader>,
) -> Result<Value, Error> {
    let empty = json!({});
    let (input, owner) = match action {
        Action::Run(v) => (*v, None),
        Action::Maintain { owner } => (&empty, Some(*owner)),
        _ => return Err(error("invalid_input")),
    };
    if let Some(owner) = owner {
        store::reminders::require_lease(tx, owner, ctx.now)?;
    }
    let selected = if let Some(v) = input.get("companySlugs") {
        v.as_array()
            .ok_or_else(denied)?
            .iter()
            .map(|s| s.as_str().map(str::to_owned).ok_or_else(denied))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        access.write.iter().cloned().collect()
    };
    // Explicit archived task sync remains available separately; autonomous maintenance is active Live only.
    let selected = selected
        .into_iter()
        .filter(|s| {
            catalog
                .companies
                .iter()
                .any(|c| c.slug == *s && !c.archived)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if owner.is_none() {
        if let Some(prior) = store::runtime_receipt(tx, ctx, "runtime-run", input)? {
            let result: MaintenanceResult =
                serde_json::from_value(prior).map_err(|_| error("invalid_control"))?;
            admit_result(tx, &result, catalog, access)?;
            return Ok(json!(result));
        }
    }
    let as_of = input["asOfDate"].as_str().map(str::to_owned).unwrap_or(
        rentemester_runtime::posting_time::latest_date(now, 0)
            .map_err(|_| error("invalid_auth_clock"))?,
    );
    let schedule_ms = if input.get("asOfDate").is_some() {
        i64::try_from(domain::instant(&format!("{as_of}T12:00:00.000Z"))?.1)
            .map_err(|_| error("invalid_input"))?
    } else {
        now
    };
    let source_key = format!(
        "runtime-source:{}",
        domain::payload_hash(&json!([ctx.idempotency_key, input]))?
    );
    let source_ctx = store::Context {
        actor: ctx.actor,
        principal: ctx.principal,
        idempotency_key: &source_key,
        expected_version: None,
        now: ctx.now,
        source_check: None,
    };
    let sync_input = json!({"companySlugs":selected,"asOfDate":as_of});
    let sync: store::sources::SyncResult = serde_json::from_value(sources::mutate(
        tx,
        &sync_input,
        &source_ctx,
        catalog,
        access,
        now,
        reader,
    )?)
    .map_err(|_| error("invalid_control"))?;
    let manage = selected
        .iter()
        .filter(|s| access.manage.contains(*s))
        .cloned()
        .collect::<Vec<_>>();
    let include_workspace = access.owner && input.get("companySlugs").is_none();
    let series_key = format!(
        "runtime-series:{}",
        domain::payload_hash(&json!([ctx.idempotency_key, input]))?
    );
    let series_ctx = store::Context {
        actor: ctx.actor,
        principal: ctx.principal,
        idempotency_key: &series_key,
        expected_version: None,
        now: ctx.now,
        source_check: None,
    };
    let materialized = store::series::materialize_in_tx(
        tx,
        &json!({"companySlugs":manage,"asOfDate":as_of}),
        &manage,
        include_workspace,
        &as_of,
        &series_ctx,
        |scope| series::manage(access, scope),
        |t| authorize_task(tx, catalog, access, t, true),
        |t| recipients(tx, catalog, t),
    )?;
    let notification_stamp = rentemester_engine::company_knowledge::timestamp(schedule_ms)?;
    let delivered = store::reminders::deliver(
        tx,
        &notification_stamp,
        |recipient_id, t| {
            if allowed(access, t) {
                recipient(tx, catalog, recipient_id, t)
            } else {
                Ok(false)
            }
        },
        |zone| {
            rentemester_runtime::posting_time::recipient_date(schedule_ms, zone)
                .map_err(|_| error("invalid_input"))
        },
    )?;
    let result = MaintenanceResult {
        scope: Scope::Workspace {
            company_slugs: selected,
        },
        includes_workspace: include_workspace,
        notification_scope: Scope::Workspace {
            company_slugs: access.read.iter().cloned().collect(),
        },
        notifications_include_workspace: access.owner,
        run: MaintenanceSummary {
            sync: sync.sync,
            materialized: materialized.materialized,
            delivered,
        },
    };
    admit_result(tx, &result, catalog, access)?;
    if let Some(owner) = owner {
        store::reminders::finish_lease(tx, owner, ctx.now, false)?;
    } else {
        store::append_runtime_receipt(tx, ctx, input, &json!(result))?;
    }
    let _ = identity;
    Ok(json!(result))
}
