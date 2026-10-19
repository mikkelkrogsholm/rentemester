//! A single workspace/control owner switches its company only under its external mutex.
use super::*;
use rentemester_application::company_creation as app;
use rentemester_application::password_login::{Failure, FailureStage};
use rentemester_application::workspace_context::{Catalog, RegisteredCompany};
use rentemester_runtime::auth::CookieVerifier;
use serde_json::{Value, json};
impl Owner {
    pub fn catalog(&mut self) -> Result<Catalog, OwnerError> {
        self.admit()?;
        let result = self
            .held
            .0
            .as_mut()
            .unwrap()
            .guard
            .catalog()
            .map_err(OwnerError::Namespace);
        self.finish(result.is_ok());
        result.map(|items| Catalog {
            workspace: self.workspace.clone(),
            companies: items
                .into_iter()
                .map(|(slug, name, archived)| RegisteredCompany {
                    slug,
                    name,
                    archived,
                })
                .collect(),
        })
    }
    /// Caller retains this same owner guard for the subsequent fixed domain command.
    pub fn select_company(&mut self, slug: &str) -> Result<(), OwnerError> {
        let catalog = self.catalog()?;
        if !catalog
            .companies
            .iter()
            .any(|c| c.slug == slug && !c.archived)
        {
            return Err(OwnerError::Context);
        }
        if slug == self.slug {
            return Ok(());
        }
        self.admit()?;
        let slots = self.held.0.as_mut().unwrap();
        if let Some(company) = slots.company.take()
            && let Err((company, _)) = company.close()
        {
            slots.company = Some(company);
            return Err(OwnerError::Database);
        }
        slots
            .guard
            .select_company(slug)
            .map_err(OwnerError::Namespace)?;
        slots.company = Some(
            Connection::open_with_flags(
                self.root.join(slug).join("data/ledger.sqlite"),
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
            )
            .map_err(|_| OwnerError::Database)?,
        );
        slots
            .company
            .as_ref()
            .unwrap()
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")
            .map_err(|_| OwnerError::Database)?;
        self.slug = slug.to_owned();
        self.observe()?;
        self.reusable = true;
        Ok(())
    }
    pub fn create_company(
        &mut self,
        cookies: &CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        input: &app::Input,
    ) -> AuthAttempt<Value> {
        self.admit_auth(cookies.workspace())?;
        let settings = match app::settings(input) {
            Ok(s) => s,
            Err(e) => {
                let retired = self.finish(true);
                return Ok(Attempt {
                    outcome: Err(Failure {
                        stage: FailureStage::NotCommitted,
                        error: e,
                        connection_reusable: true,
                    }),
                    retired,
                });
            }
        };
        let slots = self.held.0.as_mut().unwrap();
        let catalog = slots.guard.catalog().map_err(OwnerError::Namespace)?;
        let registered_or_full =
            catalog.len() >= 1000 || catalog.iter().any(|c| c.0 == settings.slug);
        let root = &self.root;
        let result = app::execute(
            slots.control.as_mut().unwrap(),
            cookies,
            clock,
            proof,
            &settings,
            registered_or_full,
            |subject| {
                slots
                    .guard
                    .create_company_files(&settings.slug)
                    .map_err(|_| rentemester_storage::company_creation::refused())?;
                slots.pending = Some(
                    Connection::open_with_flags(
                        root.join(&settings.slug).join("data/ledger.sqlite"),
                        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
                    )
                    .map_err(|_| rentemester_storage::company_creation::refused())?,
                );
                slots
                    .guard
                    .verify_pending_company(&settings.slug, slots.pending.as_ref().unwrap())
                    .map_err(|_| rentemester_storage::company_creation::refused())?;
                rentemester_storage::company_creation::initialize(
                    slots.pending.as_mut().unwrap(),
                    &settings,
                    subject,
                )?;
                slots
                    .guard
                    .verify_pending_company(&settings.slug, slots.pending.as_ref().unwrap())
                    .map_err(|_| rentemester_storage::company_creation::refused())?;
                if let Some(db) = slots.pending.take()
                    && let Err((db, _)) = db.close()
                {
                    slots.pending = Some(db);
                    return Err(rentemester_storage::company_creation::refused());
                }
                Ok(())
            },
        );
        let result=result.and_then(|()|{
     let now=clock.now_unix_ms().map_err(|_|Failure{stage:FailureStage::OutcomeUnknown,error:rentemester_storage::company_creation::refused(),connection_reusable:false})?;
     slots.guard.publish_company(&settings.slug,&settings.name,now).map_err(|_|Failure{stage:FailureStage::OutcomeUnknown,error:rentemester_engine::Error::new("company_creation_unconfirmed","company publication unconfirmed; inspect current company list before any new attempt"),connection_reusable:false})?;
     Ok(json!({"slug":settings.slug,"name":settings.name}))
   });
        let reusable = result
            .as_ref()
            .map_or_else(|f| f.connection_reusable, |_| true);
        let retired = self.finish(reusable);
        Ok(Attempt {
            outcome: result,
            retired,
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests;
