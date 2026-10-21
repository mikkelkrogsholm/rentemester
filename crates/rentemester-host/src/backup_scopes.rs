//! Company policy observations use protected handles under the caller's control writer.
use super::*;
use rentemester_application::{
    backup_status::CompanyScopes,
    password_login::{Failure, FailureStage},
};
use rentemester_engine::Error;
use rusqlite::{Transaction, TransactionBehavior};

fn unavailable() -> Error {
    Error::new(
        "backup_status_unavailable",
        "Affected company backup policy could not be observed safely",
    )
}
struct Scopes<'a> {
    root: &'a Path,
    workspace: &'a str,
    original: String,
    selected: &'a mut String,
    company: &'a mut Option<Connection>,
    guard: &'a mut NamespaceGuard,
    poisoned: bool,
}
impl Scopes<'_> {
    fn restore(&mut self) -> Result<(), Error> {
        portable_backup::switch(
            self.company,
            self.guard,
            self.root,
            &self.original,
            self.selected,
            false,
        )
    }
}
impl CompanyScopes for Scopes<'_> {
    fn require(
        &mut self,
        control: &Transaction<'_>,
        clock: &dyn Clock,
        slugs: &[String],
    ) -> Result<(), Error> {
        if self.poisoned {
            return Err(unavailable());
        }
        let outcome = (|| {
            for slug in slugs.iter().collect::<std::collections::BTreeSet<_>>() {
                portable_backup::switch(
                    self.company,
                    self.guard,
                    self.root,
                    slug,
                    self.selected,
                    false,
                )?;
                let company = self.company.as_mut().ok_or_else(unavailable)?;
                self.guard
                    .ensure_connections(control, company)
                    .map_err(|_| unavailable())?;
                let tx = company
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(|_| unavailable())?;
                let checked = rentemester_application::backup_status::require_write(
                    control,
                    &tx,
                    self.workspace,
                    slug,
                    clock,
                );
                if tx.rollback().is_err() {
                    self.poisoned = true;
                    return Err(unavailable());
                }
                checked?;
            }
            Ok(())
        })();
        // Even refused observations must restore the admitted active company.
        if self.restore().is_err() {
            self.poisoned = true;
            return Err(unavailable());
        }
        outcome
    }
}
impl Owner {
    pub(super) fn control_company_auth<T>(
        &mut self,
        workspace: &str,
        work: impl FnOnce(&mut Connection, &mut dyn CompanyScopes) -> Result<T, Failure>,
    ) -> AuthAttempt<T> {
        self.admit_auth(workspace)?;
        let slots = self.held.0.as_mut().unwrap();
        let mut scopes = Scopes {
            root: &self.root,
            workspace: &self.workspace,
            original: self.slug.clone(),
            selected: &mut self.slug,
            company: &mut slots.company,
            guard: &mut slots.guard,
            poisoned: false,
        };
        let mut outcome = work(slots.control.as_mut().unwrap(), &mut scopes);
        let restored = scopes.restore().is_ok() && !scopes.poisoned && scopes.company.is_some();
        if !restored {
            // Work may already have committed: never claim a retry is safe.
            outcome = Err(Failure {
                stage: FailureStage::OutcomeUnknown,
                error: unavailable(),
                connection_reusable: false,
            });
        }
        let reusable = restored
            && outcome
                .as_ref()
                .map_or_else(|f| f.connection_reusable, |_| true);
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
}
