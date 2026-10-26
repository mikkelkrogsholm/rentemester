//! Pure task-card policy shared by native transports. No database, clock or authority lookup.
//! Callers must hold current scope authority across receipt lookup, transition and append.
pub mod boards;
mod input;
pub mod reminders;
pub mod series;
pub mod sources;
mod transition;
use crate::Error;
pub use input::{canonical_json, date, draft, id, instant, payload_hash, references, scope, text};
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub use transition::{complete, create, move_task, prepare_create, reopen, update};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Scope {
    Company {
        #[serde(rename = "companySlug")]
        company_slug: String,
    },
    Workspace {
        #[serde(rename = "companySlugs")]
        company_slugs: Vec<String>,
    },
}
impl Scope {
    pub fn companies(&self) -> Vec<&str> {
        match self {
            Self::Company { company_slug } => vec![company_slug],
            Self::Workspace { company_slugs } => company_slugs.iter().map(String::as_str).collect(),
        }
    }
    pub fn board_id(&self) -> String {
        match self {
            Self::Company { company_slug } => format!("company:{company_slug}"),
            Self::Workspace { .. } => "workspace".into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reference {
    pub kind: String,
    #[serde(rename = "ref")]
    pub reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_slug: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub kind: String,
    pub identity: String,
    pub company_slug: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub state: String,
    pub content_hash: String,
    pub href: String,
}
/// Only an authoritative native source reader supplies this, never a task HTTP body.
#[derive(Debug, Clone)]
pub struct SourceCheck {
    pub state: String,
    pub observed_at: String,
    pub evidence: Vec<Reference>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Completion {
    pub outcome: String,
    pub note: String,
    pub references: Vec<Reference>,
    pub assurance: String,
    pub at: String,
    pub actor: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Task {
    pub task_id: String,
    pub version: u64,
    pub title: String,
    pub description: String,
    pub next_action: String,
    pub scope: Scope,
    #[serde(rename = "type")]
    pub task_type: String,
    pub origin: String,
    pub status: String,
    pub column_id: String,
    pub workspace_column_id: Option<String>,
    pub assignee: Value,
    pub waiting_on: String,
    pub work_date: Option<String>,
    pub deadline: Value,
    pub period: Value,
    pub references: Vec<Reference>,
    pub source: Option<Source>,
    pub evidence_required: bool,
    pub relevance: String,
    pub verification_required: bool,
    pub completion: Option<Completion>,
    pub series_id: Option<String>,
    pub occurrence_key: Option<String>,
    pub reminders: Vec<Value>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Column {
    pub column_id: String,
    pub name: String,
    pub status: String,
    pub is_default: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Board {
    pub board_id: String,
    pub version: u64,
    pub scope: Scope,
    pub updated_at: String,
    pub columns: Vec<Column>,
}
pub const STATUSES: &[&str] = &["open", "in_progress", "waiting", "done"];
/// Validates labels from an already authenticated application context; grants no authority.
pub fn mutation_identity(actor: &str, principal: &str, key: &str) -> Result<(), Error> {
    transition::actor(actor)?;
    text(&serde_json::json!(principal), "principal", 160, true)?;
    id(&serde_json::json!(key))?;
    Ok(())
}
fn invalid(message: impl Into<String>) -> Error {
    Error::new("invalid_input", message)
}
fn refusal(code: &'static str, message: &str) -> Error {
    Error::new(code, message)
}
fn placement(
    boards: &[Board],
    scope: &Scope,
    status: &str,
    requested: Option<&str>,
    ambiguous: bool,
) -> Result<String, Error> {
    let board = boards.iter().find(|b| b.board_id == scope.board_id());
    let Some(board) = board else {
        if requested.is_some_and(|r| r != status) {
            return Err(refusal(
                "column_invalid",
                "Selected column does not match task status",
            ));
        }
        return Ok(status.into());
    };
    let columns: Vec<_> = board
        .columns
        .iter()
        .filter(|c| c.status == status)
        .collect();
    if let Some(requested) = requested {
        return columns
            .iter()
            .find(|c| c.column_id == requested)
            .map(|c| c.column_id.clone())
            .ok_or_else(|| {
                refusal(
                    "column_invalid",
                    "Selected column does not match task status",
                )
            });
    }
    if ambiguous && columns.len() > 1 {
        return Err(refusal(
            "column_choice_required",
            "Choose the company's target column",
        ));
    }
    columns
        .iter()
        .find(|c| c.is_default)
        .map(|c| c.column_id.clone())
        .ok_or_else(|| {
            refusal(
                "column_invalid",
                "Board lacks a default column for this status",
            )
        })
}
fn existing(task: &Task, expected_version: Option<u64>) -> Result<(), Error> {
    if expected_version.is_none_or(|n| n == 0) {
        return Err(refusal("version_required", "Expected version is required"));
    }
    if expected_version != Some(task.version) {
        return Err(refusal(
            "version_conflict",
            "Task changed; reload before applying edits",
        ));
    }
    Ok(())
}
fn changed(mut task: Task, now: &str) -> Result<Task, Error> {
    task.version = task
        .version
        .checked_add(1)
        .filter(|n| *n <= 9_007_199_254_740_991)
        .ok_or_else(|| invalid("Task version exceeds supported range"))?;
    task.updated_at = instant(now)?.0;
    Ok(task)
}

#[cfg(test)]
mod tests;
