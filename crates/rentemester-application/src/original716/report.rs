//! Read-fenced source compliance assembly. Caller metadata never supplies authority.
use crate::{
    access::Permission,
    control_fence::{self, Committed, CookieAuthorization, Failure, SessionRequirement},
    documents::Context,
};
use rentemester_engine::Error;
use rusqlite::Connection;
use serde_json::{Value, json};
pub fn execute(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    input: &Value,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    let workspace = context.selection.workspace;
    let clock = context.clock;
    control_fence::execute_cookie_both(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        Permission::CompanyRead,
        true,
        false,
        |control, tx, subject| {
            if input["expectedActor"]
                .as_str()
                .is_some_and(|a| a != format!("user:{subject}"))
            {
                return Err(Error::new(
                    "access_denied",
                    "Current actor assertion did not match",
                ));
            }
            let now = clock
                .now_unix_ms()
                .map_err(|_| Error::new("clock_unavailable", "Trusted clock unavailable"))?;
            let instant = rentemester_runtime::backup_keys::timestamp(now)
                .map_err(|e| Error::new("clock_unavailable", e))?;
            let date = input["asOf"].as_str().unwrap_or(&instant[..10]);
            let mut data = rentemester_storage::original716::report::read(tx, slug, date)?;
            let gdpr =
                rentemester_runtime::gdpr_audit::original717::seal(&data["gdprAudit"], false, None)
                    .map_err(|e| Error::new("report_invalid", e))?;
            data["gdpr"] = json!({"eventCount":gdpr["events"].as_array().map(Vec::len),"fingerprint":gdpr["fingerprint"]});
            // Same redaction as native compliance: only current owners see backup details.
            let owner =
                rentemester_storage::control_access::observe_company_role(control, subject, slug)?
                    .as_deref()
                    == Some("owner");
            data["backup"] = if owner {
                crate::backup_governance::original717::data(
                    control,
                    tx,
                    workspace,
                    slug,
                    clock,
                    Some(date.into()),
                )?
            } else {
                json!({"available":false,"detailPermissionGranted":false,"reason":"Backup governance details require current owner permission"})
            };
            data["generatedAt"] = json!(input["generatedAt"].as_str().unwrap_or(&instant));
            data["generatedAtDisplay"] =
                json!(rentemester_runtime::original716::report::format_instant(
                    data["generatedAt"].as_str().unwrap_or("")
                ));
            data["commitSha"] = json!(input["commitSha"].as_str().unwrap_or("unknown"));
            let versions = data["coverage"]["bundleVersions"]
                .as_object()
                .ok_or_else(|| Error::new("report_invalid", "Bundled rule versions unavailable"))?;
            let mut versions: Vec<_> = versions.iter().collect();
            versions.sort_by_key(|(k, _)| *k);
            let version = versions
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{k}={}",
                        v.as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v.to_string())
                    )
                })
                .collect::<Vec<_>>()
                .join(";");
            data["ruleBundleVersion"] = json!(version);
            let html = rentemester_engine::compliance_report::original716::render(&data)?;
            Ok(
                json!({"ok":true,"html":html,"asOfDate":date,"generatedAt":data["generatedAt"],"commitSha":data["commitSha"],"ruleBundleVersion":version,"fingerprint":format!("sha256:{}",rentemester_engine::compliance_report::fingerprint(&html)),"bytes":html.encode_utf16().count()}),
            )
        },
    )
}

