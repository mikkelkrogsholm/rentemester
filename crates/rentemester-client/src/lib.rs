//! Origin-bound HTTP adapter shared by interactive and agent clients. No ledger logic.
//! Supply a browser session's Cookie request-header value, never a Set-Cookie header.
//! Authentication/enrollment is currently performed by the server's browser UI.
mod agent_discovery;
mod backup_destinations;
mod backup_remote;
mod backup_status;
mod audit_verify;
mod audit_log;
pub use audit_log::{AuditLogQuery,audit_log_query_schema};
mod backup_governance;
mod backup_registration;
pub use backup_registration::{BackupRegistrationMutation,MAX_BACKUP_REGISTRATION_BODY,backup_registration_body_schema};
pub use backup_status::{backup_lock_body_schema,MAX_BACKUP_LOCK_BODY};
mod bank_matches;
mod cvr;
mod eu_sales;
mod oss_report;
mod parties;
pub use parties::{PartyMutation,MAX_PARTY_BODY,party_body_schema};
mod company_knowledge;
pub use company_knowledge::{KnowledgeMutation,MAX_KNOWLEDGE_BODY,knowledge_body_schema};
mod company_creation;
pub use company_creation::{MAX_COMPANY_CREATION_BODY, company_creation_body_schema};
mod company_setup;
pub use company_setup::{CompanySetupMutation, MAX_COMPANY_SETUP_BODY, company_setup_body_schema};
mod privacy;
pub use privacy::{MAX_PRIVACY_BODY, privacy_body_schema, PrivacyOperation};
mod regulatory;
mod budget;
pub use budget::{MAX_BUDGET_BODY, budget_body_schema};
mod exceptions;
mod exceptions_original;
mod master_data_original;
pub use master_data_original::{MasterDataOriginalKind, master_data_original_create_schema};
pub use exceptions::{MAX_EXCEPTION_BODY, exception_resolve_schema};
mod mileage;
pub use mileage::{MAX_MILEAGE_BODY, mileage_body_schema};
mod accruals;
pub use accruals::{AccrualMutation, MAX_ACCRUAL_BODY, accrual_body_schema};
mod assets;
pub use assets::{AssetMutation, MAX_ASSET_BODY, asset_body_schema};
mod recurring_invoices;
pub use recurring_invoices::{MAX_RECURRING_BODY, RecurringInvoiceMutation, recurring_invoice_body_schema};
mod bank_correction;
pub use bank_correction::{bank_correction_body_schema,MAX_BANK_CORRECTION_BODY};
mod bank_reconciliation;
pub use bank_reconciliation::{bank_reconciliation_body_schema,MAX_BANK_RECONCILIATION_BODY};
mod documents;
mod document_pdf;
mod invoice_extraction;
pub use documents::{document_ingest_schema,MAX_DOCUMENT_INGEST_BODY,MAX_DOCUMENT_ORIGINAL};
mod reviewed_bookkeeping;
pub use reviewed_bookkeeping::{ReviewedRead,ReviewedMutation,reviewed_body_schema,valid_draft_id};
mod annual_reports;
mod report_reads;
pub use annual_reports::valid_annual_bounds;
mod auth;
mod bank_payables;
mod commitments;
pub use commitments::{
    CommitmentMutation, commitment_body_schema, commitment_plan_schema, valid_commitment_start,
};
mod periods;
mod period_original;
pub use periods::{PeriodMutation, period_body_schema};
mod domain;
mod receivables;
pub use auth::{AuthenticatedSession, LoginProgress, MfaFactor, PendingLogin};
pub use bank_payables::{BankMutation, bank_body_schema};
pub use domain::{
    Confirmation, DomainMutation, MAX_DOMAIN_BODY, domain_body_schema, invoice_body_schema,
};
pub use receivables::{ReceivablesMutation, receivables_body_schema};
mod local_runtime;
pub use local_runtime::LocalRuntime;
mod credentials;
pub use credentials::{read_cookie_file, read_service_key_file};

use reqwest::{
    Client, Method, Url,
    header::{COOKIE, HeaderValue},
};
use serde_json::Value;
use std::{fmt, time::Duration};
use zeroize::Zeroizing;

const MAX_RESPONSE: usize = 16 * 1024 * 1024;

