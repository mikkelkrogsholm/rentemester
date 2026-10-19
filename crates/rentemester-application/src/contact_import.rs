//! Authorized prepare, zero SQL transactions during CVR I/O, fresh fenced final apply.
use crate::{
    access::Permission,
    control_fence::{
        self, Committed, CompanySelection, CookieAuthorization, Failure, FailureStage,
        SessionRequirement,
    },
    documents::Context,
};
use rentemester_engine::{Error, contact_import::Input};
use rentemester_runtime::{contact_import::Lookup, namespace::NamespaceGuard};
use rentemester_storage::contact_import::{self as store, Lookup as LookupResult, LookupOutcome};
use rusqlite::Connection;
use serde_json::Value;
fn failure(code: &'static str, reusable: bool) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error: Error::new(
            code,
            "Native contact import refused before contact publication",
        ),
        control_reusable: reusable,
        company_reusable: reusable,
    }
}
fn selection<'a>(context: &Context<'a>) -> CompanySelection<'a> {
    CompanySelection {
        workspace: context.selection.workspace,
        slug: context.selection.slug,
        registered: context.selection.registered,
        archived: context.selection.archived,
    }
}
fn authorization<'a>(context: &Context<'a>) -> CookieAuthorization<'a> {
    CookieAuthorization {
        cookie: context.cookie,
        clock: context.clock,
        requirement: SessionRequirement::Current,
    }
}
fn now(context: &Context<'_>) -> Result<String, Failure> {
    rentemester_engine::company_knowledge::timestamp(
        context
            .clock
            .now_unix_ms()
            .map_err(|_| failure("invalid_auth_clock", true))?,
    )
    .map_err(|_| failure("invalid_auth_clock", true))
}
struct Prepared {
    input: store::Prepared,
    subject: String,
    ledger: String,
}
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    input: &Input,
    lookup: &mut dyn Lookup,
) -> Result<Committed<Value>, Failure> {
    let stamp = now(&context)?;
    let prepared = control_fence::execute_cookie_subject(
        control,
        company,
        authorization(&context),
        selection(&context),
        Permission::CompanyLedgerPost,
        |tx, subject| {
            let changes = tx.total_changes();
            let input = store::prepare(tx, input, &stamp)?;
            let ledger = tx
                .query_row(
                    "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .map_err(|_| {
                    Error::new(
                        "import_source_unavailable",
                        "Native company identity unavailable",
                    )
                })?;
            if tx.total_changes() != changes {
                return Err(Error::new(
                    "readonly_operation_changed_data",
                    "Import prepare must not mutate",
                ));
            }
            Ok(Prepared {
                input,
                subject: subject.into(),
                ledger,
            })
        },
    )?;
    if !prepared.control_reusable || !control.is_autocommit() || !company.is_autocommit() {
        return Err(failure("import_connection_unavailable", false));
    }
    let prepared = prepared.value;
    let results = collect_lookups(&prepared.input.lookup_needs, lookup, 64 * 1024 * 1024);
    guard
        .ensure_connections(control, company)
        .map_err(|_| failure("import_namespace_changed", false))?;
    let stamp = now(&context)?;
    control_fence::execute_cookie_subject(
        control,
        company,
        authorization(&context),
        selection(&context),
        Permission::CompanyLedgerPost,
        |tx, subject| {
            let ledger: String = tx
                .query_row(
                    "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .map_err(|_| {
                    Error::new(
                        "import_source_unavailable",
                        "Native company identity unavailable",
                    )
                })?;
            if subject != prepared.subject || ledger != prepared.ledger {
                return Err(Error::new(
                    "import_identity_changed",
                    "Import subject or ledger changed during lookup",
                ));
            }
            store::execute(tx, &prepared.input, &results, subject, &stamp)
        },
    )
}

// Bound retained raw entities across the import, independently of each HTTPS response limit.
fn collect_lookups(
    needs: &[store::LookupNeed],
    lookup: &mut dyn Lookup,
    budget: usize,
) -> Vec<LookupResult> {
    let mut remaining = budget;
    let mut exhausted = false;
    needs
        .iter()
        .map(|need| {
            let outcome = if exhausted {
                LookupOutcome::Failed(
                    "CVR-importens samlede svargrænse er nået; CSV-data bevares".into(),
                )
            } else {
                match lookup.lookup(&need.cvr) {
                    Ok(entity) => {
                        // Serialization is bounded by the provider's individual response limit.
                        let size = serde_json::to_vec(&entity)
                            .map(|v| v.len())
                            .unwrap_or(usize::MAX);
                        if size > remaining {
                            exhausted = true;
                            LookupOutcome::Failed(
                                "CVR-importens samlede svargrænse er nået; CSV-data bevares".into(),
                            )
                        } else {
                            remaining -= size;
                            exhausted = remaining == 0;
                            LookupOutcome::Found(entity)
                        }
                    }
                    Err(e) => LookupOutcome::Failed(e.message().into()),
                }
            };
            LookupResult {
                cvr: need.cvr.clone(),
                outcome,
            }
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Port(usize);
    impl Lookup for Port {
        fn lookup(
            &mut self,
            _: &str,
        ) -> Result<Value, rentemester_runtime::contact_import::LookupError> {
            self.0 += 1;
            Ok(serde_json::json!({"a":"b"}))
        }
    }
    #[test]
    fn aggregate_budget_stops_io_and_reports_each_remaining_lookup() {
        let needs = (0..4)
            .map(|i| store::LookupNeed {
                cvr: format!("{i:08}"),
            })
            .collect::<Vec<_>>();
        let mut port = Port(0);
        let results = collect_lookups(&needs, &mut port, 10);
        assert_eq!(port.0, 2);
        assert_eq!(results.len(), 4);
        assert!(matches!(results[0].outcome, LookupOutcome::Found(_)));
        assert!(
            results[1..]
                .iter()
                .all(|r| matches!(r.outcome, LookupOutcome::Failed(_)))
        );
        let mut port = Port(0);
        let exact = collect_lookups(&needs, &mut port, 9);
        assert_eq!(port.0, 1);
        assert!(matches!(exact[0].outcome, LookupOutcome::Found(_)));
    }
}
