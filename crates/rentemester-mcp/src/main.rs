//! Bounded MCP 2025-11-25 stdio adapter. Wire references:
//! https://modelcontextprotocol.io/specification/2025-11-25/basic/transports
//! https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle
//! https://modelcontextprotocol.io/specification/2025-11-25/server/tools
mod mileage_posting;
mod vat_purchase_original;
mod asset_disposal;
mod asset_original;
mod accrual_original;
mod mileage_original;
mod expense_original;
mod recurring_original;
mod invoice_claims_original;
mod invoice_lifecycle;
mod invoice_original;
mod payable_original;
mod direct_bank_original;
mod bank_original;
mod bank_accounts_original;
mod accounts_original;
mod bank_import;
mod bank_queries;
mod handoff_export;
mod annual_exports;
mod portfolio_overview;
mod journal_history;
mod dashboard_artifact;
mod invoice_bad_debt;
mod invoice_validation;
mod invoice_interest;
mod invoice_compensation;
mod audit_verify;
mod audit_log;
mod bank_matches;
mod cvr;
mod backup_remote;
mod backup_governance;
mod backup_registration;
mod eu_sales;
mod oss_report;
mod vat_original;
mod reminder_claims;
mod company_creation;
mod saft;
mod portable_backup;
mod backup_status;
mod company_setup;
mod privacy;
mod budget;
mod budget_original;
mod exceptions;
mod mileage;
mod company_knowledge;
mod parties;
mod workspace_inbox;
mod bilagsmail;
mod public_einvoice;
mod digisense_received;
mod corporate_records;
mod ownership;
mod intercompany;
mod elimination;
mod consolidation;
mod accruals;
mod assets;
mod recurring_invoices;
mod bank_correction;
mod bank_reconciliation;
mod documents;
mod document_pdf;
mod invoice_extraction;
mod reviewed_bookkeeping;
mod annual_reports;
mod bank_payables;
mod commitments;
mod supplier_commitment_original;
mod periods;
mod receivables;
mod tools;
use rentemester_client::{ApiClient, LocalRuntime, read_cookie_file, read_service_key_file};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, Read, Write},
    path::Path,
    process::ExitCode,
};
const PROTOCOL: &str = "2025-11-25";
const MAX_LINE: usize = 1_048_576;
const MAX_CONTACT_IMPORT_LINE: usize = 2 * rentemester_client::MAX_CONTACT_IMPORT_BODY + 65_536;
const MAX_CONSOLIDATION_LINE: usize =
    2 * rentemester_client::MAX_CONSOLIDATION_PROPOSE_BODY + 65_536;
const MAX_OWNERSHIP_LINE: usize = 2 * rentemester_client::MAX_OWNERSHIP_PROPOSE_BODY + 65_536;
const MAX_CORPORATE_LINE: usize = 2 * rentemester_client::MAX_CORPORATE_INGEST_BODY + 65_536;
const MAX_DOCUMENT_LINE: usize = 2 * rentemester_client::MAX_DOCUMENT_INGEST_BODY + 65_536;

