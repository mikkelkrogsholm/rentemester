use super::*;
impl Owner {
    pub fn manual_post_recovery(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        acknowledge: Option<i64>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::manual_post_recovery::execute(
                c,
                l,
                cookie,
                clock,
                context.selection,
                acknowledge,
            )
        })
    }
}
