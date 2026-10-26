//! Board and all relocated task events share the application's admitted write transaction.
use super::*;
use rentemester_engine::tasks::{Scope, boards as domain};
pub fn get(db: &Connection, id: &str) -> Result<Option<Board>, Error> {
    tasks::id(&json!(id))?;
    let raw: Option<String> = db
        .query_row(
            "SELECT payload_json FROM rm_current_task_boards WHERE board_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(invalid)?;
    raw.map(|s| serde_json::from_str(&s).map_err(invalid))
        .transpose()
}
pub fn admit_request<A>(
    db: &Connection,
    input: &Value,
    mut admit: A,
) -> Result<(domain::Draft, Option<Board>, bool), Error>
where
    A: FnMut(&Scope, bool) -> Result<(), Error>,
{
    let d = domain::validate(input)?;
    admit(&d.scope, false)?;
    let prior = get(db, &d.board_id)?;
    let structural = prior
        .as_ref()
        .unwrap_or(&domain::default_board(&d.scope))
        .columns
        .iter()
        .any(|c| {
            !d.columns
                .iter()
                .any(|n| n.column_id == c.column_id && n.status == c.status)
        });
    admit(&d.scope, structural)?;
    Ok((d, prior, structural))
}
pub fn preview<A, T>(
    db: &Connection,
    input: &Value,
    mut admit: A,
    mut authorize: T,
) -> Result<domain::Changes, Error>
where
    A: FnMut(&Scope, bool) -> Result<(), Error>,
    T: FnMut(&Task) -> Result<(), Error>,
{
    let (_, prior, structural) = admit_request(db, input, &mut admit)?;
    let all = if structural {
        super::list(db, |_| Ok(true))?
    } else {
        vec![]
    };
    let changes = domain::changes(input, prior.as_ref(), &all)?;
    for change in &changes.affected {
        authorize(&change.task)?;
    }
    Ok(changes)
}
pub fn save_in_tx<A, T>(
    tx: &Transaction<'_>,
    input: &Value,
    ctx: &Context<'_>,
    mut admit: A,
    mut authorize: T,
) -> Result<Board, Error>
where
    A: FnMut(&Scope, bool) -> Result<(), Error>,
    T: FnMut(&Task) -> Result<(), Error>,
{
    if tx
        .transaction_state(Some(rusqlite::MAIN_DB))
        .map_err(invalid)?
        != rusqlite::TransactionState::Write
    {
        return Err(Error::new(
            "control_fence_required",
            "Board writer transaction required",
        ));
    }
    tasks::mutation_identity(ctx.actor, ctx.principal, ctx.idempotency_key)?;
    let now = tasks::instant(ctx.now)?.0;
    // Even cached saves recheck current structural authority without relying on private workload.
    let (_, current, _) = admit_request(tx, input, &mut admit)?;
    let hash =
        tasks::payload_hash(&json!({"payload":input,"expectedVersion":ctx.expected_version}))?;
    let prior:Option<(String,String,String)>=tx.query_row("SELECT operation,payload_hash,result_json FROM rm_task_receipts WHERE principal=?1 AND idempotency_key=?2",params![ctx.principal,ctx.idempotency_key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(invalid)?;
    if let Some((operation, payload, raw)) = prior {
        if operation != "task-board-save" || payload != hash {
            return Err(Error::new(
                "idempotency_conflict",
                "Key already used for another request",
            ));
        }
        let historical: Board = serde_json::from_str(&raw).map_err(invalid)?;
        admit(&historical.scope, false)?;
        if let Some(current) = current {
            admit(&current.scope, false)?;
        }
        return Ok(historical);
    }
    // New effects validate current relocations; exact replay never reapplies mutable placement rules.
    let changes = preview(tx, input, &mut admit, &mut authorize)?;
    let (board, updated) =
        domain::apply(&changes, &super::boards(tx)?, ctx.expected_version, &now)?;
    for task in updated {
        authorize(&task)?;
        let raw = tasks::canonical_json(&json!(task));
        tx.execute("INSERT INTO rm_task_events(task_id,version,operation,payload_json,actor,principal,created_at,reason)VALUES(?1,?2,'board_changed',?3,?4,?5,?6,NULL)",params![task.task_id,i64::try_from(task.version).map_err(invalid)?,raw,ctx.actor,ctx.principal,now]).map_err(invalid)?;
    }
    let raw = tasks::canonical_json(&json!(board));
    tx.execute("INSERT INTO rm_task_board_events(board_id,version,payload_json,actor,principal,created_at)VALUES(?1,?2,?3,?4,?5,?6)",params![board.board_id,i64::try_from(board.version).map_err(invalid)?,raw,ctx.actor,ctx.principal,now]).map_err(invalid)?;
    tx.execute("INSERT INTO rm_task_receipts(principal,idempotency_key,operation,payload_hash,result_json,created_at)VALUES(?1,?2,'task-board-save',?3,?4,?5)",params![ctx.principal,ctx.idempotency_key,hash,raw,now]).map_err(invalid)?;
    Ok(board)
}