fn help() -> &'static str {
    "Audit: audit_log_list {company,limit?,offset?,fromDate?,toDate?,eventTypeLike?,actorLike?} reads current company audit with cookie or scoped service CompanyRead. UTC date bounds; case-insensitive ANDed LIKE substrings retain %/_ wildcards. Server count/total/hasMore/nextOffset, no cross-request snapshot; exact messages are plain text.\nDiscovery: rentemester-mcp --catalogue (alone; no credentials or network).\nLocal: rentemester-mcp --local-profile ABS_PRIVATE_PROFILE (--cookie-file PRIVATE_PATH | --service-key-file PRIVATE_PATH). Joins the already running admitted server through its private Unix socket; no discovery, actor impersonation, childserver or TCP fallback. After server reopen with a new logical origin, restart/reconfigure MCP explicitly. Local profile excludes --server and --allow-loopback-http.\nRun: rentemester-mcp --server HTTPS_ORIGIN (--cookie-file PRIVATE_PATH | --service-key-file PRIVATE_PATH) [--allow-loopback-http]. Service keys currently support audit_log_list, me, companies, journal/trial_balance/income_statement/balance/overview post_journal/reverse_journal, contacts/create_customer and preview_invoice/issue_invoice/post_invoice/invoices/invoice_pdf, bank_accounts/bank/import_bank, documents/document_original/ingest_document, expense_booking_options/payables/register_payable/pay_payable and explicit settle_invoice/invoice_email_status/invoice_email_send/invoice_email_reconcile/invoice_email_retry; bank administration and other tools retain human authorization. Never combine credentials. Every deliberate service retry or receipt replay may spend usage again; preserve the original unchanged envelope/key and inspect usage/business stages.\nBad debt: invoice_write_off_bad_debt {company,body,confirm:true}, exact16KiB native JSON with inner confirm:true. Current human MFA CompanyLedgerPost; operator confirms documented established loss. grossAmount is original invoice currency; omitted means current open principal. Explicit expense account only; server derives original debtor/VAT and actual DKK effects. Claims remain separate. No recovery operation, actor/key or automatic retry; unknown outcome requires canonical invoice_reminder_claims/invoice/journal and manual reconciliation.\nOpening balance: opening_balance {company} reads; opening_balance_plan {company,body} is a read-only POST without confirmation; opening_balance_apply {company,body,confirm:true} requires exact native 64KiB {input,planHash,idempotencyKey,confirm:true}. Review server cutover/account amounts/source citations. Current human authority, atomic journal/marker/provenance and keyed receipt remain server-owned; no actor/math/key/date inference or automatic retry. Preserve exact bytes/key and inspect canonical opening/journal after unknown outcomes.\nDinero: dinero_import_plan {company,body} is read-only; dinero_import_apply {company,body,confirm:true} takes exact native35MiB {fileName,contentBase64,planHash,idempotencyKey,confirm:true}. Dedicated70MiB+64KiB frames for these two operations. Current human MFA CompanyAdmin apply, CompanyRead list, CompanyDocumentsRead originals. dinero_imports {company}; dinero_import_original {company,attemptId} returns exact original integer bytes25MiB. invoice_imported_receivables {company,asOf} includes settled externally issued rows without native invoice actions. Server owns parsing, atomic journals/source/control/schedule and current authorization. No actor, inferred attempt/control, keys, automatic retry. Preserve source/key after unknown outcome and read canonical imports before deliberate exact-key replay.\nArchives: archive_import_plan {company,body} read-only; archive_import_apply {company,body,confirm:true}, exact native35MiB JSON {fileName,contentBase64,planHash/idempotencyKey/confirm for apply}, dedicated70MiB+64KiB frames only these2. archives {company,fiscalYear?}; archive_import_original {company,importId} exact application/json integerbytes max25MiB. Server owns source parsing/history/currency declaration/attestation/current rights/fences; no live migration, generated keys or retries. Unknown apply requires canonical archive/source reconciliation before exact-key action.\nCFO: cfo_analytics {scope,from,to,companySlugs?,groupProfileId?,account?,party?,dimension?,currency?,aggregate?,cursor?,limit?}; canonical read-only global source projection, no confirm/body/actor or client calculations. Preserve exact decimal strings, provenance and partial/unsupported states. Portfolio has no implicit sum.\nContacts: import_contacts {company,body,confirm:true}, exact native12MiB confirmed JSON {fileName,content,enrichCvr?,confirm:true}. Dinero contacts only; full partial-row errors/counters; server-owned optional CVR/cache, no provider credentials, local parsing, keys or automatic retry. Unknown outcomes require canonical contact inspection.\nIntercompany dispositions: group_disposition_status {id,asOf?} is global readonly; plan_group_disposition {company,body} readonly POST, propose/approve/link/settle/supersede/reopen_group_disposition {company,body,confirm:true}. Exact32KiB source bodies, no keys, automatic posting, payment, head refresh or retries. Adopt normalized plan ID before proposal; unknown writes require exact-ID status. State-specific replay; settled balanced is false.\nDirect purchase correction: direct_bank_payable_correction_plan {company,body} reads a reviewed source plan without confirmation; direct_bank_payable_correction {company,body,confirm:true} atomically reverses the direct purchase, registers a dated payable and settles on bank date. Exact native16KiB JSON; apply requires reviewed planHash/reason/caller idempotencyKey and inner confirm:true. No generated keys/math/retry; inspect payables/journal/bank after an unknown outcome.\nBatch source: bookkeeping_batch_plan accepts flat four dates plus optional companyId assertion; persist/dry_run add explicit reviewed expectedPlanHash, runKey and confirm:true (durable write, not preview). approve/apply accept runId, planHash, confirm:true. All require company; do not mix flat fields with native scope/body. Identical status {company,runId} deliberately retains native envelope including data.ok; other source forms lift core ok/errors to outer envelope. Native compatibility: bookkeeping_batch_plan {company,scope:{accountingFrom,accountingTo,bankFrom,bankTo}} and bookkeeping_batch_status {company,runId} OR {company,runKey} are read-only. bookkeeping_batch_persist/approve/apply {company,body,confirm:true}, exact native32KiB JSON. Persist binds caller runKey plus reviewed expectedPlanHash; approve/apply bind runId+planHash. Server owns independent review, atomic_v1 rollback, current source/rights/transfer fence and final checks; legacy partial history inspect-only. No math, generated keys/hashes/defaults or automatic retry; unknown persist recover through exact runKey status, unknown apply through runId status.\nTransfer: workspace_transfer_status {} and restored_receipts {company} are read-only. workspace_transfer_freeze/cancel/export/release/activate {body,confirm:true}, reconcile_restored_receipt {company,body,confirm:true}. Exact native16KiB, release/activate32KiB; no company on workspace tools, no operator key/config. Export returns backup MIME/integer bytes96MiB, no localfiles or verification claim. Current owner review, exact state hashes and destination-bound source signature; no automatic thaw/activation/dispatch/retry, no global receipt alias. Read canonical status after ambiguous outcomes.\nInvoice delivery: invoice_email_status {company} GET never sends; invoice_email_send/invoice_email_reconcile/invoice_email_retry {company,body,confirm:true}, native16KiB exact confirmed JSON. Current human external-send permission/MFA or explicitly authorized current service; known409/422 preserve canonical delivery as ok:false, 5xx/network outcome_unknown. Reconcile external evidence first, separate reviewed retry only after definite nonacceptance. No SMTP credentials or automatic resend.\nService lifecycle: service_principals {}; create_service_principal/rotate_service_principal/revoke_service_principal/recover_service_principal {body,confirm:true}, native8192-byte exact JSON/current human workspace owner and fresh MFA mutations. Create/rotate intentionally output one-time secret; keep receipt private. Caller operationId allows explicit recover after unknown, no issuance replay/secret redisclosure. Hosted: hosted_recurring_plan {company,templateId}; consent_hosted_recurring/revoke_hosted_recurring {company,body,confirm:true}, native16KiB/current human CompanyAdmin; no service secret, implicit dispatch/clientdriver or automatic retry.\nScheduled recurring: scheduled_recurring {company}, scheduled_recurring_plan {company,templateId} are readonly GETs without selectors/confirm/sideeffects. enable/pause/resume/cancel/catch_up/skip/dispatch/reconsent_scheduled_recurring {company,body,confirm:true}, exact native16KiB inner-confirm JSON, ordinary1MiBframes. Reconsent requires current CompanyAdmin, explicit planHash/reason and only records restored consent, never dispatches/activates/starts timer. Server owns consent/currentrights/template-state-plan hashes and periods; catchup queues only, skip audits only, explicit authorized dispatch creates issued JSON/PDF UNPOSTED/UNSENT. No timer/hiddencredentials/defaults/math/keys/retries; reconcile unknown writes through canonical status/plan first.\nEliminations: eliminations {company}; group_eliminations {asOf}; plan_elimination {company,body} is read-only POST with NO confirm. propose_elimination/approve_elimination/reject_elimination/apply_elimination/reverse_elimination {company,body,confirm:true} require exact native256KiB body and reviewed expectedPayloadHash or eliminationId/payloadHash. Server owns source freshness/four-eyes and exact V1/V2 evidence; no math/keys/defaults/retries/ledger posting. Read current state after uncertain writes.\nrentemester-mcp --server HTTPS_ORIGIN (--cookie-file PATH | --service-key-file PATH) [--allow-loopback-http]\nMCP 2025-11-25 over newline-delimited JSON-RPC stdio. Requires initialize then notifications/initialized.\nService file: exact private service key, no Cookie combination or Bearer/raw secret flags. Service scope: me/companies, company journal/report reads and manual post/reversal; current server permissions apply. Human-only/admin tools remain refused, with no fallback or retries.\nCookie file: one valid MFA-derived browser Cookie request-header value; private regular file (chmod 600 on Unix). Login/enrollment remain in browser.\nBank/payables: bank {company,year?}, bank_accounts {company}, payables {company,status?,asOf?}, expense_booking_options {company,id}; create_bank_account/import_bank/register_payable/match_bank_expense {company,body,confirm:true}; pay_payable additionally id. Exact native flat body; discover contentSchema via tools/list. No generic bank/payable idempotency keys or retries.\nTools: accounts/contacts/documents {company}, invoices {company,year?}; preview_invoice {company,body} returns PDF integer bytes. create_customer/create_vendor/post_invoice {company,body,confirm:true}; update_customer/update_vendor additionally id; delete_customer/delete_vendor {company,id,body,confirm:true} with exact body {confirm:true} and existing obligation/history guards; issue_invoice additionally previewReviewed:true after previewing identical body. Native flat body strings follow tools/list contentSchema; no domain idempotency keys, no retry. Reconcile uncertain domain writes through read/list before further action.\nSAF-T: saft_export {company,body,confirm:true}, exact4096-byte {periodStart,periodEnd,confirm:true}, returns tar integer bytes. Bounded custom v3 profile, audited export, no official generic validation/submission or retry.\nNew company: create_company {body,confirm:true}; WorkspaceManage, exact native16KiB JSON without inner confirm/key/company selector. Creates fresh ledger, no copied chart or retries; unknown outcomes require companies/profile reconciliation.\nInterest: invoice_interest_calc/invoice_interest_correction_calc {company,body} readonly; invoice_claim_interest/invoice_post_interest/invoice_post_interest_correction {company,body,confirm:true}, native16KiB inner-confirm writes. Current human MFA, optional referenceRatePercent uses statutory default; separate registration/posting, DKK interest. Correction canPost/requiresRefund and server original-account lines are authoritative, no caller accounts/amount/allocation for correction. No email/keys/math or automatic retries; inspect canonical calculations/claims after unknown outcomes.\nCompensation: invoice_compensation_calc {company,body} is a read-only POST; invoice_claim_compensation/invoice_post_compensation {company,body,confirm:true}, exact native16KiB. Current human MFA only: CompanyRead/draft.write/ledger.post respectively. Claims DKK, principal original currency. Register and post separately with explicit accounts, no email/keys/retries; reconcile unknown outcomes through canonical claims/calculation before manual action.\nReminder claims: invoice_reminder_claims {company,id}; register_invoice_reminder/post_invoice_reminder/settle_invoice_claims {company,body,confirm:true}, native16KiB. Separate registration/posting, current incomingDKK claim receipt only after principal paid, no email, automatic reminders, keys or retries.\nWorkspace backup: workspace_backup {confirm:true}, max96MiB exact native MIME/integer bytes. No selectors/body/path/key/files/verification/restore or automatic retry; independent operator verify/restore remains server CLI.\nIntercompany: intercompany {company}; group_reconciliation {asOf} global, no company; apply_group_manifest/propose_intercompany_mapping/approve_intercompany_mapping/revoke_intercompany_mapping {company,body,confirm:true}. Exact native256KiB inner-confirm JSON, ordinary1MiB frames. Current all-reference scope, independent authenticated approval and exact mappingHash; preserve decimal strings, no keys/defaults/calculation/FX/consolidation/retries. Unknown requires canonical current/reconciliation reads.\nConsolidation: consolidation {company,asOf}; group_report_profiles {asOf}, group_consolidated_report {profileId,from,asOf} global without company selector; propose/approve/revoke_consolidation_profile {company,body,confirm:true}. Exact inner-confirm proposal1MiB, approve/revoke32KiB. ONLY proposal admits frame2MiB+64KiB; other normal tools1MiB. Current all-company authority, independent exact-hash approval, full source decimal strings; no local accounting/defaults/keys/retries. Unknown requires canonical reads.\nOwnership: ownership {company,asOf}; ownership_history {company,snapshotId?,asOf?}; propose_ownership/review_ownership/apply_ownership {company,body,confirm:true}. Proposal2MiB/2048facts/frame4MiB+64KiB; review/apply32KiB. Independent authenticated review and exact server snapshot/diff hashes; omissions never revoke. No client ownership math, generated keys/hashes, automatic retry, accounting or consolidation-manifest change.\nCorporate: corporate_records {company,cursor?,limit?}; corporate_record/corporate_record_original {company,recordId}; ingest_corporate_record {company,body,confirm:true}; link_corporate_record/enrich_corporate_record/supersede_corporate_record additionally recordId. Ingest24MiB JSON/16MiB original, corporate-ingest frame48MiB+64KiB, otherwrites32KiB. Original exact {ok,bytesBase64,sha256}, no invented MIME/filename/files/verification. Server source/sensitivity/all-scope authority; no keys/retries/accounting.\nPublic invoices: public_invoice_config/public_invoice_submissions {company} read canonical configured and submission state without provider effects; register_public_invoice/send_public_invoice/refresh_public_invoice_status {company,body,confirm:true}, exact16KiB inner-confirm JSON. Registration {confirm:true}, send/status {invoiceDocumentId,confirm:true}. HTTP200 is not proof of delivery; preserve prepared/acknowledged/failed/uncertain/legacy and transmissionId/redeliveryBlocked. No credentials, generated keys, accounting or automatic resend. Unknown writes require submissions readback.\nInbox: workspace_inbox {company,cursor?,limit?}; workspace_inbox_source {company,sourceId}; ingest_workspace_inbox {company,body,confirm:true}; assign_workspace_inbox/complete_workspace_inbox additionally sourceId with exact {companySlug,confirm:true}. Ingest35MiB JSON/25MiB upload, named upload frame70MiB+64KiB; other tools1MiB. Explicit independent assignment/completion, no accounting, inference, generated keys or retries. Read canonical source and target documents after unknown outcome.\nParties: workspace_parties {company,query?,cursor?,limit?}; workspace_party {company,partyId}; create_workspace_party {company,body,confirm:true}; link_workspace_party_role/assert_workspace_party_field/assert_workspace_party_alias additionally partyId. Exact256KiB inner confirm. Current visible scope/full associated MasterData for global assertions, no accounting/default inference/merge/retries.\nKnowledge: canonical slug, or registered absolute company root ONLY with explicit verified --local-profile; cookie or scoped service key with current server permissions (no blanket service authority). company_knowledge {company,asOf?,includeProposed?}; propose_company_knowledge/review_company_knowledge/supersede_company_knowledge {company,body,confirm:true}, exact256KiB inner confirm. Advisory only, no automatic accounting or retry; superseded replacement requires separate review.\nCompany setup: company_profile {company}; update_company_profile/create_account/confirm_account_role {company,body,confirm:true}, native16KiB. Profile/account have no inner confirm; role does. Current company only, explicit account/role choices; no inferred defaults/new company/journal/key/retry.\nPrivacy: retention {company}; gdpr_export/gdpr_erase {company,body,confirm:true}, native16KiB inner confirm. Export is audited mutation; review full retained/erasable/unknown/coverage before explicit scoped overlay. Erase forbids asOf, uses trusted server date; no physical/backups erasure guarantee or retries.\nBudget: budget/budget_vs_actual {company,year}; budget_dimension_actuals additionally dimension?/member?; set_budget {company,body,confirm:true}, native16KiB body WITHOUT inner confirm/key. Mileage: mileage {company,year}; register_mileage {company,body,confirm:true}, native32KiB inner confirm. Server arithmetic/advisories only; no automatic retry or journal effects.\nAccruals: accruals {company}; accrual_schedule {company,id}; register_accrual {company,body,confirm:true}; recognize_accrual additionally id. Native256KiB, source/current accounts/server schedule; explicit periodIndex/date, no keys/math/automatic recognition/retries. Registration posts journal once without dedup guarantee.\nAssets: assets {company}; next_asset_depreciation {company,id}; register_asset {company,body,confirm:true}; depreciate_asset additionally id. Exact native256KiB, source-linked capitalisation and server schedule math; review explicit periodIndex, omitted selects next. No keys or automatic retries/runs.\nRecurring: recurring_invoices {company}; create_recurring_invoice {company,body,confirm:true}; generate_recurring_invoice/retire_recurring_invoice additionally id. Exact native256KiB body, no caller keys/timers/retries/hidden posting; only server-defined template-period dedup.\nBank correction: bank_reconciliation_correction_plan {company,bankTransactionId,replacementJournalEntryId}; bank_reconciliation_correction {company,body,confirm:true}, exact16KiB native body with caller idempotencyKey. Server-proven actual reversal/replacement, evidence-only effects, explicit same-principal receipt replay only, no automatic retry.\nBank link: bank_reconciliation_candidates {company,bankTransactionId}, bank_reconciliation_link {company,body,confirm:true}, exact native16KiB body and reviewed server planHash. Link-only, no keys/hash generation or automatic retry.\nBank import: import_bank {company,body,confirm:true}, exact native JSON max35MiB with confirm:true and exactly one csvContent or canonical csvBase64 (original max25MiB). Generic UTF-8/BOM; danske-bank also Windows1252. No client decoding or retries.\nDocuments: ingest_document {company,body,confirm:true}, exact native JSON string max35MiB UTF-8 bytes; bounded ingest-only frame70MiB+64KiB; document_original {company,id} returns recorded MIME/exact original integer bytes (max25MiB). No local document files, keys or retries.\nReviewed: reviewed_bank/accounting_drafts/posting_rules {company}; create_bank_draft {company,body,confirm:true}; submit_draft/reject_draft/approve_draft additionally draftId. Server-owned four-eyes authority, exact native bodies and evidence hashes; no automatic retries.\nAnnual: annual_report {company,fiscalYearStart,fiscalYearEnd} reads preparation/blockers unchanged; no confirm/body/files or filing.\nTools: me {}, companies {}, journal/trial_balance/income_statement/balance/overview {company,year?,source?:live|archive,account?:journalonly}; report_export {company,report:journal|trial-balance|income-statement|balance,format:csv|pdf,year?,source?,account?:journalonly} returns exact data {ok,contentType,bytes} max96MiB without local files or figures. Omitted source uses server selection; explicit source never falls back; post_journal/reverse_journal {company,envelope}.\ncompany is an explicit company slug. envelope is an exact JSON string containing the server's confirmed request and caller idempotencyKey; never retried or rewritten.\nHTTP requires explicit --allow-loopback-http and literal loopback IP. No local fallback. Unknown write outcomes require reconciliation with original idempotency key.\nStdin messages limited to 1 MiB except tools/call import_bank/bank_import/ingest_document/ingest_workspace_inbox: frame70MiB+64KiB, native body35MiB, decoded file25MiB; ingest_corporate_record instead frame48MiB+64KiB/body24MiB/original16MiB; propose_ownership frame4MiB+64KiB/body2MiB; propose_consolidation_profile frame2MiB+64KiB/body1MiB; string request IDs to 256 bytes; sessions to 100000 requests. Tools execute sequentially with bounded HTTP timeouts. Close stdin to stop. stdout contains protocol messages only during server operation."
}
fn config(args: Vec<String>) -> Result<ApiClient, &'static str> {
    let mut flags = BTreeMap::new();
    let mut allow_http = false;
    let mut cursor = 0;
    while cursor < args.len() {
        let arg = args[cursor].as_str();
        if arg == "--allow-loopback-http" {
            if allow_http {
                return Err("duplicate loopback flag");
            }
            allow_http = true;
            cursor += 1;
            continue;
        }
        if ![
            "--server",
            "--local-profile",
            "--cookie-file",
            "--service-key-file",
        ]
        .contains(&arg)
        {
            return Err("unknown flag; see --help");
        }
        let value = args
            .get(cursor + 1)
            .filter(|v| !v.is_empty() && !v.starts_with("--"))
            .ok_or("flag requires value")?;
        if flags.insert(arg, value.as_str()).is_some() {
            return Err("duplicate flag");
        }
        cursor += 2;
    }
    let local = if let Some(profile) = flags.get("--local-profile") {
        if flags.contains_key("--server") || allow_http {
            return Err("--local-profile excludes --server and --allow-loopback-http");
        }
        Some(LocalRuntime::load(Path::new(profile)).map_err(|_|"local runtime profile unavailable or invalid; require matching canonical private owned descriptor and socket")?)
    } else {
        None
    };
    let server = flags.get("--server").copied();
    if local.is_none() && server.is_none() {
        return Err("explicit --server or --local-profile required");
    }
    match (flags.get("--cookie-file"), flags.get("--service-key-file")) {
        (Some(_), Some(_)) => Err("choose one of --cookie-file or --service-key-file"),
        (Some(path), None) => {
            let cookie = read_cookie_file(Path::new(path)).map_err(
                |_| "cookie file unavailable or invalid; require private regular file (chmod 600)",
            )?;
            match local.as_ref() {
                Some(local) => ApiClient::new_local(local, Some(cookie)),
                None => ApiClient::new(
                    server.ok_or("explicit --server origin required")?,
                    allow_http,
                    Some(cookie),
                ),
            }
            .map_err(|_| {
                if local.is_some() {
                    "invalid local runtime binding or Cookie header; see --help"
                } else {
                    "invalid server origin or Cookie header; see --help"
                }
            })
        }
        (None, Some(path)) => {
            let key = read_service_key_file(Path::new(path)).map_err(|_| "service key file unavailable or invalid; require private regular file (chmod 600)")?;
            match local.as_ref() {
                Some(local) => ApiClient::with_local_service_key(local, key),
                None => ApiClient::with_service_key(
                    server.ok_or("explicit --server origin required")?,
                    allow_http,
                    key,
                ),
            }
            .map_err(|_| {
                if local.is_some() {
                    "invalid local runtime binding or service key; see --help"
                } else {
                    "invalid server origin or service key; see --help"
                }
            })
        }
        _ => Err("--cookie-file or --service-key-file required"),
    }
}
#[derive(Default)]
struct Session {
    initialized: bool,
    ready: bool,
    request_ids: BTreeSet<String>,
}
fn failure(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
fn success(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}
impl Session {
    async fn handle(&mut self, value: Value, api: &ApiClient) -> Option<Value> {
        let object = match value.as_object() {
            Some(v) => v,
            None => return Some(failure(Value::Null, -32600, "expected one JSON-RPC object")),
        };
        let id = object.get("id");
        let valid_id = id.is_some_and(|v| {
            v.as_str().is_some_and(|s| s.len() <= 256)
                || v.as_i64().is_some()
                || v.as_u64().is_some()
        });
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || object.get("method").and_then(Value::as_str).is_none()
            || (id.is_some() && !valid_id)
        {
            return Some(failure(
                if valid_id {
                    id.unwrap().clone()
                } else {
                    Value::Null
                },
                -32600,
                "invalid JSON-RPC request",
            ));
        }
        let method = object["method"].as_str().unwrap();
        if id.is_none() {
            if method == "notifications/initialized" && self.initialized {
                self.ready = true;
            }
            // Notifications never execute tools or produce responses. Cancellation is
            // advisory; sequential bounded HTTP operations complete before input resumes.
            return None;
        }
        let id = id.unwrap().clone();
        // Cap retained session state; reconnect rather than grow indefinitely.
        if self.request_ids.len() >= 100_000 {
            return Some(failure(
                id,
                -32000,
                "session request limit reached; reconnect",
            ));
        }
        if !self.request_ids.insert(id.to_string()) {
            return Some(failure(id, -32600, "request id already used"));
        }
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
        if !params.is_object() {
            return Some(failure(id, -32602, "expected params object"));
        }
        if method == "ping" {
            return Some(success(id, json!({})));
        }
        if method == "initialize" {
            if self.initialized {
                return Some(failure(id, -32600, "already initialized"));
            }
            if params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .is_none()
                || !params.get("capabilities").is_some_and(Value::is_object)
                || params
                    .pointer("/clientInfo/name")
                    .and_then(Value::as_str)
                    .is_none()
                || params
                    .pointer("/clientInfo/version")
                    .and_then(Value::as_str)
                    .is_none()
            {
                return Some(failure(
                    id,
                    -32602,
                    "initialize requires protocolVersion, capabilities and clientInfo",
                ));
            }
            // Only this version is supported. Per MCP negotiation, respond with the
            // supported version when requested version differs; client may disconnect.
            self.initialized = true;
            return Some(success(
                id,
                json!({"protocolVersion":PROTOCOL,"capabilities":{"tools":{}},"serverInfo":{"name":"rentemester-rust","version":env!("CARGO_PKG_VERSION")},"instructions":"Authenticated remote Rust server adapter. Discover companies before selecting an explicit slug. Keep original idempotency key when reconciling outcome_unknown; no automatic retries."}),
            ));
        }
        if !self.ready {
            return Some(failure(
                id,
                -32002,
                "initialize and notifications/initialized required",
            ));
        }
        match method {
            "tools/list" => Some(success(id, tools::list())),
            "tools/call" => {
                if params.get("task").is_some() {
                    return Some(failure(id, -32602, "task execution not supported"));
                }
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(failure(id, -32602, "tool name required"));
                };
                let args = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                match tools::call(api, name, args).await {
                    Ok(result) => Some(success(id, result)),
                    Err(message) => Some(failure(id, -32602, message)),
                }
            }
            _ => Some(failure(id, -32601, "method not found")),
        }
    }
}
fn serve(api: ApiClient) -> Result<(), &'static str> {
    let runtime = tokio::runtime::Runtime::new().map_err(|_| "could not start HTTP runtime")?;
    let mut session = Session::default();
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    loop {
        let mut line = Vec::new();
        let read = input
            .by_ref()
            .take((MAX_DOCUMENT_LINE + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|_| "stdin read failed")?;
        if read == 0 {
            return Ok(());
        }
        let mut oversized =
            line.len() > MAX_DOCUMENT_LINE || line.len() > MAX_LINE && !line.ends_with(b"\n");
        let response = if oversized || !line.ends_with(b"\n") {
            Some(failure(
                Value::Null,
                -32700,
                "invalid or oversized newline-delimited message",
            ))
        } else {
            match serde_json::from_slice::<Value>(&line) {
                Ok(value) => {
                    // Named large-message admission is bounded before any tool/HTTP execution.
                    let name = if value.get("method").and_then(Value::as_str) == Some("tools/call")
                    {
                        value.pointer("/params/name").and_then(Value::as_str)
                    } else {
                        None
                    };
                    let frame_limit = match name {
                        Some("import_contacts") => MAX_CONTACT_IMPORT_LINE,
                        Some(
                            "import_bank"
                            | "bank_import"
                            | "ingest_document"
                            | "ingest_workspace_inbox"
                            | "archive_import_plan"
                            | "archive_import_apply"
                            | "dinero_import_plan"
                            | "dinero_import_apply",
                        ) => MAX_DOCUMENT_LINE,
                        Some("ingest_corporate_record") => MAX_CORPORATE_LINE,
                        Some("propose_consolidation_profile") => MAX_CONSOLIDATION_LINE,
                        Some("propose_ownership") => MAX_OWNERSHIP_LINE,
                        _ => MAX_LINE,
                    };
                    if line.len() > frame_limit {
                        oversized = true;
                        Some(failure(
                            Value::Null,
                            -32700,
                            "message exceeds tool frame bound; only named original uploads/ownership/consolidation proposals admit larger bounded frames",
                        ))
                    } else {
                        runtime.block_on(session.handle(value, &api))
                    }
                }
                Err(_) => {
                    oversized = line.len() > MAX_LINE;
                    Some(failure(Value::Null, -32700, "invalid JSON"))
                }
            }
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut output, &response).map_err(|_| "stdout write failed")?;
            output
                .write_all(b"\n")
                .and_then(|_| output.flush())
                .map_err(|_| "stdout write failed")?;
        }
        if oversized {
            return Err("message limit exceeded");
        }
    }
}
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--catalogue") {
        if args != ["--catalogue"] {
            eprintln!("--catalogue must be the only argument");
            return ExitCode::from(2);
        }
        println!(
            "{}",
            json!({
                "format": "rentemester-native-mcp-catalogue/1",
                "version": env!("CARGO_PKG_VERSION"),
                "protocolVersion": PROTOCOL,
                "tools": tools::list()["tools"],
            })
        );
        return ExitCode::SUCCESS;
    }
    if args == ["--help"] {
        println!("{}", help());
        return ExitCode::SUCCESS;
    }
    let api = match config(args) {
        Ok(api) => api,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match serve(api) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

mod scheduled_recurring;

mod service_principals;

mod hosted_recurring;

mod invoice_email;

mod workspace_transfer;

mod bookkeeping_batch;

mod payable_correction;

mod intercompany_dispositions;

mod contact_import;

mod cfo_analytics;

mod archive_import;

mod opening_balance;

mod portfolio_cashflow;

mod dinero_import;

mod journal_original;

mod master_data_original;

mod documents_original;
mod document_party_original;
mod posting_rules_original;

mod dimensions_original;
