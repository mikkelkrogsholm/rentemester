//! Pure board consequences. Preview never grants authority or completes/reopens a task.
use super::*;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
mod tests;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Draft {
    pub board_id: String,
    pub scope: Scope,
    pub columns: Vec<Column>,
    #[serde(default)]
    pub relocations: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_hash: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatusChange {
    pub task_id: String,
    pub from: String,
    pub to: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Preview {
    pub preview_hash: String,
    pub affected_task_ids: Vec<String>,
    pub status_changes: Vec<StatusChange>,
}
pub struct Change {
    pub task: Task,
    pub column_id: String,
    pub from: String,
    pub to: String,
}
pub struct Changes {
    pub draft: Draft,
    pub prior: Board,
    pub structural: bool,
    pub meaning_changed: bool,
    pub affected: Vec<Change>,
    pub preview: Preview,
}
pub fn default_board(scope: &Scope) -> Board {
    let scope = match scope {
        Scope::Company { .. } => scope.clone(),
        Scope::Workspace { .. } => Scope::Workspace {
            company_slugs: vec![],
        },
    };
    Board {
        board_id: scope.board_id(),
        version: 0,
        scope,
        updated_at: "1970-01-01T00:00:00.000Z".into(),
        columns: STATUSES
            .iter()
            .zip(["Åben", "I gang", "Afventer", "Færdig"])
            .map(|(s, name)| Column {
                column_id: (*s).into(),
                name: name.into(),
                status: (*s).into(),
                is_default: true,
            })
            .collect(),
    }
}
pub fn validate(input: &Value) -> Result<Draft, Error> {
    let mut d: Draft =
        serde_json::from_value(input.clone()).map_err(|_| invalid("Invalid board"))?;
    d.scope = scope(&json!(d.scope))?;
    if d.board_id != d.scope.board_id()
        || matches!(&d.scope,Scope::Workspace{company_slugs}if !company_slugs.is_empty())
        || !(4..=40).contains(&d.columns.len())
    {
        return Err(invalid(
            "Board must match its scope and contain4to40columns",
        ));
    }
    let mut ids = BTreeSet::new();
    for c in &mut d.columns {
        c.column_id = id(&json!(c.column_id))?;
        c.name = text(&json!(c.name), "column name", 160, true)?;
        draft(&json!({"title":c.name,"scope":d.scope}))?;
        if !ids.insert(c.column_id.clone()) || !STATUSES.contains(&c.status.as_str()) {
            return Err(invalid("Invalid or duplicate board column"));
        }
    }
    for status in STATUSES {
        if d.columns
            .iter()
            .filter(|c| c.status == *status && c.is_default)
            .count()
            != 1
        {
            return Err(invalid("Each status requires exactly one default column"));
        }
    }
    if d.relocations.len() > 40 {
        return Err(invalid("Relocation bound"));
    }
    for (from, to) in &d.relocations {
        id(&json!(from))?;
        id(&json!(to))?;
        if !ids.contains(to) {
            return Err(invalid("Relocation target is not a board column"));
        }
    }
    Ok(d)
}
pub fn column(task: &Task, board: &Board) -> Result<String, Error> {
    let selected = match board.scope {
        Scope::Company { .. } => Some(task.column_id.as_str()),
        Scope::Workspace { .. } => task.workspace_column_id.as_deref(),
    };
    if let Some(c) = selected.and_then(|id| {
        board
            .columns
            .iter()
            .find(|c| c.column_id == id && c.status == task.status)
    }) {
        return Ok(c.column_id.clone());
    }
    board
        .columns
        .iter()
        .find(|c| c.status == task.status && c.is_default)
        .map(|c| c.column_id.clone())
        .ok_or_else(|| invalid("Board lacks default column"))
}
pub fn changes(input: &Value, prior: Option<&Board>, tasks: &[Task]) -> Result<Changes, Error> {
    let draft = validate(input)?;
    let prior = prior
        .cloned()
        .unwrap_or_else(|| default_board(&draft.scope));
    let structural = prior.columns.iter().any(|c| {
        !draft
            .columns
            .iter()
            .any(|n| n.column_id == c.column_id && n.status == c.status)
    });
    let relevant: Vec<_> = if structural {
        tasks
            .iter()
            .filter(|t| match &draft.scope {
                Scope::Workspace { .. } => true,
                Scope::Company { company_slug } => {
                    matches!(&t.scope,Scope::Company{company_slug:s}if s==company_slug)
                }
            })
            .collect()
    } else {
        vec![]
    };
    let mut affected = vec![];
    let mut facts = vec![];
    for task in relevant {
        let old = column(task, &prior)?;
        let target = if draft.columns.iter().any(|c| c.column_id == old) {
            old.clone()
        } else {
            draft.relocations.get(&old).cloned().ok_or_else(|| {
                refusal(
                    "relocation_required",
                    "Move every populated removed column to a retained column",
                )
            })?
        };
        let next = draft
            .columns
            .iter()
            .find(|c| c.column_id == target)
            .ok_or_else(|| invalid("Missing relocation column"))?;
        facts.push(
            json!({"id":task.task_id,"version":task.version,"columnId":old,"status":task.status}),
        );
        if target != old || next.status != task.status {
            affected.push(Change {
                task: task.clone(),
                column_id: target,
                from: task.status.clone(),
                to: next.status.clone(),
            });
        }
    }
    let preview_hash = payload_hash(
        &json!({"prior":prior,"draft":{"boardId":draft.board_id,"scope":draft.scope,"columns":draft.columns,"relocations":draft.relocations},"tasks":facts}),
    )?;
    let status_changes = affected
        .iter()
        .filter(|c| c.from != c.to)
        .map(|c| StatusChange {
            task_id: c.task.task_id.clone(),
            from: c.from.clone(),
            to: c.to.clone(),
        })
        .collect();
    let preview = Preview {
        preview_hash,
        affected_task_ids: affected.iter().map(|c| c.task.task_id.clone()).collect(),
        status_changes,
    };
    let meaning_changed = prior.columns.iter().any(|c| {
        draft
            .columns
            .iter()
            .any(|n| n.column_id == c.column_id && n.status != c.status)
    });
    Ok(Changes {
        draft,
        prior,
        structural,
        meaning_changed,
        affected,
        preview,
    })
}
pub fn apply(
    changes: &Changes,
    boards: &[Board],
    expected: Option<u64>,
    now: &str,
) -> Result<(Board, Vec<Task>), Error> {
    let now = instant(now)?.0;
    if expected != Some(changes.prior.version) {
        return Err(refusal(
            "version_conflict",
            "Board changed; reload before saving",
        ));
    }
    if (changes.meaning_changed || !changes.preview.status_changes.is_empty())
        && changes.draft.preview_hash.as_deref() != Some(changes.preview.preview_hash.as_str())
    {
        return Err(refusal(
            "preview_required",
            "Review current consequences before changing column meaning",
        ));
    }
    let mut tasks = vec![];
    for change in &changes.affected {
        if change.to == "done" && change.from != "done" {
            return Err(refusal(
                "completion_required",
                "Board changes cannot bypass documented completion",
            ));
        }
        if change.from == "done" && change.to != "done" {
            return Err(refusal(
                "reopen_required",
                "Reopen completed tasks before changing column meaning",
            ));
        }
        let mut t = change.task.clone();
        t.status = change.to.clone();
        t.version = t
            .version
            .checked_add(1)
            .filter(|v| *v <= 9_007_199_254_740_991)
            .ok_or_else(|| invalid("Task version bound"))?;
        t.updated_at = now.clone();
        let company = matches!(changes.draft.scope, Scope::Company { .. });
        if company {
            t.column_id = change.column_id.clone();
        } else {
            t.workspace_column_id = Some(change.column_id.clone());
        }
        if change.from != change.to && (!company || t.workspace_column_id.is_some()) {
            let scope = if company {
                Scope::Workspace {
                    company_slugs: vec![],
                }
            } else {
                t.scope.clone()
            };
            let fallback = default_board(&scope);
            let related = boards
                .iter()
                .find(|b| b.board_id == scope.board_id())
                .unwrap_or(&fallback);
            let candidates: Vec<_> = related
                .columns
                .iter()
                .filter(|c| c.status == t.status)
                .collect();
            if !company && candidates.len() > 1 {
                return Err(refusal(
                    "column_choice_required",
                    "Choose the company target column before changing board meaning",
                ));
            }
            let col = candidates
                .iter()
                .find(|c| c.is_default)
                .ok_or_else(|| invalid("Related board lacks default column"))?
                .column_id
                .clone();
            if company {
                t.workspace_column_id = Some(col);
            } else {
                t.column_id = col;
            }
        }
        tasks.push(t);
    }
    let version = changes
        .prior
        .version
        .checked_add(1)
        .filter(|v| *v <= 9_007_199_254_740_991)
        .ok_or_else(|| invalid("Board version bound"))?;
    Ok((
        Board {
            board_id: changes.draft.board_id.clone(),
            version,
            scope: changes.draft.scope.clone(),
            updated_at: now,
            columns: changes.draft.columns.clone(),
        },
        tasks,
    ))
}