/// Errors deliberately omit remote bodies, URLs, credentials and request contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InvalidInput(&'static str),
    Transport,
    Refused {
        status: u16,
    },
    Protocol,
    /// The server may have committed. Reconcile server state before retrying.
    /// Ledger writes retain their original idempotency key; authentication has none.
    OutcomeUnknown,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(reason) => write!(f, "invalid input: {reason}"),
            Self::Transport => f.write_str("server transport failed"),
            Self::Refused { status } => write!(f, "server refused request (HTTP {status})"),
            Self::Protocol => f.write_str("invalid server response"),
            Self::OutcomeUnknown => {
                f.write_str("operation outcome unknown; reconcile server state before retrying")
            }
        }
    }
}
impl std::error::Error for Error {}

/// Intentionally does not implement Debug: request Cookie/service key are secrets.
pub struct ApiClient {
    local: Option<LocalRuntime>,
    http: Client,
    base: Url,
    origin: String,
    cookie: Option<Zeroizing<String>>,
    service_key: Option<Zeroizing<String>>,
}
#[derive(Clone, Copy, Debug)]
pub enum Report {
    Journal,
    TrialBalance,
    IncomeStatement,
    Balance,
    Overview,
}
impl Report {
    fn path(self) -> &'static str {
        match self {
            Self::Journal => "journal",
            Self::TrialBalance => "trial-balance",
            Self::IncomeStatement => "income-statement",
            Self::Balance => "balance",
            Self::Overview => "overview",
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum JournalMutation {
    Post,
    Reverse,
}

impl ApiClient {
    /// `allow_loopback_http` permits HTTP only on literal 127.0.0.1 / ::1.
    /// No redirects, proxy inheritance, automatic retries, or ambient cookie jar.
    pub fn new(
        server: &str,
        allow_loopback_http: bool,
        cookie: Option<String>,
    ) -> Result<Self, Error> {
        let base = Url::parse(server)
            .map_err(|_| Error::InvalidInput("expected absolute server origin"))?;
        let loopback = base
            .host_str()
            .and_then(|host| {
                host.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .ok()
            })
            .is_some_and(|ip| ip.is_loopback());
        if !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || base.path() != "/"
            || base.host_str().is_none()
            || !(base.scheme() == "https"
                || (base.scheme() == "http" && allow_loopback_http && loopback))
        {
            return Err(Error::InvalidInput(
                "expected HTTPS origin; explicit loopback HTTP is permitted",
            ));
        }
        if let Some(value) = &cookie
            && (value.is_empty()
                || value.len() > 8192
                || value.chars().any(char::is_control)
                || HeaderValue::from_str(value).is_err())
        {
            return Err(Error::InvalidInput("invalid Cookie header"));
        }
        let local = local_runtime::process_endpoint(server)?;
        Self::construct(base, cookie, local.as_deref())
    }
    fn construct(base: Url, cookie: Option<String>, socket: Option<&std::path::Path>) -> Result<Self, Error> {
        let mut builder = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30));
        #[cfg(unix)]
        if let Some(socket) = socket { builder = builder.unix_socket(socket); }
        #[cfg(not(unix))]
        if socket.is_some() { return Err(Error::InvalidInput("local Unix transport is unavailable on this platform")); }
        let http = builder.build()
            .map_err(|_| Error::Transport)?;
        let origin = base.origin().ascii_serialization();
        Ok(Self {
            http,
            base,
            origin,
            cookie: cookie.map(Zeroizing::new),
            service_key: None,
            local: local_runtime::process_local(),
        })
    }
    /// Explicit service credential; never combined with a browser Cookie or Bearer header.
    /// Current server permissions determine which operations this identity may execute.
    pub fn with_service_key(
        server: &str,
        allow_loopback_http: bool,
        key: String,
    ) -> Result<Self, Error> {
        let key = Zeroizing::new(key);
        if key.is_empty()
            || key.len() > 4096
            || key.trim() != key.as_str()
            || key.chars().any(char::is_control)
            || HeaderValue::from_str(&key).is_err()
        {
            return Err(Error::InvalidInput("invalid service key"));
        }
        let mut client = Self::new(server, allow_loopback_http, None)?;
        client.service_key = Some(key);
        Ok(client)
    }
    pub async fn health(&self) -> Result<Value, Error> {
        self.get("/api/health", &[]).await
    }
    pub async fn me(&self) -> Result<Value, Error> {
        self.get("/api/me", &[]).await
    }
    pub async fn companies(&self) -> Result<Value, Error> {
        self.get("/api/companies", &[]).await
    }
    pub async fn report(
        &self,
        company: &str,
        report: Report,
        year: Option<&str>,
        account: Option<&str>,
    ) -> Result<Value, Error> {
        self.report_source(company, report, year, account, None).await
    }
    /// Sends the exact caller-provided JSON bytes once. Never generates/replaces an
    /// idempotency key; an uncertain outcome must be reconciled by the caller.
    pub async fn manual_journal(
        &self,
        company: &str,
        mutation: JournalMutation,
        body: Vec<u8>,
    ) -> Result<Value, Error> {
        if body.len() > 65_536 {
            return Err(Error::InvalidInput("request exceeds 65536 bytes"));
        }
        let wire: Value = serde_json::from_slice(&body)
            .map_err(|_| Error::InvalidInput("expected JSON request"))?;
        if wire.get("confirm").and_then(Value::as_bool) != Some(true)
            || wire
                .pointer("/input/idempotencyKey")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
        {
            return Err(Error::InvalidInput(
                "confirm:true and input.idempotencyKey required",
            ));
        }
        let endpoint = match mutation {
            JournalMutation::Post => "manual-journal",
            JournalMutation::Reverse => "manual-journal-reversal",
        };
        self.request(
            Method::POST,
            &company_path(company, endpoint)?,
            &[],
            Some(body),
        )
        .await
    }
    async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, Error> {
        self.request(Method::GET, path, query, None).await
    }
    async fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Vec<u8>>,
    ) -> Result<Value, Error> {
        let mutation = body.is_some();
        let mut response = self
            .send(
                method,
                path,
                query,
                body,
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await?;
        let unknown = || {
            if mutation {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        };
        let status = response.status();
        // Any 5xx on a write can follow a commit; never report these as safe refusals.
        if mutation && status.is_server_error() {
            return Err(Error::OutcomeUnknown);
        }
        if !status.is_success() {
            return Err(Error::Refused {
                status: status.as_u16(),
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unknown())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(if mutation {
                    Error::OutcomeUnknown
                } else {
                    Error::Protocol
                });
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            if mutation {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        })
    }
    async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Vec<u8>>,
        cookie: Option<&str>,
    ) -> Result<reqwest::Response, Error> {
        let mutation = body.is_some();
        let mut url = self
            .base
            .join(path)
            .map_err(|_| Error::InvalidInput("invalid endpoint"))?;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query.iter().copied());
        }
        let mut request = self
            .http
            .request(method, url)
            .header("origin", &self.origin)
            .header("accept", "application/json");
        if let Some(key) = &self.service_key {
            if cookie.is_some() {
                return Err(Error::InvalidInput(
                    "service key and Cookie cannot be combined",
                ));
            }
            let mut header = HeaderValue::from_str(key)
                .map_err(|_| Error::InvalidInput("invalid service key"))?;
            header.set_sensitive(true);
            request = request.header("x-rentemester-service-key", header);
        }
        if let Some(cookie) = cookie {
            let mut header =
                HeaderValue::from_str(cookie).map_err(|_| Error::InvalidInput("invalid cookie"))?;
            header.set_sensitive(true);
            request = request.header(COOKIE, header);
        }
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(body);
        }
        let unknown = || {
            if mutation {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        };
        request.send().await.map_err(|_| unknown())
    }
}
fn company_path(company: &str, resource: &str) -> Result<String, Error> {
    if company.is_empty()
        || !company
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(Error::InvalidInput("company must be a slug"));
    }
    Ok(format!("/api/companies/{company}/{resource}"))
}

