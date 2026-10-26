use super::{input::*, *};
use serde_json::{Value, json};

fn force_verification(task: &mut Task) {
    if task.source.as_ref().is_some_and(|s| s.state == "unknown") || task.relevance == "unknown" {
        task.verification_required = true;
    }
}
/// Normalize a draft for scope/recipient authorization before receipt lookup.
/// This does not place it on a current board or authorize storing an event.
pub fn prepare_create(input: &Value, generated_id: &str, now: &str) -> Result<Task, Error> {
    let value = draft(input)?;
    let mut result = json!({"taskId":id(&json!(generated_id))?,"version":1,"title":"","description":"","nextAction":"","scope":value["scope"],"type":"ad_hoc","origin":"manual","status":"open","columnId":"open","workspaceColumnId":null,"assignee":null,"waitingOn":"","workDate":null,"deadline":null,"period":null,"references":[],"source":null,"evidenceRequired":false,"relevance":"relevant","verificationRequired":false,"completion":null,"seriesId":null,"occurrenceKey":null,"reminders":[],"createdAt":instant(now)?.0,"updatedAt":instant(now)?.0});
    result
        .as_object_mut()
        .expect("task object")
        .extend(value.as_object().expect("validated draft").clone());
    let mut task: Task =
        serde_json::from_value(result).map_err(|_| invalid("Invalid task draft"))?;
    if task.relevance == "not_relevant" {
        return Err(invalid(
            "Record non-applicability with a reason through completion",
        ));
    }
    force_verification(&mut task);
    Ok(task)
}
/// `generated_id` is supplied by the host once for a new command; replay uses its receipt.
pub fn create(
    input: &Value,
    generated_id: &str,
    now: &str,
    boards: &[Board],
) -> Result<Task, Error> {
    let mut task = prepare_create(input, generated_id, now)?;
    task.column_id = placement(
        boards,
        &task.scope,
        &task.status,
        input["columnId"].as_str(),
        false,
    )?;
    task.workspace_column_id = input["workspaceColumnId"]
        .as_str()
        .map(|column| {
            placement(
                boards,
                &Scope::Workspace {
                    company_slugs: vec![],
                },
                &task.status,
                Some(column),
                false,
            )
        })
        .transpose()?;
    force_verification(&mut task);
    Ok(task)
}
pub fn update(
    prior: &Task,
    patch: &Value,
    expected_version: Option<u64>,
    now: &str,
    boards: &[Board],
) -> Result<Task, Error> {
    existing(prior, expected_version)?;
    let scope = match patch.get("scope") {
        Some(v) => super::scope(v)?,
        None => prior.scope.clone(),
    };
    let normalized = fields(patch, &scope)?;
    let mut value = serde_json::to_value(prior).map_err(|_| invalid("Invalid persisted task"))?;
    value
        .as_object_mut()
        .expect("task object")
        .extend(normalized.clone());
    let mut task: Task =
        serde_json::from_value(value).map_err(|_| invalid("Invalid task patch"))?;
    if prior.origin == "proposal" && patch["relevance"] == "relevant" {
        task.origin = "system".into();
    }
    references(&json!(task.references), &scope)?;
    if task
        .source
        .as_ref()
        .is_some_and(|s| !scope.companies().contains(&s.company_slug.as_str()))
    {
        return Err(invalid("Source company is outside the new scope"));
    }
    if patch.get("evidenceRequired") == Some(&Value::Bool(false)) && prior.evidence_required {
        return Err(invalid(
            "Evidence requirement cannot be removed; record a documented exception instead",
        ));
    }
    if patch["relevance"] == "not_relevant" && prior.relevance != "not_relevant" {
        return Err(invalid(
            "Record non-applicability with a reason through completion",
        ));
    }
    if prior.source.as_ref().is_some_and(|s| s.state == "unknown")
        && patch.get("verificationRequired") == Some(&Value::Bool(false))
    {
        return Err(invalid("Source must be verified first"));
    }
    if prior.source.is_none()
        && prior.verification_required
        && patch.get("verificationRequired") == Some(&Value::Bool(false))
        && !(patch["relevance"] == "relevant"
            && normalized
                .get("references")
                .and_then(Value::as_array)
                .is_some_and(|a| !a.is_empty())
            && normalized
                .get("description")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty()))
    {
        return Err(invalid(
            "Clarifying an external result requires relevance, evidence and a documented explanation",
        ));
    }
    if prior.deadline["kind"] == "statutory"
        && prior.deadline != task.deadline
        && (task.deadline.is_null() || task.deadline["basis"] == prior.deadline["basis"])
    {
        return Err(invalid(
            "Changing a statutory deadline requires a new visible basis",
        ));
    }
    if prior.scope != scope {
        task.column_id = placement(boards, &scope, &task.status, None, false)?;
    }
    force_verification(&mut task);
    changed(task, now)
}
pub fn move_task(
    prior: &Task,
    input: &Value,
    expected_version: Option<u64>,
    now: &str,
    boards: &[Board],
) -> Result<Task, Error> {
    object(input, &["status", "columnId", "workspaceColumnId"])?;
    existing(prior, expected_version)?;
    let status = choice(&input["status"], STATUSES)?;
    if status == "done" && prior.status != "done" {
        return Err(refusal(
            "completion_required",
            "Use documented completion before moving to done",
        ));
    }
    if prior.status == "done" && status != "done" {
        return Err(refusal(
            "reopen_required",
            "Reopen with a reason before moving a completed task",
        ));
    }
    let column = input.get("columnId").map(id).transpose()?;
    let workspace_column = input.get("workspaceColumnId").map(id).transpose()?;
    let mut task = prior.clone();
    if status != prior.status || column.is_some() {
        task.column_id = placement(
            boards,
            &prior.scope,
            &status,
            column.as_deref(),
            matches!(prior.scope, Scope::Company { .. }),
        )?;
    }
    let workspace = Scope::Workspace {
        company_slugs: vec![],
    };
    if let Some(column) = workspace_column {
        task.workspace_column_id = Some(placement(
            boards,
            &workspace,
            &status,
            Some(&column),
            false,
        )?);
    } else if prior.workspace_column_id.is_some() && status != prior.status {
        task.workspace_column_id = Some(placement(boards, &workspace, &status, None, false)?);
    }
    task.status = status;
    changed(task, now)
}
fn source_check(check: Option<&SourceCheck>, now: &str) -> Result<(), Error> {
    if let Some(check) = check {
        choice(&json!(check.state), &["open", "resolved", "unknown"])?;
        let age = instant(now)?.1 - instant(&check.observed_at)?.1;
        if !(-5000..=60000).contains(&age) {
            return Err(refusal(
                "source_check_stale",
                "Source must be freshly checked before completion",
            ));
        }
    }
    Ok(())
}
pub(super) fn actor(value: &str) -> Result<String, Error> {
    let actor = text(&json!(value), "actor", 160, true)?;
    let Some((kind, subject)) = value.split_once(':') else {
        return Err(invalid("Actor is required"));
    };
    if !["user", "agent", "system", "service_account"].contains(&kind)
        || subject
            .chars()
            .next()
            .is_none_or(|c| c.is_whitespace() || c == '\u{feff}')
        || subject.encode_utf16().count() < 2
        || subject.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
    {
        return Err(invalid("Actor is required"));
    }
    Ok(actor)
}
/// The source check and actor are trusted application context, not client assertions.
pub fn complete(
    prior: &Task,
    input: &Value,
    expected_version: Option<u64>,
    now: &str,
    actor_id: &str,
    check: Option<&SourceCheck>,
    boards: &[Board],
) -> Result<Task, Error> {
    existing(prior, expected_version)?;
    if prior.status == "done" {
        return Err(refusal("already_completed", "Task is already complete"));
    }
    object(input, &["outcome", "note", "references"])?;
    let outcome = choice(
        &input["outcome"],
        &["completed", "not_relevant", "cancelled", "exception"],
    )?;
    let note = content(&input["note"], "completion note", 8000, true)?;
    let mut refs = references(input.get("references").unwrap_or(&json!([])), &prior.scope)?;
    source_check(check, now)?;
    let state = check.map(|c| c.state.as_str());
    let clarified = outcome == "not_relevant"
        && state == Some("unknown")
        && prior
            .source
            .as_ref()
            .is_some_and(|s| ["annual_reporting", "vat_registration"].contains(&s.kind.as_str()));
    if prior.source.is_some()
        && !["exception", "cancelled"].contains(&outcome.as_str())
        && !clarified
        && state != Some("resolved")
    {
        return Err(refusal(
            "source_unresolved",
            "Authoritative source has not confirmed completion",
        ));
    }
    if outcome == "completed" && prior.relevance == "unknown" {
        return Err(refusal(
            "relevance_unknown",
            "Task relevance must be clarified before completion",
        ));
    }
    if outcome == "completed" && prior.verification_required && prior.source.is_none() {
        return Err(refusal(
            "verification_required",
            "External result must be verified before completion",
        ));
    }
    let evidence = if let Some(check) = check.filter(|c| c.state == "resolved") {
        references(&json!(check.evidence), &prior.scope)?
    } else {
        vec![]
    };
    let verified = outcome == "completed"
        && prior.source.is_some()
        && state == Some("resolved")
        && !evidence.is_empty();
    for reference in evidence {
        if !refs.contains(&reference) {
            refs.push(reference);
        }
    }
    // User references can also repeat one another; canonical order is first occurrence.
    let mut dedup = Vec::new();
    for reference in refs {
        if !dedup.contains(&reference) {
            dedup.push(reference);
        }
    }
    if prior.evidence_required && outcome == "completed" && dedup.is_empty() {
        return Err(refusal(
            "evidence_required",
            "Completion evidence is required; record a reasoned exception instead",
        ));
    }
    let mut task = prior.clone();
    task.status = "done".into();
    task.column_id = placement(boards, &task.scope, "done", None, false)?;
    if task.workspace_column_id.is_some() {
        task.workspace_column_id = Some(placement(
            boards,
            &Scope::Workspace {
                company_slugs: vec![],
            },
            "done",
            None,
            false,
        )?);
    }
    task.completion = Some(Completion {
        outcome: outcome.clone(),
        note,
        references: dedup,
        assurance: if verified {
            "product_verified"
        } else {
            "user_reported"
        }
        .into(),
        at: instant(now)?.0,
        actor: actor(actor_id)?,
    });
    if let (Some(source), Some(check)) = (&mut task.source, check) {
        source.state = check.state.clone();
    }
    if outcome == "not_relevant" {
        task.relevance = "not_relevant".into();
    }
    if outcome == "completed" && (task.source.is_none() || state == Some("resolved")) {
        task.verification_required = false;
    }
    changed(task, now)
}
pub fn reopen(
    prior: &Task,
    reason: &Value,
    expected_version: Option<u64>,
    now: &str,
    boards: &[Board],
) -> Result<Task, Error> {
    existing(prior, expected_version)?;
    content(reason, "reopen reason", 2000, true)?;
    if prior.status != "done" {
        return Err(refusal("not_completed", "Task is not complete"));
    }
    let mut task = prior.clone();
    task.status = "open".into();
    task.column_id = placement(boards, &task.scope, "open", None, false)?;
    if task.workspace_column_id.is_some() {
        task.workspace_column_id = Some(placement(
            boards,
            &Scope::Workspace {
                company_slugs: vec![],
            },
            "open",
            None,
            false,
        )?);
    }
    task.completion = None;
    if task.relevance == "not_relevant" {
        task.relevance = "unknown".into();
    }
    if task.relevance == "unknown" {
        task.verification_required = true;
    }
    changed(task, now)
}
