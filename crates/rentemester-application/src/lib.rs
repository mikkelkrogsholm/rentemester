//! Shared use-case policy. Trusted adapters own authentication and current source observations.
pub mod corporate_records;
pub mod company_knowledge;
pub mod parties;
pub mod access;
pub mod backup_management;
pub mod control_fence;
pub mod email_verification;
pub mod mfa_enrollment;
pub mod mfa_login;
pub mod password_change;
pub mod password_login;
pub mod password_reset;
pub mod reset_request;
pub mod session_control;
pub mod session_read;
pub mod verification_request;

pub mod workspace_context;

pub mod service_context;

pub mod service_lifecycle;

pub mod journal_post;
pub mod journal_reversal;

mod journal_mutation;
pub mod service_company;

pub mod bootstrap;

pub mod owned_report;

pub mod documents;

pub mod bank;

pub mod expense;

pub mod company_setup;

pub mod sales;

pub mod periods;

pub mod assets;

pub mod accruals;
pub mod budget;
pub mod mileage;

pub mod recurring_invoices;

pub mod dashboard;

pub mod accountant;

pub mod exceptions;
pub mod integrity;

pub mod multi_year;

pub mod dimensions;

pub mod reviewed_bookkeeping;
pub mod gdpr;
pub mod supplier_commitments;

pub mod annual_report;
pub mod obligations;

pub mod bank_reconciliation;

pub mod workspace_access;

pub mod company_creation;
pub mod saft;

pub mod reminder_claims;

pub mod portable_backup;

pub mod workspace_inbox;

pub mod ownership;

pub mod intercompany;

pub mod eliminations;
pub mod consolidation;

pub mod scheduled_recurring;
