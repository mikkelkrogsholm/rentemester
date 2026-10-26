//! Fixed control-only task admission; no selected company ledger is opened.
use super::*;
use rentemester_application::tasks::{self, Action, Authority};
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
        let workspace = catalog.workspace.clone();
        self.control_auth(&workspace, |control| {
            tasks::execute(control, &catalog, clock, authority, action, key, expected)
        })
    }
}
