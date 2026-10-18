//! Private physical ownership for an offline-provisioned dedicated Linux broker.
//! This is not schema adoption, fresh initialization, or transport authorization.
//! Never open other regular descriptors for owned database inodes in this process.
use rentemester_application::{
    control_fence::{Committed, CompanySelection},
    journal_post, journal_reversal, service_company,
};
use rentemester_runtime::{
    auth::{Clock, VerifiedCookie},
    namespace::{NamespaceError, NamespaceGuard},
    service_key::ServiceKey,
};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerError {
    Namespace(NamespaceError),
    Database,
    Retired,
}
impl std::fmt::Display for OwnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Namespace(error) => error.code(),
            Self::Database => "broker_database_refused",
            Self::Retired => "broker_owner_retired",
        })
    }
}
impl std::error::Error for OwnerError {}

// Kept together through every partial constructor and explicit/implicit close.
// Never expose G or a connection, including through a generic callback.
struct Slots<G> {
    control: Option<Connection>,
    company: Option<Connection>,
    guard: G,
}
impl<G> Slots<G> {
    fn close(&mut self) -> bool {
        for slot in [&mut self.company, &mut self.control] {
            if let Some(connection) = slot.take()
                && let Err((connection, _)) = connection.close()
            {
                *slot = Some(connection);
            }
        }
        self.company.is_none() && self.control.is_none()
    }
}
struct Held<G>(Option<Slots<G>>);
impl<G> Drop for Held<G> {
    fn drop(&mut self) {
        if let Some(mut slots) = self.0.take()
            && !slots.close()
        {
            // sqlite3_close BUSY leaves the connection alive. Ordinary Connection
            // Drop ignores that failure. Retain the ENTIRE resource set, including
            // lease and pinned regular descriptors, until process exit instead.
            std::mem::forget(slots);
        }
    }
}

