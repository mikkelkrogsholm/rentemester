//! Explicit directional registration. Authorization response is private and never Debug/Serialize.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Outbound,
    Inbound,
}
impl Direction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Outbound => "outbound",
            Self::Inbound => "inbound",
        }
    }
}
pub struct Authorization {
    constraint: Option<String>,
}
impl Authorization {
    pub fn permits(&self, key: &str) -> bool {
        self.constraint.as_deref().is_none_or(|v| v == key)
    }
    pub fn permits_new_company(&self) -> bool {
        self.constraint.is_none()
    }
}
impl Client {
    pub fn validate_auth(&self) -> Result<Authorization, Error> {
        let (status, bytes) = self.request(self.url(&["validate-auth"])?, None)?;
        let bytes = Zeroizing::new(bytes);
        if status != 200 {
            return Err(Error::Refused { status });
        }
        // Do not retain the response or its secret fields in a public result.
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Auth {
            api_license_key: String,
            signature_secret: String,
            test_gln_number: String,
            company_key_constraint: Option<String>,
            label: String,
        }
        // An empty success is not authorization evidence; the source response
        // explicitly carries a nullable applicable-company constraint.
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        if !value
            .as_object()
            .is_some_and(|v| v.contains_key("companyKeyConstraint"))
        {
            return Err(Error::Protocol);
        }
        drop(value);
        let v: Auth = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        let key = Zeroizing::new(v.api_license_key);
        let signature = Zeroizing::new(v.signature_secret);
        if key.as_str() != self.secret.as_str()
            || !text(&signature, 4096)
            || v.test_gln_number.len() != 13
            || !v.test_gln_number.bytes().all(|b| b.is_ascii_digit())
            || !text(&v.label, 4096)
            || v.company_key_constraint
                .as_ref()
                .is_some_and(|s| !company_key(s) || s.contains(self.secret.as_str()))
        {
            return Err(Error::Protocol);
        }
        Ok(Authorization {
            constraint: v.company_key_constraint,
        })
    }
    pub fn register_direction(
        &self,
        key: &str,
        cvr_id: &str,
        network: Network,
        direction: Direction,
    ) -> Result<ParticipantRegistration, Error> {
        if !company_key(key) || !cvr(cvr_id) {
            return Err(Error::InvalidInput);
        }
        let body = serde_json::to_vec(&json!({"direction":direction.name(),"participantType":"DK:CVR","participantId":cvr_id,"companyKey":key,"webhookUrl":null,"documentProfiles":format!("default-{}",network.name())})).map_err(|_| Error::InvalidInput)?;
        let (status, bytes) = self.request(
            self.url(&["v2", "register-participant", network.name()])?,
            Some(("application/json", body)),
        )?;
        if status != 200 {
            return Err(Error::Refused { status });
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        if v["registeredOnNetwork"] != true || v["webhookRegistered"] != false {
            return Err(Error::Protocol);
        }
        Ok(ParticipantRegistration {
            registered_on_network: true,
            webhook_registered: false,
        })
    }
}
