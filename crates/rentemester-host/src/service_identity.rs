//! Fixed service identity operations on the continuously admitted Owner.
use super::*;
use rentemester_application::{service_context, service_lifecycle};
use rentemester_runtime::auth::CookieVerifier;
impl Owner {
    pub fn service_lifecycle(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        operation: &service_lifecycle::Operation,
    ) -> AuthAttempt<service_lifecycle::Receipt> {
        self.control_auth(cookies.workspace(), |control| {
            service_lifecycle::execute(control, cookies, clock, proof, operation)
        })
    }
    pub fn service_context(
        &mut self,
        clock: &dyn Clock,
        key: &ServiceKey,
    ) -> AuthAttempt<rentemester_application::workspace_context::WorkspaceContext> {
        let catalog = self.catalog()?;
        self.control_auth(key.workspace(), |control| {
            service_context::execute(control, key.workspace(), clock, key, &catalog)
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use rentemester_runtime::auth::{AuthError, CookieProfile};
    use service_lifecycle::Operation;
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1767225600000)
        }
    }
    #[test]
    fn owned_service_lifecycle_current_context_rotation_revoke_and_reuse() {
        let dir = tempfile::tempdir().unwrap();
        let uid = String::from_utf8(
            std::process::Command::new("id")
                .arg("-u")
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .parse()
        .unwrap();
        let mut owner = Owner::create_with_chart_profile(
            &dir.path().canonicalize().unwrap().join("workspace"),
            uid,
            "w",
            "a",
            "Synthetic services",
            "2026-01-01T00:00:00.000Z",
            &[],
            NamespaceProfile::LocalUserMacos,
        )
        .unwrap();
        owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','owner-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
        let cookies = CookieVerifier::new(
            "w",
            CookieProfile::Development,
            "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
        )
        .unwrap();
        let cookie = cookies.issue_session("owner-token").unwrap();
        let proof = cookies
            .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
            .unwrap();
        let issued = owner
            .service_lifecycle(
                &cookies,
                &Fixed,
                &proof,
                &Operation::Create {
                    display_name: "Synthetic executor".into(),
                    operation_id: None,
                    expires_in_seconds: None,
                },
            )
            .unwrap()
            .outcome
            .unwrap();
        let issued: serde_json::Value = serde_json::from_str(issued.as_json()).unwrap();
        // Field names and one-time key are the existing identity receipt contract.
        let subject = issued["serviceAccountId"].as_str().unwrap();
        let credential = issued["credentialId"].as_str().unwrap();
        let key = ServiceKey::new("w", issued["secret"].as_str().unwrap()).unwrap();
        assert!(
            owner
                .service_context(&Fixed, &key)
                .unwrap()
                .outcome
                .is_err()
        );
        // Explicit synthetic activation; credential creation itself grants no workspace/company role.
        owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')",[subject]).unwrap();
        let context = owner
            .service_context(&Fixed, &key)
            .unwrap()
            .outcome
            .unwrap();
        let context: serde_json::Value = serde_json::from_str(context.as_json()).unwrap();
        assert_eq!(context["companies"], serde_json::json!([])); // create grants no company access
        let listed = owner
            .service_lifecycle(&cookies, &Fixed, &proof, &Operation::List)
            .unwrap()
            .outcome
            .unwrap();
        assert!(
            !listed
                .as_json()
                .contains(issued["secret"].as_str().unwrap())
        );
        let rotated = owner
            .service_lifecycle(
                &cookies,
                &Fixed,
                &proof,
                &Operation::Rotate {
                    subject: subject.into(),
                    credential: credential.into(),
                    operation_id: None,
                    expires_in_seconds: None,
                },
            )
            .unwrap()
            .outcome
            .unwrap();
        let rotated: serde_json::Value = serde_json::from_str(rotated.as_json()).unwrap();
        assert!(
            owner
                .service_context(&Fixed, &key)
                .unwrap()
                .outcome
                .is_err()
        );
        let next = ServiceKey::new("w", rotated["secret"].as_str().unwrap()).unwrap();
        owner
            .service_context(&Fixed, &next)
            .unwrap()
            .outcome
            .unwrap();
        owner
            .service_lifecycle(
                &cookies,
                &Fixed,
                &proof,
                &Operation::Revoke {
                    subject: subject.into(),
                    credential: rotated["credentialId"].as_str().unwrap().into(),
                    operation_id: None,
                },
            )
            .unwrap()
            .outcome
            .unwrap();
        assert!(
            owner
                .service_context(&Fixed, &next)
                .unwrap()
                .outcome
                .is_err()
        );
        owner
            .service_lifecycle(&cookies, &Fixed, &proof, &Operation::List)
            .unwrap()
            .outcome
            .unwrap();
        let db = owner.held.0.as_ref().unwrap().control.as_ref().unwrap();
        let persisted: String = db
            .query_row("SELECT group_concat(key,' ') FROM apikey", [], |r| {
                r.get::<_, Option<String>>(0)
            })
            .unwrap()
            .unwrap_or_default();
        assert!(!persisted.contains(issued["secret"].as_str().unwrap()));
    }
}
