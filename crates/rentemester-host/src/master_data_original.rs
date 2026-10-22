//! Source actions use the same admitted owner and typed principal fences.
use super::*;
use rentemester_application::master_data_original::{self as app, Output, Request};
impl Owner {
    pub fn master_data_original(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        request: Request<'_>,
    ) -> Result<
        Attempt<Result<Committed<Output>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |control, company, _, context| {
            app::execute(control, company, context, request)
        })
    }
    pub fn master_data_original_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        request: Request<'_>,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult<Output>, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().ok_or(OwnerError::Retired)?;
        let outcome = app::execute_service(
            slots.control.as_mut().ok_or(OwnerError::Retired)?,
            slots.company.as_mut().ok_or(OwnerError::Retired)?,
            service_company::Context {
                key,
                clock,
                max_future_days: 45,
                selection: CompanySelection {
                    workspace: &self.workspace,
                    slug: &self.slug,
                    registered: true,
                    archived: false,
                },
            },
            request,
        );
        let reusable = outcome.as_ref().map_or_else(
            |f| f.control_reusable && f.company_reusable,
            |v| v.control_reusable,
        );
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
    use serde_json::json;
    struct Fixed(i64);
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(self.0)
        }
    }
    const NOW: i64 = 1767225600000;
    fn setup() -> (
        tempfile::TempDir,
        Owner,
        CookieVerifier,
        VerifiedCookie,
        u32,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap().join("workspace");
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
            &root,
            uid,
            "w",
            "a",
            "Synthetic A",
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
        (dir, owner, cookies, proof, uid)
    }

    #[test]
    fn master_data_original_service_lifecycle_current_grants_and_disabled_key() {
        use rentemester_application::{
            service_lifecycle::Operation,
            workspace_access::{Action, Runtime},
        };
        use rentemester_runtime::invitation::InvitationKey;
        use rentemester_storage::master_data_original::{Create, Kind};
        let (_dir, mut owner, cookies, proof, _uid) = setup();
        let created = owner
            .service_lifecycle(
                &cookies,
                &Fixed(NOW),
                &proof,
                &Operation::Create {
                    display_name: "Synthetic source service".into(),
                    operation_id: None,
                    expires_in_seconds: None,
                },
            )
            .unwrap()
            .outcome
            .unwrap();
        let created: serde_json::Value = serde_json::from_str(created.as_json()).unwrap();
        let subject = created["serviceAccountId"].as_str().unwrap();
        let key = ServiceKey::new("w", created["secret"].as_str().unwrap()).unwrap();
        let catalog = owner.catalog().unwrap();
        let invitation =
            InvitationKey::new("w", "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE").unwrap();
        let runtime = Runtime {
            cookies: &cookies,
            clock: &Fixed(NOW),
            key: &invitation,
            catalog: &catalog,
        };
        for action in [
            Action::Access {
                user: subject,
                role: Some("member"),
            },
            Action::Company {
                user: subject,
                slug: "a",
                role: Some("reader"),
            },
        ] {
            owner
                .workspace_access(&runtime, &proof, action, &mut |_, _| Ok(()))
                .unwrap()
                .outcome
                .unwrap();
        }
        assert!(
            owner
                .master_data_original_service(
                    &key,
                    &Fixed(NOW),
                    Request::List(Kind::Customer, false)
                )
                .unwrap()
                .outcome
                .is_ok()
        );
        let input: Create =
            serde_json::from_value(json!({"input":{"name":"Synthetic supplier"},"confirm":true}))
                .unwrap();
        assert!(
            owner
                .master_data_original_service(
                    &key,
                    &Fixed(NOW),
                    Request::Create(Kind::Vendor, &input)
                )
                .unwrap()
                .outcome
                .is_err()
        );
        owner
            .workspace_access(
                &runtime,
                &proof,
                Action::Company {
                    user: subject,
                    slug: "a",
                    role: Some("bookkeeper"),
                },
                &mut |_, _| Ok(()),
            )
            .unwrap()
            .outcome
            .unwrap();
        owner
            .master_data_original_service(&key, &Fixed(NOW), Request::Create(Kind::Vendor, &input))
            .unwrap()
            .outcome
            .unwrap();
        let admission = match owner
            .master_data_original_service(&key, &Fixed(NOW), Request::PrepareVat("DK12345678"))
            .unwrap()
            .outcome
            .unwrap()
            .result
        {
            Output::Admission(a) => a,
            _ => panic!("admission required"),
        };
        owner
            .workspace_access(
                &runtime,
                &proof,
                Action::Company {
                    user: subject,
                    slug: "a",
                    role: None,
                },
                &mut |_, _| Ok(()),
            )
            .unwrap()
            .outcome
            .unwrap();
        assert!(
            owner
                .master_data_original_service(
                    &key,
                    &Fixed(NOW),
                    Request::Finish(admission, &json!({"normalized":"DK12345678","valid":true}))
                )
                .unwrap()
                .outcome
                .is_err()
        );
        assert!(
            owner
                .master_data_original_service(&key, &Fixed(NOW), Request::List(Kind::Vendor, false))
                .unwrap()
                .outcome
                .is_err()
        );
        owner
            .workspace_access(
                &runtime,
                &proof,
                Action::Company {
                    user: subject,
                    slug: "a",
                    role: Some("bookkeeper"),
                },
                &mut |_, _| Ok(()),
            )
            .unwrap()
            .outcome
            .unwrap();
        owner
            .service_lifecycle(
                &cookies,
                &Fixed(NOW),
                &proof,
                &Operation::Revoke {
                    subject: subject.into(),
                    credential: created["credentialId"].as_str().unwrap().into(),
                    operation_id: None,
                },
            )
            .unwrap()
            .outcome
            .unwrap();
        assert!(
            owner
                .master_data_original_service(&key, &Fixed(NOW), Request::List(Kind::Vendor, false))
                .unwrap()
                .outcome
                .is_err()
        );
        assert!(
            owner
                .master_data_original_service(&key, &Fixed(NOW), Request::PrepareVat("DK12345678"))
                .unwrap()
                .outcome
                .is_err()
        );
        let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
        assert_eq!(
            db.query_row::<i64, _, _>(
                "SELECT count(*) FROM audit_log WHERE event_type='vendor_create' AND actor=?1",
                [format!("service_account:{subject}")],
                |r| r.get(0)
            )
            .unwrap(),
            1
        );
    }
}
