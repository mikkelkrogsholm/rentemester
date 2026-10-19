//! Bounded MCP 2025-11-25 stdio adapter. Wire references:
//! https://modelcontextprotocol.io/specification/2025-11-25/basic/transports
//! https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle
//! https://modelcontextprotocol.io/specification/2025-11-25/server/tools
mod reminder_claims;
mod company_creation;
mod saft;
mod portable_backup;
mod company_setup;
mod privacy;
mod budget;
mod mileage;
mod company_knowledge;
mod parties;
mod workspace_inbox;
mod corporate_records;
mod ownership;
mod intercompany;
mod accruals;
mod assets;
mod recurring_invoices;
mod bank_correction;
mod bank_reconciliation;
mod documents;
mod reviewed_bookkeeping;
mod annual_reports;
mod bank_payables;
mod commitments;
mod periods;
mod receivables;
mod tools;
use rentemester_client::{ApiClient, read_cookie_file};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, Read, Write},
    path::Path,
    process::ExitCode,
};
const PROTOCOL: &str = "2025-11-25";
const MAX_LINE: usize = 1_048_576;
const MAX_OWNERSHIP_LINE:usize=2*rentemester_client::MAX_OWNERSHIP_PROPOSE_BODY+65_536;
const MAX_CORPORATE_LINE: usize=2*rentemester_client::MAX_CORPORATE_INGEST_BODY+65_536;
const MAX_DOCUMENT_LINE: usize = 2 * rentemester_client::MAX_DOCUMENT_INGEST_BODY + 65_536;

