//! Original group structure read under the ordinary authoritative Owner.
use super::*;
use serde_json::Value;
impl Owner {
    pub fn group_overview_original716(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        as_of: &str,
    ) -> AuthAttempt<Value> {
        let catalog = self.catalog()?;
        // Only catalog-derived paths. No SQLite handle is opened/closed for these
        // physical observations; closing an alias could disturb live POSIX locks.
        let root = self.root.clone();
        self.control_auth(cookies.workspace(), |control| {
            rentemester_application::original716::group_overview(
                control, cookies, clock, proof, &catalog, as_of,
                |slug| {
                    let mut path = root.clone();
                    for (part, directory) in [(slug, true), ("data", true), ("ledger.sqlite", false)] {
                        path.push(part);
                        let Ok(meta) = std::fs::symlink_metadata(&path) else { return false; };
                        if meta.file_type().is_symlink() || (directory && !meta.is_dir())
                            || (!directory && (!meta.is_file() || meta.len() == 0)) { return false; }
                    }
                    true
                },
            )
        })
    }
}

pub mod bootstrap;
pub mod custody;
pub mod report;
pub mod schema;

pub mod transfer;
