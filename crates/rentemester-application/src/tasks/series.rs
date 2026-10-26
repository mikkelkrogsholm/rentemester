//! Current routine scope admission and metadata. Historical labels never confer access.
use super::*;
use rentemester_engine::tasks::series::{self as engine, ProjectionQuery, Series};
fn fields(value: &Value, allowed: &[&str]) -> Result<(), Error> {
    if value
        .as_object()
        .is_none_or(|m| m.keys().any(|k| !allowed.contains(&k.as_str())))
    {
        return Err(error("invalid_input"));
    }
    Ok(())
}
pub(super) fn manage(access: &Access, scope: &Scope) -> Result<(), Error> {
    let slugs = scope.companies();
    if if slugs.is_empty() {
        access.owner
    } else {
        slugs.iter().all(|s| access.manage.contains(*s))
    } {
        Ok(())
    } else {
        Err(denied())
    }
}
fn selection(input: &Value, access: &Access, manage: bool) -> Result<Vec<String>, Error> {
    let allowed = if manage { &access.manage } else { &access.read };
    let Some(value) = input.get("companySlugs") else {
        return Ok(allowed.iter().cloned().collect());
    };
    let values = value
        .as_array()
        .filter(|a| a.len() <= 100)
        .ok_or_else(denied)?;
    let mut selected = vec![];
    for v in values {
        let s = v
            .as_str()
            .filter(|s| allowed.contains(*s))
            .ok_or_else(denied)?;
        if !selected.iter().any(|v| v == s) {
            selected.push(s.to_string());
        }
    }
    Ok(selected)
}
fn public_series(value: &Value) -> Result<Value, Error> {
    let _ = engine::validate_series(value)?;
    if value["workDayOffset"]
        .as_i64()
        .is_none_or(|n| n.unsigned_abs() > 366)
        || !value["deadlineDayOffset"].is_null()
            && value["deadlineDayOffset"]
                .as_i64()
                .is_none_or(|n| n.unsigned_abs() > 366)
    {
        return Err(error("invalid_input"));
    }
    let mut out = value.clone();
    out["scope"] = json!(domain::scope(&value["scope"])?);
    out["template"] = manual_draft(&value["template"])?;
    out["template"]["scope"] = out["scope"].clone();
    engine::validate_series(&out)?;
    Ok(out)
}
pub(super) fn preadmit(
    tx: &Transaction<'_>,
    action: &Action<'_>,
    access: &Access,
    expected: Option<u64>,
) -> Result<(), Error> {
    match action {
        Action::SeriesSave(v) => {
            if expected.is_none_or(|n| n > 9_007_199_254_740_991) {
                return Err(error("version_required"));
            }
            let input = public_series(v)?;
            let requested = engine::validate_series(&input)?;
            manage(access, &requested.scope)?;
            if let Some(current) = store::series::get(tx, &requested.series_id)? {
                manage(access, &current.scope)?;
            }
        }
        Action::SeriesMaterialize(v) => {
            fields(v, &["companySlugs", "asOfDate"])?;
            if expected.is_some() {
                return Err(error("invalid_input"));
            }
            let selected = selection(v, access, true)?;
            if selected.is_empty() && !access.owner {
                return Err(denied());
            }
            if let Some(date) = v.get("asOfDate") {
                domain::date(date)?;
            }
            // All explicit companies were admitted even when they currently contain no routine.
        }
        _ => {}
    }
    Ok(())
}
pub(super) fn mutate(
    tx: &Transaction<'_>,
    action: &Action<'_>,
    ctx: &store::Context<'_>,
    catalog: &Catalog,
    access: &Access,
) -> Result<Value, Error> {
    match action {
        Action::SeriesSave(v) => {
            let input = public_series(v)?;
            let result = store::series::save_in_tx(
                tx,
                &input,
                ctx,
                |scope| manage(access, scope),
                |s| {
                    let task = domain::prepare_create(&s.template, "template-recipient", ctx.now)?;
                    recipients(tx, catalog, &task)
                },
            )?;
            Ok(json!({"series":result}))
        }
        Action::SeriesMaterialize(v) => {
            let selected = selection(v, access, true)?;
            let include_workspace = access.owner && v.get("companySlugs").is_none();
            let as_of = v["asOfDate"].as_str().unwrap_or(&ctx.now[..10]);
            let result = store::series::materialize_in_tx(
                tx,
                v,
                &selected,
                include_workspace,
                as_of,
                ctx,
                |scope| manage(access, scope),
                |task| authorize_task(tx, catalog, access, task, true),
                |task| recipients(tx, catalog, task),
            )?;
            Ok(json!(result))
        }
        _ => Err(error("invalid_input")),
    }
}
pub(super) fn metadata(
    tx: &Transaction<'_>,
    input: &Value,
    catalog: &Catalog,
    access: &Access,
    now: &str,
) -> Result<(Vec<Series>, Value), Error> {
    let selected = selection(input, access, false)?;
    let histories = store::series::histories(tx)?;
    let visible = |scope: &Scope| {
        scope_allowed(access, scope, false)
            && (input.get("companySlugs").is_none()
                || scope
                    .companies()
                    .iter()
                    .any(|s| selected.iter().any(|c| c == s)))
            && (input["includeArchived"] == true
                || !catalog
                    .companies
                    .iter()
                    .any(|c| c.archived && scope.companies().contains(&c.slug.as_str())))
    };
    let series = histories
        .iter()
        .filter_map(|h| h.last())
        .filter(|s| visible(&s.scope))
        .cloned()
        .collect();
    // A single calendar bound must not turn an accepted backlog view into a reversed interval.
    let year = &input["from"]
        .as_str()
        .or(input["to"].as_str())
        .unwrap_or(now)[..4];
    let from = input["from"]
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| format!("{year}-01-01"));
    let to = input["to"]
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| format!("{year}-12-31"));
    // Include all concrete tasks for stable identities; independently filter actual returned scopes.
    let tasks = store::list(tx, |_| Ok(true))?;
    let projections = engine::project_series(
        &histories,
        &tasks,
        &ProjectionQuery {
            from,
            to,
            company_slugs: Some(selected.clone()),
            include_workspace: access.owner,
        },
    )?
    .into_iter()
    .filter(|p| visible(&p.scope))
    .collect::<Vec<_>>();
    Ok((series, json!(projections)))
}
pub(super) fn read(
    tx: &Transaction<'_>,
    action: &Action<'_>,
    catalog: &Catalog,
    access: &Access,
    now: &str,
) -> Result<Value, Error> {
    let (input, project) = match action {
        Action::SeriesList(v) => (*v, false),
        Action::SeriesProject(v) => (*v, true),
        _ => return Err(error("invalid_input")),
    };
    fields(
        input,
        if project {
            &["companySlugs", "includeArchived", "from", "to"]
        } else {
            &["companySlugs", "includeArchived"]
        },
    )?;
    if input
        .get("includeArchived")
        .is_some_and(|v| !v.is_boolean())
        || project && (input.get("from").is_none() || input.get("to").is_none())
    {
        return Err(error("invalid_input"));
    }
    for key in ["from", "to"] {
        if let Some(value) = input.get(key) {
            domain::date(value)?;
        }
    }
    let (series, projections) = metadata(tx, input, catalog, access, now)?;
    Ok(if project {
        json!({"projections":projections})
    } else {
        json!({"series":series})
    })
}
