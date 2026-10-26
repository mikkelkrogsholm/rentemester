//! Series streams and explicit occurrence writes inside the already admitted control transaction.
use super::*;
use rentemester_engine::tasks::{
    Scope,
    series::{self as domain, ProjectionQuery, Series},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Materialized {
    pub created: usize,
    pub existing: usize,
    pub task_ids: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Materialization {
    pub scope: Scope,
    pub includes_workspace: bool,
    pub materialized: Materialized,
}
pub fn get(db: &Connection, id: &str) -> Result<Option<Series>, Error> {
    tasks::id(&json!(id))?;
    let raw: Option<String> = db
        .query_row(
            "SELECT payload_json FROM rm_current_task_series WHERE series_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(invalid)?;
    raw.map(|s| serde_json::from_str(&s).map_err(invalid))
        .transpose()
}
pub fn histories(db: &Connection) -> Result<Vec<Vec<Series>>, Error> {
    let mut q = db
        .prepare("SELECT payload_json FROM rm_task_series_events ORDER BY series_id,version")
        .map_err(invalid)?;
    let mut histories: Vec<Vec<Series>> = vec![];
    for row in q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?
    {
        let series: Series = serde_json::from_str(&row.map_err(invalid)?).map_err(invalid)?;
        if histories
            .last()
            .is_none_or(|h| h.last().is_none_or(|s| s.series_id != series.series_id))
        {
            histories.push(vec![]);
        }
        histories.last_mut().unwrap().push(series);
    }
    Ok(histories)
}
pub(super) fn fence(tx: &Transaction<'_>, ctx: &Context<'_>) -> Result<(), Error> {
    if tx
        .transaction_state(Some(rusqlite::MAIN_DB))
        .map_err(invalid)?
        != rusqlite::TransactionState::Write
    {
        return Err(Error::new(
            "control_fence_required",
            "Series writer transaction required",
        ));
    }
    tasks::mutation_identity(ctx.actor, ctx.principal, ctx.idempotency_key)?;
    tasks::instant(ctx.now)?;
    Ok(())
}
pub(super) fn prior(
    tx: &Transaction<'_>,
    ctx: &Context<'_>,
    operation: &str,
    payload: &Value,
) -> Result<Option<Value>, Error> {
    let hash =
        tasks::payload_hash(&json!({"payload":payload,"expectedVersion":ctx.expected_version}))?;
    let row:Option<(String,String,String)>=tx.query_row("SELECT operation,payload_hash,result_json FROM rm_task_receipts WHERE principal=?1 AND idempotency_key=?2",params![ctx.principal,ctx.idempotency_key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(invalid)?;
    row.map(|(op, h, raw)| {
        if op != operation || h != hash {
            Err(Error::new(
                "idempotency_conflict",
                "Key already used for another request",
            ))
        } else {
            serde_json::from_str(&raw).map_err(invalid)
        }
    })
    .transpose()
}
pub(super) fn append_receipt(
    tx: &Transaction<'_>,
    ctx: &Context<'_>,
    operation: &str,
    payload: &Value,
    result: &Value,
) -> Result<(), Error> {
    let hash =
        tasks::payload_hash(&json!({"payload":payload,"expectedVersion":ctx.expected_version}))?;
    tx.execute("INSERT INTO rm_task_receipts(principal,idempotency_key,operation,payload_hash,result_json,created_at)VALUES(?1,?2,?3,?4,?5,?6)",params![ctx.principal,ctx.idempotency_key,operation,hash,tasks::canonical_json(result),tasks::instant(ctx.now)?.0]).map_err(invalid)?;
    Ok(())
}
pub fn save_in_tx<A, V>(
    tx: &Transaction<'_>,
    input: &Value,
    ctx: &Context<'_>,
    mut authorize: A,
    mut proposed: V,
) -> Result<Series, Error>
where
    A: FnMut(&Scope) -> Result<(), Error>,
    V: FnMut(&Series) -> Result<(), Error>,
{
    fence(tx, ctx)?;
    let requested = domain::validate_series(input)?;
    authorize(&requested.scope)?;
    let current = get(tx, &requested.series_id)?;
    if let Some(current) = &current {
        authorize(&current.scope)?;
    }
    if let Some(value) = prior(tx, ctx, "series-save", input)? {
        let historical: Series = serde_json::from_value(value).map_err(invalid)?;
        let live = get(tx, &historical.series_id)?
            .ok_or_else(|| Error::new("not_found", "Series unavailable"))?;
        authorize(&live.scope)?;
        authorize(&historical.scope)?;
        return Ok(historical);
    }
    let series = domain::save_series(current.as_ref(), input, ctx.expected_version, ctx.now)?;
    proposed(&series)?;
    let raw = tasks::canonical_json(&json!(series));
    tx.execute("INSERT INTO rm_task_series_events(series_id,version,payload_json,actor,principal,created_at)VALUES(?1,?2,?3,?4,?5,?6)",params![series.series_id,i64::try_from(series.version).map_err(invalid)?,raw,ctx.actor,ctx.principal,series.updated_at]).map_err(invalid)?;
    append_receipt(tx, ctx, "series-save", input, &json!(series))?;
    Ok(series)
}
#[allow(clippy::too_many_arguments)]
pub fn materialize_in_tx<A, T, V>(
    tx: &Transaction<'_>,
    payload: &Value,
    companies: &[String],
    include_workspace: bool,
    as_of: &str,
    ctx: &Context<'_>,
    mut authorize_scope: A,
    mut authorize_task: T,
    mut proposed: V,
) -> Result<Materialization, Error>
where
    A: FnMut(&Scope) -> Result<(), Error>,
    T: FnMut(&Task) -> Result<(), Error>,
    V: FnMut(&Task) -> Result<(), Error>,
{
    fence(tx, ctx)?;
    tasks::date(&json!(as_of))?;
    let scope = Scope::Workspace {
        company_slugs: companies.to_vec(),
    };
    authorize_scope(&scope)?;
    if include_workspace {
        authorize_scope(&Scope::Workspace {
            company_slugs: vec![],
        })?;
    }
    if let Some(value) = prior(tx, ctx, "series-materialize", payload)? {
        let result: Materialization = serde_json::from_value(value).map_err(invalid)?;
        authorize_scope(&result.scope)?;
        if result.includes_workspace {
            authorize_scope(&Scope::Workspace {
                company_slugs: vec![],
            })?;
        }
        for id in &result.materialized.task_ids {
            authorize_task(
                &read(tx, id)?.ok_or_else(|| Error::new("not_found", "Task unavailable"))?,
            )?;
        }
        return Ok(result);
    }
    let histories = histories(tx)?;
    let mut tasks = super::list(tx, |_| Ok(true))?;
    let mut result = Materialization {
        scope,
        includes_workspace: include_workspace,
        materialized: Materialized {
            created: 0,
            existing: 0,
            task_ids: vec![],
        },
    };
    for history in histories {
        let Some(current) = history.last() else {
            continue;
        };
        let slugs = current.scope.companies();
        let selected = if slugs.is_empty() {
            include_workspace
        } else {
            slugs.iter().all(|s| companies.iter().any(|c| c == s))
        };
        if !selected {
            continue;
        }
        authorize_scope(&current.scope)?;
        let start = history.iter().map(|s| s.start_date.as_str()).min().unwrap();
        if start > as_of {
            continue;
        }
        let projections = domain::project_series(
            std::slice::from_ref(&history),
            &tasks,
            &ProjectionQuery {
                from: start.into(),
                to: as_of.into(),
                company_slugs: Some(companies.to_vec()),
                include_workspace,
            },
        )?;
        for projection in projections {
            if projection.work_date.as_str() > as_of {
                continue;
            }
            if let Some(id) = &projection.concrete_task_id {
                let current =
                    read(tx, id)?.ok_or_else(|| Error::new("not_found", "Task unavailable"))?;
                authorize_task(&current)?;
                result.materialized.existing += 1;
                result.materialized.task_ids.push(id.clone());
                continue;
            }
            let effective = history
                .iter()
                .rev()
                .find(|s| s.version == 1 || s.updated_at[..10] <= projection.work_date[..])
                .ok_or_else(|| invalid("No effective series template"))?;
            authorize_scope(&effective.scope)?;
            let input = domain::materialization_draft(effective, &projection)?;
            let key = format!("materialize:{}", projection.projection_id);
            let out = super::mutate_in_tx_checked(
                tx,
                Command::Create {
                    input: &input,
                    generated_id: &projection.projection_id,
                },
                Context {
                    actor: ctx.actor,
                    principal: ctx.principal,
                    idempotency_key: &key,
                    expected_version: None,
                    now: ctx.now,
                    source_check: None,
                },
                |_, t| authorize_task(t),
                |_, t| proposed(t),
            )?;
            result.materialized.created += 1;
            result.materialized.task_ids.push(out.task.task_id.clone());
            tasks.push(out.task);
        }
    }
    append_receipt(tx, ctx, "series-materialize", payload, &json!(result))?;
    Ok(result)
}
/// Metadata only; present resource and historical result scopes remain independent checks.
pub fn receipt<A, T>(
    db: &Connection,
    principal: &str,
    key: &str,
    mut authorize_scope: A,
    mut authorize_task: T,
) -> Result<Option<Value>, Error>
where
    A: FnMut(&Scope) -> Result<(), Error>,
    T: FnMut(&Task) -> Result<(), Error>,
{
    tasks::id(&json!(key))?;
    let row:Option<(String,String)>=db.query_row("SELECT operation,result_json FROM rm_task_receipts WHERE principal=?1 AND idempotency_key=?2",params![principal,key],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(invalid)?;
    let Some((operation, raw)) = row else {
        return Ok(None);
    };
    match operation.as_str() {
        "series-save" => {
            let s: Series = serde_json::from_str(&raw).map_err(invalid)?;
            authorize_scope(&s.scope)?;
            authorize_scope(
                &get(db, &s.series_id)?
                    .ok_or_else(|| Error::new("not_found", "Series unavailable"))?
                    .scope,
            )?;
            Ok(Some(json!({"operation":operation,"version":s.version})))
        }
        "series-materialize" => {
            let m: Materialization = serde_json::from_str(&raw).map_err(invalid)?;
            authorize_scope(&m.scope)?;
            if m.includes_workspace {
                authorize_scope(&Scope::Workspace {
                    company_slugs: vec![],
                })?;
            }
            for id in m.materialized.task_ids {
                authorize_task(
                    &read(db, &id)?.ok_or_else(|| Error::new("not_found", "Task unavailable"))?,
                )?;
            }
            Ok(Some(json!({"operation":operation})))
        }
        _ => super::receipt(db, principal, key, authorize_task),
    }
}
