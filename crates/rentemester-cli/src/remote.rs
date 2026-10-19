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
        return Ok(json!({"ok":true,
            "usage":"remote --server ORIGIN [--allow-loopback-http] [--cookie-file PATH] COMMAND [--company SLUG] [--year YEAR] [--account CODE]",
            "commands":["retention","gdpr-export","gdpr-erase","budget","budget-vs-actual","budget-dimension-actuals","set-budget","mileage","register-mileage","accruals","accrual-schedule","register-accrual","recognize-accrual","assets","next-asset-depreciation","register-asset","depreciate-asset","recurring-invoices","create-recurring-invoice","generate-recurring-invoice","retire-recurring-invoice","bank-reconciliation-correction-plan","bank-reconciliation-correction","bank-reconciliation-candidates","bank-reconciliation-link","ingest-document","document-original","reviewed-bank","accounting-drafts","posting-rules","create-bank-draft","submit-draft","reject-draft","approve-draft","annual-report","health","me","companies","journal","trial-balance","income-statement","balance","post-journal","reverse-journal","login","accounts","contacts","documents","invoices","invoice-pdf","preview-invoice","create-customer","update-customer","delete-customer","create-vendor","update-vendor","delete-vendor","issue-invoice","post-invoice","settle-invoice","credit-invoice","refund-invoice","bank","bank-accounts","payables","expense-booking-options","create-bank-account","import-bank","register-payable","pay-payable","match-bank-expense","vat","periods","period-close-readiness","review-period","close-period","reopen-period","report-period","supplier-commitments","supplier-commitment-matches","plan-supplier-commitment","approve-supplier-commitment","change-supplier-commitment","match-supplier-commitment"],
            "authentication":"Supply a valid MFA-derived browser session as one Cookie request-header value in an owner-only regular file (0600). No Set-Cookie attributes. All commands except health/login require --cookie-file. Login uses --email EMAIL --factor totp|backup --session-out NEW_PATH, reads password then factor code as two newline-terminated stdin lines; stdin must be piped/private. Output file is created0600 and never overwritten. MFA enrollment remains in browser.",
            "writes":"post-journal/reverse-journal read exact bounded server JSON envelope from stdin; requires confirm:true and input.idempotencyKey. Authenticated server session determines actor. No retry; outcome_unknown requires reconciliation using original key.",
            "domainWrites":"Native flat JSON body on stdin, forwarded byte-for-byte once. Domain writes require --confirm; updates/deletions also --id. Deletion body is {confirm:true}; open obligations and historical links can refuse deletion. issue-invoice requires --preview-reviewed after preview-invoice with identical body. These native endpoints do NOT support idempotency keys: never add a key/envelope or auto-retry; reconcile outcome_unknown using contacts/invoices/journal/documents. Native post-invoice body also contains confirm:true. Preview returns PDF bytes as JSON integer array and creates no invoice. Native invoices currently support DKK/25% VAT. Contacts contains customers and vendors. invoices optionally accepts --year.",
            "domainBodySchemas":{"contactDelete":rentemester_client::domain_body_schema(DomainMutation::DeleteCustomer(1)),"customerCreate":rentemester_client::domain_body_schema(DomainMutation::CreateCustomer),"customerUpdate":rentemester_client::domain_body_schema(DomainMutation::UpdateCustomer(1)),"vendorCreate":rentemester_client::domain_body_schema(DomainMutation::CreateVendor),"vendorUpdate":rentemester_client::domain_body_schema(DomainMutation::UpdateVendor(1)),"invoicePreviewOrIssue":rentemester_client::invoice_body_schema(),"invoicePost":rentemester_client::domain_body_schema(DomainMutation::PostInvoice)},
            "bankPayables":"bank/bank-accounts/payables/expense-booking-options are reads. bank accepts optional --year; payables accepts --status open|paid|overdue|all and --as-of YYYY-MM-DD; expense-booking-options requires --id DOCUMENT_ID. Bank/payable writes use exact native flat stdin JSON (max262144 bytes) and --confirm; pay-payable requires --id PAYABLE_ID. Native body also contains confirm:true except create-bank-account. CSV is generic UTF-8 with ISO dates; named profiles unavailable. match-bank-expense posts a documented purchase against a bank row via documents/book-expense, not arbitrary journal matching. Registration uses immutable original, expense account and confirmed creditors/VAT roles. Payment uses outgoing DKK row, mapped bank ledger account and current open balance. No generic idempotency keys/retry: reconcile uncertain writes using read/list first.",
            "bankBodySchemas":{"createBankAccount":rentemester_client::bank_body_schema(BankMutation::CreateAccount),"importBank":rentemester_client::bank_body_schema(BankMutation::ImportStatement),"registerPayable":rentemester_client::bank_body_schema(BankMutation::RegisterPayable),"payPayable":rentemester_client::bank_body_schema(BankMutation::PayPayable(1)),"matchBankExpense":rentemester_client::bank_body_schema(BankMutation::MatchExpense)},
            "supplierCommitments":crate::commitments::help(),
            "periodLifecycle":"vat optionally accepts --year; periods lists effective locks and original closePacketHash. period-close-readiness requires --from/--to ISO dates. review-period/close-period/reopen-period/report-period require --confirm and exact native stdin body. Review appends audit and returns review.id plus review.packet.hash; close must explicitly cite both, refuses stale proof and blockers. force:true unavailable. Reopen requires justification and unreported closed period. Report records an already completed external filing receipt against original close hash; no tax submission. No auto-retry or generic idempotency; reconcile outcome_unknown with periods/readiness before action.",
            "periodBodySchemas":{"review":rentemester_client::period_body_schema(PeriodMutation::Review),"close":rentemester_client::period_body_schema(PeriodMutation::Close),"reopen":rentemester_client::period_body_schema(PeriodMutation::Reopen),"report":rentemester_client::period_body_schema(PeriodMutation::Report)},
            "invoicePdf":"invoice-pdf --company SLUG --id POSITIVE_DOCUMENT_ID reads the immutable issued original via its current authenticated documents-read permission. No stdin body or --confirm. Output data.pdf is {contentType:application/pdf,bytes:[exact byte integers]}; creates no local files.",
            "receivables":crate::receivables::help(),
            "reviewedBookkeeping":crate::reviewed_bookkeeping::help(),
            "privacyLifecycle":crate::privacy::help(),
            "budgetLifecycle":crate::budget::help(),
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
            "example":"rentemester-rust remote --server http://127.0.0.1:8080 --allow-loopback-http --cookie-file /private/session.cookie trial-balance --company example --year 2026"}));
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
                "--dimension",
                "--member",
                "--replacement-journal-entry-id",
                "--bank-transaction-id",
                "--draft-id",
                "--fiscal-year-start",
                "--fiscal-year-end",
                "--server",
                "--cookie-file",
                "--company",
                "--status",
                "--as-of",
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
    if crate::privacy::accepts(command){return crate::privacy::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::budget::accepts(command){return crate::budget::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if flags.contains_key("--dimension")||flags.contains_key("--member"){return Err(invalid("dimension/member only on budget-dimension-actuals"));}
    if crate::mileage::accepts(command){return crate::mileage::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::accruals::accepts(command){return crate::accruals::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::assets::accepts(command){return crate::assets::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if crate::recurring_invoices::accepts(command){return crate::recurring_invoices::run(command,&flags,allow_http,confirmed,preview_reviewed);}
    if matches!(command,"bank-reconciliation-correction-plan"|"bank-reconciliation-correction") {
        return crate::bank_correction::run(command,&flags,allow_http,confirmed,preview_reviewed);
    }
    if flags.contains_key("--replacement-journal-entry-id") {return Err(invalid("replacement-journal-entry-id allowed only on correction-plan"));}
    if matches!(command,"bank-reconciliation-candidates"|"bank-reconciliation-link") {
        return crate::bank_reconciliation::run(command,&flags,allow_http,confirmed,preview_reviewed);
    }
    if flags.contains_key("--bank-transaction-id") {return Err(invalid("bank-transaction-id allowed only on reconciliation candidates"));}
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
    let cookie = match flags.get("--cookie-file") {
        Some(path) => Some(
            rentemester_client::read_cookie_file(std::path::Path::new(path))
                .map_err(client_error)?,
        ),
        None if command == "health" => None,
        None => {
            return Err(invalid(
                "authenticated remote commands require --cookie-file",
            ));
        }
    };
    let client = ApiClient::new(server, allow_http, cookie).map_err(client_error)?;
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
            .take(262_145)
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
