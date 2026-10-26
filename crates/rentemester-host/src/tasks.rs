//! Control-only task authority. Native financial sources are opened lazily AFTER scope admission.
use super::*;
use rentemester_application::tasks::{self, Action, Authority, SourceReader, SourceRequest};
use rentemester_engine::{
    Error,
    tasks::{SourceCheck, Task, sources::Observation},
};
use rusqlite::{Transaction, TransactionBehavior};
struct Sources<'a> {
    root: &'a Path,
    original: String,
    selected: &'a mut String,
    company: &'a mut Option<Connection>,
    guard: &'a mut NamespaceGuard,
    poisoned: bool,
}
fn unavailable() -> Error {
    Error::new(
        "connection_refused",
        "Native source owner could not be restored safely",
    )
}
impl Sources<'_> {
    fn read<T>(
        &mut self,
        control: &Transaction<'_>,
        slug: &str,
        work: impl FnOnce(&Transaction<'_>, &mut NamespaceGuard) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if self.poisoned {
            return Err(unavailable());
        }
        let result = (|| {
            // Retained archived Live tasks may be verified. Application authority precedes this selection.
            portable_backup::switch(
                self.company,
                self.guard,
                self.root,
                slug,
                self.selected,
                true,
            )?;
            let company = self.company.as_mut().ok_or_else(unavailable)?;
            self.guard
                .ensure_connections(control, company)
                .map_err(|_| unavailable())?;
            let read = company
                .transaction_with_behavior(TransactionBehavior::Deferred)
                .map_err(|_| {
                    Error::new(
                        "task_source_unavailable",
                        "Native source read could not begin",
                    )
                })?;
            let initial = read.total_changes();
            let result = work(&read, self.guard);
            let unchanged = read.total_changes() == initial;
            if read.rollback().is_err() || !unchanged {
                self.poisoned = true;
                return Err(unavailable());
            }
            result
        })();
        if portable_backup::switch(
            self.company,
            self.guard,
            self.root,
            &self.original,
            self.selected,
            false,
        )
        .is_err()
        {
            self.poisoned = true;
        }
        if self.poisoned {
            Err(unavailable())
        } else {
            result
        }
    }
}
impl SourceReader for Sources<'_> {
    fn observe(
        &mut self,
        control: &Transaction<'_>,
        request: SourceRequest<'_>,
    ) -> Result<(Vec<Observation>, bool), Error> {
        let stamp = rentemester_engine::company_knowledge::timestamp(request.now)?;
        let result = self.read(control, request.slug, |tx, guard| {
            let today = rentemester_runtime::posting_time::latest_date(request.now, 0)
                .map_err(|_| unavailable())?;
            let latest = rentemester_engine::fiscal_year::Date::parse(
                &rentemester_runtime::posting_time::latest_date(request.now, 45)
                    .map_err(|_| unavailable())?,
            )?;
            let integrity = rentemester_application::periods::verify_sources(tx, guard);
            Ok(rentemester_storage::tasks::sources::observe(
                tx,
                rentemester_storage::tasks::sources::ReadContext {
                    slug: request.slug,
                    as_of: request.as_of,
                    now: &stamp,
                    today: &today,
                    latest,
                    integrity,
                },
                request.existing,
                request.discover,
                |plan| rentemester_application::bookkeeping_batch::verify_plan(tx, guard, plan),
            ))
        });
        match result {
            Ok(v) => Ok(v),
            Err(e) if self.poisoned || e.code == "connection_refused" => Err(unavailable()),
            Err(_) => {
                // An unavailable ledger does not imply completion or erase previously known work.
                let mut out = vec![];
                for task in request.existing {
                    if let Some(source) = &task.source {
                        let mut source = source.clone();
                        if source.state != "unknown" {
                            source.content_hash = rentemester_engine::tasks::payload_hash(
                                &serde_json::json!({"previousContent":source.content_hash,"state":"unknown"}),
                            )?;
                        }
                        source.state = "unknown".into();
                        out.push(Observation{draft:serde_json::json!({"title":task.title,"scope":task.scope,"source":source,"verificationRequired":true}),check:SourceCheck{state:"unknown".into(),observed_at:stamp.clone(),evidence:vec![]}});
                    }
                }
                Ok((out, true))
            }
        }
    }
    fn check(
        &mut self,
        control: &Transaction<'_>,
        slug: &str,
        task: &Task,
        now: i64,
    ) -> Result<SourceCheck, Error> {
        let stamp = rentemester_engine::company_knowledge::timestamp(now)?;
        let result = self.read(control, slug, |tx, guard| {
            let integrity = rentemester_application::periods::verify_sources(tx, guard);
            rentemester_storage::tasks::sources::check(tx, slug, task, &stamp, integrity)
        });
        match result {
            Ok(v) => Ok(v),
            Err(e) if self.poisoned || e.code == "connection_refused" => Err(unavailable()),
            Err(_) => Ok(SourceCheck {
                state: "unknown".into(),
                observed_at: stamp,
                evidence: vec![],
            }),
        }
    }
}
impl Owner {
    pub fn tasks(
        &mut self,
        clock: &dyn Clock,
        authority: Authority<'_>,
        action: Action<'_>,
        key: Option<&str>,
        expected: Option<u64>,
    ) -> AuthAttempt<serde_json::Value> {
        let catalog = self.catalog()?;
        self.admit_auth(&catalog.workspace)?;
        let slots = self.held.0.as_mut().unwrap();
        let mut reader = Sources {
            root: &self.root,
            original: self.slug.clone(),
            selected: &mut self.slug,
            company: &mut slots.company,
            guard: &mut slots.guard,
            poisoned: false,
        };
        let outcome = tasks::execute_with_sources(
            slots.control.as_mut().unwrap(),
            &catalog,
            clock,
            authority,
            action,
            key,
            expected,
            Some(&mut reader),
        );
        let reusable = !reader.poisoned
            && outcome
                .as_ref()
                .map_or_else(|f| f.connection_reusable, |_| true);
        // All financial reads restore before the application can commit; post-observation retirement
        // must preserve an already committed outcome rather than manufacture a retry signal.
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
}

#[cfg(test)]
mod tests;
