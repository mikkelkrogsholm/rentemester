//! Continuous single Owner mutex; admitted company selection only. No path-based reader.
use super::*;
use rentemester_application::intercompany as app;
use rentemester_engine::{intercompany::refused, money::Money};
use serde_json::{Value, json};
impl Owner {
    pub fn intercompany(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        anchor: Option<&str>,
        action: app::Action<'_>,
    ) -> AuthAttempt<Value> {
        let catalog = self.catalog()?;
        let slug = self.slug.clone();
        let date = if let app::Action::Reconciliation(d) = action {
            Some(d.to_owned())
        } else {
            None
        };
        let mut attempt = self.control_company_auth(cookies.workspace(), |c, backup| {
            app::execute_scoped(
                c,
                cookies,
                clock,
                proof,
                &catalog,
                anchor.unwrap_or(""),
                action,
                backup,
            )
        })?;
        if let (Some(date), Ok(v)) = (date, &mut attempt.outcome) {
            let states: Vec<rentemester_storage::intercompany::State> =
                serde_json::from_value(v["rows"].clone()).map_err(|_| OwnerError::Context)?;
            let mut rows = vec![];
            for s in states {
                let m = &s.mapping;
                let mut sides = vec![];
                for (company, accounts, position) in [
                    (&m.left_company_slug, &m.left_account_nos, &m.left_position),
                    (
                        &m.right_company_slug,
                        &m.right_account_nos,
                        &m.right_position,
                    ),
                ] {
                    if !catalog
                        .companies
                        .iter()
                        .any(|c| &c.slug == company && !c.archived)
                    {
                        break;
                    }
                    self.select_company(company)?;
                    self.admit()?;
                    let result = (|| {
                        let slots = self.held.0.as_mut().unwrap();
                        let db = slots.company.as_mut().unwrap();
                        let tx = db
                            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                            .map_err(|_| refused())?;
                        if !rentemester_application::periods::verify_sources(&tx, &mut slots.guard)?
                            .is_empty()
                        {
                            tx.rollback().map_err(|_| refused())?;
                            return Err(refused());
                        }
                        let value = rentemester_storage::intercompany::side(
                            &tx, company, accounts, position, &date,
                        );
                        tx.rollback().map_err(|_| refused())?;
                        value
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
                        Ok(mut v) => {
                            v["sourceSnapshot"]["sourceAssurance"] =
                                json!("native_period_sources_and_physical_originals");
                            sides.push(v);
                        }
                        Err(_) => break,
                    }
                }
                if sides.len() != 2 {
                    rows.push(json!({"mappingId":m.id,"status":"not-comparable","reason":"blocked","blockers":["source ledger is unavailable or failed canonical journal validation"]}));
                    continue;
                }
                let left = &sides[0];
                let right = &sides[1];
                if left["currency"] != right["currency"] {
                    rows.push(json!({"mappingId":m.id,"mappingHash":s.mapping_hash,"left":left,"right":right,"status":"not-comparable","reason":"currency-mismatch","blockers":["no approved FX policy exists"]}));
                } else {
                    let difference = Money::parse_canonical(
                        left["balance"].as_str().ok_or(OwnerError::Context)?,
                    )
                    .map_err(|_| OwnerError::Context)?
                    .subtract(
                        &Money::parse_canonical(
                            right["balance"].as_str().ok_or(OwnerError::Context)?,
                        )
                        .map_err(|_| OwnerError::Context)?,
                    );
                    rows.push(json!({"mappingId":m.id,"mappingHash":s.mapping_hash,"left":left,"right":right,"status":if difference==Money::default(){"matched"}else{"mismatch"},"difference":difference.decimal(),"reason":"exact-native-currency-difference","blockers":[]}));
                }
            }
            self.select_company(&slug)?;
            v["rows"] = json!(rows);
        }
        Ok(attempt)
    }
}
#[cfg(test)]
#[path = "intercompany_tests.rs"]
mod tests;
