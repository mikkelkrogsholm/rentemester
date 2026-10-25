//! Exclusive stopped-server schema operator. No request/path authority.
use crate::{Attempt, Owner, OwnerError};
use rentemester_runtime::bootstrap::Operator;
use serde_json::Value;
impl Owner {
    /// Physical admission only; namespace lease rejects an active server owner.
    /// Policy is resolved by the private server CLI, not by a caller actor flag.
    pub fn original716_schema(
        &mut self,
        operator: &Operator,
        repair: bool,
        apply: bool,
        reason: Option<&str>,
    ) -> Result<Attempt<Result<Value, rentemester_engine::Error>>, OwnerError> {
        if operator.workspace() != self.workspace {
            return Err(OwnerError::Context);
        }
        self.admit()?;
        let db = self
            .held
            .0
            .as_mut()
            .expect("live owner")
            .company
            .as_mut()
            .unwrap();
        let outcome = if repair {
            rentemester_storage::original716::schema::repair_views(
                db,
                apply,
                reason,
                operator.actor(),
            )
        } else {
            rentemester_storage::original716::schema::migrate(db, apply, operator.actor())
        };
        // A committed result is retained even if the subsequent observation retires the owner.
        let retired = self.finish(outcome.is_ok());
        Ok(Attempt { outcome, retired })
    }
}
