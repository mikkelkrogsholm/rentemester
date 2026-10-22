//! Original purchase operations on the admitted Owner namespace.
use super::*;
use rentemester_application::expense_original::{self as app, Admission, Input, ProviderResult};
impl Owner {
    pub fn original_expense_read(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        id: i64,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::read(control, company, guard, context, id)
        })
    }
    pub fn original_expense_prepare(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        id: i64,
        expected: Option<&str>,
    ) -> Result<
        Attempt<Result<Committed<Admission>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::prepare(control, company, guard, context, id, expected)
        })
    }
    pub fn original_expense_finish(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        admission: Admission,
        observation: Option<ProviderResult>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::finish(control, company, guard, context, admission, observation)
        })
    }
    pub fn original_expense_book(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: &Input,
        expected: Option<&str>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, guard, context| {
            app::book(control, company, guard, context, input, expected)
        })
    }
    pub fn original_expense_read_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        id: i64,
    ) -> Result<
        Attempt<
            Result<
                service_company::ServiceResult<serde_json::Value>,
                service_company::ServiceFailure,
            >,
        >,
        OwnerError,
    > {
        self.service_document_action(key, clock, |control, company, guard, context| {
            app::read_service(control, company, guard, context, id)
        })
    }
    pub fn original_expense_prepare_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        id: i64,
        expected: Option<&str>,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<Admission>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.service_document_action(key, clock, |control, company, guard, context| {
            app::prepare_service(control, company, guard, context, id, expected)
        })
    }
    pub fn original_expense_finish_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        admission: Admission,
        observation: Option<ProviderResult>,
    ) -> Result<
        Attempt<
            Result<
                service_company::ServiceResult<serde_json::Value>,
                service_company::ServiceFailure,
            >,
        >,
        OwnerError,
    > {
        self.service_document_action(key, clock, |control, company, guard, context| {
            app::finish_service(control, company, guard, context, admission, observation)
        })
    }
    pub fn original_expense_book_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        input: &Input,
        expected: Option<&str>,
    ) -> Result<
        Attempt<
            Result<
                service_company::ServiceResult<serde_json::Value>,
                service_company::ServiceFailure,
            >,
        >,
        OwnerError,
    > {
        self.service_document_action(key, clock, |control, company, guard, context| {
            app::book_service(control, company, guard, context, input, expected)
        })
    }
}
