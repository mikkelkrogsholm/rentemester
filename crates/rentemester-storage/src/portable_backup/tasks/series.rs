//! Explicit codec-2 receipt evidence. Requests are not persisted and hashes cannot be reconstructed.
use super::*;
use crate::tasks::series::Materialization;
use rentemester_engine::tasks::series::{self as engine, Period, Series};
pub(super) fn needed(db: &Connection) -> Result<bool, Error> {
    db.query_row("SELECT EXISTS(SELECT 1 FROM rm_task_receipts WHERE operation IN ('series-save','series-materialize'))",[],|r|r.get(0)).map_err(sql)
}
fn snapshot(raw: &str) -> Result<Series, Error> {
    let mut value = decoded(raw)?;
    let series: Series = serde_json::from_value(value.clone()).map_err(sql)?;
    if series.version == 0 || series.version > 9_007_199_254_740_991 {
        return Err(refused("series snapshot version differs"));
    }
    domain::instant(&series.updated_at)?;
    value
        .as_object_mut()
        .ok_or_else(|| refused("series snapshot shape"))?
        .remove("version");
    value.as_object_mut().unwrap().remove("updatedAt");
    engine::validate_series(&value)?;
    Ok(series)
}
pub(super) fn events(db: &Connection) -> Result<(), Error> {
    let mut q=db.prepare("SELECT series_id,version,payload_json,actor,principal,created_at FROM rm_task_series_events ORDER BY series_id,version").map_err(sql)?;
    let mut previous: Option<Series> = None;
    for row in q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })
        .map_err(sql)?
    {
        let (id, version, raw, actor, principal, at) = row.map_err(sql)?;
        domain::mutation_identity(&actor, &principal, "historical-series")?;
        let s = snapshot(&raw)?;
        if s.series_id != id
            || i64::try_from(s.version).map_err(sql)? != version
            || s.updated_at != at
            || previous.as_ref().is_some_and(|p| {
                p.series_id == id && (p.scope != s.scope || p.updated_at > s.updated_at)
            })
        {
            return Err(refused("series event identity/scope/chronology differs"));
        }
        previous = Some(s);
    }
    Ok(())
}
pub(super) fn receipt(
    db: &Connection,
    principal: &str,
    operation: &str,
    raw: &str,
    at: &str,
) -> Result<bool, Error> {
    match operation {
        "series-save" => {
            let s = snapshot(raw)?;
            let matches:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM rm_task_series_events WHERE series_id=?1 AND version=?2 AND principal=?3 AND payload_json=?4 AND created_at=?5)",params![s.series_id,i64::try_from(s.version).map_err(sql)?,principal,raw,at],|r|r.get(0)).map_err(sql)?;
            if !matches {
                return Err(refused(
                    "series receipt lacks exact historical result/principal/event",
                ));
            }
            Ok(true)
        }
        "series-materialize" => {
            let result: Materialization = serde_json::from_str(raw).map_err(sql)?;
            let Scope::Workspace { company_slugs } = &result.scope else {
                return Err(refused("materialization result scope differs"));
            };
            domain::scope(&serde_json::json!(result.scope))?;
            domain::instant(at)?;
            let m = &result.materialized;
            if m.created.checked_add(m.existing) != Some(m.task_ids.len())
                || m.task_ids.len() > 100000
            {
                return Err(refused("materialization counts differ"));
            }
            let mut seen = BTreeSet::new();
            let mut own_created = 0usize;
            for id in &m.task_ids {
                domain::id(&serde_json::json!(id))?;
                if !seen.insert(id) {
                    return Err(refused("duplicate materialized task"));
                }
                let row:Option<(String,String,String)>=db.query_row("SELECT payload_json,principal,created_at FROM rm_task_events WHERE task_id=?1 AND version=1 AND operation='created' AND created_at<=?2",params![id,at],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(sql)?;
                let Some((payload, created_by, created_at)) = row else {
                    return Err(refused("materialized task lacks historical creation"));
                };
                let task = task_snapshot(&payload)?;
                let series_id = task
                    .series_id
                    .as_ref()
                    .ok_or_else(|| refused("materialized task lacks series"))?;
                let period: Period = serde_json::from_value(task.period.clone()).map_err(sql)?;
                if task.task_type != "routine"
                    || engine::occurrence_id(series_id, &period) != *id
                    || task.occurrence_key.as_deref()
                        != Some(format!("{}:{}", period.from, period.to).as_str())
                {
                    return Err(refused("materialized occurrence identity differs"));
                }
                let has_series:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM rm_task_series_events WHERE series_id=?1 AND created_at<=?2)",params![series_id,created_at],|r|r.get(0)).map_err(sql)?;
                let slugs = task.scope.companies();
                if !has_series
                    || if slugs.is_empty() {
                        !result.includes_workspace
                    } else {
                        !slugs.iter().all(|s| company_slugs.iter().any(|c| c == s))
                    }
                {
                    return Err(refused("materialized task exceeds receipt scope"));
                }
                // A newly created occurrence has its own exact principal-bound task-create receipt.
                if created_by == principal && created_at == at {
                    let key = format!("materialize:{id}");
                    let matches:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM rm_task_receipts WHERE principal=?1 AND idempotency_key=?2 AND operation='task-create' AND result_json=?3 AND created_at=?4)",params![principal,key,payload,at],|r|r.get(0)).map_err(sql)?;
                    if !matches {
                        return Err(refused(
                            "materialized creation lacks exact principal receipt",
                        ));
                    }
                    own_created += 1;
                }
            }
            if m.created > own_created {
                return Err(refused("materialized created count lacks event evidence"));
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}
use rentemester_engine::tasks::Scope;
use rusqlite::OptionalExtension;
