//! Private physical ownership under explicit Linux-broker or macOS-local profiles.
//! This is not schema adoption, fresh initialization, or transport authorization.
//! Never open other regular descriptors for owned database inodes in this process.
use rentemester_application::{
    control_fence::{Committed, CompanySelection},
    journal_post, journal_reversal, service_company,
};
use rentemester_runtime::{
    auth::{Clock, VerifiedCookie},
    namespace::{NamespaceError, NamespaceGuard, NamespaceProfile},
    service_key::ServiceKey,
};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerError {
    Namespace(NamespaceError),
    Database,
    Retired,
    Context,
}
impl std::fmt::Display for OwnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Namespace(error) => error.code(),
            Self::Database => "broker_database_refused",
            Self::Retired => "broker_owner_retired",
            Self::Context => "broker_context_refused",
        })
    }
}
impl std::error::Error for OwnerError {}

// Kept together through every partial constructor and explicit/implicit close.
// Never expose G or a connection, including through a generic callback.
struct Slots<G> {
    control: Option<Connection>,
    company: Option<Connection>,
    pending: Option<Connection>,
    guard: G,
}
impl<G> Slots<G> {
    fn close(&mut self) -> bool {
        for slot in [&mut self.pending, &mut self.company, &mut self.control] {
            if let Some(connection) = slot.take()
                && let Err((connection, _)) = connection.close()
            {
                *slot = Some(connection);
            }
        }
        self.pending.is_none() && self.company.is_none() && self.control.is_none()
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
/// DedicatedLinux trusts the launcher and broker-UID code; clients use another UID.
/// LocalUserMacos trusts all local-account code, including same-account HTTP clients.
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
    root: std::path::PathBuf,
    slug: String,
    reusable: bool,
}
/// Preserve the original application outcome even if subsequent observation
/// retires this owner: a committed transaction must never become a retry signal.
pub type AuthAttempt<T> =
    Result<Attempt<Result<T, rentemester_application::password_login::Failure>>, OwnerError>;
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
        Self::open_with_profile(
            root,
            broker_uid,
            workspace,
            slug,
            NamespaceProfile::DedicatedLinux,
        )
    }
    /// Explicit local-account macOS admission is distinct from the Linux broker.
    pub fn open_with_profile(
        root: &Path,
        broker_uid: u32,
        workspace: &str,
        slug: &str,
        profile: NamespaceProfile,
    ) -> Result<Self, OwnerError> {
        let guard =
            NamespaceGuard::acquire_with_profile(root, broker_uid, workspace, slug, profile)
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
        Self::create_new_with_profile(
            root,
            broker_uid,
            workspace,
            slug,
            name,
            created_at,
            NamespaceProfile::DedicatedLinux,
        )
    }
    pub fn create_new_with_profile(
        root: &Path,
        broker_uid: u32,
        workspace: &str,
        slug: &str,
        name: &str,
        created_at: &str,
        profile: NamespaceProfile,
    ) -> Result<(Self, rentemester_storage::fresh::Initialized), OwnerError> {
        let guard = NamespaceGuard::create_fresh_with_profile(
            root, broker_uid, workspace, slug, name, created_at, profile,
        )
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
    /// Create a native workspace and install its explicitly supplied chart before serving.
    pub fn create_with_chart(
        root: &Path,
        broker_uid: u32,
        workspace: &str,
        slug: &str,
        name: &str,
        created_at: &str,
        accounts: &[rentemester_storage::fresh::InitialAccount],
    ) -> Result<Self, OwnerError> {
        Self::create_with_chart_profile(
            root,
            broker_uid,
            workspace,
            slug,
            name,
            created_at,
            accounts,
            NamespaceProfile::DedicatedLinux,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn create_with_chart_profile(
        root: &Path,
        broker_uid: u32,
        workspace: &str,
        slug: &str,
        name: &str,
        created_at: &str,
        accounts: &[rentemester_storage::fresh::InitialAccount],
        profile: NamespaceProfile,
    ) -> Result<Self, OwnerError> {
        let (mut owner, _) = Self::create_new_with_profile(
            root, broker_uid, workspace, slug, name, created_at, profile,
        )?;
        owner.reusable = false;
        rentemester_storage::fresh::initialize_accounts(
            owner.held.0.as_mut().unwrap().company.as_mut().unwrap(),
            accounts,
        )
        .map_err(|_| OwnerError::Database)?;
        owner.observe()?;
        owner.reusable = true;
        Ok(owner)
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
                pending: None,
                guard,
            })),
            workspace: workspace.to_owned(),
            root: root.to_owned(),
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
    /// Private local operator bootstrap. This must never be mounted as an open
    /// signup endpoint. Operator policy, physical/native admission and bounded
    /// KDF/mail execution belong to the trusted host adapter.
    pub fn bootstrap_first_identity(
        &mut self,
        operator: &rentemester_runtime::bootstrap::Operator,
        runtime: &rentemester_application::bootstrap::Runtime<'_>,
        sender: &mut dyn rentemester_runtime::recovery::VerificationSender,
        input: rentemester_application::bootstrap::Input<'_>,
    ) -> Result<
        Attempt<
            Result<
                rentemester_application::bootstrap::Reservation,
                rentemester_application::bootstrap::Failure,
            >,
        >,
        OwnerError,
    > {
        self.admit()?;
        let outcome = rentemester_application::bootstrap::execute(
            self.held.0.as_mut().unwrap().control.as_mut().unwrap(),
            operator,
            runtime,
            sender,
            CompanySelection {
                workspace: &self.workspace,
                slug: &self.slug,
                registered: true,
                archived: false,
            },
            input,
        );
        let reusable = match &outcome {
            Ok(_) => true,
            Err(f) => f.connection_reusable,
        };
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn request_email_verification(
        &mut self,
        runtime: &rentemester_application::verification_request::Runtime<'_>,
        sender: &mut dyn rentemester_runtime::recovery::VerificationSender,
        email: &str,
        redirect: Option<&str>,
        caller: Option<&VerifiedCookie>,
    ) -> Result<Attempt<Result<(), rentemester_application::reset_request::Failure>>, OwnerError>
    {
        self.admit_auth(runtime.cookies.workspace())?;
        let outcome = rentemester_application::verification_request::execute(
            self.held.0.as_mut().unwrap().control.as_mut().unwrap(),
            runtime,
            sender,
            email,
            redirect,
            caller,
        );
        let reusable = outcome
            .as_ref()
            .map_or_else(|f| f.connection_reusable, |_| true);
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    fn admit_auth(&mut self, workspace: &str) -> Result<(), OwnerError> {
        self.admit()?;
        if workspace != self.workspace {
            return Err(OwnerError::Context);
        }
        Ok(())
    }
    pub fn complete_email_verification(
        &mut self,
        runtime: &rentemester_application::verification_request::Runtime<'_>,
        token: &str,
        redirect: Option<&str>,
    ) -> AuthAttempt<Option<rentemester_runtime::recovery::SensitiveUrl>> {
        self.admit_auth(runtime.cookies.workspace())?;
        if runtime.verification.signer.workspace() != self.workspace {
            return Err(OwnerError::Context);
        }
        let outcome = rentemester_application::verification_request::complete(
            self.held.0.as_mut().unwrap().control.as_mut().unwrap(),
            runtime,
            token,
            redirect,
        );
        let reusable = match &outcome {
            Ok(_) => true,
            Err(f) => f.connection_reusable,
        };
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn begin_password_login(
        &mut self,
        runtime: &rentemester_application::mfa_login::Runtime<'_>,
        email: &str,
        password: &str,
    ) -> AuthAttempt<rentemester_application::mfa_login::BeginOutcome> {
        self.admit_auth(runtime.cookies.workspace())?;
        let outcome = rentemester_application::mfa_login::begin(
            self.held.0.as_mut().unwrap().control.as_mut().unwrap(),
            runtime,
            email,
            password,
        );
        let reusable = match &outcome {
            Ok(_) => true,
            Err(f) => f.connection_reusable,
        };
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn begin_mfa_enrollment(
        &mut self,
        runtime: &rentemester_application::mfa_login::Runtime<'_>,
        email: &str,
        password: &str,
        issuer: &str,
    ) -> AuthAttempt<rentemester_application::mfa_enrollment::Setup> {
        self.admit_auth(runtime.cookies.workspace())?;
        let outcome = rentemester_application::mfa_enrollment::begin(
            self.held.0.as_mut().unwrap().control.as_mut().unwrap(),
            runtime,
            email,
            password,
            issuer,
        );
        let reusable = match &outcome {
            Ok(_) => true,
            Err(f) => f.connection_reusable,
        };
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn confirm_mfa_enrollment(
        &mut self,
        runtime: &rentemester_application::mfa_login::Runtime<'_>,
        challenge: &rentemester_runtime::auth::VerifiedChallenge,
        code: &str,
    ) -> AuthAttempt<rentemester_application::mfa_enrollment::ConfirmOutcome> {
        self.admit_auth(runtime.cookies.workspace())?;
        let outcome = rentemester_application::mfa_enrollment::confirm(
            self.held.0.as_mut().unwrap().control.as_mut().unwrap(),
            runtime,
            challenge,
            code,
        );
        let reusable = match &outcome {
            Ok(_) => true,
            Err(f) => f.connection_reusable,
        };
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    // Private dispatch keeps lifetime admission and retirement identical for all
    // fixed identity operations; callers cannot supply connection callbacks.
    fn control_auth<T>(
        &mut self,
        workspace: &str,
        work: impl FnOnce(
            &mut Connection,
        ) -> Result<T, rentemester_application::password_login::Failure>,
    ) -> AuthAttempt<T> {
        self.admit_auth(workspace)?;
        let outcome = work(self.held.0.as_mut().unwrap().control.as_mut().unwrap());
        let reusable = outcome
            .as_ref()
            .map_or_else(|f| f.connection_reusable, |_| true);
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn first_identity_phase(
        &mut self,
        operator: &rentemester_runtime::bootstrap::Operator,
    ) -> AuthAttempt<Option<rentemester_application::bootstrap::Phase>> {
        self.control_auth(
            operator.workspace(),
            rentemester_application::bootstrap::phase,
        )
    }
    pub fn verify_login_factor(
        &mut self,
        runtime: &rentemester_application::mfa_login::Runtime<'_>,
        challenge: &rentemester_runtime::auth::VerifiedChallenge,
        code: &str,
        backup: bool,
    ) -> AuthAttempt<rentemester_application::mfa_login::VerifyOutcome> {
        self.control_auth(runtime.cookies.workspace(), |control| {
            if backup {
                rentemester_application::mfa_login::verify_backup(control, runtime, challenge, code)
            } else {
                rentemester_application::mfa_login::verify(control, runtime, challenge, code)
            }
        })
    }
    pub fn current_session(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
    ) -> AuthAttempt<rentemester_application::session_read::SessionView> {
        self.control_auth(cookies.workspace(), |control| {
            rentemester_application::session_read::execute(control, cookies, clock, proof)
        })
    }
    pub fn sign_out(
        &mut self,
        runtime: &rentemester_application::session_control::Runtime<'_>,
        proof: &VerifiedCookie,
    ) -> AuthAttempt<rentemester_application::session_control::Outcome> {
        self.control_auth(runtime.cookies.workspace(), |control| {
            rentemester_application::session_control::execute(
                control,
                runtime,
                proof,
                rentemester_application::session_control::Operation::SignOut,
            )
        })
    }
    pub fn workspace_access(
        &mut self, runtime: &rentemester_application::workspace_access::Runtime<'_>,
        proof: &VerifiedCookie, action: rentemester_application::workspace_access::Action<'_>,
        deliver: &mut dyn FnMut(&str,&str)->Result<(),()>,
    ) -> AuthAttempt<serde_json::Value> {
        self.control_auth(runtime.cookies.workspace(), |control| rentemester_application::workspace_access::execute(control,runtime,proof,action,deliver))
    }
    pub fn claim_invitation(
        &mut self, runtime: &rentemester_application::workspace_access::Runtime<'_>,
        verification: &rentemester_application::verification_request::Runtime<'_>,
        sender: &mut dyn rentemester_runtime::recovery::VerificationSender,
        input: rentemester_application::workspace_access::Claim<'_>,
    ) -> AuthAttempt<serde_json::Value> {
        self.control_auth(runtime.cookies.workspace(), |control| rentemester_application::workspace_access::claim(control,runtime,verification,sender,input))
    }
    pub fn workspace_context(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        clock: &dyn Clock,
        proof: &VerifiedCookie,
        catalog: &rentemester_application::workspace_context::Catalog,
    ) -> AuthAttempt<rentemester_application::workspace_context::WorkspaceContext> {
        self.control_auth(cookies.workspace(), |control| {
            rentemester_application::workspace_context::execute(
                control, cookies, clock, proof, catalog,
            )
        })
    }
    fn document_action<T>(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        work: impl FnOnce(
            &mut Connection,
            &mut Connection,
            &mut NamespaceGuard,
            rentemester_application::documents::Context<'_>,
        )
            -> Result<Committed<T>, rentemester_application::control_fence::Failure>,
    ) -> Result<
        Attempt<Result<Committed<T>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().unwrap();
        let context = rentemester_application::documents::Context {
            cookie,
            clock,
            selection: CompanySelection {
                workspace: &self.workspace,
                slug: &self.slug,
                registered: true,
                archived: false,
            },
        };
        let outcome = work(
            slots.control.as_mut().unwrap(),
            slots.company.as_mut().unwrap(),
            &mut slots.guard,
            context,
        );
        let reusable = outcome.as_ref().map_or_else(
            |f| f.control_reusable && f.company_reusable,
            |v| v.control_reusable,
        );
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn vat_report(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        today: &str,
        year: Option<&str>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::owned_report::vat(c, l, context, today, year)
        })
    }
    pub fn sales(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::sales::Action<'_>,
        today: &str,
        now: &str,
        year: Option<&str>,
    ) -> Result<
        Attempt<
            Result<
                Committed<rentemester_application::sales::Output>,
                rentemester_application::control_fence::Failure,
            >,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::sales::execute(
                c,
                l,
                g,
                context,
                rentemester_application::sales::Request {
                    action,
                    today,
                    now,
                    year,
                },
            )
        })
    }
    pub fn periods(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::periods::Action<'_>,
        today: &str,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::periods::execute(c, l, g, context, action, today)
        })
    }
    pub fn integrity(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::integrity::read(c, l, context)
        })
    }
    pub fn company_setup(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::company_setup::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::company_setup::execute(c, l, context, action)
        })
    }
    pub fn bank(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::bank::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::bank::execute(c, l, g, context, action)
        })
    }
    pub fn gdpr(
        &mut self, cookie: &VerifiedCookie, clock: &dyn Clock, today: &str,
        action: rentemester_application::gdpr::Action<'_>,
    ) -> Result<Attempt<Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>>, OwnerError> {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::gdpr::execute(c, l, context, today, action)
        })
    }
    pub fn dimensions(
        &mut self, cookie: &VerifiedCookie, clock: &dyn Clock,
        action: rentemester_application::dimensions::Action<'_>,
    ) -> Result<Attempt<Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>>, OwnerError> {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::dimensions::execute(c, l, context, action)
        })
    }
    pub fn annual_report(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        start: &str,
        end: &str,
    ) -> Result<
        Attempt<Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::annual_report::read(c, l, context, start, end)
        })
    }
    pub fn multi_year(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        today: &str,
    ) -> Result<
        Attempt<Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::multi_year::read(c, l, context, today)
        })
    }
    pub fn obligations(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        today: &str,
        year: Option<&str>,
    ) -> Result<
        Attempt<Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>>,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::obligations::read(c, l, context, today, year)
        })
    }
    pub fn bank_reconciliation(&mut self,cookie:&VerifiedCookie,clock:&dyn Clock,action:rentemester_application::bank_reconciliation::Action<'_>)->Result<Attempt<Result<Committed<serde_json::Value>,rentemester_application::control_fence::Failure>>,OwnerError>{
        self.document_action(cookie,clock,|c,l,g,context|rentemester_application::bank_reconciliation::execute(c,l,g,context,action))
    }
    pub fn exceptions(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::exceptions::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::exceptions::execute(c, l, context, action)
        })
    }
    pub fn reminder_claims(&mut self,cookie:&VerifiedCookie,clock:&dyn Clock,action:rentemester_application::reminder_claims::Action<'_>,today:&str)->Result<Attempt<Result<Committed<serde_json::Value>,rentemester_application::control_fence::Failure>>,OwnerError>{
        self.document_action(cookie,clock,|c,l,g,context|rentemester_application::reminder_claims::execute(c,l,g,context,action,today))
    }
    pub fn accountant_export(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: &rentemester_storage::accountant::Input,
    ) -> Result<
        Attempt<
            Result<
                Committed<rentemester_storage::accountant::Download>,
                rentemester_application::control_fence::Failure,
            >,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::accountant::export(c, l, g, context, input)
        })
    }
    pub fn saft_export(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: &rentemester_storage::saft::Input,
    ) -> Result<
        Attempt<
            Result<
                Committed<rentemester_storage::saft::Download>,
                rentemester_application::control_fence::Failure,
            >,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::saft::export(c, l, g, context, input)
        })
    }
    pub fn dashboard(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        today: &str,
        year: Option<&str>,
        as_of: Option<&str>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::dashboard::read(c, l, context, today, year, as_of)
        })
    }
    pub fn reviewed_bookkeeping(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::reviewed_bookkeeping::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::reviewed_bookkeeping::execute(c, l, context, action)
        })
    }
    pub fn budget(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::budget::Action<'_>,
        today: &str,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::budget::execute(c, l, context, action, today)
        })
    }
    pub fn mileage(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::mileage::Action<'_>,
        today: &str,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::mileage::execute(c, l, context, action, today)
        })
    }
    pub fn assets(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::assets::Action<'_>,
        today: &str,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::assets::execute(c, l, g, context, action, today)
        })
    }
    pub fn accruals(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::accruals::Action<'_>,
        today: &str,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::accruals::execute(c, l, g, context, action, today)
        })
    }
    pub fn recurring_invoices(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::recurring_invoices::Action<'_>,
        today: &str,
        now: &str,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::recurring_invoices::execute(
                c, l, g, context, action, today, now,
            )
        })
    }
    pub fn supplier_commitments(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::supplier_commitments::Action<'_>,
        now: &str,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::supplier_commitments::execute(
                c, l, g, context, action, now,
            )
        })
    }
    pub fn expense(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        action: rentemester_application::expense::Action<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::expense::execute(c, l, g, context, action)
        })
    }
    pub fn ingest_document(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        input: rentemester_application::documents::Ingest<'_>,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::documents::ingest(c, l, g, context, input)
        })
    }
    pub fn documents(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
    ) -> Result<
        Attempt<
            Result<Committed<serde_json::Value>, rentemester_application::control_fence::Failure>,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, _, context| {
            rentemester_application::documents::list(c, l, context)
        })
    }
    pub fn document_original(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        id: i64,
    ) -> Result<
        Attempt<
            Result<
                Committed<rentemester_application::documents::Original>,
                rentemester_application::control_fence::Failure,
            >,
        >,
        OwnerError,
    > {
        self.document_action(cookie, clock, |c, l, g, context| {
            rentemester_application::documents::read(c, l, g, context, id)
        })
    }
    pub fn cockpit_snapshot(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        request: rentemester_application::owned_report::CockpitRequest<'_>,
    ) -> Result<
        Attempt<
            Result<
                Committed<rentemester_storage::cockpit::Snapshot>,
                rentemester_application::control_fence::Failure,
            >,
        >,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().unwrap();
        let outcome = rentemester_application::owned_report::cockpit(
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
            request,
        );
        let reusable = outcome.as_ref().map_or_else(
            |f| f.control_reusable && f.company_reusable,
            |c| c.control_reusable,
        );
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
    }
    pub fn trial_balance(
        &mut self,
        cookie: &VerifiedCookie,
        clock: &dyn Clock,
        period: rentemester_engine::trial_balance::Period,
    ) -> Result<
        Attempt<
            Result<
                Committed<rentemester_engine::trial_balance::Report>,
                rentemester_application::control_fence::Failure,
            >,
        >,
        OwnerError,
    > {
        self.admit()?;
        let slots = self.held.0.as_mut().unwrap();
        let outcome = rentemester_application::owned_report::trial_balance(
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
            period,
        );
        let reusable = match &outcome {
            Ok(v) => v.control_reusable,
            Err(f) => f.control_reusable && f.company_reusable,
        };
        let retired = self.finish(reusable);
        Ok(Attempt { outcome, retired })
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

mod company_catalog;

mod portable_backup;
impl Owner {
    /// Stable admitted company owner; all knowledge effects use its canonical control writer.
    pub fn company_knowledge(
        &mut self,
        cookies: &rentemester_runtime::auth::CookieVerifier,
        clock: &dyn rentemester_runtime::auth::Clock,
        proof: &VerifiedCookie,
        action: rentemester_application::company_knowledge::Action<'_>,
    ) -> AuthAttempt<serde_json::Value> {
        let catalog = self.catalog()?;
        let slug = self.slug.clone();
        self.control_auth(cookies.workspace(), |control| {
            rentemester_application::company_knowledge::execute(
                control, cookies, clock, proof, &catalog, &slug, action,
            )
        })
    }
}

impl Owner{
 /// Current stable company selection anchors a workspace-control party operation.
 pub fn parties(&mut self,cookies:&rentemester_runtime::auth::CookieVerifier,clock:&dyn rentemester_runtime::auth::Clock,proof:&VerifiedCookie,action:rentemester_application::parties::Action<'_>)->AuthAttempt<serde_json::Value>{let catalog=self.catalog()?;let slug=self.slug.clone();self.control_auth(cookies.workspace(),|c|rentemester_application::parties::execute(c,cookies,clock,proof,&catalog,&slug,action))}
}

mod workspace_inbox;
mod corporate_records;

mod ownership;

pub mod intercompany;

pub mod eliminations;
pub mod consolidation;

mod scheduled_recurring;
#[cfg(all(test,target_os="macos"))]
mod scheduled_recurring_tests;

mod hosted_recurring;
mod service_identity;

mod password_recovery;

mod invoice_email;

mod workspace_transfer;

mod receipt_reconciliation;

mod bookkeeping_batch;

mod document_party_links;
