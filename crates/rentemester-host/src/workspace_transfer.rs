//! Fixed durable handoff methods under the continuously held Owner and namespace lease.
use super::*;
use rentemester_application::{
    password_login::{Failure, FailureStage},
    workspace_transfer as app,
};
use rentemester_runtime::{
    auth::CookieVerifier,
    backup_artifact::{SigningKey, TrustedPublicKey},
};
use rentemester_storage::workspace_transfer as store;
use rusqlite::TransactionBehavior;
use serde_json::{Value, json};
fn failure(error: rentemester_engine::Error, connection_reusable: bool) -> Failure {
    Failure {
        stage: FailureStage::NotCommitted,
        error,
        connection_reusable,
    }
}
impl Owner {
    fn transfer_inventory(
        &mut self,
    ) -> Result<
        (
            rentemester_storage::workspace_context::Catalog,
            Vec<store::Company>,
            Vec<Value>,
        ),
        OwnerError,
    > {
        let catalog = self.catalog()?;
        let selected = self.slug.clone();
        let mut companies = vec![];
        let mut pending = vec![];
        for c in &catalog.companies {
            self.select_company(&c.slug)?;
            self.admit()?;
            let db = self.held.0.as_ref().unwrap().company.as_ref().unwrap();
            let uuid = db
                .query_row(
                    "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
                    [],
                    |r| r.get(0),
                )
                .map_err(|_| OwnerError::Database)?;
            companies.push(store::Company {
                slug: c.slug.clone(),
                ledger_uuid: uuid,
            });
            let rows =
                rentemester_storage::invoice_email::rows(db).map_err(|_| OwnerError::Database)?;
            for r in rows {
                if r["state"] == "unknown" {
                    pending.push(
                        json!({"companySlug":c.slug,"messageId":r["messageId"],"state":"unknown"}),
                    );
                }
            }
            self.finish(true);
        }
        self.select_company(&selected)?;
        Ok((catalog, companies, pending))
    }
    fn transfer_action(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        action: app::Action<'_>,
    ) -> AuthAttempt<Value> {
        let (catalog, companies, pending) = self.transfer_inventory()?;
        self.control_auth(cookies.workspace(), |c| {
            let mut v = app::execute(c, cookies, clock, proof, &catalog, &companies, action)?;
            let full = v.as_object_mut().unwrap().remove("_full");
            if full != Some(json!(false)) {
                v["pendingEffects"] = json!(pending);
            }
            Ok(v)
        })
    }
    pub fn workspace_transfer_status(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
    ) -> AuthAttempt<Value> {
        self.transfer_action(cookies, clock, proof, app::Action::Status)
    }
    pub fn freeze_workspace(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        i: &store::Freeze,
    ) -> AuthAttempt<Value> {
        self.transfer_action(cookies, clock, proof, app::Action::Freeze(i))
    }
    pub fn cancel_workspace_transfer(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        i: &store::Cancel,
    ) -> AuthAttempt<Value> {
        self.transfer_action(cookies, clock, proof, app::Action::Cancel(i))
    }
    pub fn release_workspace_transfer(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        i: &store::Release,
        key: &SigningKey,
    ) -> AuthAttempt<Value> {
        self.transfer_action(cookies, clock, proof, app::Action::Release(i, key))
    }
    pub fn activate_workspace_transfer(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        i: &store::Activate,
        key: &TrustedPublicKey,
    ) -> AuthAttempt<Value> {
        self.transfer_action(cookies, clock, proof, app::Action::Activate(i, key))
    }
    pub fn export_workspace_transfer(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        i: &store::Export,
        key: &SigningKey,
    ) -> AuthAttempt<Vec<u8>> {
        let (catalog, _, _) = self.transfer_inventory()?;
        let checked = self.control_auth(cookies.workspace(), |db| {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|_| failure(store::refused(), false))?;
            let result = (|| {
                app::authorize_fresh(&tx, cookies, clock, proof, &catalog)?;
                let s = store::current(&tx)?;
                s.reviewed(&i.expected_state_hash, i.confirm)?;
                if s.mode != "frozen" {
                    return Err(store::refused());
                }
                Ok(())
            })();
            let clean = tx.rollback().is_ok();
            result.map_err(|e| failure(e, clean))
        })?;
        if let Err(f) = checked.outcome {
            return Ok(Attempt {
                outcome: Err(f),
                retired: checked.retired,
            });
        }
        let snapshot = self.backup_workspace(cookies, clock, proof, key)?;
        let bytes = match snapshot.outcome {
            Ok(b) if !snapshot.retired => b,
            Ok(_) => return Err(OwnerError::Retired),
            Err(f) => {
                return Ok(Attempt {
                    outcome: Err(f),
                    retired: snapshot.retired,
                });
            }
        };
        let hash = store::digest(&bytes);
        self.control_auth(cookies.workspace(), |db| {
            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|_| failure(store::refused(), false))?;
            let result = (|| {
                app::authorize_fresh(&tx, cookies, clock, proof, &catalog)?;
                let s = store::current(&tx)?;
                s.reviewed(&i.expected_state_hash, i.confirm)?;
                let now = clock.now_unix_ms().map_err(|_| store::refused())?;
                let current = rentemester_storage::cookie_session::resolve(
                    &tx,
                    proof.token(),
                    now,
                    rentemester_storage::cookie_session::SessionRequirement::Fresh,
                )?;
                store::append(
                    &tx,
                    store::Fact::Offered {
                        archive_sha256: hash,
                    },
                    &format!("user:{}", current.subject),
                    &rentemester_engine::company_knowledge::timestamp(now)?,
                )?;
                Ok(())
            })();
            if let Err(e) = result {
                let clean = tx.rollback().is_ok();
                return Err(failure(e, clean));
            }
            tx.commit().map_err(|_| Failure {
                stage: FailureStage::OutcomeUnknown,
                error: store::refused(),
                connection_reusable: false,
            })?;
            Ok(bytes)
        })
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests;
