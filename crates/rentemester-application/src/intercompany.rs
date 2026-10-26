//! Authenticated workspace authority and canonical control transaction. No ledger writes.
use crate::{
    access::{self, CompanyRole, Permission, WorkspaceRole},
    mfa_login::{error, failure},
    password_login::{Failure, FailureStage},
};
use rentemester_engine::{
    Error,
    fiscal_year::Date,
    intercompany::{Manifest, Mapping, refused},
};
use rentemester_runtime::auth::{Clock, CookieVerifier, VerifiedCookie};
use rentemester_storage::{intercompany as store, workspace_context::Catalog};
use rusqlite::{Connection, TransactionBehavior};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Exact {
    pub mapping_id: String,
    pub mapping_hash: String,
}
#[derive(Clone, Copy)]
pub enum Action<'a> {
    Current,
    Reconciliation(&'a str),
    Manifest(&'a Manifest),
    Propose(&'a Mapping),
    Approve(&'a Exact),
    Revoke(&'a Exact),
}
#[allow(clippy::too_many_arguments)]
pub fn execute_scoped(
    db: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    slug: &str,
    action: Action<'_>,
    backup: &mut dyn crate::backup_status::CompanyScopes,
) -> Result<Value, Failure> {
    crate::control_fence::connection_ready(db).map_err(|e| failure(e, false))?;
    if !cookies.accepts_session(proof) || proof.workspace() != catalog.workspace {
        return Err(failure(error("invalid_credentials"), true));
    }
    rentemester_storage::workspace_context::validate_catalog(catalog)
        .map_err(|e| failure(e, true))?;
    let write = !matches!(action, Action::Current | Action::Reconciliation(_));
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| failure(error("control_busy_or_unavailable"), false))?;
    let result = (|| -> Result<Value, Error> {
        if write {
            rentemester_storage::workspace_transfer::require_business_write(&tx)?;
        }
        let now = clock
            .now_unix_ms()
            .map_err(|_| error("invalid_auth_clock"))?;
        let at = rentemester_engine::company_knowledge::timestamp(now)?;
        let current = rentemester_storage::cookie_session::resolve(
            &tx,
            proof.token(),
            now,
            rentemester_storage::cookie_session::SessionRequirement::Current,
        )?;
        rentemester_storage::session_control::authorize(&tx, &current)?;
        let f = rentemester_storage::control_access::observe_human(
            &tx,
            &current.subject,
            &current.session_id,
            slug,
        )?;
        let role: WorkspaceRole = serde_json::from_value(json!(
            f.workspace_role.ok_or_else(|| error("access_denied"))?
        ))
        .map_err(|_| error("invalid_control"))?;
        if !access::workspace_role_allows(
            role,
            if write {
                Permission::WorkspaceManage
            } else {
                Permission::WorkspaceRead
            },
        ) {
            return Err(error("access_denied"));
        }
        let mut visible = BTreeSet::new();
        let mut admins = BTreeSet::new();
        let registered: BTreeSet<_> = catalog.companies.iter().map(|c| c.slug.clone()).collect();
        for c in &catalog.companies {
            let f = rentemester_storage::control_access::observe_human(
                &tx,
                &current.subject,
                &current.session_id,
                &c.slug,
            )?;
            if let Some(r) = f.company_role {
                let r: CompanyRole =
                    serde_json::from_value(json!(r)).map_err(|_| error("invalid_control"))?;
                if access::company_role_allows(r, Permission::CompanyOwnershipRead) {
                    visible.insert(c.slug.clone());
                }
                if !c.archived && access::company_role_allows(r, Permission::CompanyAdmin) {
                    admins.insert(c.slug.clone());
                }
            }
        }
        if !slug.is_empty() && !visible.contains(slug) {
            return Err(error("access_denied"));
        }
        let old = store::current(&tx, &registered)?;
        let actor = format!("user:{}", current.subject);
        let require = |s: &BTreeSet<String>| {
            if s.is_subset(&admins) {
                Ok(())
            } else {
                Err(error("access_denied"))
            }
        };
        if let Action::Manifest(input) = action {
            let m = input.clone().normalize(&registered)?;
            let mut refs = m.companies();
            if let Some((previous, _)) = &old {
                refs.extend(previous.companies());
            }
            require(&refs)?;
            store::states(&tx, &m)?;
            let unchanged = old
                .as_ref()
                .map(|(previous, _)| previous.canonical())
                .transpose()?
                .as_deref()
                == Some(m.canonical()?.as_str());
            if !unchanged {
                backup.require(&tx, clock, &refs.into_iter().collect::<Vec<_>>())?;
            }
            return store::apply_manifest(&tx, &m, old.as_ref().map(|v| v.1.as_str()), &actor, &at);
        }
        let states = match &old {
            Some((m, _)) => store::states(&tx, m)?,
            None => Default::default(),
        };
        match action {
            Action::Current => {
                let current = old
                    .as_ref()
                    .filter(|(m, _)| m.companies().is_subset(&visible));
                let rows: Vec<_> = states
                    .values()
                    .filter(|s| s.mapping.companies().is_subset(&visible))
                    .collect();
                Ok(
                    json!({"manifest":current.map(|v|&v.0),"manifestHash":current.map(|v|&v.1),"mappings":rows,"consolidationAvailable":true}),
                )
            }
            Action::Reconciliation(d) => {
                Date::parse(d)?;
                let rows: Vec<_> = states
                    .values()
                    .filter(|s| {
                        s.status == "approved"
                            && s.mapping.interval.active(d)
                            && s.mapping.companies().is_subset(&visible)
                    })
                    .collect();
                Ok(json!({"scope":"intercompany-reconciliation","asOf":d,"rows":rows}))
            }
            Action::Propose(input) => {
                let m = input
                    .clone()
                    .normalize(&old.as_ref().ok_or_else(refused)?.0)?;
                let mut refs = m.companies();
                if let Some(previous) = states.get(&m.id) {
                    refs.extend(previous.mapping.companies());
                }
                require(&refs)?;
                backup.require(&tx, clock, &refs.into_iter().collect::<Vec<_>>())?;
                store::append(&tx, &states, &m, "proposed", &actor, &at)
            }
            Action::Approve(exact) | Action::Revoke(exact) => {
                let s = states.get(&exact.mapping_id).ok_or_else(refused)?;
                require(&s.mapping.companies())?;
                if s.mapping_hash != exact.mapping_hash {
                    return Err(refused());
                }
                backup.require(
                    &tx,
                    clock,
                    &s.mapping.companies().into_iter().collect::<Vec<_>>(),
                )?;
                store::append(
                    &tx,
                    &states,
                    &s.mapping,
                    if matches!(action, Action::Approve(_)) {
                        "approved"
                    } else {
                        "revoked"
                    },
                    &actor,
                    &at,
                )
            }
            Action::Manifest(_) => Err(refused()),
        }
    })();
    match result {
        Ok(v) => {
            if write {
                tx.commit().map_err(|_| Failure {
                    stage: FailureStage::OutcomeUnknown,
                    error: error("commit_outcome_unknown"),
                    connection_reusable: false,
                })?;
            } else if tx.rollback().is_err() {
                return Err(failure(error("connection_refused"), false));
            }
            Ok(v)
        }
        Err(e) => {
            let reusable = tx.rollback().is_ok();
            Err(failure(e, reusable))
        }
    }
}

#[cfg(test)]
pub fn execute(
    db: &mut Connection,
    cookies: &CookieVerifier,
    clock: &dyn Clock,
    proof: &VerifiedCookie,
    catalog: &Catalog,
    slug: &str,
    action: Action<'_>,
) -> Result<Value, Failure> {
    execute_scoped(
        db,
        cookies,
        clock,
        proof,
        catalog,
        slug,
        action,
        &mut crate::backup_status::UnlockedTestScopes,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{consolidation as consolidation_app, eliminations as elimination_app};
    use rentemester_runtime::auth::{AuthError, CookieProfile};
    use rentemester_storage::workspace_context::RegisteredCompany;
    struct Fixed;
    impl Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(1767225660000)
        }
    }
    #[derive(Default)]
    struct RejectingScopes {
        calls: Vec<Vec<String>>,
    }
    impl crate::backup_status::CompanyScopes for RejectingScopes {
        fn require(
            &mut self,
            _: &rusqlite::Transaction<'_>,
            _: &dyn Clock,
            slugs: &[String],
        ) -> Result<(), Error> {
            self.calls.push(slugs.to_vec());
            Err(error("backup_overdue"))
        }
    }
    fn fixture() -> (
        tempfile::TempDir,
        Connection,
        CookieVerifier,
        VerifiedCookie,
        Catalog,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Connection::open(dir.path().join("control.sqlite")).unwrap();
        let mut l = Connection::open_in_memory().unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic scopes")
            .unwrap();
        c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('factor','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');").unwrap();
        for slug in ["a", "b", "c", "unrelated"] {
            c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u',?1,'grant','owner','user:u')",[slug]).unwrap();
        }
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
            companies: ["a", "b", "c", "unrelated"]
                .into_iter()
                .map(|s| RegisteredCompany {
                    slug: s.into(),
                    name: format!("Synthetic {s}"),
                    purpose: Default::default(), archived: false,
                })
                .collect(),
        };
        (dir, c, cookies, proof, catalog)
    }
    fn manifest() -> Manifest {
        serde_json::from_value(json!({"version":1,"groups":[{"id":"g","name":"Synthetic","memberships":[{"id":"a","companySlug":"a","validFrom":"2026-01-01"},{"id":"b","companySlug":"b","validFrom":"2026-01-01"},{"id":"c","companySlug":"c","validFrom":"2026-01-01"}],"ownership":[]}]})).unwrap()
    }
    fn mapping() -> Mapping {
        serde_json::from_value(json!({"id":"map","groupId":"g","leftCompanySlug":"a","rightCompanySlug":"b","leftAccountNos":["1400"],"rightAccountNos":["2400"],"leftPosition":"receivable","rightPosition":"payable","evidenceRefs":["synthetic"],"validFrom":"2026-01-01"})).unwrap()
    }
    fn seed_manifest(c: &mut Connection) {
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        store::apply_manifest(
            &tx,
            &manifest(),
            None,
            "user:maker",
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap();
        tx.commit().unwrap();
    }
    fn seed_mapping(c: &mut Connection, status: &str) -> String {
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let m = mapping();
        let states = Default::default();
        let v = store::append(
            &tx,
            &states,
            &m,
            "proposed",
            "user:maker",
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap();
        if status != "proposed" {
            let states = store::states(&tx, &manifest()).unwrap();
            store::append(
                &tx,
                &states,
                &m,
                "approved",
                "user:reviewer",
                "2026-01-01T00:00:00.000Z",
            )
            .unwrap();
            if status == "revoked" {
                let states = store::states(&tx, &manifest()).unwrap();
                store::append(
                    &tx,
                    &states,
                    &m,
                    "revoked",
                    "user:reviewer",
                    "2026-01-01T00:00:00.000Z",
                )
                .unwrap();
            }
        }
        tx.commit().unwrap();
        v["mappingHash"].as_str().unwrap().into()
    }
    fn audit(c: &Connection) -> i64 {
        c.query_row("SELECT COUNT(*) FROM workspace_audit", [], |r| r.get(0))
            .unwrap()
    }
    #[test]
    fn manifest_replacement_and_mapping_lifecycle_fence_actual_union_before_audit() {
        let (_dir, mut c, cookies, proof, cat) = fixture();
        let mut backup = RejectingScopes::default();
        let before = audit(&c);
        assert_eq!(
            execute_scoped(
                &mut c,
                &cookies,
                &Fixed,
                &proof,
                &cat,
                "b",
                Action::Manifest(&manifest()),
                &mut backup
            )
            .unwrap_err()
            .error
            .code,
            "backup_overdue"
        );
        assert_eq!(backup.calls, vec![vec!["a", "b", "c"]]);
        assert_eq!(audit(&c), before);
        seed_manifest(&mut c);
        let mut replacement = manifest();
        replacement.groups[0]
            .memberships
            .retain(|m| m.company_slug == "b");
        let before = audit(&c);
        backup.calls.clear();
        assert_eq!(
            execute_scoped(
                &mut c,
                &cookies,
                &Fixed,
                &proof,
                &cat,
                "b",
                Action::Manifest(&replacement),
                &mut backup
            )
            .unwrap_err()
            .error
            .code,
            "backup_overdue"
        );
        assert_eq!(backup.calls, vec![vec!["a", "b", "c"]]);
        assert_eq!(audit(&c), before);
        // Identical manifest is a no-op, not a blocked write.
        backup.calls.clear();
        assert_eq!(
            execute_scoped(
                &mut c,
                &cookies,
                &Fixed,
                &proof,
                &cat,
                "b",
                Action::Manifest(&manifest()),
                &mut backup
            )
            .unwrap()["status"],
            "unchanged"
        );
        assert!(backup.calls.is_empty());
        for status in ["proposed", "approved", "revoked"] {
            let (_dir, mut c, cookies, proof, cat) = fixture();
            seed_manifest(&mut c);
            let hash = seed_mapping(&mut c, status);
            let exact = Exact {
                mapping_id: "map".into(),
                mapping_hash: hash,
            };
            let mut newer = mapping();
            newer.left_company_slug = "c".into();
            let action = match status {
                "proposed" => Action::Approve(&exact),
                "approved" => Action::Revoke(&exact),
                _ => Action::Propose(&newer),
            };
            let mut backup = RejectingScopes::default();
            let before = audit(&c);
            assert_eq!(
                execute_scoped(
                    &mut c,
                    &cookies,
                    &Fixed,
                    &proof,
                    &cat,
                    "b",
                    action,
                    &mut backup
                )
                .unwrap_err()
                .error
                .code,
                "backup_overdue"
            );
            assert_eq!(
                backup.calls,
                vec![if status == "revoked" {
                    vec!["a", "b", "c"]
                } else {
                    vec!["a", "b"]
                }]
            );
            assert_eq!(audit(&c), before);
        }
    }
    fn profile() -> rentemester_engine::consolidation::Profile {
        serde_json::from_value(json!({"version":1,"id":"profile","groupId":"g","currency":"DKK","reportingLines":[{"id":"asset","label":"Asset","section":"asset","displayOrder":1},{"id":"liability","label":"Liability","section":"liability","displayOrder":2},{"id":"equity","label":"Result","section":"equity","displayOrder":3,"role":"current-result"},{"id":"income","label":"Income","section":"income","displayOrder":4},{"id":"expense","label":"Expense","section":"expense","displayOrder":5}],"accountMappings":[{"id":"account","companySlug":"a","accountNo":"1400","reportingLineId":"asset","validFrom":"2026-01-01"}],"evidenceRefs":["synthetic"],"validFrom":"2026-01-01"})).unwrap()
    }
    #[test]
    fn consolidation_all_mutations_fence_group_members_not_just_mapping_or_anchor() {
        for status in ["new", "proposed", "approved", "revoked"] {
            let (_dir, mut c, cookies, proof, cat) = fixture();
            seed_manifest(&mut c);
            let p = profile();
            let mut hash = String::new();
            if status != "new" {
                let tx = c
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .unwrap();
                hash = rentemester_storage::consolidation::propose(
                    &tx,
                    &manifest(),
                    &p,
                    "user:maker",
                    "2026-01-01T00:00:00.000Z",
                )
                .unwrap()["profileHash"]
                    .as_str()
                    .unwrap()
                    .into();
                if status != "proposed" {
                    let states =
                        rentemester_storage::consolidation::states(&tx, &manifest()).unwrap();
                    rentemester_storage::consolidation::approve(
                        &tx,
                        &states["profile"],
                        &hash,
                        "user:reviewer",
                        "2026-01-01T00:00:00.000Z",
                    )
                    .unwrap();
                    if status == "revoked" {
                        let states =
                            rentemester_storage::consolidation::states(&tx, &manifest()).unwrap();
                        rentemester_storage::consolidation::revoke(
                            &tx,
                            &states["profile"],
                            "user:reviewer",
                            "2026-01-01T00:00:00.000Z",
                        )
                        .unwrap();
                    }
                }
                tx.commit().unwrap();
            }
            let exact = consolidation_app::Exact {
                profile_id: "profile".into(),
                profile_hash: hash,
            };
            let id = consolidation_app::Id {
                profile_id: "profile".into(),
            };
            let action = match status {
                "proposed" => consolidation_app::Action::Approve(&exact),
                "approved" => consolidation_app::Action::Revoke(&id),
                _ => consolidation_app::Action::Propose(&p),
            };
            let mut backup = RejectingScopes::default();
            let before = audit(&c);
            assert_eq!(
                consolidation_app::execute_scoped(
                    &mut c,
                    &cookies,
                    &Fixed,
                    &proof,
                    &cat,
                    "b",
                    action,
                    &mut backup
                )
                .unwrap_err()
                .error
                .code,
                "backup_overdue"
            );
            assert_eq!(backup.calls, vec![vec!["a", "b", "c"]]);
            assert_eq!(audit(&c), before);
        }
    }
    #[test]
    fn moving_revoked_profile_between_groups_checks_old_and_new_companies() {
        let (_dir, mut c, cookies, proof, cat) = fixture();
        let mut groups = manifest();
        groups.groups[0]
            .memberships
            .retain(|m| m.company_slug != "c");
        let mut other = groups.groups[0].clone();
        other.id = "other".into();
        other.memberships = vec![rentemester_engine::intercompany::Membership {
            id: "other-c".into(),
            company_slug: "c".into(),
            interval: rentemester_engine::intercompany::Interval {
                valid_from: "2026-01-01".into(),
                valid_to_exclusive: None,
            },
        }];
        groups.groups.push(other);
        let p = profile();
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        store::apply_manifest(&tx, &groups, None, "user:maker", "2026-01-01T00:00:00.000Z")
            .unwrap();
        let hash = rentemester_storage::consolidation::propose(
            &tx,
            &groups,
            &p,
            "user:maker",
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap()["profileHash"]
            .as_str()
            .unwrap()
            .to_owned();
        let states = rentemester_storage::consolidation::states(&tx, &groups).unwrap();
        rentemester_storage::consolidation::approve(
            &tx,
            &states["profile"],
            &hash,
            "user:reviewer",
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap();
        let states = rentemester_storage::consolidation::states(&tx, &groups).unwrap();
        rentemester_storage::consolidation::revoke(
            &tx,
            &states["profile"],
            "user:reviewer",
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap();
        tx.commit().unwrap();
        let mut replacement = p;
        replacement.group_id = "other".into();
        replacement.account_mappings[0].company_slug = "c".into();
        let mut backup = RejectingScopes::default();
        let before = audit(&c);
        assert_eq!(
            consolidation_app::execute_scoped(
                &mut c,
                &cookies,
                &Fixed,
                &proof,
                &cat,
                "c",
                consolidation_app::Action::Propose(&replacement),
                &mut backup
            )
            .unwrap_err()
            .error
            .code,
            "backup_overdue"
        );
        assert_eq!(backup.calls, vec![vec!["a", "b", "c"]]);
        assert_eq!(audit(&c), before);
        backup.calls.clear();
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u')").unwrap();
        assert_eq!(
            consolidation_app::execute_scoped(
                &mut c,
                &cookies,
                &Fixed,
                &proof,
                &cat,
                "c",
                consolidation_app::Action::Propose(&replacement),
                &mut backup
            )
            .unwrap_err()
            .error
            .code,
            "access_denied"
        );
        assert!(backup.calls.is_empty());
        assert_eq!(audit(&c), before);
    }
    fn fresh(hash: &str) -> Value {
        json!({"scope":"intercompany-reconciliation","asOf":"2026-01-01","rows":[{"status":"matched","mappingId":"map","mappingHash":hash,"left":{"companySlug":"a","currency":"DKK","position":"receivable","balance":"12.34","accountNos":["1400"],"sourceSnapshot":{"selectionHash":"b".repeat(64),"ledgerHeadHash":"c".repeat(64),"entryCount":1}},"right":{"companySlug":"b","currency":"DKK","position":"payable","balance":"12.34","accountNos":["2400"],"sourceSnapshot":{"selectionHash":"d".repeat(64),"ledgerHeadHash":"e".repeat(64),"entryCount":1}}}]})
    }
    #[test]
    fn elimination_all_events_fence_both_companies_and_prepare_plan_remain_available() {
        for kind in ["propose", "approve", "reject", "apply", "reverse"] {
            let (_dir, mut c, cookies, proof, cat) = fixture();
            seed_manifest(&mut c);
            let hash = seed_mapping(&mut c, "approved");
            let fresh = fresh(&hash);
            let input:rentemester_engine::eliminations::Input=serde_json::from_value(json!({"id":"elim","mappingId":"map","asOf":"2026-01-01","evidenceRefs":["synthetic"]})).unwrap();
            let payload = rentemester_engine::eliminations::derive(
                &input,
                &mapping(),
                &hash,
                &fresh["rows"][0],
            )
            .unwrap();
            let payload_hash = store::sha(&payload.canonical().unwrap());
            if kind != "propose" {
                let tx = c
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .unwrap();
                rentemester_storage::eliminations::append(
                    &tx,
                    &payload,
                    "proposed",
                    "user:maker",
                    "2026-01-01T00:00:00.000Z",
                )
                .unwrap();
                if ["apply", "reverse"].contains(&kind) {
                    rentemester_storage::eliminations::append(
                        &tx,
                        &payload,
                        "approved",
                        "user:reviewer",
                        "2026-01-01T00:00:00.000Z",
                    )
                    .unwrap();
                }
                if kind == "reverse" {
                    rentemester_storage::eliminations::append(
                        &tx,
                        &payload,
                        "applied",
                        "user:reviewer",
                        "2026-01-01T00:00:00.000Z",
                    )
                    .unwrap();
                }
                tx.commit().unwrap();
            }
            let proposal = elimination_app::Proposal {
                input: input.clone(),
                expected_payload_hash: payload_hash.clone(),
            };
            let exact = elimination_app::Exact {
                elimination_id: "elim".into(),
                payload_hash,
            };
            let action = match kind {
                "propose" => elimination_app::Action::Propose(&proposal),
                "approve" => elimination_app::Action::Approve(&exact),
                "reject" => elimination_app::Action::Reject(&exact),
                "apply" => elimination_app::Action::Apply(&exact),
                _ => elimination_app::Action::Reverse(&exact),
            };
            let mut backup = RejectingScopes::default();
            let before = audit(&c);
            assert_eq!(
                elimination_app::execute_scoped(
                    &mut c,
                    &cookies,
                    &Fixed,
                    &proof,
                    &cat,
                    Some("b"),
                    action,
                    Some(&fresh),
                    &mut backup
                )
                .unwrap_err()
                .error
                .code,
                "backup_overdue"
            );
            assert_eq!(backup.calls, vec![vec!["a", "b"]]);
            assert_eq!(audit(&c), before);
            backup.calls.clear();
            elimination_app::prepare(&mut c, &cookies, &Fixed, &proof, &cat, Some("b"), action)
                .unwrap();
            elimination_app::execute_scoped(
                &mut c,
                &cookies,
                &Fixed,
                &proof,
                &cat,
                Some("b"),
                elimination_app::Action::Plan(&input),
                Some(&fresh),
                &mut backup,
            )
            .unwrap();
            assert!(backup.calls.is_empty());
            assert_eq!(audit(&c), before);
        }
    }
    #[test]
    fn current_full_scope_authority_precedes_backup_observation_and_reads_are_exempt() {
        let (_dir, mut c, cookies, proof, cat) = fixture();
        seed_manifest(&mut c);
        let mut backup = RejectingScopes::default();
        execute_scoped(
            &mut c,
            &cookies,
            &Fixed,
            &proof,
            &cat,
            "b",
            Action::Current,
            &mut backup,
        )
        .unwrap();
        execute_scoped(
            &mut c,
            &cookies,
            &Fixed,
            &proof,
            &cat,
            "b",
            Action::Reconciliation("2026-01-01"),
            &mut backup,
        )
        .unwrap();
        consolidation_app::execute_scoped(
            &mut c,
            &cookies,
            &Fixed,
            &proof,
            &cat,
            "b",
            consolidation_app::Action::Current("2026-01-01"),
            &mut backup,
        )
        .unwrap();
        consolidation_app::execute_scoped(
            &mut c,
            &cookies,
            &Fixed,
            &proof,
            &cat,
            "b",
            consolidation_app::Action::Available("2026-01-01"),
            &mut backup,
        )
        .unwrap();
        elimination_app::execute_scoped(
            &mut c,
            &cookies,
            &Fixed,
            &proof,
            &cat,
            Some("b"),
            elimination_app::Action::Current,
            None,
            &mut backup,
        )
        .unwrap();
        elimination_app::execute_scoped(
            &mut c,
            &cookies,
            &Fixed,
            &proof,
            &cat,
            Some("b"),
            elimination_app::Action::Overview("2026-01-01"),
            None,
            &mut backup,
        )
        .unwrap();
        assert!(backup.calls.is_empty());
        c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u')").unwrap();
        let before = audit(&c);
        assert_eq!(
            execute_scoped(
                &mut c,
                &cookies,
                &Fixed,
                &proof,
                &cat,
                "b",
                Action::Propose(&mapping()),
                &mut backup
            )
            .unwrap_err()
            .error
            .code,
            "access_denied"
        );
        assert_eq!(
            consolidation_app::execute_scoped(
                &mut c,
                &cookies,
                &Fixed,
                &proof,
                &cat,
                "b",
                consolidation_app::Action::Propose(&profile()),
                &mut backup
            )
            .unwrap_err()
            .error
            .code,
            "access_denied"
        );
        let proposal=elimination_app::Proposal{input:serde_json::from_value(json!({"id":"elim","mappingId":"missing","asOf":"2026-01-01","evidenceRefs":["synthetic"]})).unwrap(),expected_payload_hash:"a".repeat(64)};
        // Current session rejection is checked even before interpreting the supplied mutation.
        c.execute("DELETE FROM session WHERE id='s'", []).unwrap();
        assert_eq!(
            elimination_app::execute_scoped(
                &mut c,
                &cookies,
                &Fixed,
                &proof,
                &cat,
                Some("b"),
                elimination_app::Action::Propose(&proposal),
                None,
                &mut backup
            )
            .unwrap_err()
            .error
            .code,
            "invalid_credentials"
        );
        assert!(backup.calls.is_empty());
        assert_eq!(audit(&c), before);
    }
}
