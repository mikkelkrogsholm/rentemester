//! Pure recurring task templates and annual-wheel projection. No storage or implicit execution.
use super::{Scope, Task, date, draft, id, input, instant, invalid, refusal};
use crate::Error;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SeriesDraft {
    pub series_id: String,
    pub title: String,
    pub scope: Scope,
    pub template: Value,
    pub cadence: String,
    pub every: u32,
    pub anchor: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub fiscal_year_start_month: u32,
    pub work_day_offset: i32,
    pub deadline_day_offset: Option<i32>,
    pub relevance: String,
    pub active: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Series {
    pub series_id: String,
    pub title: String,
    pub scope: Scope,
    pub template: Value,
    pub cadence: String,
    pub every: u32,
    pub anchor: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub fiscal_year_start_month: u32,
    pub work_day_offset: i32,
    pub deadline_day_offset: Option<i32>,
    pub relevance: String,
    pub active: bool,
    pub version: u64,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Period {
    pub from: String,
    pub to: String,
    pub label: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Deadline {
    pub date: String,
    pub kind: String,
    pub basis: String,
    pub certainty: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Projection {
    pub projection_id: String,
    pub series_id: String,
    pub title: String,
    pub scope: Scope,
    pub period: Period,
    pub work_date: String,
    pub deadline: Option<Deadline>,
    pub relevance: String,
    pub concrete_task_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectionQuery {
    pub from: String,
    pub to: String,
    pub company_slugs: Option<Vec<String>>,
    pub include_workspace: bool,
}
const FIELDS: &[&str] = &[
    "seriesId",
    "title",
    "scope",
    "template",
    "cadence",
    "every",
    "anchor",
    "startDate",
    "endDate",
    "fiscalYearStartMonth",
    "workDayOffset",
    "deadlineDayOffset",
    "relevance",
    "active",
];

pub fn validate_series(value: &Value) -> Result<SeriesDraft, Error> {
    let object = input::object(value, FIELDS)?;
    if FIELDS.iter().any(|key| !object.contains_key(*key)) {
        return Err(invalid(
            "Series requires explicit periods, cadence, title and scope",
        ));
    }
    let result: SeriesDraft =
        serde_json::from_value(value.clone()).map_err(|_| invalid("Invalid series draft"))?;
    id(&json!(result.series_id))?;
    input::content(&json!(result.title), "series title", 240, true)?;
    super::scope(&value["scope"])?;
    input::choice(&value["cadence"], &["month", "quarter", "year", "custom"])?;
    input::choice(&value["anchor"], &["calendar", "fiscal"])?;
    input::choice(&value["relevance"], &["relevant", "unknown", "activity"])?;
    date(&value["startDate"])?;
    if let Some(end) = &result.end_date {
        date(&json!(end))?;
        if end < &result.start_date {
            return Err(invalid("Series ends before its start"));
        }
    }
    if !(1..=120).contains(&result.every)
        || !(1..=12).contains(&result.fiscal_year_start_month)
        || result.work_day_offset.unsigned_abs() > 3660
        || result
            .deadline_day_offset
            .is_some_and(|n| n.unsigned_abs() > 3660)
    {
        return Err(invalid("Invalid series interval or date offset"));
    }
    draft(&result.template)?;
    if super::canonical_json(&value["scope"]) != super::canonical_json(&result.template["scope"]) {
        return Err(invalid("Series and template scopes must match"));
    }
    for key in ["taskId", "source", "seriesId", "occurrenceKey"] {
        if result.template.get(key).is_some_and(|v| !v.is_null()) {
            return Err(invalid(
                "Series templates cannot reuse a concrete task or source identity",
            ));
        }
    }
    if result.template["status"] == "done"
        || result.template["reminders"]
            .as_array()
            .is_some_and(|a| a.iter().any(|r| r["enabled"] == true))
    {
        return Err(invalid(
            "Series templates cannot complete tasks or enable reminders automatically",
        ));
    }
    Ok(result)
}

/// An append-only new template; existing occurrences are deliberately not inputs or outputs.
pub fn save_series(
    previous: Option<&Series>,
    input: &Value,
    expected: Option<u64>,
    now: &str,
) -> Result<Series, Error> {
    let value = validate_series(input)?;
    if previous.map_or(expected.is_some_and(|n| n != 0), |p| {
        expected != Some(p.version) || p.version == 0
    }) {
        return Err(refusal(
            "version_conflict",
            "Series changed; reload before editing",
        ));
    }
    if previous.is_some_and(|p| p.series_id != value.series_id || p.scope != value.scope) {
        return Err(invalid(
            "An existing series cannot change identity or scope",
        ));
    }
    let version = previous
        .map_or(0, |p| p.version)
        .checked_add(1)
        .filter(|n| *n <= 9_007_199_254_740_991)
        .ok_or_else(|| invalid("Series version exceeds supported range"))?;
    let mut saved = serde_json::to_value(value).map_err(|_| invalid("Invalid series"))?;
    saved["title"] = json!(input::content(&input["title"], "series title", 240, true)?);
    saved["version"] = json!(version);
    saved["updatedAt"] = json!(instant(now)?.0);
    serde_json::from_value(saved).map_err(|_| invalid("Invalid series"))
}

fn selected(scope: &Scope, query: &ProjectionQuery) -> bool {
    if matches!(scope, Scope::Workspace { company_slugs } if company_slugs.is_empty()) {
        return query.include_workspace;
    }
    query.company_slugs.as_ref().is_none_or(|slugs| {
        let companies = scope.companies();
        !companies.is_empty() && companies.iter().all(|s| slugs.iter().any(|v| v == s))
    })
}
fn month_number(value: &str) -> Result<i32, Error> {
    date(&json!(value))?;
    Ok(value[..4]
        .parse::<i32>()
        .map_err(|_| invalid("Invalid year"))?
        * 12
        + value[5..7]
            .parse::<i32>()
            .map_err(|_| invalid("Invalid month"))?
        - 1)
}
fn first_day(month: i32) -> Result<time::Date, Error> {
    let year = month.div_euclid(12);
    if !(0..=9999).contains(&year) {
        return Err(invalid("Projection is outside supported calendar range"));
    }
    time::Date::from_calendar_date(
        year,
        time::Month::try_from((month.rem_euclid(12) + 1) as u8)
            .map_err(|_| invalid("Invalid month"))?,
        1,
    )
    .map_err(|_| invalid("Invalid period"))
}
fn iso(day: time::Date) -> Result<String, Error> {
    if !(0..=9999).contains(&day.year()) {
        return Err(invalid("Projection is outside supported calendar range"));
    }
    Ok(format!(
        "{:04}-{:02}-{:02}",
        day.year(),
        day.month() as u8,
        day.day()
    ))
}
fn add_days(day: time::Date, days: i32) -> Result<String, Error> {
    iso(day
        .checked_add(time::Duration::days(i64::from(days)))
        .ok_or_else(|| invalid("Date offset overflow"))?)
}
fn span(series: &Series) -> Result<i32, Error> {
    let unit = match series.cadence.as_str() {
        "month" | "custom" => 1,
        "quarter" => 3,
        "year" => 12,
        _ => return Err(invalid("Invalid cadence")),
    };
    if !(1..=120).contains(&series.every) {
        return Err(invalid("Invalid series interval"));
    }
    Ok(unit * series.every as i32)
}
fn anchor(series: &Series, months: i32) -> Result<i32, Error> {
    let start = month_number(&series.start_date)?;
    let fiscal = match series.anchor.as_str() {
        "calendar" => 0,
        "fiscal" if (1..=12).contains(&series.fiscal_year_start_month) => {
            series.fiscal_year_start_month as i32 - 1
        }
        _ => return Err(invalid("Invalid series anchor")),
    };
    let base = (start - fiscal).div_euclid(12) * 12 + fiscal;
    Ok(base + (start - base).div_euclid(months) * months)
}
/// Identical to the original SHA-256 period identity; completion dates never enter it.
pub fn occurrence_id(series_id: &str, period: &Period) -> String {
    let hash = crate::sha256_bytes(format!("{series_id}:{}:{}", period.from, period.to).as_bytes());
    format!("occurrence-{}", &hash[..32])
}
fn project(series: &Series, month: i32, tasks: &[Task]) -> Result<Projection, Error> {
    let start = first_day(month)?;
    let from = iso(start)?;
    let to = add_days(first_day(month + span(series)?)?, -1)?;
    let period = Period {
        label: format!("{from} – {to}"),
        from,
        to,
    };
    let projection_id = occurrence_id(&series.series_id, &period);
    let concrete = tasks.iter().find(|t| t.task_id == projection_id);
    let deadline = series
        .deadline_day_offset
        .map(|offset| {
            let template = &series.template["deadline"];
            Ok::<_, Error>(Deadline {
                date: add_days(start, offset)?,
                kind: template["kind"].as_str().unwrap_or("internal").into(),
                basis: template["basis"]
                    .as_str()
                    .unwrap_or("Den valgte rutines frist")
                    .into(),
                certainty: template["certainty"]
                    .as_str()
                    .unwrap_or("unconfirmed")
                    .into(),
                rule_id: template["ruleId"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(String::from),
            })
        })
        .transpose()?;
    Ok(Projection {
        projection_id,
        series_id: series.series_id.clone(),
        title: concrete.map_or_else(|| series.title.clone(), |t| t.title.clone()),
        scope: concrete.map_or_else(|| series.scope.clone(), |t| t.scope.clone()),
        period: concrete
            .filter(|t| !t.period.is_null())
            .map(|t| {
                serde_json::from_value(t.period.clone())
                    .map_err(|_| invalid("Invalid concrete period"))
            })
            .transpose()?
            .unwrap_or(period),
        work_date: concrete
            .and_then(|t| t.work_date.clone())
            .map(Ok)
            .unwrap_or_else(|| add_days(start, series.work_day_offset))?,
        deadline: concrete
            .filter(|t| !t.deadline.is_null())
            .map(|t| {
                serde_json::from_value(t.deadline.clone())
                    .map_err(|_| invalid("Invalid concrete deadline"))
            })
            .transpose()?
            .or(deadline),
        relevance: series.relevance.clone(),
        concrete_task_id: concrete.map(|t| t.task_id.clone()),
    })
}

/// Read-only historical calendar including concrete periods after cadence edits or pauses.
pub fn project_series(
    histories: &[Vec<Series>],
    tasks: &[Task],
    query: &ProjectionQuery,
) -> Result<Vec<Projection>, Error> {
    let from = month_number(&query.from)?;
    let to = month_number(&query.to)?;
    if query.from > query.to || to - from > 1200 {
        return Err(invalid(
            "Projection requires an ordered interval of at most 100 years",
        ));
    }
    let mut projections = BTreeMap::new();
    let mut currents = BTreeMap::new();
    for history in histories {
        let mut versions = history.iter().collect::<Vec<_>>();
        versions.sort_by_key(|s| s.version);
        let Some(current) = versions.last() else {
            continue;
        };
        currents.insert(current.series_id.as_str(), *current);
        if !selected(&current.scope, query) {
            continue;
        }
        for (index, series) in versions.iter().enumerate() {
            let effective_from = if index == 0 {
                series.start_date.clone()
            } else {
                instant(&series.updated_at)?.0[..10].into()
            };
            let next_from = versions
                .get(index + 1)
                .map(|s| instant(&s.updated_at).map(|v| v.0[..10].to_string()))
                .transpose()?;
            let months = span(series)?;
            let anchor = anchor(series, months)?;
            let mut month = anchor.max(anchor + (from - anchor).div_euclid(months) * months);
            while month <= to {
                let projection = project(series, month, tasks)?;
                if projection.period.to >= series.start_date
                    && projection.period.to >= query.from
                    && projection.period.from <= query.to
                {
                    let scheduled = series.active
                        && projection.work_date >= series.start_date
                        && projection.work_date >= effective_from
                        && next_from
                            .as_ref()
                            .is_none_or(|next| projection.work_date < *next)
                        && series
                            .end_date
                            .as_ref()
                            .is_none_or(|end| projection.period.from <= *end);
                    if scheduled {
                        projections.insert(projection.projection_id.clone(), projection);
                    }
                }
                month += months;
            }
        }
    }
    // Concrete task scope is independently selected; old periods survive all template changes.
    for task in tasks {
        let Some(series_id) = &task.series_id else {
            continue;
        };
        if task.period.is_null()
            || !currents.contains_key(series_id.as_str())
            || !selected(&task.scope, query)
        {
            continue;
        }
        let period: Period = serde_json::from_value(task.period.clone())
            .map_err(|_| invalid("Invalid concrete period"))?;
        if period.to < query.from || period.from > query.to {
            continue;
        }
        projections.insert(
            task.task_id.clone(),
            Projection {
                projection_id: task.task_id.clone(),
                series_id: series_id.clone(),
                title: task.title.clone(),
                scope: task.scope.clone(),
                work_date: task
                    .work_date
                    .clone()
                    .unwrap_or_else(|| period.from.clone()),
                period,
                deadline: serde_json::from_value(task.deadline.clone())
                    .map_err(|_| invalid("Invalid concrete deadline"))?,
                relevance: if task.relevance == "unknown" {
                    "unknown"
                } else {
                    "relevant"
                }
                .into(),
                concrete_task_id: Some(task.task_id.clone()),
            },
        );
    }
    let mut values = projections.into_values().collect::<Vec<_>>();
    values.sort_by(|a, b| {
        a.period
            .from
            .cmp(&b.period.from)
            .then(a.projection_id.cmp(&b.projection_id))
    });
    Ok(values)
}

/// Builds a draft only. The application chooses the effective template and explicitly admits writes.
pub fn materialization_draft(series: &Series, projection: &Projection) -> Result<Value, Error> {
    if projection.series_id != series.series_id
        || projection.scope != series.scope
        || projection.projection_id != occurrence_id(&series.series_id, &projection.period)
        || projection.concrete_task_id.is_some()
    {
        return Err(invalid(
            "Projection does not identify a new occurrence in the series scope",
        ));
    }
    let mut value = series.template.clone();
    value["taskId"] = json!(projection.projection_id);
    value["title"] = json!(series.title);
    value["scope"] = json!(series.scope);
    value["type"] = json!("routine");
    value["seriesId"] = json!(series.series_id);
    value["occurrenceKey"] = json!(format!(
        "{}:{}",
        projection.period.from, projection.period.to
    ));
    value["period"] = json!(projection.period);
    value["workDate"] = json!(projection.work_date);
    value["deadline"] = json!(projection.deadline);
    value["relevance"] = json!(if series.relevance == "relevant" {
        "relevant"
    } else {
        "unknown"
    });
    value["reminders"] = json!([]);
    draft(&value)
}

#[cfg(test)]
mod tests;
