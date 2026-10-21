//! Current-authorized portfolio sources on the held native workspace barrier.
use super::*;
use rentemester_application::{
    password_login::{Failure, FailureStage},
    portfolio_cashflow as app,
};
use rentemester_engine::Error;
use rentemester_runtime::auth::CookieVerifier;
use rusqlite::TransactionBehavior;
use serde_json::{Value, json};

fn unavailable(_: impl std::fmt::Display) -> Error {
    Error::new(
        "portfolio_source_unavailable",
        "Admitted portfolio source unavailable",
    )
}

impl Owner {
    pub fn portfolio(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        as_of: &str,
    ) -> AuthAttempt<Value> {
        self.financial_overview(cookies, clock, proof, as_of, None, None)
    }

    pub fn portfolio_overview(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        as_of: &str,
        today: &str,
    ) -> AuthAttempt<Value> {
        rentemester_engine::fiscal_year::Date::parse(today).map_err(|_| OwnerError::Context)?;
        self.financial_overview(cookies, clock, proof, as_of, None, Some(today))
    }

    pub fn cashflow(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        slug: &str,
        today: &str,
        year: Option<&str>,
    ) -> AuthAttempt<Value> {
        self.financial_overview(cookies, clock, proof, today, Some((slug, year)), None)
    }

    fn financial_overview(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        as_of: &str,
        cashflow: Option<(&str, Option<&str>)>,
        overview_today: Option<&str>,
    ) -> AuthAttempt<Value> {
        let catalog = self.catalog()?;
        self.admit_auth(cookies.workspace())?;
        let selected = self.slug.clone();
        let slots = self.held.0.as_mut().unwrap();
        let tx = slots
            .control
            .as_mut()
            .unwrap()
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| OwnerError::Database)?;
        let before = tx.total_changes();
        let outcome = (|| {
            let auth = app::authorize_portfolio(&tx, cookies, clock, proof, &catalog, as_of)?;
            let companies = if let Some((slug, _)) = cashflow {
                if !auth.companies.iter().any(|allowed| allowed == slug) {
                    return Err(Error::new("access_denied", "Company is not accessible"));
                }
                vec![slug.to_owned()]
            } else {
                auth.companies.clone()
            };
            let mut sources = vec![];
            for slug in &companies {
                super::cfo_analytics::switch(
                    &mut slots.company,
                    &mut slots.guard,
                    &self.root,
                    slug,
                    &mut self.slug,
                )?;
                slots
                    .guard
                    .ensure_connections(&tx, slots.company.as_ref().unwrap())
                    .map_err(unavailable)?;
                let read = slots
                    .company
                    .as_mut()
                    .unwrap()
                    .transaction_with_behavior(TransactionBehavior::Deferred)
                    .map_err(unavailable)?;
                let changes = read.total_changes();
                let assured = (|| {
                    if !rentemester_application::periods::verify_sources(&read, &mut slots.guard)?
                        .is_empty()
                    {
                        return Err(unavailable("physical source differs"));
                    }
                    rentemester_application::archive_import::verify_physical(
                        &read,
                        &mut slots.guard,
                    )?;
                    if overview_today.is_some() {
                        rentemester_storage::portfolio_overview::require_integrity(&read)?;
                    }
                    Ok(())
                })();
                let source_assured = assured.is_ok();
                let source = assured.and_then(|()| {
                    if let Some(today) = overview_today {
                        let dashboard =
                            rentemester_application::dashboard_json::compose_authorized(
                                &tx,
                                &read,
                                rentemester_application::control_fence::CompanySelection {
                                    workspace: &catalog.workspace,
                                    slug,
                                    registered: true,
                                    archived: false,
                                },
                                auth.subject(),
                                clock,
                                as_of,
                                today,
                            )?;
                        return rentemester_storage::portfolio_overview::row(&dashboard);
                    }
                    match cashflow {
                        Some((_, year)) => {
                            rentemester_storage::cashflow::read(&read, slug, as_of, year)
                        }
                        None => rentemester_storage::portfolio::company(&read, slug, as_of),
                    }
                });
                let unchanged = read.total_changes() == changes;
                read.rollback().map_err(unavailable)?;
                if !unchanged {
                    return Err(unavailable("readonly source changed data"));
                }
                let source = match source {
                    Ok(value) => value,
                    Err(error)
                        if overview_today.is_some()
                            && source_assured
                            && !matches!(
                                error.code,
                                "access_denied"
                                    | "invalid_control"
                                    | "dashboard_source_unavailable"
                                    | "backup_status_unavailable"
                                    | "backup_state_unavailable"
                                    | "backup_inventory_unavailable"
                                    | "invalid_credentials"
                                    | "control_busy_or_unavailable"
                            ) =>
                    {
                        let name = &catalog
                            .companies
                            .iter()
                            .find(|c| c.slug == *slug)
                            .ok_or_else(|| unavailable("visible company catalog differs"))?
                            .name;
                        json!({"slug":slug,"name":name,"ok":false,"error":"company source unavailable","code":error.code})
                    }
                    Err(error) => return Err(error),
                };
                sources.push(source);
            }
            if app::authorize_portfolio(&tx, cookies, clock, proof, &catalog, as_of)? != auth
                || tx.total_changes() != before
            {
                return Err(unavailable("current authority changed"));
            }
            if overview_today.is_some() {
                sources.sort_by(|a, b| a["slug"].as_str().cmp(&b["slug"].as_str()));
                return Ok(json!({"asOf":as_of,"companyCount":sources.len(),"companies":sources}));
            }
            if cashflow.is_some() {
                sources
                    .pop()
                    .ok_or_else(|| unavailable("missing company source"))
            } else {
                rentemester_engine::portfolio::render(&catalog.workspace, as_of, &sources)
            }
        })();
        let restore = super::cfo_analytics::switch(
            &mut slots.company,
            &mut slots.guard,
            &self.root,
            &selected,
            &mut self.slug,
        )
        .and_then(|_| {
            slots
                .guard
                .ensure_connections(&tx, slots.company.as_ref().unwrap())
                .map_err(unavailable)
        });
        let reusable = tx.rollback().is_ok() && restore.is_ok();
        let outcome = match (outcome, restore) {
            (Ok(value), Ok(())) if reusable => Ok(value),
            (result, restored) => Err(Failure {
                stage: FailureStage::NotCommitted,
                error: restored
                    .err()
                    .or_else(|| result.err())
                    .unwrap_or_else(|| unavailable("rollback")),
                connection_reusable: reusable,
            }),
        };
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests;
