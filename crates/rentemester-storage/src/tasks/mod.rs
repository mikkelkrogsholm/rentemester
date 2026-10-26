//! Workspace task streams and receipts. The caller supplies current authority, never JSON grants.
//! These primitives require the task schema. They do not migrate/adopt a workspace or enable routes.
use crate::invalid;
use rentemester_engine::{
    Error,
    tasks::{self, Board, SourceCheck, Task},
};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Serialize;
use serde_json::{Value, json};

pub enum Command<'a> {
    Create {
        input: &'a Value,
        generated_id: &'a str,
    },
    Update {
        id: &'a str,
        patch: &'a Value,
    },
    Move {
        id: &'a str,
        input: &'a Value,
    },
    Complete {
        id: &'a str,
        input: &'a Value,
    },
    Reopen {
        id: &'a str,
        reason: &'a Value,
    },
}
impl Command<'_> {
    fn operation(&self) -> &'static str {
        match self {
            Self::Create { .. } => "task-create",
            Self::Update { .. } => "task-update",
            Self::Move { .. } => "task-move",
            Self::Complete { .. } => "task-complete",
            Self::Reopen { .. } => "task-reopen",
        }
    }
    fn event(&self) -> &'static str {
        match self {
            Self::Create { .. } => "created",
            Self::Update { .. } => "updated",
            Self::Move { .. } => "moved",
            Self::Complete { .. } => "completed",
            Self::Reopen { .. } => "reopened",
        }
    }
    fn payload(&self) -> Value {
        match self {
            Self::Create { input, .. } => (*input).clone(),
            Self::Update { id, patch } => json!({"id":id,"patch":patch}),
            Self::Move { id, input } | Self::Complete { id, input } => {
                json!({"id":id,"input":input})
            }
            Self::Reopen { id, reason } => json!({"id":id,"reason":reason}),
        }
    }
    fn id(&self) -> Option<&str> {
        match self {
            Self::Create { .. } => None,
            Self::Update { id, .. }
            | Self::Move { id, .. }
            | Self::Complete { id, .. }
            | Self::Reopen { id, .. } => Some(id),
        }
    }
}
/// Constructed by the verified application session/service boundary, never deserialized from a body.
pub struct Context<'a> {
    pub actor: &'a str,
    pub principal: &'a str,
    pub idempotency_key: &'a str,
    pub expected_version: Option<u64>,
    pub now: &'a str,
    pub source_check: Option<&'a SourceCheck>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub task_id: String,
    pub version: u64,
    pub operation: String,
    pub actor: String,
    pub principal: String,
    pub at: String,
    pub task: Task,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
