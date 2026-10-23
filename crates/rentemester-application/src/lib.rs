//! Shared use-case policy. Trusted adapters own authentication and current source observations.
pub mod budget_original;
pub mod journal_original;
pub mod invoice_compensation;
pub mod invoice_interest;
pub mod invoice_bad_debt;
pub mod dinero_import;
pub mod audit_verify;
pub mod bank_matches;
pub mod cvr;
pub mod eu_sales;
pub mod oss_report;
pub mod vat_original;
pub mod public_einvoice;
pub mod mail_intake;
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
pub mod document_pdf;
pub mod invoice_extraction;

pub mod bank;
pub mod bank_original;

pub mod expense;
pub mod payable_original;

pub mod company_setup;

pub mod sales;
pub mod invoice_original;

pub mod periods;

pub mod assets;

pub mod accruals;
pub mod budget;
pub mod mileage;

pub mod recurring_invoices;

pub mod dashboard;
pub mod dashboard_artifact;

pub mod accountant;

pub mod exceptions;
pub mod integrity;

pub mod multi_year;

pub mod dimensions;

pub mod reviewed_bookkeeping;
pub mod gdpr;
pub mod supplier_commitments;

pub mod annual_report;
pub mod annual_exports;
pub mod obligations;

pub mod bank_reconciliation;

pub mod workspace_access;

pub mod company_creation;
pub mod saft;

pub mod reminder_claims;

pub mod portable_backup;
pub mod backup_destination;
pub mod backup_status;

pub mod workspace_inbox;

pub mod ownership;

pub mod intercompany;
pub mod intercompany_dispositions;

pub mod eliminations;
pub mod consolidation;

pub mod scheduled_recurring;

pub mod hosted_recurring;

pub mod invoice_email;

pub mod receipt_reconciliation;
pub mod workspace_transfer;

pub mod bookkeeping_batch;

pub mod document_party_links;

pub mod direct_bank_payable_correction;

pub mod contact_import;

pub mod cfo_analytics;

pub mod archive_import;
pub mod opening_balance;
pub mod archive_reports;

pub mod portfolio_cashflow;

pub mod manual_post_recovery;

pub mod backup_remote;

pub mod backup_governance;

pub mod backup_registration;

pub mod audit_log;

pub mod dashboard_json;

pub mod invoice_lifecycle;

pub mod invoice_claims_original;

pub mod invoice_bad_debt_original;

pub mod recurring_workspace;

pub mod master_data_original;
pub mod documents_original;

pub mod expense_original;

pub mod accrual_original;
pub mod asset_original;

pub mod mileage_original;

pub mod vat_purchase_original;

pub mod supplier_commitment_original;

pub mod dimensions_original;

pub mod posting_rules_original;

pub mod accounts_original;

pub mod accounting_draft_original;

mod document_access;

pub mod compliance;

pub mod mileage_posting;
