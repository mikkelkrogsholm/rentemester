//! Fixed admitted read-only source collection; no caller paths or source callbacks.
use super::*;
use rentemester_application::{
    cfo_analytics as app,
    password_login::{Failure, FailureStage},
};
use rentemester_engine::{Error, cfo_analytics::Query};
use rentemester_runtime::auth::CookieVerifier;
use rusqlite::TransactionBehavior;
use serde_json::{Value, json};
fn unavailable(_: impl std::fmt::Display) -> Error {
    Error::new("cfo_source_unavailable", "Admitted CFO source unavailable")
}
fn switch(
    company: &mut Option<Connection>,
    guard: &mut NamespaceGuard,
    root: &Path,
    slug: &str,
    current: &mut String,
) -> Result<(), Error> {
    if current == slug && company.is_some() {
        return Ok(());
    }
    if let Some(db) = company.take()
        && let Err((db, e)) = db.close()
    {
        *company = Some(db);
        return Err(unavailable(e));
    }
    guard.select_company(slug).map_err(unavailable)?;
    *company = Some(
        Connection::open_with_flags(
            root.join(slug).join("data/ledger.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(unavailable)?,
    );
    company
        .as_ref()
        .unwrap()
        .execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;PRAGMA trusted_schema=OFF;")
        .map_err(unavailable)?;
    *current = slug.into();
    Ok(())
}

impl Owner {
    pub fn cfo_analytics(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        query: &Query,
    ) -> AuthAttempt<Value> {
        if query.scope == "group" {
            return self.cfo_group(cookies, clock, proof, query);
        }
        let catalog = self.catalog()?;
        self.admit_auth(cookies.workspace())?;
        let selected = self.slug.clone();
        let slots = self.held.0.as_mut().unwrap();
        let tx = slots
            .control
            .as_mut()
            .unwrap()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| OwnerError::Database)?;
        let before = tx.total_changes();
        let outcome = (|| {
            let auth = app::authorize(&tx, cookies, clock, proof, &catalog, query)?;
            let mut sources = vec![];
            for slug in &auth.companies {
                switch(
                    &mut slots.company,
                    &mut slots.guard,
                    &self.root,
                    slug,
                    &mut self.slug,
                )?;
                slots
                    .guard
                    .ensure_connections(&tx, slots.company.as_ref().unwrap())
                    .map_err(unavailable)?;
                let read = slots
                    .company
                    .as_mut()
                    .unwrap()
                    .transaction_with_behavior(TransactionBehavior::Deferred)
                    .map_err(unavailable)?;
                let initial = read.total_changes();
                let source = (|| {
                    if !rentemester_application::periods::verify_sources(&read, &mut slots.guard)?
                        .is_empty()
                    {
                        return Err(unavailable("physical source differs"));
                    }
                    let archives = rentemester_application::archive_import::verify_physical(
                        &read,
                        &mut slots.guard,
                    )?;
                    rentemester_storage::cfo_analytics::collect_with_archives(
                        &read,
                        &tx,
                        slug,
                        query,
                        &auth.visible,
                        &archives,
                    )
                })();
                let unchanged = read.total_changes() == initial;
                read.rollback().map_err(unavailable)?;
                if !unchanged {
                    return Err(unavailable("readonly source changed data"));
                }
                sources.push(source?);
            }
            if app::authorize(&tx, cookies, clock, proof, &catalog, query)? != auth
                || tx.total_changes() != before
            {
                return Err(unavailable("current authority changed"));
            }
            rentemester_engine::cfo_analytics::render(query, &auth.selection(), &sources)
        })();
        let restore = switch(
            &mut slots.company,
            &mut slots.guard,
            &self.root,
            &selected,
            &mut self.slug,
        )
        .and_then(|_| {
            slots
                .guard
                .ensure_connections(&tx, slots.company.as_ref().unwrap())
                .map_err(unavailable)
        });
        let reusable = tx.rollback().is_ok() && restore.is_ok();
        let outcome = match (outcome, restore) {
            (Ok(value), Ok(())) if reusable => Ok(value),
            (result, restored) => Err(Failure {
                stage: FailureStage::NotCommitted,
                error: restored
                    .err()
                    .or_else(|| result.err())
                    .unwrap_or_else(|| unavailable("rollback")),
                connection_reusable: reusable,
            }),
        };
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    fn cfo_group(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        query: &Query,
    ) -> AuthAttempt<Value> {
        let catalog = self.catalog()?;
        let check = self.control_auth(cookies.workspace(), |db| {
            app::current(db, cookies, clock, proof, &catalog, query)
        })?;
        let allowed_companies = match check.outcome {
            Err(f) => {
                return Ok(Attempt {
                    outcome: Err(f),
                    retired: check.retired,
                });
            }
            Ok(auth) if !auth.group_supported => {
                return Ok(Attempt {
                    outcome: Ok(
                        json!({"ok":true,"schemaVersion":"rentemester-cfo-analytics-v1","scope":"group","status":"unsupported","asOf":query.to,"limitations":["No approved profile is available within the requested current company scope; no figures are manufactured."],"group":{"status":"blocked","blockers":["approved profile or requested company scope unavailable"],"consolidatedFigures":null,"sourceSnapshots":[]}}),
                    ),
                    retired: check.retired,
                });
            }
            Ok(auth) => auth.companies.into_iter().collect(),
        };
        let mut report = self.consolidation_bound(
            cookies,
            clock,
            proof,
            None,
            rentemester_application::consolidation::Action::Plan {
                profile_id: query.group_profile_id.as_deref().unwrap_or(""),
                from: &query.from,
                as_of: &query.to,
            },
            Some(&allowed_companies),
        )?;
        if let Ok(group) = report.outcome {
            let ready = group["status"] == "ready";
            report.outcome = Ok(
                json!({"ok":true,"schemaVersion":"rentemester-cfo-analytics-v1","scope":"group","status":if ready{"ready"}else{"unsupported"},"asOf":query.to,"limitations":[if ready{"Group analytics delegates to the approved consolidation profile; it does not infer ownership, FX or eliminations."}else{"The approved consolidated report is blocked; no analytics totals are manufactured."}],"group":group}),
            );
        }
        Ok(report)
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