/// No Clone, connection accessor, callbacks, Deref, or raw extraction.
/// Root/launcher and all broker-UID code are trusted. Clients run another UID.
/// ```compile_fail
/// fn escape(mut owner: rentemester_host::Owner) {
///     let _ = owner.held;
/// }
/// ```
/// ```compile_fail
/// fn escape(mut owner: rentemester_host::Owner) {
///     let _ = owner.control;
/// }
/// ```
/// ```compile_fail
/// fn escape(mut owner: rentemester_host::Owner) {
///     let _ = owner.company;
/// }
/// ```
/// ```compile_fail
/// fn escape(mut owner: rentemester_host::Owner) {
///     owner.with_connections(|control, _| { let _ = control; });
/// }
/// ```
/// ```compile_fail
/// fn escape(mut owner: rentemester_host::Owner) {
///     let _ = owner.into_inner();
/// }
/// ```
/// ```compile_fail
/// fn escape(mut owner: rentemester_host::Owner) {
///     let _: &mut rusqlite::Connection = std::convert::AsMut::as_mut(&mut owner);
/// }
/// ```
pub struct Owner {
    held: Held<NamespaceGuard>,
    workspace: String,
    slug: String,
    reusable: bool,
}
/// Preserve the original application outcome even if subsequent observation
/// retires this owner: a committed transaction must never become a retry signal.
pub struct Attempt<T> {
    pub outcome: T,
    pub retired: bool,
}
/// Contains a retired owner with any still-open connections and its namespace.
/// Dropping this failure quarantines resources if close still fails.
pub struct CloseFailure {
    owner: Box<Owner>,
}
impl std::fmt::Debug for CloseFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CloseFailure(broker_close_pending)")
    }
}
impl CloseFailure {
    pub fn retry(self) -> Result<(), Self> {
        (*self.owner).try_close()
    }
}
impl Owner {
    /// Physical admission only. Caller must provide an offline-provisioned native
    /// workspace and separately establish bootstrap/schema/legacy-drain authority.
    /// No CREATE, explicit repair/migration command, arbitrary SQL, or caller-owned
    /// connections. SQLite opening/configuration may perform normal journal recovery.
    pub fn open_protected(
        root: &Path,
        broker_uid: u32,
        workspace: &str,
        slug: &str,
    ) -> Result<Self, OwnerError> {
        let guard = NamespaceGuard::acquire(root, broker_uid, workspace, slug)
            .map_err(OwnerError::Namespace)?;
        Self::open_guarded(root, workspace, slug, guard)
    }
    /// Exclusively create a fresh protected Linux namespace and complete native
    /// schemas. Existing paths and private partial failures are never adopted.
    /// Credentials/access/bootstrap and reopen lineage are separate required work.
    pub fn create_new(
        root: &Path,
        broker_uid: u32,
        workspace: &str,
        slug: &str,
        name: &str,
        created_at: &str,
    ) -> Result<(Self, rentemester_storage::fresh::Initialized), OwnerError> {
        let guard =
            NamespaceGuard::create_fresh(root, broker_uid, workspace, slug, name, created_at)
                .map_err(OwnerError::Namespace)?;
        let mut owner = Self::open_guarded(root, workspace, slug, guard)?;
        owner.reusable = false;
        #[cfg(test)]
        tests::fresh_fault(&mut owner)?;
        let slots = owner.held.0.as_mut().unwrap();
        let initialized = rentemester_storage::fresh::initialize_empty_pair(
            slots.control.as_mut().unwrap(),
            slots.company.as_mut().unwrap(),
            name,
        )
        .map_err(|_| OwnerError::Database)?;
        owner.observe()?;
        owner.reusable = true;
        Ok((owner, initialized))
    }
    fn open_guarded(
        root: &Path,
        workspace: &str,
        slug: &str,
        guard: NamespaceGuard,
    ) -> Result<Self, OwnerError> {
        let mut owner = Self {
            held: Held(Some(Slots {
                control: None,
                company: None,
                guard,
            })),
            workspace: workspace.to_owned(),
            slug: slug.to_owned(),
            reusable: false,
        };
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW;
        let slots = owner.held.0.as_mut().expect("live owner");
        slots.control = Some(
            Connection::open_with_flags(root.join(".rentemester/workspace-control.sqlite"), flags)
                .map_err(|_| OwnerError::Database)?,
        );
        #[cfg(test)]
        tests::constructor_fault(slots)?;
        slots.company = Some(
            Connection::open_with_flags(root.join(slug).join("data/ledger.sqlite"), flags)
                .map_err(|_| OwnerError::Database)?,
        );
        for db in [
            slots.control.as_ref().unwrap(),
            slots.company.as_ref().unwrap(),
        ] {
            db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")
                .map_err(|_| OwnerError::Database)?;
        }
        owner.observe()?;
        owner.reusable = true;
        Ok(owner)
    }
    fn observe(&mut self) -> Result<(), OwnerError> {
        let slots = self.held.0.as_mut().expect("live owner");
        slots
            .guard
            .ensure_connections(
                slots.control.as_ref().unwrap(),
                slots.company.as_ref().unwrap(),
            )
            .map_err(OwnerError::Namespace)
    }
    fn admit(&mut self) -> Result<(), OwnerError> {
        if !self.reusable {
            return Err(OwnerError::Retired);
        }
        // Remain retired on an error or panic anywhere in application dispatch.
        self.reusable = false;
        self.observe()
    }
    fn finish(&mut self, reusable: bool) -> bool {
        self.reusable = reusable && self.observe().is_ok();
        !self.reusable
    }
    pub fn try_close(mut self) -> Result<(), CloseFailure> {
        self.reusable = false;
        if self.held.0.as_mut().expect("live owner").close() {
            Ok(())
        } else {
            Err(CloseFailure {
                owner: Box::new(self),
            })
        }
    }
    pub fn post_human(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        policy: &journal_post::Policy,
        input: journal_post::Input,
    ) -> Result<
        Attempt<Result<Committed<journal_post::PostResult>, journal_post::PostFailure>>,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().unwrap();
        let outcome = journal_post::execute(
            slots.control.as_mut().unwrap(),
            slots.company.as_mut().unwrap(),
            cookie,
            clock,
            CompanySelection {
                workspace: &self.workspace,
                slug: &self.slug,
                registered: true,
                archived: false,
            },
            policy,
            input,
        );
        let reusable = match &outcome {
            Ok(v) => v.control_reusable,
            Err(v) => v.fence.control_reusable && v.fence.company_reusable,
        };
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn reverse_human(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        policy: &journal_reversal::Policy,
        input: journal_reversal::Input,
    ) -> Result<
        Attempt<
            Result<Committed<journal_reversal::ReversalResult>, journal_reversal::ReversalFailure>,
        >,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().unwrap();
        let outcome = journal_reversal::execute(
            slots.control.as_mut().unwrap(),
            slots.company.as_mut().unwrap(),
            cookie,
            clock,
            CompanySelection {
                workspace: &self.workspace,
                slug: &self.slug,
                registered: true,
                archived: false,
            },
            policy,
            input,
        );
        let reusable = match &outcome {
            Ok(v) => v.control_reusable,
            Err(v) => v.fence.control_reusable && v.fence.company_reusable,
        };
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn post_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        max_future_days: u32,
        input: journal_post::Input,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().unwrap();
        let outcome = service_company::execute_post(
            slots.control.as_mut().unwrap(),
            slots.company.as_mut().unwrap(),
            service_company::Context {
                key,
                clock,
                max_future_days,
                selection: CompanySelection {
                    workspace: &self.workspace,
                    slug: &self.slug,
                    registered: true,
                    archived: false,
                },
            },
            input,
        );
        let reusable = match &outcome {
            Ok(v) => v.control_reusable,
            Err(v) => v.control_reusable && v.company_reusable,
        };
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn reverse_service(
        &mut self,
        key: &ServiceKey,
        clock: &dyn Clock,
        max_future_days: u32,
        input: journal_reversal::Input,
    ) -> Result<
        Attempt<Result<service_company::ServiceResult, service_company::ServiceFailure>>,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().unwrap();
        let outcome = service_company::execute_reversal(
            slots.control.as_mut().unwrap(),
            slots.company.as_mut().unwrap(),
            service_company::Context {
                key,
                clock,
                max_future_days,
                selection: CompanySelection {
                    workspace: &self.workspace,
                    slug: &self.slug,
                    registered: true,
                    archived: false,
                },
            },
            input,
        );
        let reusable = match &outcome {
            Ok(v) => v.control_reusable,
            Err(v) => v.control_reusable && v.company_reusable,
        };
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
}
#[cfg(test)]
mod tests;