/// Asserted source privacy/retention operation. Actor is compared under the same lock as effect.
pub fn remaining(
    control: &mut Connection,
    company: &mut Connection,
    context: Context<'_>,
    operation: &str,
    input: &Value,
) -> Result<Committed<Value>, Failure> {
    let slug = context.selection.slug;
    let clock = context.clock;
    let readonly = operation == "retention";
    let permission = match operation {
        "retention" => Permission::CompanyRead,
        "export" => Permission::CompanyExport,
        _ => Permission::CompanyAdmin,
    };
    control_fence::execute_cookie_subject_mode(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        context.selection,
        permission,
        readonly,
        |tx, subject| {
            if input["expectedActor"]
                .as_str()
                .is_some_and(|a| a != format!("user:{subject}"))
            {
                return Err(Error::new(
                    "access_denied",
                    "Current actor assertion did not match",
                ));
            }
            let now = clock
                .now_unix_ms()
                .map_err(|_| Error::new("clock_unavailable", "Trusted clock unavailable"))?;
            let instant = rentemester_runtime::backup_keys::timestamp(now)
                .map_err(|e| Error::new("clock_unavailable", e))?;
            let today = &instant[..10];
            let mut body = input.clone();
            body.as_object_mut()
                .ok_or_else(|| Error::new("invalid_input", "Expected object"))?
                .remove("expectedActor");
            match operation {
                "retention" => {
                    let date = body["asOf"].as_str().unwrap_or(today);
                    if rentemester_engine::fiscal_year::Date::parse(date).is_err() {
                        return Ok(
                            json!({"ok":false,"asOf":date,"rows":[],"appliedRules":["DK-BOOKKEEPING-RETENTION-001"],"errors":["asOf must be YYYY-MM-DD"]}),
                        );
                    }
                    let mut report =
                        rentemester_storage::retention::read(tx, slug, date)?["report"].clone();
                    for r in report["rows"].as_array_mut().into_iter().flatten() {
                        r.as_object_mut().unwrap().remove("unknown");
                    }
                    Ok(report)
                }
                "export" => {
                    let i = serde_json::from_value(body)
                        .map_err(|_| Error::new("invalid_input", "Invalid GDPR export"))?;
                    rentemester_storage::gdpr::original717::subject(
                        tx, slug, &i, today, subject, false,
                    )
                }
                "erase" => {
                    if body.get("asOf").is_some() {
                        return Err(Error::new("invalid_input", "Erasure does not accept asOf"));
                    }
                    let i = serde_json::from_value(body)
                        .map_err(|_| Error::new("invalid_input", "Invalid GDPR erase"))?;
                    rentemester_storage::gdpr::erase(tx, &i, today, subject)
                }
                _ => Err(Error::new("invalid_input", "Unknown operation")),
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_fence::CompanySelection;
    use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1_767_225_600_000)
        }
    }
    #[test]
    fn original716_report_actual_read_fence_hash_metadata_and_actor() {
        // Exercise the real connection fence: in-memory journal mode is deliberately
        // not an admitted Owner connection.
        let dir = tempfile::tempdir().unwrap();
        let mut c = Connection::open(dir.path().join("control.sqlite")).unwrap();
        let mut l = Connection::open(dir.path().join("company.sqlite")).unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic <Report>")
            .unwrap();
        c.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled)VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId)VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','agent:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','agent:synthetic');").unwrap();
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../rentemester-runtime/fixtures/session-cookie.json"
        ))
        .unwrap();
        let cookie =
            CookieVerifier::new("w", CookieProfile::Secure, fixture["key"].as_str().unwrap())
                .unwrap()
                .verify(fixture["vectors"][0]["header"].as_str(), None)
                .unwrap();
        let context = || Context {
            cookie: &cookie,
            clock: &Fixed,
            selection: CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
            },
        };
        let before = l.total_changes();
        let cbefore = c.total_changes();
        let input = json!({"asOf":"2026-01-01","generatedAt":"2025-01-02T03:04:05.000Z","commitSha":"123abcd","expectedActor":"user:u"});
        let report = execute(&mut c, &mut l, context(), &input).unwrap().value;
        let html = report["html"].as_str().unwrap();
        assert!(html.contains("Synthetic &lt;Report&gt;"));
        assert!(html.contains("2025-01-02 03:04 UTC"));
        assert!(html.contains("123abcd"));
        assert!(html.contains("current owner permission"));
        assert!(!html.contains("${"));
        assert_eq!(
            report["fingerprint"],
            format!(
                "sha256:{}",
                rentemester_engine::compliance_report::fingerprint(html)
            )
        );
        assert_eq!(report["bytes"], html.encode_utf16().count());
        assert_eq!(report["asOfDate"], "2026-01-01");
        let mut forged = input.clone();
        forged["expectedActor"] = json!("user:forged");
        assert!(execute(&mut c, &mut l, context(), &forged).is_err());
        assert!(
            remaining(
                &mut c,
                &mut l,
                context(),
                "erase",
                &json!({"confirm":true,"name":"Synthetic","expectedActor":"user:u"})
            )
            .is_err()
        );
        assert_eq!(l.total_changes(), before);
        assert_eq!(c.total_changes(), cbefore);
        // Execute the actual common erasure handler as current owner: retained material survives.
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','agent:synthetic');").unwrap();
        l.execute_batch("INSERT INTO customers(id,name,vat_or_cvr)VALUES(1,'Synthetic Retained','SYN-RETAINED');INSERT INTO documents(id,source,document_type,sender_name,sender_vat_cvr,invoice_date,sha256_hash)VALUES(1,'synthetic','cash_register_receipt','Synthetic Retained','SYN-RETAINED','2026-01-01','synthetic-document-hash');").unwrap();
        let refused = remaining(
            &mut c,
            &mut l,
            context(),
            "erase",
            &json!({"confirm":true,"cvr":"SYN-RETAINED","expectedActor":"user:u"}),
        )
        .unwrap()
        .value;
        assert_eq!(refused["ok"], true);
        assert!(refused["refusedCount"].as_u64().unwrap() > 0);
        assert!(
            refused["refused"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["source"] == "documents")
        );
        let preserved: String = l
            .query_row("SELECT sender_name FROM documents WHERE id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(preserved, "Synthetic Retained");
    }
}
