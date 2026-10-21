use super::*;
impl Owner {
    pub fn corporate_records(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        clock: &dyn rentemester_runtime::auth::Clock,
        proof: &VerifiedCookie,
        action: rentemester_application::corporate_records::Action<'_>,
    ) -> AuthAttempt<serde_json::Value> {
        let catalog = self.catalog()?;
        let slug = self.slug.clone();
        self.control_company_auth(cookies.workspace(), |c, backup| {
            rentemester_application::corporate_records::execute_scoped(
                c, cookies, clock, proof, &catalog, &slug, action, backup,
            )
        })
    }
}
