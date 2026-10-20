//! Admitted read-only company sources under one Owner lock; canonical control recheck before return.
use super::*;
use rentemester_application::consolidation as app;
use rentemester_engine::consolidation::{self as policy, Applied, Ledger, Profile};
use serde_json::Value;
impl Owner {
    pub fn consolidation(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        anchor: Option<&str>,
        action: app::Action<'_>,
    ) -> AuthAttempt<Value> {
        self.consolidation_bound(cookies, clock, proof, anchor, action, None)
    }
    /// Internal read bound checked against the actual plan, never a reusable authorization.
    pub(crate) fn consolidation_bound(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        anchor: Option<&str>,
        action: app::Action<'_>,
        allowed_companies: Option<&std::collections::BTreeSet<String>>,
    ) -> AuthAttempt<Value> {
        let catalog = self.catalog()?;
        let selected = self.slug.clone();
        let mut attempt = self.control_auth(cookies.workspace(), |db| {
            app::execute(
                db,
                cookies,
                clock,
                proof,
                &catalog,
                anchor.unwrap_or(""),
                action,
            )
        })?;
        if let app::Action::Plan { from, as_of, .. } = action
            && let Ok(plan) = &attempt.outcome
            && plan.get("profile").is_some()
        {
            let plan = plan.clone();
            let companies: Vec<String> = serde_json::from_value(plan["companies"].clone())
                .map_err(|_| OwnerError::Context)?;
            if allowed_companies
                .is_some_and(|allowed| companies.iter().any(|company| !allowed.contains(company)))
            {
                attempt.outcome = Ok(
                    serde_json::json!({"scope":"consolidated-report","period":{"from":from,"to":as_of},"status":"blocked","blockers":["approved profile exceeds requested company scope"],"rawCompanySums":[],"appliedEliminations":[],"consolidatedFigures":null,"sourceSnapshots":[]}),
                );
                return Ok(attempt);
            }
            let result = (|| -> Result<Value, OwnerError> {
                let profile: Profile = serde_json::from_value(plan["profile"].clone())
                    .map_err(|_| OwnerError::Context)?;
                let mut blockers: Vec<String> = serde_json::from_value(plan["blockers"].clone())
                    .map_err(|_| OwnerError::Context)?;
                let applied: Vec<Applied> = serde_json::from_value(plan["applied"].clone())
                    .map_err(|_| OwnerError::Context)?;
                let mut ledgers: Vec<Ledger> = vec![];
                let mut opening_bases = std::collections::BTreeMap::new();
                if blockers.is_empty() {
                    for company in companies {
                        self.select_company(&company)?;
                        self.admit()?;
                        let result = (|| {
                            let slots = self.held.0.as_mut().unwrap();
                            let db = slots.company.as_mut().unwrap();
                            let tx = db
                                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                                .map_err(|_| policy::refused())?;
                            let result = (|| {
                                let opening_basis =
                                    rentemester_storage::opening_balance::require_report_as_of(
                                        &tx, as_of,
                                    )?;
                                if rentemester_application::periods::verify_sources(
                                    &tx,
                                    &mut slots.guard,
                                )?
                                .is_empty()
                                {
                                    rentemester_storage::consolidation::ledger(
                                        &tx, &company, from, as_of,
                                    )
                                    .map(|ledger| (ledger, opening_basis))
                                } else {
                                    Err(policy::refused())
                                }
                            })();
                            tx.rollback().map_err(|_| policy::refused())?;
                            result
                        })();
                        let reusable = self
                            .held
                            .0
                            .as_ref()
                            .unwrap()
                            .company
                            .as_ref()
                            .unwrap()
                            .is_autocommit();
                        self.finish(reusable);
                        if !reusable {
                            return Err(OwnerError::Database);
                        }
                        match result {
                            Ok((l, basis)) => {
                                opening_bases.insert(company.clone(), basis);
                                if l.currency != profile.currency {
                                    blockers.push("all company functional currencies must equal profile currency".into());
                                }
                                ledgers.push(l)
                            }
                            Err(error) if error.code == "archive_report_unsupported" => {
                                blockers.push(error.message)
                            }
                            Err(_) => blockers.push(
                                "company ledger is unavailable or failed integrity validation"
                                    .into(),
                            ),
                        }
                    }
                }
                let mut rows = vec![];
                if blockers.is_empty()
                    && applied
                        .iter()
                        .any(|e| e.payload.group_id == profile.group_id)
                {
                    let reconciliation = self.intercompany(
                        cookies,
                        clock,
                        proof,
                        anchor,
                        rentemester_application::intercompany::Action::Reconciliation(as_of),
                    )?;
                    if let Ok(v) = reconciliation.outcome {
                        rows = serde_json::from_value(v["rows"].clone())
                            .map_err(|_| OwnerError::Context)?;
                    } else {
                        return Err(OwnerError::Context);
                    }
                }
                let mut report = policy::calculate(
                    &profile,
                    plan["profileHash"].as_str().ok_or(OwnerError::Context)?,
                    from,
                    as_of,
                    &ledgers,
                    &applied,
                    &rows,
                    blockers,
                    rentemester_storage::intercompany::sha,
                )
                .map_err(|_| OwnerError::Context)?;
                report["openingBalanceBases"] = serde_json::json!(opening_bases);
                Ok(report)
            })();
            self.select_company(&selected)?;
            let check = self.control_auth(cookies.workspace(), |db| {
                app::execute(
                    db,
                    cookies,
                    clock,
                    proof,
                    &catalog,
                    anchor.unwrap_or(""),
                    action,
                )
            })?;
            match check.outcome {
                Ok(current) if current == plan => {
                    attempt.outcome = Ok(result?);
                }
                Err(f) => attempt.outcome = Err(f),
                _ => return Err(OwnerError::Context),
            }
        }
        Ok(attempt)
    }
}
#[cfg(test)]
#[path = "consolidation_tests.rs"]
mod tests;