mod saft;
pub use saft::{saft_body_schema,MAX_SAFT_BODY,MAX_SAFT_PACKAGE};
mod reminder_claims;
pub use reminder_claims::{ReminderClaimMutation,reminder_claim_body_schema,MAX_REMINDER_CLAIM_BODY};

mod portable_backup;
pub use portable_backup::{MAX_WORKSPACE_BACKUP_BYTES, WORKSPACE_BACKUP_CONTENT_TYPE};

mod workspace_inbox;
mod bilagsmail;
pub use bilagsmail::{BilagsmailMutation, MAX_BILAGSMAIL_BODY, bilagsmail_body_schema};
pub use workspace_inbox::{WorkspaceInboxOriginal,WorkspaceInboxMutation,MAX_WORKSPACE_INBOX_BODY,workspace_inbox_body_schema};

mod corporate_records;
pub use corporate_records::{CorporateRecordMutation,MAX_CORPORATE_BODY,MAX_CORPORATE_INGEST_BODY,MAX_CORPORATE_ORIGINAL_JSON,corporate_record_body_schema};

mod ownership;
pub use ownership::{OwnershipMutation,MAX_OWNERSHIP_BODY,MAX_OWNERSHIP_PROPOSE_BODY,ownership_body_schema};

mod intercompany;
pub use intercompany::{IntercompanyMutation,MAX_INTERCOMPANY_BODY,intercompany_body_schema};

