use super::*;
impl Owner {
    pub fn original717_invoice_send_email(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        body: &Value,
    ) -> Result<
        Attempt<Result<Committed<Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::original717::send_original717(c, l, g, context, body)
        })
    }
}
