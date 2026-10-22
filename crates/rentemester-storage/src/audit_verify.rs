//! Deterministic composition; transport success never implies clean accounting.
use rentemester_engine::Error;
use rusqlite::Transaction;
use serde_json::{Value, json};
fn unavailable(name: &str, error: Error) -> Value {
    json!({"name":name,"available":false,"ok":false,"checked":{},"errors":[{"code":error.code,"error":error.message}]})
}
fn component(name: &str, result: Result<Value, Error>) -> Value {
    match result {
        Ok(mut value)
            if value["available"] == true
                && value["checked"].is_object()
                && value["errors"].is_array()
                && value["ok"] == value["errors"].as_array().is_some_and(Vec::is_empty) =>
        {
            value["name"] = json!(name);
            if let Some(errors) = value["errors"].as_array_mut() {
                errors.sort_by_cached_key(Value::to_string);
            }
            value
        }
        Ok(_) => unavailable(
            name,
            Error::new(
                "audit_component_invalid",
                "audit component result could not be confirmed",
            ),
        ),
        Err(error) => unavailable(name, error),
    }
}
fn health(tx: &Transaction<'_>) -> Result<Value, Error> {
    crate::journal_preview::require_current(tx)?;
    crate::documents_original::validate_history(tx)?;
    let mut statement = tx
        .prepare("PRAGMA integrity_check")
        .map_err(crate::invalid)?;
    let findings = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(crate::invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(crate::invalid)?;
    let errors = findings
        .into_iter()
        .filter(|message| message != "ok")
        .map(|message| json!({"code":"SQLITE_INTEGRITY","error":message}))
        .collect::<Vec<_>>();
    Ok(
        json!({"available":true,"ok":errors.is_empty(),"checked":{"schema":1,"sqliteIntegrity":1},"errors":errors}),
    )
}
pub fn report(
    tx: &Transaction<'_>,
    slug: &str,
    originals: Result<Value, Error>,
) -> Result<Value, Error> {
    let entries: i64 = tx
        .query_row("SELECT count(*) FROM journal_entries", [], |row| row.get(0))
        .map_err(crate::invalid)?;
    let components = vec![
        component("ledger", crate::audit_ledger::verify(tx)),
        component("bank", crate::audit_bank::verify(tx)),
        component("invoices", crate::audit_invoice::verify(tx)),
        component("originals", originals),
        component("physical_schema", health(tx)),
    ];
    let available = components.iter().all(|value| value["available"] == true);
    let ok = available && components.iter().all(|value| value["ok"] == true);
    let errors = components
        .iter()
        .flat_map(|value| {
            value["errors"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|finding| {
                    format!("{}: {}", value["name"].as_str().unwrap_or("audit"), finding)
                })
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"ok":true,"audit":{"format":"rentemester-native-audit-verification/1","slug":slug,"ok":ok,"available":available,"entries":entries,"errors":errors,"components":components}}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn component_failure_and_inconsistent_green_never_become_clean_results() {
        for result in [
            Err(Error::new(
                "synthetic_unavailable",
                "Synthetic incomplete check",
            )),
            Ok(
                json!({"available":true,"ok":true,"checked":{},"errors":[{"code":"synthetic_corruption"}]}),
            ),
        ] {
            let value = component("synthetic", result);
            assert_eq!(value["available"], false);
            assert_eq!(value["ok"], false);
            assert!(!value["errors"].as_array().unwrap().is_empty());
        }
        let value = component(
            "synthetic",
            Ok(
                json!({"available":true,"ok":false,"checked":{"entries":1},"errors":[{"code":"synthetic_corruption"}]}),
            ),
        );
        assert_eq!(value["available"], true);
        assert_eq!(value["ok"], false);
    }
}
