//! Trusted native source observations; never constructed from a public task write DTO.
use super::*;
use serde_json::json;
#[cfg(test)]
mod tests;
#[derive(Debug, Clone)]
pub struct Observation {
    pub draft: Value,
    pub check: SourceCheck,
}
pub fn synchronize(
    prior: Option<&Task>,
    input: &Value,
    expected: Option<u64>,
    now: &str,
    boards: &[Board],
) -> Result<(Task, Option<&'static str>), Error> {
    let value = draft(input)?;
    let source: Source = serde_json::from_value(
        value
            .get("source")
            .filter(|v| !v.is_null())
            .ok_or_else(|| invalid("Source synchronization requires a source"))?
            .clone(),
    )
    .map_err(|_| invalid("Invalid source"))?;
    let Some(prior) = prior else {
        let id = format!(
            "source-{}",
            &payload_hash(&json!([source.company_slug, source.kind, source.identity]))?[..32]
        );
        let mut v = value;
        v["origin"] = json!(if v["origin"] == "proposal" {
            "proposal"
        } else {
            "system"
        });
        return Ok((create(&v, &id, now, boards)?, Some("source_created")));
    };
    if let Some(version) = expected {
        if version != prior.version {
            return Err(refusal(
                "version_conflict",
                "Task changed; reload before source synchronization",
            ));
        }
    }
    let previous = prior
        .source
        .as_ref()
        .ok_or_else(|| invalid("Existing source task lacks a source"))?;
    if previous.company_slug != source.company_slug
        || previous.kind != source.kind
        || previous.identity != source.identity
    {
        return Err(invalid("Source identity cannot change"));
    }
    if canonical_json(&json!(previous)) == canonical_json(&json!(source)) {
        return Ok((prior.clone(), None));
    }
    let mut task = prior.clone();
    task.source = Some(source);
    task.verification_required =
        task.source.as_ref().unwrap().state == "unknown" || task.relevance == "unknown";
    let reopen = task.source.as_ref().unwrap().state != "resolved" && prior.status == "done";
    if reopen {
        task.status = "open".into();
        task.completion = None;
        task.column_id = placement(boards, &prior.scope, "open", None, false)?;
        task.workspace_column_id = prior
            .workspace_column_id
            .as_ref()
            .map(|_| {
                placement(
                    boards,
                    &Scope::Workspace {
                        company_slugs: vec![],
                    },
                    "open",
                    None,
                    false,
                )
            })
            .transpose()?;
    }
    let event = if task.source.as_ref().unwrap().state == "unknown" {
        "source_verification_required"
    } else if reopen {
        "source_reopened"
    } else {
        "source_updated"
    };
    Ok((changed(task, now)?, Some(event)))
}
