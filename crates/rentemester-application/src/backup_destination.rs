//! Whole-workspace human authority held across bounded local publication/readback.
use crate::password_login::{Failure, FailureStage};
use rentemester_engine::Error;
use rentemester_runtime::{
    auth::{Clock, CookieVerifier, VerifiedCookie},
    backup_artifact::TrustedPublicKey,
    backup_destination::{self as destination, Config},
};
use rentemester_storage::workspace_context::Catalog;
use rusqlite::{Connection, TransactionBehavior, params};
use serde_json::{Value, json};

pub enum Action<'a> {
    List,
    Place {
        destination: &'a str,
        operation: &'a str,
        wire: &'a [u8],
    },
    Verify {
        destination: &'a str,
        operation: &'a str,
    },
}
pub struct Runtime<'a> {
    pub cookies: &'a CookieVerifier,
    pub clock: &'a dyn Clock,
    pub configs: &'a [Config],
    pub uid: u32,
    pub trusted: &'a TrustedPublicKey,
}
pub fn execute(
    control: &mut Connection,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    runtime: &Runtime<'_>,
    action: Action<'_>,
) -> Result<Value, Failure> {
    let Runtime {
        cookies,
        clock,
        configs,
        uid,
        trusted,
    } = runtime;
    let (cookies, clock, configs, uid, trusted) = (*cookies, *clock, *configs, *uid, *trusted);

    let fail = |code: &'static str, unknown: bool, reusable: bool| Failure {
        stage: if unknown {
            FailureStage::OutcomeUnknown
        } else {
            FailureStage::NotCommitted
        },
        error: Error::new(
            code,
            "Backup destination operation refused; reconcile the exact operation ID before retrying any publication.",
        ),
        connection_reusable: reusable,
    };
    crate::control_fence::connection_ready(control)
        .map_err(|_| fail("control_unavailable", false, false))?;
    let tx = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| fail("control_unavailable", false, true))?;
    let mut published = false;
    let outcome = (|| -> Result<Value, Error> {
        crate::portable_backup::authorize(&tx, cookies, clock, proof, catalog)?;
        let now = clock
            .now_unix_ms()
            .map_err(|_| Error::new("clock_unavailable", "backup clock unavailable"))?;
        let current = rentemester_storage::cookie_session::resolve(
            &tx,
            proof.token(),
            now,
            rentemester_storage::cookie_session::SessionRequirement::Current,
        )?;
        let actor = format!("user:{}", current.subject);
        let (id, operation, wire) = match action {
            Action::List => {
                return Ok(
                    json!({"ok":true,"workspaceId":catalog.workspace,"destinations":configs.iter().map(Config::view).collect::<Vec<_>>(),"freshnessEvaluated":false,"automaticBackupLock":false}),
                );
            }
            Action::Place {
                destination,
                operation,
                wire,
            } => (destination, operation, Some(wire)),
            Action::Verify {
                destination,
                operation,
            } => (destination, operation, None),
        };
        let config = configs.iter().find(|c| c.id == id).ok_or_else(|| {
            Error::new(
                "backup_destination_not_found",
                "Unknown configured destination",
            )
        })?;
        let mut receipt = if let Some(wire) = wire {
            let result =
                destination::place(config, uid, operation, wire, trusted, &catalog.workspace);
            // A failed publication can still have landed bytes; never retry it automatically.
            if result
                .as_ref()
                .is_err_and(|code| *code == "backup_destination_publication_unknown")
            {
                published = true;
            }
            let receipt = result.map_err(|code| {
                Error::new(
                    code,
                    "Destination publication requires exact-ID readback on uncertainty",
                )
            })?;
            published = true;
            receipt
        } else {
            destination::verify(config, uid, operation, trusted, &catalog.workspace)
                .map_err(|code| Error::new(code, "Destination readback refused"))?
        };
        if wire.is_some() {
            receipt["placedBy"] = json!(actor);
            receipt["placedAt"] = json!(
                rentemester_runtime::backup_keys::timestamp(now)
                    .map_err(|_| Error::new("clock_unavailable", "backup clock unavailable"))?
            );
            tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor)VALUES('native_backup_placed','native_backup_placement',?1,?2)",params![receipt.to_string(),actor]).map_err(|_|Error::new("backup_destination_audit_unavailable","Placement may exist; reconcile exact operation ID"))?;
            receipt["auditRecorded"] = json!(true);
        } else {
            let record:Option<String>=tx.query_row("SELECT entity_id FROM workspace_audit WHERE event_type='native_backup_placed' AND entity_type='native_backup_placement' AND json_valid(entity_id) AND json_extract(entity_id,'$.destinationId')=?1 AND json_extract(entity_id,'$.operationId')=?2 AND json_extract(entity_id,'$.archiveSha256')=?3 ORDER BY id DESC LIMIT 1",params![id,operation,receipt["archiveSha256"].as_str().unwrap()],|row|row.get(0)).optional().map_err(|_|Error::new("backup_destination_audit_unavailable","Placement audit unavailable"))?;
            receipt["auditRecorded"] = json!(record.is_some());
            if let Some(record) = record {
                let record: Value = serde_json::from_str(&record).map_err(|_| {
                    Error::new(
                        "backup_destination_audit_unavailable",
                        "Invalid placement audit",
                    )
                })?;
                receipt["placedBy"] = record["placedBy"].clone();
                receipt["placedAt"] = record["placedAt"].clone();
            }
        }
        Ok(json!({"ok":true,"placement":receipt}))
    })();
    match outcome {
        Ok(value) => {
            tx.commit()
                .map_err(|_| fail("backup_destination_commit_unknown", published, false))?;
            Ok(value)
        }
        Err(error) => {
            let rollback = tx.rollback().is_ok();
            Err(Failure {
                stage: if published {
                    FailureStage::OutcomeUnknown
                } else {
                    FailureStage::NotCommitted
                },
                error,
                connection_reusable: rollback,
            })
        }
    }
}
use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;
    use rentemester_runtime::auth::{AuthError, CookieProfile};
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1_767_226_200_000)
        }
    }
    #[test]
    fn destination_metadata_needs_current_human_workspace_and_every_company_admin() {
        let directory=tempfile::tempdir().unwrap();
        let mut control = Connection::open(directory.path().join("control.sqlite")).unwrap();
        let mut company = Connection::open(directory.path().join("company.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut company,
            "Synthetic backup destination",
        )
        .unwrap();
        control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','synthetic','u',1);INSERT INTO rm_workspace_mfa_events(user_id,event_type,actor,created_at)VALUES('u','mfa_enabled','system:better-auth','2026-01-01T00:00:00.000Z');INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId)VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:01:00.000Z','2026-01-01T00:01:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','system:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','system:synthetic');").unwrap();
        let cookies = CookieVerifier::new(
            "w",
            CookieProfile::Secure,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        )
        .unwrap();
        let issued = cookies.issue_session("synthetic-token").unwrap();
        let proof = cookies.verify(Some(issued.request_header()), None).unwrap();
        let catalog = Catalog {
            workspace: "w".into(),
            companies: vec![rentemester_storage::workspace_context::RegisteredCompany {
                slug: "a".into(),
                name: "Synthetic".into(),
                archived: false,
            }],
        };
        let trusted = TrustedPublicKey::from_bytes(&[1u8; 32]).unwrap();
        let runtime = Runtime {
            cookies: &cookies,
            clock: &Fixed,
            configs: &[],
            uid: 10001,
            trusted: &trusted,
        };
        let before = control.total_changes();
        let value = execute(&mut control, &proof, &catalog, &runtime, Action::List).unwrap();
        assert_eq!(value["destinations"], json!([]));
        assert_eq!(control.total_changes(), before);
        control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','system:synthetic')").unwrap();
        assert_eq!(
            execute(&mut control, &proof, &catalog, &runtime, Action::List)
                .unwrap_err()
                .error
                .code,
            "access_denied"
        );
        control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','system:synthetic');DELETE FROM session WHERE id='s';").unwrap();
        assert_eq!(
            execute(&mut control, &proof, &catalog, &runtime, Action::List)
                .unwrap_err()
                .error
                .code,
            "invalid_credentials"
        );
    }
}