fn help() -> &'static str {
    "rentemester-mcp --server HTTPS_ORIGIN --cookie-file PATH [--allow-loopback-http]\nMCP 2025-11-25 over newline-delimited JSON-RPC stdio. Requires initialize then notifications/initialized.\nCookie file: one valid MFA-derived browser Cookie request-header value; private regular file (chmod 600 on Unix). Login/enrollment remain in browser.\nBank/payables: bank {company,year?}, bank_accounts {company}, payables {company,status?,asOf?}, expense_booking_options {company,id}; create_bank_account/import_bank/register_payable/match_bank_expense {company,body,confirm:true}; pay_payable additionally id. Exact native flat body; discover contentSchema via tools/list. No generic bank/payable idempotency keys or retries.\nTools: accounts/contacts/documents {company}, invoices {company,year?}; preview_invoice {company,body} returns PDF integer bytes. create_customer/create_vendor/post_invoice {company,body,confirm:true}; update_customer/update_vendor additionally id; delete_customer/delete_vendor {company,id,body,confirm:true} with exact body {confirm:true} and existing obligation/history guards; issue_invoice additionally previewReviewed:true after previewing identical body. Native flat body strings follow tools/list contentSchema; no domain idempotency keys, no retry. Reconcile uncertain domain writes through read/list before further action.\nSAF-T: saft_export {company,body,confirm:true}, exact4096-byte {periodStart,periodEnd,confirm:true}, returns tar integer bytes. Bounded custom v3 profile, audited export, no official generic validation/submission or retry.\nNew company: create_company {body,confirm:true}; WorkspaceManage, exact native16KiB JSON without inner confirm/key/company selector. Creates fresh ledger, no copied chart or retries; unknown outcomes require companies/profile reconciliation.\nReminder claims: invoice_reminder_claims {company,id}; register_invoice_reminder/post_invoice_reminder/settle_invoice_claims {company,body,confirm:true}, native16KiB. Separate registration/posting, current incomingDKK claim receipt only after principal paid, no email, automatic reminders, keys or retries.\nWorkspace backup: workspace_backup {confirm:true}, max96MiB exact native MIME/integer bytes. No selectors/body/path/key/files/verification/restore or automatic retry; independent operator verify/restore remains server CLI.\nIntercompany: intercompany {company}; group_reconciliation {asOf} global, no company; apply_group_manifest/propose_intercompany_mapping/approve_intercompany_mapping/revoke_intercompany_mapping {company,body,confirm:true}. Exact native256KiB inner-confirm JSON, ordinary1MiB frames. Current all-reference scope, independent authenticated approval and exact mappingHash; preserve decimal strings, no keys/defaults/calculation/FX/consolidation/retries. Unknown requires canonical current/reconciliation reads.\nOwnership: ownership {company,asOf}; ownership_history {company,snapshotId?,asOf?}; propose_ownership/review_ownership/apply_ownership {company,body,confirm:true}. Proposal2MiB/2048facts/frame4MiB+64KiB; review/apply32KiB. Independent authenticated review and exact server snapshot/diff hashes; omissions never revoke. No client ownership math, generated keys/hashes, automatic retry, accounting or consolidation-manifest change.\nCorporate: corporate_records {company,cursor?,limit?}; corporate_record/corporate_record_original {company,recordId}; ingest_corporate_record {company,body,confirm:true}; link_corporate_record/enrich_corporate_record/supersede_corporate_record additionally recordId. Ingest24MiB JSON/16MiB original, corporate-ingest frame48MiB+64KiB, otherwrites32KiB. Original exact {ok,bytesBase64,sha256}, no invented MIME/filename/files/verification. Server source/sensitivity/all-scope authority; no keys/retries/accounting.\nInbox: workspace_inbox {company,cursor?,limit?}; workspace_inbox_source {company,sourceId}; ingest_workspace_inbox {company,body,confirm:true}; assign_workspace_inbox/complete_workspace_inbox additionally sourceId with exact {companySlug,confirm:true}. Ingest35MiB JSON/25MiB upload, named upload frame70MiB+64KiB; other tools1MiB. Explicit independent assignment/completion, no accounting, inference, generated keys or retries. Read canonical source and target documents after unknown outcome.\nParties: workspace_parties {company,query?,cursor?,limit?}; workspace_party {company,partyId}; create_workspace_party {company,body,confirm:true}; link_workspace_party_role/assert_workspace_party_field/assert_workspace_party_alias additionally partyId. Exact256KiB inner confirm. Current visible scope/full associated MasterData for global assertions, no accounting/default inference/merge/retries.\nKnowledge: company_knowledge {company,asOf?,includeProposed?}; propose_company_knowledge/review_company_knowledge/supersede_company_knowledge {company,body,confirm:true}, exact256KiB inner confirm. Advisory only, no automatic accounting or retry; superseded replacement requires separate review.\nCompany setup: company_profile {company}; update_company_profile/create_account/confirm_account_role {company,body,confirm:true}, native16KiB. Profile/account have no inner confirm; role does. Current company only, explicit account/role choices; no inferred defaults/new company/journal/key/retry.\nPrivacy: retention {company}; gdpr_export/gdpr_erase {company,body,confirm:true}, native16KiB inner confirm. Export is audited mutation; review full retained/erasable/unknown/coverage before explicit scoped overlay. Erase forbids asOf, uses trusted server date; no physical/backups erasure guarantee or retries.\nBudget: budget/budget_vs_actual {company,year}; budget_dimension_actuals additionally dimension?/member?; set_budget {company,body,confirm:true}, native16KiB body WITHOUT inner confirm/key. Mileage: mileage {company,year}; register_mileage {company,body,confirm:true}, native32KiB inner confirm. Server arithmetic/advisories only; no automatic retry or journal effects.\nAccruals: accruals {company}; accrual_schedule {company,id}; register_accrual {company,body,confirm:true}; recognize_accrual additionally id. Native256KiB, source/current accounts/server schedule; explicit periodIndex/date, no keys/math/automatic recognition/retries. Registration posts journal once without dedup guarantee.\nAssets: assets {company}; next_asset_depreciation {company,id}; register_asset {company,body,confirm:true}; depreciate_asset additionally id. Exact native256KiB, source-linked capitalisation and server schedule math; review explicit periodIndex, omitted selects next. No keys or automatic retries/runs.\nRecurring: recurring_invoices {company}; create_recurring_invoice {company,body,confirm:true}; generate_recurring_invoice/retire_recurring_invoice additionally id. Exact native256KiB body, no caller keys/timers/retries/hidden posting; only server-defined template-period dedup.\nBank correction: bank_reconciliation_correction_plan {company,bankTransactionId,replacementJournalEntryId}; bank_reconciliation_correction {company,body,confirm:true}, exact16KiB native body with caller idempotencyKey. Server-proven actual reversal/replacement, evidence-only effects, explicit same-principal receipt replay only, no automatic retry.\nBank link: bank_reconciliation_candidates {company,bankTransactionId}, bank_reconciliation_link {company,body,confirm:true}, exact native16KiB body and reviewed server planHash. Link-only, no keys/hash generation or automatic retry.\nDocuments: ingest_document {company,body,confirm:true}, exact native JSON string max35MiB UTF-8 bytes; bounded ingest-only frame70MiB+64KiB; document_original {company,id} returns recorded MIME/exact original integer bytes (max25MiB). No local document files, keys or retries.\nReviewed: reviewed_bank/accounting_drafts/posting_rules {company}; create_bank_draft {company,body,confirm:true}; submit_draft/reject_draft/approve_draft additionally draftId. Server-owned four-eyes authority, exact native bodies and evidence hashes; no automatic retries.\nAnnual: annual_report {company,fiscalYearStart,fiscalYearEnd} reads preparation/blockers unchanged; no confirm/body/files or filing.\nTools: me {}, companies {}, journal/trial_balance/income_statement/balance {company,year?,account?}; post_journal/reverse_journal {company,envelope}.\ncompany is an explicit company slug. envelope is an exact JSON string containing the server's confirmed request and caller idempotencyKey; never retried or rewritten.\nHTTP requires explicit --allow-loopback-http and literal loopback IP. No local fallback. Unknown write outcomes require reconciliation with original idempotency key.\nStdin messages limited to 1 MiB except tools/call ingest_document/ingest_workspace_inbox: frame70MiB+64KiB, native body35MiB, decoded file25MiB; ingest_corporate_record instead frame48MiB+64KiB/body24MiB/original16MiB; string request IDs to 256 bytes; sessions to 100000 requests. Tools execute sequentially with bounded HTTP timeouts. Close stdin to stop. stdout contains protocol messages only during server operation."
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
        if !["--server", "--cookie-file"].contains(&arg) {
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
    let server = flags
        .get("--server")
        .ok_or("explicit --server origin required")?;
    let path = flags
        .get("--cookie-file")
        .ok_or("--cookie-file with valid MFA-derived session required")?;
    let cookie = read_cookie_file(Path::new(path)).map_err(
        |_| "cookie file unavailable or invalid; require private regular file (chmod 600)",
    )?;
    ApiClient::new(server, allow_http, Some(cookie))
        .map_err(|_| "invalid server origin or Cookie header; see --help")
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
                    let name=if value.get("method").and_then(Value::as_str)==Some("tools/call"){value.pointer("/params/name").and_then(Value::as_str)}else{None};
                    let frame_limit=match name{Some("ingest_document"|"ingest_workspace_inbox")=>MAX_DOCUMENT_LINE,Some("ingest_corporate_record")=>MAX_CORPORATE_LINE,Some("propose_ownership")=>MAX_OWNERSHIP_LINE,_=>MAX_LINE};
                    if line.len() > frame_limit {
                        oversized = true;
                        Some(failure(
                            Value::Null,
                            -32700,
                            "message exceeds tool frame bound; only named original uploads/ownership proposals admit larger bounded frames",
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
