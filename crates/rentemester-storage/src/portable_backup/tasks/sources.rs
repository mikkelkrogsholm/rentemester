//! Codec4 supports explicit source sync including honest no-event receipts and scoped summaries.
use super::*;
use crate::tasks::sources::SyncResult;
use rentemester_engine::tasks::{Scope, Source};
pub(super) fn needed(db: &Connection) -> Result<bool, Error> {
    db.query_row("SELECT EXISTS(SELECT 1 FROM rm_task_receipts WHERE operation IN ('task-source-sync','sources-sync')) OR EXISTS(SELECT 1 FROM rm_task_events WHERE operation IN ('source_created','source_updated','source_verification_required','source_reopened'))",[],|r|r.get(0)).map_err(sql)
}
pub(super) fn receipt(
    db: &Connection,
    operation: &str,
    raw: &str,
    at: &str,
) -> Result<bool, Error> {
    domain::instant(at)?;
    match operation {
        "task-source-sync" => {
            let task = task_snapshot(raw)?;
            if task.source.is_none() {
                return Err(refused("Source receipt lacks source identity"));
            }
            // A no-op sync can legitimately be performed by another current principal and at a later
            // time than this exact snapshot. Original request bytes are not stored or reconstructed.
            let matched:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM rm_task_events WHERE task_id=?1 AND version=?2 AND payload_json=?3 AND created_at<=?4)",params![task.task_id,i64::try_from(task.version).map_err(sql)?,raw,at],|r|r.get(0)).map_err(sql)?;
            if !matched {
                return Err(refused("Source receipt lacks exact historical snapshot"));
            }
            Ok(true)
        }
        "sources-sync" => {
            let result: SyncResult = serde_json::from_str(raw).map_err(sql)?;
            let Scope::Workspace { company_slugs } = result.scope else {
                return Err(refused("Source summary scope differs"));
            };
            if result.includes_workspace
                || company_slugs.len() > 200
                || company_slugs.iter().collect::<BTreeSet<_>>().len() != company_slugs.len()
            {
                return Err(refused("Source summary selection differs"));
            }
            for slug in &company_slugs {
                domain::scope(&serde_json::json!({"kind":"company","companySlug":slug}))?;
            }
            let s = result.sync;
            if [s.created, s.updated, s.resolved, s.reopened, s.unknown]
                .iter()
                .any(|n| *n > 100000)
                || s.errors.len() > company_slugs.len()
            {
                return Err(refused("Source summary count bound"));
            }
            let mut errors = BTreeSet::new();
            for e in s.errors {
                if !company_slugs.contains(&e.company_slug)
                    || !errors.insert(e.company_slug)
                    || e.reason.trim().is_empty()
                    || e.reason.len() > 2000
                {
                    return Err(refused("Source summary error scope differs"));
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}
pub(super) fn events(db: &Connection) -> Result<(), Error> {
    let mut q=db.prepare("SELECT task_id,version,operation,payload_json,actor,principal,created_at FROM rm_task_events WHERE operation IN ('source_created','source_updated','source_verification_required','source_reopened') ORDER BY task_id,version").map_err(sql)?;
    for row in q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })
        .map_err(sql)?
    {
        let (id, version, op, raw, actor, principal, at) = row.map_err(sql)?;
        domain::mutation_identity(&actor, &principal, "historical-source")?;
        let task = task_snapshot(&raw)?;
        let source = task
            .source
            .as_ref()
            .ok_or_else(|| refused("Source event has no source"))?;
        if task.task_id != id
            || i64::try_from(task.version).map_err(sql)? != version
            || task.updated_at != at
        {
            return Err(refused("Source event identity/version/time differs"));
        }
        if op == "source_created" {
            let expected = format!(
                "source-{}",
                &domain::payload_hash(&serde_json::json!([
                    source.company_slug,
                    source.kind,
                    source.identity
                ]))?[..32]
            );
            if version != 1
                || id != expected
                || task.status != "open"
                || task.completion.is_some()
                || task.created_at != at
                || !["system", "proposal"].contains(&task.origin.as_str())
            {
                return Err(refused("Source creation differs"));
            }
        } else {
            let before: String = db
                .query_row(
                    "SELECT payload_json FROM rm_task_events WHERE task_id=?1 AND version=?2",
                    params![id, version - 1],
                    |r| r.get(0),
                )
                .map_err(sql)?;
            let prior = task_snapshot(&before)?;
            let previous: &Source = prior
                .source
                .as_ref()
                .ok_or_else(|| refused("Source prior lacks identity"))?;
            if previous.identity != source.identity
                || previous.kind != source.kind
                || previous.company_slug != source.company_slug
                || previous == source
            {
                return Err(refused(
                    "Source event changes identity or repeats unchanged source",
                ));
            }
            let reopened = source.state != "resolved" && prior.status == "done";
            let expected_op = if source.state == "unknown" {
                "source_verification_required"
            } else if reopened {
                "source_reopened"
            } else {
                "source_updated"
            };
            let mut expected = prior.clone();
            expected.source = task.source.clone();
            expected.verification_required =
                source.state == "unknown" || prior.relevance == "unknown";
            expected.version = prior
                .version
                .checked_add(1)
                .ok_or_else(|| refused("Source version overflow"))?;
            expected.updated_at = at;
            if reopened {
                expected.status = "open".into();
                expected.completion = None;
                expected.column_id = task.column_id.clone();
                expected.workspace_column_id = task.workspace_column_id.clone();
            }
            if op != expected_op || expected != task {
                return Err(refused("Source transition changes unrelated task fields"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{Context, sources as writes};
    #[test]
    fn source_codec_roundtrip_noevent_other_principal_and_downgrade_refusal() {
        let mut c = Connection::open_in_memory().unwrap();
        crate::control_migration::initialize_empty(&mut c).unwrap();
        let draft = serde_json::json!({"title":"Læs den faktiske kilde","scope":{"kind":"company","companySlug":"synthetic"},"source":{"kind":"vat_filing","identity":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:vat_filing:2026-07-01:2026-09-30","companySlug":"synthetic","ref":"2026-07-01:2026-09-30","state":"open","contentHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","href":"/companies/synthetic/moms"}});
        for (principal, key, at) in [
            ("one", "first", "2026-10-26T12:00:00.000Z"),
            ("two", "noop", "2026-10-26T13:00:00.000Z"),
        ] {
            let tx = c
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            writes::synchronize_in_tx(
                &tx,
                &draft,
                &Context {
                    actor: "user:synthetic",
                    principal,
                    idempotency_key: key,
                    expected_version: None,
                    now: at,
                    source_check: None,
                },
                |_| Ok(()),
            )
            .unwrap();
            tx.commit().unwrap();
        }
        assert_eq!(
            c.query_row::<i64, _, _>("SELECT count(*) FROM rm_task_events", [], |r| r.get(0))
                .unwrap(),
            1
        );
        let bytes = export(&c).unwrap();
        let mut body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["format"], FORMAT4);
        let mut target = Connection::open_in_memory().unwrap();
        crate::control_migration::initialize_empty(&mut target).unwrap();
        initialize(&mut target, &bytes, &["synthetic".into()]).unwrap();
        assert_eq!(export(&target).unwrap(), bytes);
        body["format"] = serde_json::json!(FORMAT3);
        let mut empty = Connection::open_in_memory().unwrap();
        crate::control_migration::initialize_empty(&mut empty).unwrap();
        assert!(
            initialize(
                &mut empty,
                &serde_json::to_vec(&body).unwrap(),
                &["synthetic".into()]
            )
            .is_err()
        );
        assert_eq!(
            empty
                .query_row::<i64, _, _>("SELECT count(*) FROM rm_task_events", [], |r| r.get(0))
                .unwrap(),
            0
        );
        for state in ["resolved","unknown"] {
            let mut observed=draft.clone();observed["source"]["state"]=serde_json::json!(state);observed["source"]["contentHash"]=serde_json::json!(if state=="resolved"{"b".repeat(64)}else{"c".repeat(64)});
            let check=rentemester_engine::tasks::SourceCheck{state:state.into(),observed_at:"2026-10-26T14:00:00.000Z".into(),evidence:if state=="resolved"{vec![rentemester_engine::tasks::Reference{kind:"period".into(),reference:"2026-07-01:2026-09-30".into(),company_slug:Some("synthetic".into())}]}else{vec![]}};
            let tx=c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).unwrap();
            writes::sync_in_tx(&tx,&serde_json::json!({}),&["synthetic".into()],&Context{actor:"user:synthetic",principal:"one",idempotency_key:state,expected_version:None,now:"2026-10-26T14:00:00.000Z",source_check:None},|_|Ok(()),|_,_|Ok((vec![rentemester_engine::tasks::sources::Observation{draft:observed.clone(),check:check.clone()}],false))).unwrap();tx.commit().unwrap();
        }
        let full=export(&c).unwrap();let mut recovered=Connection::open_in_memory().unwrap();crate::control_migration::initialize_empty(&mut recovered).unwrap();initialize(&mut recovered,&full,&["synthetic".into()]).unwrap();assert_eq!(export(&recovered).unwrap(),full);
        let raw = rentemester_engine::tasks::canonical_json(
            &serde_json::json!({"scope":{"kind":"workspace","companySlugs":["synthetic"]},"includesWorkspace":false,"sync":{"created":0,"updated":0,"resolved":0,"reopened":0,"unknown":1,"errors":[{"companySlug":"other","reason":"unavailable"}]}}),
        );
        assert!(receipt(&target, "sources-sync", &raw, "2026-10-26T13:00:00.000Z").is_err());
    }
}
