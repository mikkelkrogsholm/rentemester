use rentemester_client::{
    ApiClient, BankMutation, Confirmation, DomainMutation, Error as ClientError, JournalMutation,
    LoginProgress, MfaFactor, PeriodMutation, Report,
};
use rentemester_engine::Error;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};

type Failure = (u8, Error);
fn invalid(message: &str) -> Failure {
    (2, Error::new("invalid_input", message))
}

pub fn run(args: &[String]) -> Result<Value, Failure> {
    if args == ["--help"] {
        let mut commands=vec!["workspace-transfer-status","workspace-transfer-freeze","workspace-transfer-cancel","workspace-transfer-export","workspace-transfer-release","workspace-transfer-activate","restored-receipts","reconcile-restored-receipt","invoice-email-status","invoice-email-send","invoice-email-reconcile","invoice-email-retry","hosted-recurring-plan","consent-hosted-recurring","revoke-hosted-recurring","service-principals","create-service-principal","rotate-service-principal","revoke-service-principal","recover-service-principal","scheduled-recurring","scheduled-recurring-plan","enable-scheduled-recurring","pause-scheduled-recurring","resume-scheduled-recurring","cancel-scheduled-recurring","catch-up-scheduled-recurring","skip-scheduled-recurring","dispatch-scheduled-recurring","reconsent-scheduled-recurring","eliminations","group-eliminations","plan-elimination","propose-elimination","approve-elimination","reject-elimination","apply-elimination","reverse-elimination","consolidation","group-report-profiles","group-consolidated-report","propose-consolidation-profile","approve-consolidation-profile","revoke-consolidation-profile","intercompany","group-reconciliation","apply-group-manifest","propose-intercompany-mapping","approve-intercompany-mapping","revoke-intercompany-mapping","ownership","ownership-history","propose-ownership","review-ownership","apply-ownership","corporate-records","corporate-record","corporate-record-original","ingest-corporate-record","link-corporate-record","enrich-corporate-record","supersede-corporate-record","workspace-inbox","workspace-inbox-source","ingest-workspace-inbox","assign-workspace-inbox","complete-workspace-inbox","workspace-parties","workspace-party","create-workspace-party","link-workspace-party-role","assert-workspace-party-field","assert-workspace-party-alias","workspace-backup","company-knowledge","propose-company-knowledge","review-company-knowledge","supersede-company-knowledge","create-company","company-profile","update-company-profile","create-account","confirm-account-role","retention","gdpr-export","gdpr-erase","budget","budget-vs-actual","budget-dimension-actuals","set-budget","mileage","register-mileage","accruals","accrual-schedule","register-accrual","recognize-accrual","assets","next-asset-depreciation","register-asset","depreciate-asset","recurring-invoices","create-recurring-invoice","generate-recurring-invoice","retire-recurring-invoice","bank-reconciliation-correction-plan","bank-reconciliation-correction","bank-reconciliation-candidates","bank-reconciliation-link","ingest-document","document-original","reviewed-bank","accounting-drafts","posting-rules","create-bank-draft","submit-draft","reject-draft","approve-draft","annual-report","health","me","companies","journal","trial-balance","income-statement","balance","post-journal","reverse-journal","login","accounts","contacts","documents","invoices","invoice-pdf","preview-invoice","create-customer","update-customer","delete-customer","create-vendor","update-vendor","delete-vendor","issue-invoice","post-invoice","settle-invoice","credit-invoice","refund-invoice","bank","bank-accounts","payables","expense-booking-options","create-bank-account","import-bank","register-payable","pay-payable","match-bank-expense","vat","periods","period-close-readiness","review-period","close-period","reopen-period","report-period","supplier-commitments","supplier-commitment-matches","plan-supplier-commitment","approve-supplier-commitment","change-supplier-commitment","match-supplier-commitment"];
        commands.extend(["bilagsmail", "configure-bilagsmail", "delete-bilagsmail-config", "set-bilagsmail-alias", "poll-bilagsmail", "review-workspace-inbox", "workspace-inbox-original"]);
        let mut help = json!({"ok":true,
            "usage":"remote --server ORIGIN [--allow-loopback-http] [--cookie-file PATH | --service-key-file PATH] COMMAND [--company SLUG] [--year YEAR] [--account CODE]",
            "commands":commands,
            "authentication":"Supply a valid MFA-derived browser session as one Cookie request-header value in an owner-only regular file (0600). No Set-Cookie attributes. Authenticated commands require --cookie-file or the explicitly supported --service-key-file alternative. Login uses --email EMAIL --factor totp|backup --session-out NEW_PATH, reads password then factor code as two newline-terminated stdin lines; stdin must be piped/private. Output file is created0600 and never overwritten. MFA enrollment remains in browser.",
            "writes":"post-journal/reverse-journal read exact bounded server JSON envelope from stdin; requires confirm:true and input.idempotencyKey. Authenticated server session determines actor. No retry; outcome_unknown requires reconciliation using original key.",
            "domainWrites":"Native flat JSON body on stdin, forwarded byte-for-byte once. Domain writes require --confirm; updates/deletions also --id. Deletion body is {confirm:true}; open obligations and historical links can refuse deletion. issue-invoice requires --preview-reviewed after preview-invoice with identical body. These native endpoints do NOT support idempotency keys: never add a key/envelope or auto-retry; reconcile outcome_unknown using contacts/invoices/journal/documents. Native post-invoice body also contains confirm:true. Preview returns PDF bytes as JSON integer array and creates no invoice. Native invoices support private full and simplified invoices, explicit tax classifications and frozen foreign-currency valuation; EU B2B services require explicit VIES validation. Optional deliveryDate or paired deliveryPeriodStart/deliveryPeriodEnd and document/line unitCode are preserved in original and PDF. Ordinary buyer identifiers may be foreign without implying tax treatment or VIES validity. Foreign reverse-charge bases support EU_B2B_SERVICES, DK_MOMSLOVEN_§46_STK_1_NR_3, EU_MOMSDIREKTIV_ART_196 and EU_MOMSDIREKTIV_ART_199; all require trusted VIES and explicit note. This is not a TypeScript-workspace adoption command. Reduced simplified bodies use invoiceType:simplified, description-only lines and totals.grossAmount/vatRate:0.25/vatComputationBasis:VAT_20_OF_GROSS; maximum gross DKK3000 after conversion. Contacts contains customers and vendors. invoices optionally accepts --year.",
            "domainBodySchemas":{"contactDelete":rentemester_client::domain_body_schema(DomainMutation::DeleteCustomer(1)),"customerCreate":rentemester_client::domain_body_schema(DomainMutation::CreateCustomer),"customerUpdate":rentemester_client::domain_body_schema(DomainMutation::UpdateCustomer(1)),"vendorCreate":rentemester_client::domain_body_schema(DomainMutation::CreateVendor),"vendorUpdate":rentemester_client::domain_body_schema(DomainMutation::UpdateVendor(1)),"invoicePreviewOrIssue":rentemester_client::invoice_body_schema(),"invoicePost":rentemester_client::domain_body_schema(DomainMutation::PostInvoice)},
            "bankPayables":"bank/bank-accounts/payables/expense-booking-options are reads. bank accepts optional --year; payables accepts --status open|paid|overdue|all and --as-of YYYY-MM-DD; expense-booking-options requires --id DOCUMENT_ID. Bank/payable writes use exact native flat stdin JSON (max262144 bytes) and --confirm; pay-payable requires --id PAYABLE_ID. Native body also contains confirm:true except create-bank-account. CSV is UTF-8: omit profile or use null/empty for generic ISO dates; danske-bank selects semicolon CSV with declared day-month-year dates. Unknown profiles and ambiguous generic dates are refused. match-bank-expense posts a documented purchase against a bank row via documents/book-expense, not arbitrary journal matching. Registration uses immutable original, expense account and confirmed creditors/VAT roles. Payment uses outgoing DKK row, mapped bank ledger account and current open balance. No generic idempotency keys/retry: reconcile uncertain writes using read/list first.",
            "bankBodySchemas":{"createBankAccount":rentemester_client::bank_body_schema(BankMutation::CreateAccount),"importBank":rentemester_client::bank_body_schema(BankMutation::ImportStatement),"registerPayable":rentemester_client::bank_body_schema(BankMutation::RegisterPayable),"payPayable":rentemester_client::bank_body_schema(BankMutation::PayPayable(1)),"matchBankExpense":rentemester_client::bank_body_schema(BankMutation::MatchExpense)},
            "supplierCommitments":crate::commitments::help(),
            "periodLifecycle":"vat optionally accepts --year; periods lists effective locks and original closePacketHash. period-close-readiness requires --from/--to ISO dates. review-period/close-period/reopen-period/report-period require --confirm and exact native stdin body. Review appends audit and returns review.id plus review.packet.hash; close must explicitly cite both, refuses stale proof and blockers. force:true unavailable. Reopen requires justification and unreported closed period. Report records an already completed external filing receipt against original close hash; no tax submission. No auto-retry or generic idempotency; reconcile outcome_unknown with periods/readiness before action.",
            "periodBodySchemas":{"review":rentemester_client::period_body_schema(PeriodMutation::Review),"close":rentemester_client::period_body_schema(PeriodMutation::Close),"reopen":rentemester_client::period_body_schema(PeriodMutation::Reopen),"report":rentemester_client::period_body_schema(PeriodMutation::Report)},
            "invoicePdf":"invoice-pdf --company SLUG --id POSITIVE_DOCUMENT_ID reads the immutable issued original via its current authenticated documents-read permission. No stdin body or --confirm. Output data.pdf is {contentType:application/pdf,bytes:[exact byte integers]}; creates no local files.",
            "receivables":crate::receivables::help(),
            "reviewedBookkeeping":crate::reviewed_bookkeeping::help(),
            "workspaceBackup":"workspace-backup --confirm; explicit server/cookie-file, no stdin body or company/year/id/body/path/key selectors. Sends exact {confirm:true} once; returns data.backup {contentType,bytes} with max96MiB exact native artifact bytes. No local file writes, archive verification/restore claim or automatic retry. Independent operator verify/restore remains server CLI.",
            "saftExport":"saft-export --company SLUG --confirm with exact stdin {periodStart,periodEnd,confirm:true}, max4096 bytes. Audited bounded custom v3 XML tar download returned as JSON {export:{contentType,bytes}}, no local files/official validation/submission or automatic retries.",
            "reminderClaims":"invoice-reminder-claims --company SLUG --id ID; register-invoice-reminder/post-invoice-reminder/settle-invoice-claims --company SLUG --confirm with exact16KiB stdin body. Separate registration/posting, incoming DKK claim receipts after principal. No email/automatic reminders/keys/retries; canonical read first after unknown write.",
            "companySetup":crate::company_setup::help(),
            "companyCreation":crate::company_creation::help(),
            "privacyLifecycle":crate::privacy::help(),
            "budgetLifecycle":crate::budget::help(),
            "workspaceParties":crate::parties::help(),
            "workspaceInbox":crate::workspace_inbox::help(),
            "corporateRecords":crate::corporate_records::help(),
            "ownershipGraph":crate::ownership::help(),
            "intercompany":crate::intercompany::help(),
            "companyKnowledge":crate::company_knowledge::help(),
            "mileageLifecycle":crate::mileage::help(),
            "accrualLifecycle":crate::accruals::help(),
            "assetLifecycle":crate::assets::help(),
            "recurringInvoices":crate::recurring_invoices::help(),
            "bankCorrection":{"usage":"bank-reconciliation-correction-plan --company SLUG --bank-transaction-id ID --replacement-journal-entry-id ID reads actual reversal/replacement/history proof. bank-reconciliation-correction --company SLUG --confirm < exact-native.json, native16KiB body with original caller idempotencyKey. Explicit server/cookie required. Server CompanyRead/CompanyLedgerPost and current source/original/principal checks apply. Correction only appends evidence after actual reversal child and replacement already exist; no accounting writes. Preserve full nested plan/result/receipt JSON. No generated keys/hashes, normalization or automatic retries. On unknown read canonical bank/history first; only explicit exact key/body/current-principal action may recover durable receipt; changed key/stale chain/payload refuse.","bodySchema":rentemester_client::bank_correction_body_schema()},
            "bankReconciliation":{"usage":"bank-reconciliation-candidates --company SLUG --bank-transaction-id ID reads actual server plans; bank-reconciliation-link --company SLUG --confirm < exact-native.json. Explicit server/cookie required. Native body16KiB {bankTransactionId,journalEntryId,planHash,note,confirm:true}; review exact current source/account/movement/journal/document evidence before selecting server planHash. Server controls archived source/current facts and CompanyRead/CompanyLedgerPost authority. Link only, no new journal/payable/payment, no correction flow. No invented proof/actor/keys/retry; duplicates/stale refused. On outcome_unknown inspect canonical bank reconciliationId/proof and candidates before any further action; repeating link is not a safe retry.","bodySchema":rentemester_client::bank_reconciliation_body_schema()},
            "documentEvidence":{"usage":"ingest-document --company SLUG --confirm < exact-native-upload.json; document-original --company SLUG --id POSITIVE_ID. Explicit server/cookie flags required. No document file access. Ingest max35MiB JSON; original max25MiB bytes in data.original {contentType,bytes}. Server enforces base64/MIME/hash/retention/metadata/current upload/read authority. No forced/vendor-linked uploads, keys or retries; outcome_unknown requires documents/original reads and original request preservation; duplicate content is a definite refusal, not permission to retry.","bodySchema":rentemester_client::document_ingest_schema()},
            "annualReport":"annual-report --company SLUG --fiscal-year-start YYYY-MM-DD --fiscal-year-end YYYY-MM-DD. Read-only class-B preparation; server report.ok:false preserves prerequisite errors. No stdin body, confirmation or files; no filing/export.",
            "transport":"HTTPS only unless --allow-loopback-http and literal loopback IP; --server is an origin without path/query/userinfo. No local fallback, redirects, ambient proxies or automatic retries.",
            "exitCodes":{"0":"success","1":"server/transport/protocol failure","2":"usage/configuration failure"},
            "example":"rentemester-rust remote --server http://127.0.0.1:8080 --allow-loopback-http --cookie-file /private/session.cookie trial-balance --company example --year 2026"});
        help["serviceAuthentication"] = json!("Private --service-key-file replaces --cookie-file for me/companies, journal/report reads, post-journal/reverse-journal only. Current server permissions apply; no lifecycle/admin access, Bearer/raw-secret flags, fallback or retry.");
        help["publicEInvoice"]=crate::public_einvoice::help();
        help["commands"].as_array_mut().unwrap().extend(crate::public_einvoice::COMMANDS.map(|s|json!(s)));
        help["bilagsmail"] = crate::bilagsmail::help();
        help["invoiceExtraction"]=crate::invoice_extraction::help();
        help["commands"].as_array_mut().unwrap().extend(crate::invoice_extraction::COMMANDS.map(|s|json!(s)));
        help["documentPdf"]=json!({"usage":"parse-document --company SLUG --id ID --confirm; document-parse-status --company SLUG --id ID; document-parsed-text --company SLUG --id ID [--cursor OFFSET] [--limit 1..10]; parse-pending-documents --company SLUG --confirm [--cursor ID] [--limit 1..100]. Explicit --server/--cookie-file required. Text default10 pages, pending default100 documents, cursor default0. No stdin, local parsing, OCR, metadata/accounting inference, keys or retries. Server owns source/evidence integrity and upload/read authority; inspect status after an uncertain parse outcome."});
        help["commands"].as_array_mut().unwrap().extend(["parse-document","document-parse-status","document-parsed-text","parse-pending-documents"].map(|s|json!(s)));
        help["service_principals"]=crate::service_principals::help();
        help["commands"].as_array_mut().unwrap().extend([json!("overview"),json!("report-export")]);
        help["reportReads"]=json!({"usage":"journal/trial-balance/income-statement/balance/overview --company SLUG [--year LABEL] [--source live|archive]; --account only journal. report-export --company SLUG --report journal|trial-balance|income-statement|balance --format csv|pdf [--year LABEL] [--source live|archive] [--account journalonly]. Read-only, explicit server/cookie. Server owns source selection and unavailable states. Preserve exact decimal strings. Exports return data {ok,contentType,bytes}, max96MiB; no files/client calculations. Omitted source uses server live-first selection; explicit archive never falls back."});
        help["openingBalance"]=crate::opening_balance::help();
        help["commands"].as_array_mut().unwrap().extend(crate::opening_balance::COMMANDS.map(|s|json!(s)));
        help["archiveImport"]=crate::archive_import::help();
        help["commands"].as_array_mut().unwrap().extend(crate::archive_import::COMMANDS.map(|s|json!(s)));
        help["ossReport"]=crate::oss_report::help();
        help["commands"].as_array_mut().unwrap().push(json!("oss-report"));
        help["euSales"]=crate::eu_sales::help();
        help["commands"].as_array_mut().unwrap().extend([json!("eu-sales"),json!("vies-validate")]);
        help["portfolioCashflow"]=crate::portfolio_cashflow::help();
        help["commands"].as_array_mut().unwrap().extend([json!("portfolio"),json!("cashflow")]);
        help["cfoAnalytics"]=crate::cfo_analytics::help();
        help["commands"].as_array_mut().unwrap().push(json!("cfo-analytics"));
        help["contactImport"]=crate::contact_import::help();
        help["commands"].as_array_mut().unwrap().push(json!("import-contacts"));
        help["intercompanyDispositions"]=crate::intercompany_dispositions::help();
        help["commands"].as_array_mut().unwrap().extend(["group-disposition-status","plan-group-disposition","propose-group-disposition","approve-group-disposition","link-group-disposition","settle-group-disposition","supersede-group-disposition","reopen-group-disposition"].map(|s|json!(s)));
        help["directBankPayableCorrection"]=crate::payable_correction::help();
        help["commands"].as_array_mut().unwrap().extend(["direct-bank-payable-correction-plan","direct-bank-payable-correction"].map(|s|json!(s)));
        help["cvr"]=crate::cvr::help();
        help["commands"].as_array_mut().unwrap().extend(["cvr-status","cvr-lookup","sync-company-cvr"].map(|s|json!(s)));
        help["bankMatches"]=crate::bank_matches::help();
        help["commands"].as_array_mut().unwrap().push(json!("bank-suggest-matches"));
        help["bankImport"]=json!("import-bank --company SLUG --confirm: pipe one native JSON object with confirm:true and exactly one csvContent (UTF-8 text) or csvBase64 (canonical base64 original bytes). Optional account/profile; 35MiB envelope,25MiB original. Generic UTF-8/BOM; danske-bank also Windows1252. No client decoding, keys, actor or automatic retries.");
        help["exceptions"]=crate::exceptions::help();
        help["commands"].as_array_mut().unwrap().extend(["list-exceptions","resolve-exception"].map(|s|json!(s)));
        help["bookkeepingBatch"]=crate::bookkeeping_batch::help();
        help["commands"].as_array_mut().unwrap().extend(["bookkeeping-batch-plan","bookkeeping-batch-status","bookkeeping-batch-persist","bookkeeping-batch-approve","bookkeeping-batch-apply"].map(|s|json!(s)));
        help["workspaceTransfer"]=crate::workspace_transfer::help();
        help["invoiceEmail"]=crate::invoice_email::help();
        help["hosted_recurring"]=crate::hosted_recurring::help();
        help["scheduledRecurring"]=crate::scheduled_recurring::help();
        help["eliminations"] = crate::elimination::help();
        help["consolidation"] = crate::consolidation::help();
        return Ok(help);
    }
    let mut flags = BTreeMap::new();
    let mut command = None;
    let mut allow_http = false;
    let mut confirmed = false;
    let mut preview_reviewed = false;
    let mut cursor = 0;
    while cursor < args.len() {
        let arg = args[cursor].as_str();
        if arg == "--confirm" || arg == "--preview-reviewed" {
            let target = if arg == "--confirm" {
                &mut confirmed
            } else {
                &mut preview_reviewed
            };
            if *target {
                return Err(invalid("duplicate confirmation flag"));
            }
            *target = true;
            cursor += 1;
            continue;
        }
        if arg == "--allow-loopback-http" {
            if allow_http {
                return Err(invalid("duplicate --allow-loopback-http"));
            }
            allow_http = true;
            cursor += 1;
            continue;
        }
        if arg.starts_with("--") {
            if ![
                "--vat-or-cvr",
                "--document-id",
                "--source",
                "--report",
                "--format",
                "--fiscal-year",
                "--import-id",
                "--scope",
                "--company-slugs-json",
                "--group-profile-id",
                "--party",
                "--currency",
                "--aggregate",
                "--dimension",
                "--member",
                "--replacement-journal-entry-id",
                "--bank-transaction-id",
                "--max",
                "--cvr",
                "--draft-id",
                "--fiscal-year-start",
                "--fiscal-year-end",
                "--run-key",
                "--accounting-from",
                "--accounting-to",
                "--bank-from",
                "--bank-to",
                "--server",
                "--cookie-file",
                "--service-key-file",
                "--company",
                "--status",
                "--as-of",
                "--include-proposed",
                "--include-archived",
                "--party-id",
                "--source-id",
                "--output",
                "--record-id",
                "--snapshot-id",
                "--profile-id",
                "--query",
                "--cursor",
                "--limit",
                "--start-date",
                "--from",
                "--to",
                "--id",
                "--year",
                "--account",
                "--email",
                "--factor",
                "--session-out",
            ]
            .contains(&arg)
            {
                return Err(invalid("unknown remote flag; see remote --help"));
            }
            let value = args
                .get(cursor + 1)
                .filter(|s| !s.is_empty() && !s.starts_with("--"))
                .ok_or_else(|| invalid("remote flag requires a value"))?;
            if flags.insert(arg, value.as_str()).is_some() {
                return Err(invalid("duplicate remote flag"));
            }
            cursor += 2;
        } else {
            if command.replace(arg).is_some() {
                return Err(invalid("expected one remote command"));
            }
            cursor += 1;
        }
    }
    let command = command.ok_or_else(|| invalid("expected remote command; see remote --help"))?;
    if flags.contains_key("--cookie-file") && flags.contains_key("--service-key-file") {
        return Err(invalid("choose one of --cookie-file or --service-key-file"));
    }
    if flags.contains_key("--service-key-file")
        && ![
            "health",
            "me",
            "companies",
            "journal",
            "trial-balance",
            "income-statement",
            "balance",
            "overview",
            "post-journal",
            "reverse-journal",
        ]
        .contains(&command)
    {
        return Err(invalid(
            "service authentication is supported only for context, company reports and manual journal post/reversal",
        ));
    }
    if crate::invoice_extraction::accepts(command) {
        return crate::invoice_extraction::run(command,&flags,allow_http,confirmed,preview_reviewed);
    }
    if flags.contains_key("--document-id") {return Err(invalid("document-id allowed only for invoice extraction commands"));}
    if !crate::report_reads::accepts(command) && flags.keys().any(|k|["--source","--report","--format"].contains(k)){return Err(invalid("source/report/format allowed only on report reads/exports"));}
    if command == "login" {
        if confirmed || preview_reviewed {
            return Err(invalid("confirmation flags are not login flags"));
        }
        return login(&flags, allow_http);
    }
    if flags.contains_key("--email")
        || flags.contains_key("--factor")
        || flags.contains_key("--session-out")
    {
        return Err(invalid("email/factor/session-out allowed only with login"));
    }
    if crate::report_reads::accepts(command){return crate::report_reads::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if command=="oss-report"{return crate::oss_report::run(&flags,allow_http,confirmed,preview_reviewed);}
    if crate::eu_sales::accepts(command){return crate::eu_sales::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::portfolio_cashflow::accepts(command){return crate::portfolio_cashflow::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::cfo_analytics::accepts(command){return crate::cfo_analytics::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if flags.keys().any(|k|["--scope","--company-slugs-json","--group-profile-id","--party","--currency","--aggregate"].contains(k)){return Err(invalid("CFO selectors allowed only on cfo-analytics"));}
    if crate::opening_balance::accepts(command){return crate::opening_balance::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::archive_import::accepts(command){return crate::archive_import::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if flags.keys().any(|k|["--fiscal-year","--import-id"].contains(k)){return Err(invalid("archive selectors allowed only on archive commands"));}
    if command=="create-company" {return crate::company_creation::run(&flags,allow_http,confirmed,preview_reviewed);}
    if command=="workspace-backup"{return crate::portable_backup::run(&flags,allow_http,confirmed,preview_reviewed);}
    if command=="saft-export"{return crate::saft::run(&flags,allow_http,confirmed,preview_reviewed);}
    if crate::reminder_claims::accepts(command){return crate::reminder_claims::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::company_setup::accepts(command){return crate::company_setup::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::privacy::accepts(command){return crate::privacy::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::budget::accepts(command){return crate::budget::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if flags.contains_key("--dimension")||flags.contains_key("--member"){return Err(invalid("dimension/member only on budget-dimension-actuals"));}
    if flags.contains_key("--profile-id") && command != "group-consolidated-report" { return Err(invalid("profile-id allowed only for group-consolidated-report")); }
    if crate::consolidation::accepts(command){return crate::consolidation::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::elimination::accepts(command){return crate::elimination::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::intercompany::accepts(command){return crate::intercompany::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::ownership::accepts(command){return crate::ownership::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::corporate_records::accepts(command){return crate::corporate_records::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::public_einvoice::accepts(command){return crate::public_einvoice::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::bilagsmail::accepts(command){return crate::bilagsmail::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::workspace_inbox::accepts(command){return crate::workspace_inbox::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::parties::accepts(command){return crate::parties::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::company_knowledge::accepts(command){return crate::company_knowledge::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::cvr::accepts(command){return crate::cvr::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if flags.contains_key("--cvr"){return Err(invalid("--cvr is only supported by cvr-lookup"));}
    if crate::bank_matches::accepts(command){return crate::bank_matches::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if flags.contains_key("--max"){return Err(invalid("--max is only supported by bank-suggest-matches"));}
    if crate::exceptions::accepts(command){return crate::exceptions::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::mileage::accepts(command){return crate::mileage::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::accruals::accepts(command){return crate::accruals::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::assets::accepts(command){return crate::assets::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::service_principals::accepts(command){return crate::service_principals::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::contact_import::accepts(command){return crate::contact_import::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::intercompany_dispositions::accepts(command){return crate::intercompany_dispositions::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::payable_correction::accepts(command){return crate::payable_correction::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::bookkeeping_batch::accepts(command){return crate::bookkeeping_batch::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::workspace_transfer::accepts(command){return crate::workspace_transfer::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::invoice_email::accepts(command){return crate::invoice_email::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::hosted_recurring::accepts(command){return crate::hosted_recurring::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::scheduled_recurring::accepts(command){return crate::scheduled_recurring::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::recurring_invoices::accepts(command){return crate::recurring_invoices::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if matches!(command,"bank-reconciliation-correction-plan"|"bank-reconciliation-correction") {
        return crate::bank_correction::run(command,&flags,allow_http,confirmed,preview_reviewed);
    }
    if flags.contains_key("--replacement-journal-entry-id") {return Err(invalid("replacement-journal-entry-id allowed only on correction-plan"));}
    if matches!(command,"bank-reconciliation-candidates"|"bank-reconciliation-link") {
        return crate::bank_reconciliation::run(command,&flags,allow_http,confirmed,preview_reviewed);
    }
    if flags.contains_key("--bank-transaction-id") {return Err(invalid("bank-transaction-id allowed only on reconciliation candidates"));}
    if crate::document_pdf::accepts(command) {
        return crate::document_pdf::run(command,&flags,allow_http,confirmed,preview_reviewed);
    }
    if matches!(command,"ingest-document"|"document-original") {
        return crate::documents::run(command,&flags,allow_http,confirmed,preview_reviewed);
    }
    if crate::reviewed_bookkeeping::accepts(command) {
        return crate::reviewed_bookkeeping::run(command, &flags, allow_http, confirmed, preview_reviewed);
    }
    if flags.contains_key("--draft-id") {return Err(invalid("draft-id allowed only on submit/reject/approve-draft"));}
    if command == "annual-report" {
        return crate::annual_reports::run(&flags, allow_http, confirmed, preview_reviewed);
    }
    if flags.contains_key("--fiscal-year-start") || flags.contains_key("--fiscal-year-end") {
        return Err(invalid("fiscal-year-start/end allowed only on annual-report"));
    }
    if crate::commitments::accepts(command) {
        return crate::commitments::run(command, &flags, allow_http, confirmed, preview_reviewed);
    }
    if flags.contains_key("--start-date") {
        return Err(invalid("start-date allowed only on supplier-commitments"));
    }
    let report = match command {
        "journal" => Some(Report::Journal),
        "trial-balance" => Some(Report::TrialBalance),
        "income-statement" => Some(Report::IncomeStatement),
        "balance" => Some(Report::Balance),
        "health"
        | "me"
        | "companies"
        | "post-journal"
        | "reverse-journal"
        | "contacts"
        | "accounts"
        | "documents"
        | "invoices"
        | "invoice-pdf"
        | "preview-invoice"
        | "create-customer"
        | "update-customer"
        | "delete-customer"
        | "create-vendor"
        | "update-vendor"
        | "delete-vendor"
        | "issue-invoice"
        | "post-invoice"
        | "settle-invoice"
        | "credit-invoice"
        | "refund-invoice"
        | "bank"
        | "bank-accounts"
        | "payables"
        | "expense-booking-options"
        | "create-bank-account"
        | "import-bank"
        | "register-payable"
        | "pay-payable"
        | "match-bank-expense"
        | "vat"
        | "periods"
        | "period-close-readiness"
        | "review-period"
        | "close-period"
        | "reopen-period"
        | "report-period" => None,
        _ => return Err(invalid("unknown remote command")),
    };
    let period_operation = match command {
        "review-period" => Some(PeriodMutation::Review),
        "close-period" => Some(PeriodMutation::Close),
        "reopen-period" => Some(PeriodMutation::Reopen),
        "report-period" => Some(PeriodMutation::Report),
        _ => None,
    };
    if (command == "period-close-readiness")
        != (flags.contains_key("--from") && flags.contains_key("--to"))
        || (command != "period-close-readiness"
            && (flags.contains_key("--from") || flags.contains_key("--to")))
    {
        return Err(invalid(
            "only period-close-readiness requires --from and --to",
        ));
    }
    let receivables_operation = crate::receivables::operation(command);
    let mutation = matches!(command, "post-journal" | "reverse-journal");
    let domain = match command {
        "create-customer" => Some(DomainMutation::CreateCustomer),
        "create-vendor" => Some(DomainMutation::CreateVendor),
        "issue-invoice" => Some(DomainMutation::IssueInvoice),
        "post-invoice" => Some(DomainMutation::PostInvoice),
        "update-customer" | "update-vendor" | "delete-customer" | "delete-vendor" => {
            let id = flags
                .get("--id")
                .and_then(|v| v.parse::<i64>().ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("update/deletion requires positive --id"))?;
            Some(match command {
                "update-customer" => DomainMutation::UpdateCustomer(id),
                "update-vendor" => DomainMutation::UpdateVendor(id),
                "delete-customer" => DomainMutation::DeleteCustomer(id),
                _ => DomainMutation::DeleteVendor(id),
            })
        }
        _ => None,
    };
    let bank_operation = match command {
        "create-bank-account" => Some(BankMutation::CreateAccount),
        "import-bank" => Some(BankMutation::ImportStatement),
        "register-payable" => Some(BankMutation::RegisterPayable),
        "match-bank-expense" => Some(BankMutation::MatchExpense),
        "pay-payable" => Some(BankMutation::PayPayable(
            flags
                .get("--id")
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("payment requires positive --id PAYABLE_ID"))?,
        )),
        _ => None,
    };
    if matches!(command, "expense-booking-options" | "invoice-pdf")
        && flags
            .get("--id")
            .and_then(|s| s.parse::<i64>().ok())
            .is_none_or(|n| n <= 0)
    {
        return Err(invalid(
            "PDF/booking options require positive --id DOCUMENT_ID",
        ));
    }
    if command != "payables" && (flags.contains_key("--status") || flags.contains_key("--as-of")) {
        return Err(invalid("status/as-of allowed only on payables"));
    }
    if flags.contains_key("--id")
        && !matches!(
            command,
            "update-customer"
                | "update-vendor"
                | "delete-customer"
                | "delete-vendor"
                | "pay-payable"
                | "expense-booking-options"
                | "invoice-pdf"
        )
    {
        return Err(invalid(
            "id allowed only on contact updates/deletions, payable payment, invoice PDF or document booking options",
        ));
    }
    if confirmed
        != (domain.is_some()
            || bank_operation.is_some()
            || period_operation.is_some()
            || receivables_operation.is_some())
        || preview_reviewed != (command == "issue-invoice")
    {
        return Err(invalid(
            "domain writes require --confirm; issue-invoice also requires --preview-reviewed after previewing identical input",
        ));
    }
    let company = flags.get("--company").copied();
    if (report.is_some()
        || mutation
        || period_operation.is_some()
        || receivables_operation.is_some()
        || domain.is_some()
        || bank_operation.is_some()
        || matches!(
            command,
            "contacts"
                | "accounts"
                | "documents"
                | "invoices"
                | "preview-invoice"
                | "bank"
                | "bank-accounts"
                | "payables"
                | "expense-booking-options"
                | "invoice-pdf"
                | "vat"
                | "periods"
                | "period-close-readiness"
        ))
        != company.is_some()
        || ((report.is_none() && !matches!(command, "invoices" | "bank" | "vat"))
            && flags.contains_key("--year"))
        || (report.is_none() && flags.contains_key("--account"))
    {
        return Err(invalid(
            "company required for company operations; year allowed on reports/invoices/bank, account only on reports",
        ));
    }
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server origin required"))?;
    let client = credential_client(server, allow_http, &flags, command == "health")?;
    let body = if mutation
        || receivables_operation.is_some()
        || domain.is_some()
        || bank_operation.is_some()
        || period_operation.is_some()
        || command == "preview-invoice"
    {
        let mut body = Vec::new();
        std::io::stdin()
            .lock()
            .take(if command == "import-bank" { 35 * 1024 * 1024 + 1 } else { 262_145 })
            .read_to_end(&mut body)
            .map_err(|_| invalid("could not read request from stdin"))?;
        Some(body)
    } else {
        None
    };
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let response = runtime
        .block_on(async {
            if let Some(report) = report {
                client
                    .report(
                        company.unwrap(),
                        report,
                        flags.get("--year").copied(),
                        flags.get("--account").copied(),
                    )
                    .await
            } else if let Some(operation) = receivables_operation {
                client
                    .receivables_mutation(company.unwrap(), operation, body.unwrap(), confirmed)
                    .await
            } else if let Some(operation) = domain {
                client
                    .domain_mutation(
                        company.unwrap(),
                        operation,
                        body.unwrap(),
                        Confirmation {
                            confirm: confirmed,
                            invoice_preview_reviewed: preview_reviewed,
                        },
                    )
                    .await
            } else if let Some(operation) = period_operation {
                client
                    .period_mutation(company.unwrap(), operation, body.unwrap(), confirmed)
                    .await
            } else if let Some(operation) = bank_operation {
                client
                    .bank_mutation(company.unwrap(), operation, body.unwrap(), confirmed)
                    .await
            } else {
                match command {
                    "vat" => {
                        client
                            .vat(company.unwrap(), flags.get("--year").copied())
                            .await
                    }
                    "periods" => client.periods(company.unwrap()).await,
                    "period-close-readiness" => {
                        client
                            .period_close_readiness(
                                company.unwrap(),
                                flags["--from"],
                                flags["--to"],
                            )
                            .await
                    }
                    "health" => client.health().await,
                    "me" => client.me().await,
                    "companies" => client.companies().await,
                    "bank" => {
                        client
                            .bank(company.unwrap(), flags.get("--year").copied())
                            .await
                    }
                    "bank-accounts" => client.bank_accounts(company.unwrap()).await,
                    "payables" => {
                        client
                            .payables(
                                company.unwrap(),
                                flags.get("--status").copied(),
                                flags.get("--as-of").copied(),
                            )
                            .await
                    }
                    "expense-booking-options" => {
                        client
                            .expense_booking_options(
                                company.unwrap(),
                                flags["--id"].parse::<i64>().unwrap(),
                            )
                            .await
                    }
                    "accounts" => client.accounts(company.unwrap()).await,
                    "contacts" => client.contacts(company.unwrap()).await,
                    "documents" => client.documents(company.unwrap()).await,
                    "invoice-pdf" => {
                        client
                            .invoice_pdf(company.unwrap(), flags["--id"].parse::<i64>().unwrap())
                            .await
                    }
                    "invoices" => {
                        client
                            .invoices(company.unwrap(), flags.get("--year").copied())
                            .await
                    }
                    "preview-invoice" => {
                        client
                            .invoice_preview(company.unwrap(), body.unwrap())
                            .await
                    }
                    _ => {
                        client
                            .manual_journal(
                                company.unwrap(),
                                if command == "post-journal" {
                                    JournalMutation::Post
                                } else {
                                    JournalMutation::Reverse
                                },
                                body.unwrap(),
                            )
                            .await
                    }
                }
            }
        })
        .map_err(client_error)?;
    Ok(json!({"ok":response.get("ok").and_then(Value::as_bool).unwrap_or(true),"data":response}))
}
pub(crate) fn client_error(error: ClientError) -> Failure {
    let (exit, code) = match error {
        ClientError::InvalidInput(_) => (2, "invalid_input"),
        ClientError::Transport => (1, "transport"),
        ClientError::Refused { .. } => (1, "server_refused"),
        ClientError::Protocol => (1, "protocol"),
        ClientError::OutcomeUnknown => (1, "outcome_unknown"),
    };
    (exit, Error::new(code, error.to_string()))
}

fn login(flags: &BTreeMap<&str, &str>, allow_http: bool) -> Result<Value, Failure> {
    use std::io::{BufRead, IsTerminal};
    if flags
        .keys()
        .any(|key| !["--server", "--email", "--factor", "--session-out"].contains(key))
    {
        return Err(invalid(
            "login cannot use existing cookie/company/report flags",
        ));
    }
    let server = flags
        .get("--server")
        .ok_or_else(|| invalid("explicit --server origin required"))?;
    let email = flags
        .get("--email")
        .ok_or_else(|| invalid("login requires --email"))?;
    let factor = match flags.get("--factor").copied() {
        Some("totp") => MfaFactor::Totp,
        Some("backup") => MfaFactor::BackupCode,
        _ => return Err(invalid("login requires --factor totp or backup")),
    };
    let output = std::path::Path::new(
        flags
            .get("--session-out")
            .ok_or_else(|| invalid("login requires --session-out NEW_PATH"))?,
    );
    match std::fs::symlink_metadata(output) {
        Ok(_) => {
            return Err(invalid(
                "session output already exists; choose a new private path",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(invalid("cannot inspect session output path")),
    }
    let client = ApiClient::new(server, allow_http, None).map_err(client_error)?;
    let input = std::io::stdin();
    if input.is_terminal() {
        return Err(invalid(
            "pipe password and factor code via private stdin; terminal echo is refused",
        ));
    }
    let mut input = input.lock();
    fn secret(input: &mut impl BufRead) -> Result<String, Failure> {
        let mut bytes = Vec::new();
        input
            .by_ref()
            .take(4099)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| invalid("could not read secret stdin line"))?;
        if bytes.pop() != Some(b'\n') {
            return Err(invalid("secret stdin lines must end in newline"));
        }
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        if bytes.is_empty() || bytes.len() > 4096 {
            return Err(invalid("secret stdin line outside server input bounds"));
        }
        String::from_utf8(bytes).map_err(|_| invalid("secret stdin line must be UTF-8"))
    }
    let password = secret(&mut input)?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|_| (1, Error::new("transport", "could not start HTTP runtime")))?;
    let pending = match runtime
        .block_on(client.begin_login(email, password))
        .map_err(client_error)?
    {
        LoginProgress::MfaRequired(pending) => pending,
        LoginProgress::EnrollmentRequired => {
            return Ok(
                json!({"ok":false,"code":"mfa_enrollment_required","errors":["MFA enrollment required; complete enrollment in server UI"]}),
            );
        }
    };
    let code = secret(&mut input)?;
    let session = runtime
        .block_on(client.complete_login(&pending, factor, code))
        .map_err(client_error)?;
    session
        .write_cookie_file_new(output)
        .map_err(client_error)?;
    Ok(json!({"ok":true,"data":{"status":"authenticated"}}))
}

pub(crate) fn credential_client(
    server: &str,
    allow_http: bool,
    flags: &BTreeMap<&str, &str>,
    anonymous: bool,
) -> Result<ApiClient, (u8, Error)> {
    match (flags.get("--cookie-file"), flags.get("--service-key-file")) {
        (Some(_), Some(_)) => Err(invalid("choose one of --cookie-file or --service-key-file")),
        (Some(path), None) => {
            let cookie = rentemester_client::read_cookie_file(std::path::Path::new(path))
                .map_err(client_error)?;
            ApiClient::new(server, allow_http, Some(cookie)).map_err(client_error)
        }
        (None, Some(path)) => {
            let key = rentemester_client::read_service_key_file(std::path::Path::new(path))
                .map_err(client_error)?;
            ApiClient::with_service_key(server, allow_http, key).map_err(client_error)
        }
        (None, None) if anonymous => ApiClient::new(server, allow_http, None).map_err(client_error),
        _ => Err(invalid(
            "authenticated remote commands require --cookie-file or --service-key-file",
        )),
    }
}
