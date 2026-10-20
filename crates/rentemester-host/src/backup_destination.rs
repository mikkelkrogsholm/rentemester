use super::*;
impl Owner {
    pub fn backup_destination(
        &mut self,
        runtime: &rentemester_application::backup_destination::Runtime<'_>,
        proof: &VerifiedCookie,
        action: rentemester_application::backup_destination::Action<'_>,
    ) -> AuthAttempt<serde_json::Value> {
        let catalog = self.catalog()?;
        self.control_auth(runtime.cookies.workspace(), |control| {
            rentemester_application::backup_destination::execute(
                control, proof, &catalog, runtime, action,
            )
        })
    }
}