#[derive(Debug)]
pub struct Outcome {
    pub task: Task,
    pub replayed: bool,
}
fn read(db: &Connection, id: &str) -> Result<Option<Task>, Error> {
    tasks::id(&json!(id))?;
    let raw: Option<String> = db
        .query_row(
            "SELECT payload_json FROM rm_current_tasks WHERE task_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(invalid)?;
    raw.map(|s| serde_json::from_str(&s).map_err(invalid))
        .transpose()
}
fn boards(db: &Connection) -> Result<Vec<Board>, Error> {
    let mut q = db
        .prepare("SELECT payload_json FROM rm_current_task_boards ORDER BY board_id")
        .map_err(invalid)?;
    q.query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?
        .map(|r| serde_json::from_str(&r.map_err(invalid)?).map_err(invalid))
        .collect()
}
/// One IMMEDIATE control transaction holds live authorization, receipt lookup, effect and receipt.
/// Authorization is mandatory for the current task, proposed/new scope and returned receipt result.
/// The application also validates live assignee/reminder access in this callback.
pub fn mutate<F>(
    db: &mut Connection,
    command: Command<'_>,
    ctx: Context<'_>,
    mut authorize: F,
) -> Result<Outcome, Error>
where
    F: FnMut(&Transaction<'_>, &Task) -> Result<(), Error>,
{
    tasks::mutation_identity(ctx.actor, ctx.principal, ctx.idempotency_key)?;
    let now = tasks::instant(ctx.now)?.0;
    let hash = tasks::payload_hash(
        &json!({"payload":command.payload(),"expectedVersion":ctx.expected_version}),
    )?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(invalid)?;
    let current = command
        .id()
        .map(|id| read(&tx, id)?.ok_or_else(|| Error::new("not_found", "Task is unavailable")))
        .transpose()?;
    // Even create replay rechecks the requested scope before the receipt is examined.
    let candidate = if let Command::Create {
        input,
        generated_id,
    } = &command
    {
        Some(tasks::prepare_create(input, generated_id, &now)?)
    } else {
        None
    };
    authorize(
        &tx,
        current
            .as_ref()
            .or(candidate.as_ref())
            .ok_or_else(|| invalid("Task command has no scope"))?,
    )?;
    let prior:Option<(String,String,String)>=tx.query_row("SELECT operation,payload_hash,result_json FROM rm_task_receipts WHERE principal=?1 AND idempotency_key=?2",params![ctx.principal,ctx.idempotency_key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(invalid)?;
    if let Some((operation, payload, result)) = prior {
        if operation != command.operation() || payload != hash {
            return Err(Error::new(
                "idempotency_conflict",
                "Idempotency key was already used for another request",
            ));
        }
        let task: Task = serde_json::from_str(&result).map_err(invalid)?;
        let live = read(&tx, &task.task_id)?
            .ok_or_else(|| Error::new("not_found", "Task is unavailable"))?;
        authorize(&tx, &live)?;
        authorize(&tx, &task)?;
        tx.commit().map_err(|_| {
            Error::new(
                "commit_outcome_unknown",
                "Task receipt outcome is unknown; retain the original idempotency key",
            )
        })?;
        return Ok(Outcome {
            task,
            replayed: true,
        });
    }
    let boards = boards(&tx)?;
    let task = match &command {
        Command::Create {
            input,
            generated_id,
        } => {
            let task = tasks::create(input, generated_id, &now, &boards)?;
            if read(&tx, &task.task_id)?.is_some() {
                return Err(Error::new("task_exists", "Task reference already exists"));
            }
            task
        }
        Command::Update { patch, .. } => tasks::update(
            current.as_ref().expect("existing task"),
            patch,
            ctx.expected_version,
            &now,
            &boards,
        )?,
        Command::Move { input, .. } => tasks::move_task(
            current.as_ref().expect("existing task"),
            input,
            ctx.expected_version,
            &now,
            &boards,
        )?,
        Command::Complete { input, .. } => tasks::complete(
            current.as_ref().expect("existing task"),
            input,
            ctx.expected_version,
            &now,
            ctx.actor,
            ctx.source_check,
            &boards,
        )?,
        Command::Reopen { reason, .. } => tasks::reopen(
            current.as_ref().expect("existing task"),
            reason,
            ctx.expected_version,
            &now,
            &boards,
        )?,
    };
    authorize(&tx, &task)?;
    let result = tasks::canonical_json(&json!(task));
    if result.encode_utf16().count() > 1_048_576 {
        return Err(Error::new(
            "invalid_input",
            "Task payload must be bounded JSON",
        ));
    }
    let reason = if let Command::Reopen { reason, .. } = command {
        Some(tasks::text(reason, "event reason", 2000, true)?)
    } else {
        None
    };
    let version = i64::try_from(task.version).map_err(invalid)?;
    tx.execute("INSERT INTO rm_task_events(task_id,version,operation,payload_json,actor,principal,created_at,reason) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![task.task_id,version,command.event(),result,ctx.actor,ctx.principal,now,reason]).map_err(invalid)?;
    tx.execute("INSERT INTO rm_task_receipts(principal,idempotency_key,operation,payload_hash,result_json,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![ctx.principal,ctx.idempotency_key,command.operation(),hash,result,now]).map_err(invalid)?;
    tx.commit().map_err(|_| {
        Error::new(
            "commit_outcome_unknown",
            "Task mutation outcome is unknown; retain the original idempotency key",
        )
    })?;
    Ok(Outcome {
        task,
        replayed: false,
    })
}
pub fn get<F>(db: &Connection, id: &str, mut authorize: F) -> Result<Option<Task>, Error>
where
    F: FnMut(&Task) -> Result<(), Error>,
{
    let task = read(db, id)?;
    if let Some(task) = &task {
        authorize(task)?;
    }
    Ok(task)
}
/// Caller has already verified the requested view scope; hidden tasks are never returned.
pub fn list<F>(db: &Connection, mut visible: F) -> Result<Vec<Task>, Error>
where
    F: FnMut(&Task) -> Result<bool, Error>,
{
    let mut q = db
        .prepare("SELECT payload_json FROM rm_current_tasks ORDER BY created_at,task_id")
        .map_err(invalid)?;
    let rows = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?;
    let mut out = vec![];
    for row in rows {
        let task = serde_json::from_str(&row.map_err(invalid)?).map_err(invalid)?;
        if visible(&task)? {
            out.push(task);
        }
    }
    Ok(out)
}
/// Both current and historical scopes must be authorized before returning history.
pub fn history<F>(db: &Connection, id: &str, mut authorize: F) -> Result<Vec<Event>, Error>
where
    F: FnMut(&Task) -> Result<(), Error>,
{
    if let Some(task) = read(db, id)? {
        authorize(&task)?;
    }
    let mut q=db.prepare("SELECT task_id,version,operation,actor,principal,created_at,payload_json,reason FROM rm_task_events WHERE task_id=?1 ORDER BY version").map_err(invalid)?;
    let rows = q
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(invalid)?;
    let mut out = vec![];
    for row in rows {
        let (task_id, version, operation, actor, principal, at, raw, reason) =
            row.map_err(invalid)?;
        let task = serde_json::from_str(&raw).map_err(invalid)?;
        authorize(&task)?;
        out.push(Event {
            task_id,
            version: u64::try_from(version).map_err(invalid)?,
            operation,
            actor,
            principal,
            at,
            task,
            reason,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
