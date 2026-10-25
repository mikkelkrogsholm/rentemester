use super::*;
impl Owner {
    pub fn original717_efaktura_konfigurer(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        body: &serde_json::Value,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        let path = self
            .root
            .join(&self.slug)
            .join("config/digisense.json")
            .to_string_lossy()
            .into_owned();
        self.document_action(cookie, clock, |c, l, g, context| {
            app::original717::configure(c, l, g, context, body).map(|mut c| {
                if c.value["ok"] == true {
                    c.value["configPath"] = serde_json::json!(path);
                }
                c
            })
        })
    }
    pub fn original717_digisense_secret(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
    ) -> Result<
        Attempt<
            Result<
                Committed<Option<(Vec<u8>, String)>>,
                rentemester_application::control_fence::Failure,
            >,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::original717::secret(c, l, g, context)
        })
    }
}

impl Owner {
    pub fn original717_onboarding_status(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        fallback: Option<&str>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            app::original717::onboarding(c, l, g, context, fallback)
        })
    }
    pub fn original717_registration_result(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        environment: &str,
        network: rentemester_runtime::public_einvoice::Network,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _g, context| {
            app::original717::registration_result(c, l, context, environment, network)
        })
    }
}