mod consolidation;
pub use consolidation::{ConsolidationMutation,MAX_CONSOLIDATION_BODY,MAX_CONSOLIDATION_PROPOSE_BODY,consolidation_body_schema};

mod elimination;
pub use elimination::{EliminationMutation,MAX_ELIMINATION_BODY,elimination_plan_schema,elimination_body_schema};

mod scheduled_recurring;
pub use scheduled_recurring::{ScheduledRecurringMutation,MAX_SCHEDULED_RECURRING_BODY,scheduled_recurring_body_schema};

mod service_principals;
pub use service_principals::{ServicePrincipalMutation,MAX_SERVICE_PRINCIPAL_BODY,service_principal_body_schema};
mod hosted_recurring;
pub use hosted_recurring::{HostedRecurringMutation,MAX_HOSTED_RECURRING_BODY,hosted_recurring_body_schema};

mod invoice_email;
pub use invoice_email::{InvoiceEmailMutation, MAX_INVOICE_EMAIL_BODY, invoice_email_body_schema};

mod workspace_transfer;
pub use workspace_transfer::{WorkspaceTransferMutation,MAX_WORKSPACE_TRANSFER_BODY,MAX_WORKSPACE_TRANSFER_PROOF_BODY,workspace_transfer_body_schema,restored_receipt_body_schema};

mod bookkeeping_batch;
pub use bookkeeping_batch::{BookkeepingBatchMutation,MAX_BOOKKEEPING_BATCH_BODY,bookkeeping_batch_body_schema,bookkeeping_batch_scope_schema};

mod payable_correction;
pub use payable_correction::{MAX_PAYABLE_CORRECTION_BODY,payable_correction_body_schema};

mod intercompany_dispositions;
pub use intercompany_dispositions::{DispositionAction,MAX_DISPOSITION_BODY,disposition_body_schema};

mod contact_import;
pub use contact_import::{MAX_CONTACT_IMPORT_BODY,contact_import_body_schema};

mod cfo_analytics;
pub use cfo_analytics::{CfoAnalyticsQuery,cfo_analytics_schema};

mod archive_import;
pub use archive_import::{MAX_ARCHIVE_IMPORT_BODY,MAX_ARCHIVE_ORIGINAL,archive_import_body_schema};

mod opening_balance;
pub use opening_balance::{MAX_OPENING_BALANCE_BODY,opening_balance_body_schema};

mod public_einvoice;
pub use public_einvoice::{PublicInvoiceMutation,MAX_PUBLIC_INVOICE_BODY,public_invoice_body_schema};

pub mod portfolio_cashflow;

mod invoice_compensation;
pub use invoice_compensation::{InvoiceCompensationOperation,MAX_INVOICE_COMPENSATION_BODY,invoice_compensation_body_schema};

mod invoice_interest;
pub use invoice_interest::{InvoiceInterestOperation,MAX_INVOICE_INTEREST_BODY,invoice_interest_body_schema};

mod invoice_bad_debt;
pub use invoice_bad_debt::{invoice_bad_debt_original_schema,validate_invoice_bad_debt_original,MAX_INVOICE_BAD_DEBT_BODY,invoice_bad_debt_body_schema};

mod dinero_import;
pub use dinero_import::{MAX_DINERO_IMPORT_BODY,MAX_DINERO_ORIGINAL,dinero_import_body_schema};

