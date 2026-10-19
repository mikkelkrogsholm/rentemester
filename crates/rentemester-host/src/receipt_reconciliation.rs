//! Typed restore receipt management through the continuously admitted Owner only.
use super::*;
impl Owner {
    pub fn restored_receipts(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.receipt_reconciliation_action(
            cookie,
            clock,
            rentemester_application::receipt_reconciliation::Action::Inventory,
        )
    }
    pub fn reconcile_restored_receipt(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: &rentemester_storage::receipt_reconciliation::Reconcile,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.receipt_reconciliation_action(
            cookie,
            clock,
            rentemester_application::receipt_reconciliation::Action::Reconcile(input),
        )
    }
    fn receipt_reconciliation_action(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::receipt_reconciliation::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::receipt_reconciliation::execute(
                c,
                l,
                cookie,
                clock,
                context.selection,
                action,
            )
        })
    }
}
