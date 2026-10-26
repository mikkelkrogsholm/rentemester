//! Explicit source sync: all selected current roles before receipt and before any ledger probe.
use super::*;
fn selection(input: &Value, access: &Access) -> Result<Vec<String>, Error> {
    let selected = if let Some(value) = input.get("companySlugs") {
        let a = value
            .as_array()
            .filter(|a| a.len() <= 200)
            .ok_or_else(denied)?;
        let mut out = vec![];
        for value in a {
            let s = value
                .as_str()
                .filter(|s| access.write.contains(*s))
                .ok_or_else(denied)?;
            if !out.iter().any(|v| v == s) {
                out.push(s.to_owned());
            }
        }
        out
    } else {
        access.write.iter().cloned().collect()
    };
    if selected.is_empty() && !access.owner {
        return Err(denied());
    }
    Ok(selected)
}
pub(super) fn preadmit(input: &Value, access: &Access, expected: Option<u64>) -> Result<(), Error> {
    if expected.is_some()
        || input.as_object().is_none_or(|m| {
            m.keys()
                .any(|k| !["companySlugs", "asOfDate"].contains(&k.as_str()))
        })
    {
        return Err(error("invalid_input"));
    }
    selection(input, access)?;
    if let Some(date) = input.get("asOfDate") {
        domain::date(date)?;
    }
    Ok(())
}
pub(super) fn mutate(
    tx: &Transaction<'_>,
    input: &Value,
    ctx: &store::Context<'_>,
    catalog: &Catalog,
    access: &Access,
    now: i64,
    reader: &mut Option<&mut dyn SourceReader>,
) -> Result<Value, Error> {
    let selected = selection(input, access)?;
    let as_of = input["asOfDate"].as_str().unwrap_or(&ctx.now[..10]);
    let result = store::sources::sync_in_tx(
        tx,
        input,
        &selected,
        ctx,
        |task| authorize_task(tx, catalog, access, task, true),
        |slug, existing| {
            // The fixed host reader is invoked only after current scope was admitted under this writer.
            let discover = !catalog
                .companies
                .iter()
                .any(|c| c.slug == slug && c.archived);
            let r = reader
                .as_mut()
                .ok_or_else(|| error("task_sources_unavailable"))?;
            r.observe(
                tx,
                SourceRequest {
                    slug,
                    as_of,
                    now,
                    existing,
                    discover,
                },
            )
        },
    )?;
    if !scope_allowed(access, &result.scope, true) || result.includes_workspace {
        return Err(denied());
    }
    Ok(json!(result))
}
