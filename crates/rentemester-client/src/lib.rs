//! Origin-bound HTTP adapter shared by interactive and agent clients. No ledger logic.
//! Supply a browser session's Cookie request-header value, never a Set-Cookie header.
//! Authentication/enrollment is currently performed by the server's browser UI.
mod company_creation;
pub use company_creation::{MAX_COMPANY_CREATION_BODY, company_creation_body_schema};
mod company_setup;
pub use company_setup::{CompanySetupMutation, MAX_COMPANY_SETUP_BODY, company_setup_body_schema};
mod privacy;
pub use privacy::{MAX_PRIVACY_BODY, privacy_body_schema};
mod budget;
pub use budget::{MAX_BUDGET_BODY, budget_body_schema};
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
pub use documents::{document_ingest_schema,MAX_DOCUMENT_INGEST_BODY,MAX_DOCUMENT_ORIGINAL};
mod reviewed_bookkeeping;
pub use reviewed_bookkeeping::{ReviewedRead,ReviewedMutation,reviewed_body_schema,valid_draft_id};
mod annual_reports;
pub use annual_reports::valid_annual_bounds;
mod auth;
mod bank_payables;
mod commitments;
pub use commitments::{
    CommitmentMutation, commitment_body_schema, commitment_plan_schema, valid_commitment_start,
};
mod periods;
pub use periods::{PeriodMutation, period_body_schema};
mod domain;
mod receivables;
pub use auth::{AuthenticatedSession, LoginProgress, MfaFactor, PendingLogin};
pub use bank_payables::{BankMutation, bank_body_schema};
pub use domain::{
    Confirmation, DomainMutation, MAX_DOMAIN_BODY, domain_body_schema, invoice_body_schema,
};
pub use receivables::{ReceivablesMutation, receivables_body_schema};
mod credentials;
pub use credentials::read_cookie_file;

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

/// Intentionally does not implement Debug: request Cookie is a secret.
pub struct ApiClient {
    http: Client,
    base: Url,
    origin: String,
    cookie: Option<Zeroizing<String>>,
}
#[derive(Clone, Copy, Debug)]
pub enum Report {
    Journal,
    TrialBalance,
    IncomeStatement,
    Balance,
}
impl Report {
    fn path(self) -> &'static str {
        match self {
            Self::Journal => "journal",
            Self::TrialBalance => "trial-balance",
            Self::IncomeStatement => "income-statement",
            Self::Balance => "balance",
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
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| Error::Transport)?;
        let origin = base.origin().ascii_serialization();
        Ok(Self {
            http,
            base,
            origin,
            cookie: cookie.map(Zeroizing::new),
        })
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
        let path = company_path(company, report.path())?;
        let mut query = Vec::new();
        if let Some(year) = year {
            query.push(("year", year));
        }
        if let Some(account) = account {
            query.push(("account", account));
        }
        self.get(&path, &query).await
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
