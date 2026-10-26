//! Codec3 binds board receipts and relocation evidence; earlier codecs keep their own coverage.
use super::*;
use rentemester_engine::tasks::boards as engine;
pub(super) fn needed(db: &Connection) -> Result<bool, Error> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM rm_task_receipts WHERE operation='task-board-save')",
        [],
        |r| r.get(0),
    )
    .map_err(sql)
}
fn snapshot(raw: &str) -> Result<Board, Error> {
    let b: Board = serde_json::from_str(raw).map_err(sql)?;
    if b.version == 0 || b.version > 9_007_199_254_740_991 {
        return Err(refused("Board snapshot version differs"));
    }
    domain::instant(&b.updated_at)?;
    engine::validate(
        &serde_json::json!({"boardId":b.board_id,"scope":b.scope,"columns":b.columns}),
    )?;
    Ok(b)
}
pub(super) fn receipt(
    db: &Connection,
    principal: &str,
    op: &str,
    raw: &str,
    at: &str,
) -> Result<bool, Error> {
    if op != "task-board-save" {
        return Ok(false);
    }
    let b = snapshot(raw)?;
    let matched:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM rm_task_board_events WHERE board_id=?1 AND version=?2 AND principal=?3 AND payload_json=?4 AND created_at=?5)",params![b.board_id,i64::try_from(b.version).map_err(sql)?,principal,raw,at],|r|r.get(0)).map_err(sql)?;
    if !matched {
        return Err(refused(
            "Board receipt lacks exact historical result/principal/event",
        ));
    }
    Ok(true)
}
pub(super) fn events(db: &Connection) -> Result<(), Error> {
    let mut q=db.prepare("SELECT board_id,version,payload_json,actor,principal,created_at FROM rm_task_board_events ORDER BY board_id,version").map_err(sql)?;
    let mut previous: Option<Board> = None;
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
        domain::mutation_identity(&actor, &principal, "historical-board")?;
        let b = snapshot(&raw)?;
        if b.board_id != id
            || i64::try_from(b.version).map_err(sql)? != version
            || b.updated_at != at
            || previous.as_ref().is_some_and(|p| {
                p.board_id == id && (p.scope != b.scope || p.updated_at > b.updated_at)
            })
        {
            return Err(refused("Board event identity/scope/chronology differs"));
        }
        previous = Some(b);
    }
    let mut q=db.prepare("SELECT task_id,version,payload_json,principal,created_at FROM rm_task_events WHERE operation='board_changed'").map_err(sql)?;
    for row in q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(sql)?
    {
        let (id, version, raw, principal, at) = row.map_err(sql)?;
        let t = task_snapshot(&raw)?;
        let before: String = db
            .query_row(
                "SELECT payload_json FROM rm_task_events WHERE task_id=?1 AND version=?2",
                params![id, version - 1],
                |r| r.get(0),
            )
            .map_err(sql)?;
        let prior = task_snapshot(&before)?;
        if t.task_id != id
            || i64::try_from(t.version).map_err(sql)? != version
            || prior.version.checked_add(1) != Some(t.version)
        {
            return Err(refused("Board task event identity/version differs"));
        }
        if (prior.status == "done") != (t.status == "done") {
            return Err(refused("Board event bypasses completion/reopen"));
        }
        let mut expected = prior.clone();
        expected.version = t.version;
        expected.updated_at = t.updated_at.clone();
        expected.status = t.status.clone();
        expected.column_id = t.column_id.clone();
        expected.workspace_column_id = t.workspace_column_id.clone();
        if expected != t {
            return Err(refused("Board event changes unrelated task fields"));
        }
        let mut boards=db.prepare("SELECT payload_json FROM rm_task_board_events WHERE principal=?1 AND created_at=?2").map_err(sql)?;
        let mut bound = false;
        for raw in boards
            .query_map(params![principal, at], |r| r.get::<_, String>(0))
            .map_err(sql)?
        {
            let b = snapshot(&raw.map_err(sql)?)?;
            let selected = match &b.scope {
                domain::Scope::Workspace { .. } => t.workspace_column_id.as_deref(),
                domain::Scope::Company { .. } if b.scope == t.scope => Some(t.column_id.as_str()),
                _ => None,
            };
            if selected.is_some_and(|s| {
                b.columns
                    .iter()
                    .any(|c| c.column_id == s && c.status == t.status)
            }) {
                bound = true;
            }
        }
        if !bound {
            return Err(refused(
                "Board task event lacks historical board consequence",
            ));
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests;
