use super::*;
impl Owner {
    pub fn ownership(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        clock: &dyn rentemester_runtime::auth::Clock,
        proof: &VerifiedCookie,
        action: rentemester_application::ownership::Action<'_>,
    ) -> AuthAttempt<serde_json::Value> {
        let catalog = self.catalog()?;
        let slug = self.slug.clone();
        self.control_company_auth(cookies.workspace(), |c, backup| {
            rentemester_application::ownership::execute_scoped(
                c, cookies, clock, proof, &catalog, &slug, action, backup,
            )
        })
    }
}