mod dashboard_artifact;
pub use dashboard_artifact::{MAX_DASHBOARD_BYTES,DASHBOARD_CONTENT_TYPE};

mod journal_history;
pub use journal_history::{JournalHistoryQuery,journal_history_query_schema};

mod dated_reports;

mod portfolio_overview;

mod annual_exports;

mod handoff_export;
pub use handoff_export::{handoff_body_schema,write_handoff_package,MAX_HANDOFF_BODY,MAX_HANDOFF_PACKAGE};

mod saft_artifact;
pub use saft_artifact::write_saft_package;

mod bank_queries;
pub use bank_queries::BankQuery;

mod bank_import;
pub use bank_import::bank_import_file_body;

mod bank_accounts_original;
pub use bank_accounts_original::{BankAccountOriginalMutation,bank_account_original_schema};

mod bank_original;
pub use bank_original::{BankOriginalMutation,bank_original_body_schema};

mod direct_bank_original;
pub use direct_bank_original::direct_bank_original_schema;

mod payable_original;
pub use payable_original::{PayableOriginalMutation,PayableOriginalQuery,payable_original_body_schema};

mod invoice_original;
pub use invoice_original::{InvoiceOriginalKind,InvoiceOriginalQuery,invoice_original_schema};

mod invoice_lifecycle;
pub use invoice_lifecycle::{InvoiceLifecycleKind,invoice_lifecycle_schema,validate_invoice_lifecycle};

mod invoice_claims_original;
pub use invoice_claims_original::{OriginalInvoiceClaimKind,invoice_claims_original_schema,validate_invoice_claims_original};

mod invoice_public_original;
pub use invoice_public_original::{OriginalPublicInvoiceKind,invoice_public_original_schema,validate_invoice_public_original,public_invoice_artifact};

mod recurring_original;
pub use recurring_original::{OriginalRecurringKind,recurring_original_schema,recurring_original_invoice_schema,validate_recurring_original,recurring_workspace_schema,validate_recurring_workspace};

mod journal_original;

mod documents_original;
pub use documents_original::{DocumentsOriginalMutation, documents_original_schema, documents_original_file};

mod document_party_original;
pub use document_party_original::{DocumentPartyOriginalOperation, document_party_original_schema, document_party_original_validate};

mod invoice_extraction_original;
pub use invoice_extraction_original::original_invoice_extraction_surface_schema;

mod expense_original;
pub use expense_original::{original_expense_book_schema, validate_original_expense_book};

mod accrual_original;
pub use accrual_original::{original_accrual_schema, validate_original_accrual, validate_original_accrual_actor};

mod asset_original;
pub use asset_original::{original_asset_schema, validate_original_asset, validate_original_asset_actor};

mod mileage_original;
pub use mileage_original::{validate_original_mileage_shape, original_mileage_log_schema, publish_original_mileage};

mod budget_original;
pub use budget_original::{original_budget_set_schema, validate_original_budget_result};

mod vat_original;
pub use vat_original::validate_original_vat_result;

mod vat_purchase_original;
pub use vat_purchase_original::{original_vat_purchase_schema, validate_original_vat_purchase};

mod supplier_commitment_original;
pub use supplier_commitment_original::{SupplierCommitmentOriginalOperation, original_supplier_commitment_schema, validate_original_supplier_commitment};

mod dimensions_original;
pub use dimensions_original::{DimensionsOriginalOperation, original_dimensions_schema, validate_original_dimensions};

mod posting_rules_original;
pub use posting_rules_original::{PostingRulesOriginalOperation, original_posting_rules_schema, parse_original_posting_rules_json, validate_original_posting_rules};

mod accounts_original;
pub use accounts_original::{AccountsOriginalOperation, original_accounts_schema, validate_original_accounts};

mod accounting_draft_original;
pub use accounting_draft_original::{AccountingDraftOriginalOperation, MAX_ACCOUNTING_DRAFT_BODY, original_accounting_draft_schema, validate_original_accounting_draft, parse_original_accounting_draft_json, original_accounting_draft_file};

mod bookkeeping_batch_original;
pub use bookkeeping_batch_original::{OriginalBatchOperation, bookkeeping_batch_original_schema, validate_bookkeeping_batch_original};

pub use local_runtime::knowledge_company_selector;
