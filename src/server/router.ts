// HTTP request handler for the cockpit backend (#170).
//
// `handleRequest` is the pure heart of the server: a `(Request, config) =>
// Promise<Response>` with no `Bun.serve` dependency, so tests drive it
// directly. `src/cli/serve.ts` wires it into `Bun.serve`.
//
// Request flow, in order, for EVERY request:
//   1. authMiddleware  — the single auth seam (throws ApiError to reject)
//   2. route dispatch  — match method + path
//   3. handler         — a read, a workspace-management op, or a write
//   4. error edge      — any throw is mapped to a safe JSON error here
//
// No handler does its own auth and no handler shapes its own errors: both
// concerns live exactly once, here. Bookkeeping WRITE routes (#213) go through
// the `withCompanyMutation` pipeline in `mutations.ts`, which adds the backup
// lock, the confirm gate, actor attribution and the localhost hard-gate that
// the agent CLI / MCP stacks enforce — the server does not inherit them.

import type { ServerConfig } from "./config";
import { authMiddleware } from "./auth";
import { ApiError, toErrorResponse } from "./errors";
import { serveStatic } from "./static";
import { jsonResponse } from "./router/_shared";
import {
  handleHealth,
  handleRules,
  handleSystemCvrStatus,
} from "./router/system";
import {
  handleCompanyList,
  handlePortfolio,
} from "./router/portfolio";
import {
  handleCompanyDashboard,
  handleCompanyFiscalYears,
  handleCompanyMultiYear,
  handleCompanyOverview,
} from "./router/dashboard";
import {
  handleCompanyBalance,
  handleCompanyIncomeStatement,
  handleCompanyJournal,
  handleCompanyJournalExport,
  handleCompanyStatementExport,
  handleCompanyTrialBalance,
  handleCompanyVatExport,
} from "./router/statements";
import {
  handleCompanyBank,
  handleCompanyBankAccounts,
} from "./router/bank";
import { handleCompanyVat } from "./router/vat";
import {
  handleCompanyDocumentBookingOptions,
  handleCompanyDocumentFile,
  handleCompanyDocuments,
} from "./router/documents";
import {
  handleCompanyInvoicePdf,
  handleCompanyInvoices,
  handleCompanyRecurringInvoices,
} from "./router/invoices";
import { handleCompanyContacts } from "./router/contacts";
import {
  handleAssetNextDepreciation,
  handleCompanyAssets,
} from "./router/assets";
import {
  handleCompanyAccounts,
  handleCompanyAccruals,
  handleCompanyAgentSuggestions,
  handleCompanyAnnualReport,
  handleCompanyArchiveYear,
  handleCompanyBilagsmail,
  handleCompanyBudget,
  handleCompanyBudgetVsActual,
  handleCompanyCashflow,
  handleCompanyExceptions,
  handleCompanyGdprExport,
  handleCompanyIntegrity,
  handleCompanyMileage,
  handleCompanyObligations,
  handleCompanyPayables,
  handleCompanyPeriods,
  handleCompanyRetention,
  handleCompanySettings,
  handleCompanySyncCvr,
} from "./router/company";
import {
  handleCompanyCreate,
  handleCompanyUpdate,
} from "./router/workspace-writes";
import {
  handleAccountantExport,
  handleApproveAgentSuggestion,
  handleAssetDepreciate,
  handleAssetRegister,
  handleAssetWriteOff,
  handleBankImport,
  handleClosePeriod,
  handleCreateBankAccount,
  handleDeleteBilagsmailImapConfig,
  handleGdprErase,
  handleSaveBilagsmailImapConfig,
  handleSetBilagsmailAlias,
  handleCompanyProfile,
  handleCreateCustomer,
  handleCreateVendor,
  handleCvrLookup,
  handleDeleteCustomer,
  handleDeleteVendor,
  handleDataImport,
  handleDocumentBookExpense,
  handleDocumentIngest,
  handleGenerateRecurringInvoice,
  handleInvoiceCreditNote,
  handleInvoiceSendPublic,
  handleInvoiceSendEmail,
  handleInvoiceSendReminder,
  handleInvoiceIssue,
  handleInvoicePost,
  handleCreateRecurringInvoiceTemplate,
  handleInvoicePreview,
  handleInvoiceSettle,
  handleMileageCreate,
  handlePayablePay,
  handlePayableRegister,
  handleRejectAgentSuggestion,
  handleReopenPeriod,
  handleResolveException,
  handleRetireRecurringInvoiceTemplate,
  handleSetBudget,
  handleUpdateCustomer,
  handleUpdateVendor,
} from "./write-handlers";

// --------------------------------------------------------------------------
// Route handlers — reads + workspace management only.
// --------------------------------------------------------------------------

/**
 * The HTTP route catalog (#376) — a machine-readable list of every route
 * `handleRequest` dispatches, used by `GET /api` and `GET /api/health` so an
 * agent can enumerate the HTTP surface without reading source. Each entry
 * carries the `method`, the path `pattern` (with `:param`-placeholders) and a
 * short Danish `summary`. Order is the dispatch order in `handleRequest` to
 * make drift obvious in code review.
 *
 * The list is exported so `tests/unit/surface-diff-discoverable.test.ts` can
 * assert that it stays the single source of truth for the catalog.
 */
export const ROUTE_CATALOG: ReadonlyArray<{
  method: string;
  pattern: string;
  summary: string;
}> = [
  { method: "GET", pattern: "/api", summary: "Sundhedstjek + rute-katalog." },
  { method: "GET", pattern: "/api/health", summary: "Alias for GET /api." },
  { method: "GET", pattern: "/api/rules", summary: "Lovgrundlag — bundler, regler og SHA-256-citationer (#347)." },
  { method: "GET", pattern: "/api/system/cvr-status", summary: "Er CVR-login konfigureret på serveren? (#402)" },
  { method: "GET", pattern: "/api/portfolio", summary: "Workspace-portfolio." },
  { method: "GET", pattern: "/api/companies", summary: "Lister virksomheder i workspacet." },
  { method: "POST", pattern: "/api/companies", summary: "Opretter virksomhed i workspacet." },
  { method: "PATCH", pattern: "/api/companies/:slug", summary: "Omdøber/arkiverer en virksomhed." },
  { method: "GET", pattern: "/api/companies/:slug/dashboard", summary: "Virksomhedens dashboard." },
  { method: "GET", pattern: "/api/companies/:slug/fiscal-years", summary: "Kendte regnskabsår." },
  { method: "GET", pattern: "/api/companies/:slug/overview", summary: "Nøgletalsoverblik." },
  { method: "GET", pattern: "/api/companies/:slug/income-statement", summary: "Resultatopgørelse." },
  { method: "GET", pattern: "/api/companies/:slug/income-statement/export", summary: "Resultatopgørelse som CSV-download (#372)." },
  { method: "GET", pattern: "/api/companies/:slug/balance", summary: "Balance." },
  { method: "GET", pattern: "/api/companies/:slug/balance/export", summary: "Balance som CSV-download (#372)." },
  { method: "GET", pattern: "/api/companies/:slug/trial-balance", summary: "Saldobalance." },
  { method: "GET", pattern: "/api/companies/:slug/trial-balance/export", summary: "Saldobalance som CSV-download (#372)." },
  { method: "GET", pattern: "/api/companies/:slug/journal", summary: "Journalposter." },
  { method: "GET", pattern: "/api/companies/:slug/journal/export", summary: "Posteringer (kassekladde) som CSV-download (#465)." },
  { method: "GET", pattern: "/api/companies/:slug/vat/export", summary: "Moms-rapport som PDF-download m. SKAT-rubrikker + frist (#464)." },
  { method: "GET", pattern: "/api/companies/:slug/retention", summary: "5-års retention-status pr. data-domæne (#343)." },
  { method: "GET", pattern: "/api/companies/:slug/integrity", summary: "Audit chain + backup status panel (#333)." },
  { method: "GET", pattern: "/api/companies/:slug/accounts", summary: "Kontoplan — read-only liste (#344)." },
  { method: "GET", pattern: "/api/companies/:slug/bank", summary: "Bank-transaktioner." },
  { method: "GET", pattern: "/api/companies/:slug/vat", summary: "Momsoplysninger." },
  { method: "GET", pattern: "/api/companies/:slug/documents", summary: "Bilagsliste." },
  { method: "GET", pattern: "/api/companies/:slug/documents/:id/file", summary: "Henter et bilag." },
  { method: "GET", pattern: "/api/companies/:slug/documents/:id/booking-options", summary: "Forslagsdata til bogføring af et bilag." },
  { method: "GET", pattern: "/api/companies/:slug/recurring-invoices", summary: "Gentagende fakturaer." },
  { method: "POST", pattern: "/api/companies/:slug/recurring-invoices", summary: "Opretter faktura-skabelon (#386)." },
  { method: "POST", pattern: "/api/companies/:slug/recurring-invoices/:id/generate", summary: "Materialiserer en gentagende faktura." },
  { method: "GET", pattern: "/api/companies/:slug/archive/:year", summary: "Arkiveret regnskabsår." },
  { method: "GET", pattern: "/api/companies/:slug/multi-year", summary: "Flerårsoversigt." },
  { method: "GET", pattern: "/api/companies/:slug/invoices", summary: "Udstedte fakturaer." },
  { method: "GET", pattern: "/api/companies/:slug/invoices/:id/pdf", summary: "Henter en faktura-PDF." },
  { method: "GET", pattern: "/api/companies/:slug/contacts", summary: "Kunder + leverandører." },
  { method: "POST", pattern: "/api/companies/:slug/customers", summary: "Opretter kunde." },
  { method: "PATCH", pattern: "/api/companies/:slug/customers/:id", summary: "Opdaterer kunde." },
  { method: "DELETE", pattern: "/api/companies/:slug/customers/:id", summary: "Sletter kunde (#430). Blokeres ved åbne fakturaer." },
  { method: "POST", pattern: "/api/companies/:slug/vendors", summary: "Opretter leverandør." },
  { method: "PATCH", pattern: "/api/companies/:slug/vendors/:id", summary: "Opdaterer leverandør." },
  { method: "DELETE", pattern: "/api/companies/:slug/vendors/:id", summary: "Sletter leverandør (#430). Blokeres ved åbne gælder." },
  { method: "GET", pattern: "/api/companies/:slug/cvr-lookup", summary: "Slår CVR op." },
  { method: "GET", pattern: "/api/companies/:slug/company", summary: "Virksomhedens stamdata." },
  { method: "PATCH", pattern: "/api/companies/:slug/company", summary: "Opdaterer stamdata + bank/betaling." },
  { method: "POST", pattern: "/api/companies/:slug/sync-cvr", summary: "Synkroniserer stamdata fra CVR." },
  { method: "GET", pattern: "/api/companies/:slug/obligations", summary: "Frister og forpligtelser." },
  { method: "GET", pattern: "/api/companies/:slug/cashflow", summary: "Likviditetsprognose." },
  { method: "GET", pattern: "/api/companies/:slug/budget", summary: "Budget pr. konto pr. måned (#339)." },
  { method: "GET", pattern: "/api/companies/:slug/budget-vs-actual", summary: "Budget vs. faktisk for året (#339)." },
  { method: "POST", pattern: "/api/companies/:slug/budget", summary: "Sætter (append-only revision) en budgetlinje (#339)." },
  { method: "GET", pattern: "/api/companies/:slug/exceptions", summary: "Exceptions queue — undtagelser, filtrerbar pr. status (#332)." },
  { method: "POST", pattern: "/api/companies/:slug/exceptions/:id/resolve", summary: "Løser en exception." },
  { method: "GET", pattern: "/api/companies/:slug/periods", summary: "Periodelås-liste med effective status (#342)." },
  { method: "GET", pattern: "/api/companies/:slug/bank-accounts", summary: "Registrerede bankkonti + CSV-mapping-profiler (#345)." },
  { method: "POST", pattern: "/api/companies/:slug/bank-accounts", summary: "Opretter en bankkonto (#345)." },
  { method: "GET", pattern: "/api/companies/:slug/gdpr/export", summary: "GDPR-indsigt — finder personoplysninger for en data-subject (#334)." },
  { method: "POST", pattern: "/api/companies/:slug/gdpr/erase", summary: "GDPR-anonymisering — append-only tombstones (#334)." },
  { method: "GET", pattern: "/api/companies/:slug/bilagsmail", summary: "Bilagsmail-status: IMAP-config, alias, inbox (#348/#350/#351)." },
  { method: "POST", pattern: "/api/companies/:slug/bilagsmail/imap-config", summary: "Gemmer IMAP-config til config/imap.json (#348)." },
  { method: "DELETE", pattern: "/api/companies/:slug/bilagsmail/imap-config", summary: "Sletter den gemte IMAP-config (#348)." },
  { method: "PATCH", pattern: "/api/companies/:slug/bilagsmail/alias", summary: "Sætter eller rydder mail-alias (#350)." },
  { method: "GET", pattern: "/api/companies/:slug/accruals", summary: "Periodiseringsregister (#337)." },
  { method: "GET", pattern: "/api/companies/:slug/annual-report", summary: "Årsrapport-builder (regnskabsklasse-B) (#338)." },
  { method: "POST", pattern: "/api/companies/:slug/bank/import", summary: "Importerer bank-CSV." },
  { method: "POST", pattern: "/api/companies/:slug/import", summary: "Generel data-import." },
  { method: "POST", pattern: "/api/companies/:slug/accountant-export", summary: "Revisor-eksport (.tar)." },
  { method: "POST", pattern: "/api/companies/:slug/documents/ingest", summary: "Modtager et bilag." },
  { method: "POST", pattern: "/api/companies/:slug/documents/book-expense", summary: "Bogfører et bilag som udgift mod en banktransaktion." },
  { method: "POST", pattern: "/api/companies/:slug/invoices/issue", summary: "Udsteder en faktura." },
  { method: "POST", pattern: "/api/companies/:slug/invoices/preview", summary: "Forhåndsviser en faktura-PDF uden at udstede." },
  { method: "POST", pattern: "/api/companies/:slug/invoices/post", summary: "Bogfører en udstedt faktura." },
  { method: "POST", pattern: "/api/companies/:slug/invoices/settle", summary: "Afregner faktura fra bank." },
  { method: "POST", pattern: "/api/companies/:slug/invoices/credit-note", summary: "Udsteder kreditnota." },
  { method: "POST", pattern: "/api/companies/:slug/invoices/send-public", summary: "Sender faktura som e-faktura (NemHandel/PEPPOL)." },
  { method: "POST", pattern: "/api/companies/:slug/invoices/send-email", summary: "Sender faktura til kundens e-mail med PDF vedhæftet." },
  { method: "POST", pattern: "/api/companies/:slug/invoices/send-reminder", summary: "Registrerer rykker (rentel. § 9b) og sender den på e-mail." },
  { method: "POST", pattern: "/api/companies/:slug/periods/close", summary: "Lukker regnskabsperiode." },
  { method: "POST", pattern: "/api/companies/:slug/periods/reopen", summary: "Genåbner regnskabsperiode (#247-modstykke til CLI-only)." },
  { method: "GET", pattern: "/api/companies/:slug/mileage", summary: "Kørselsregister for valgt regnskabsår (#335)." },
  { method: "POST", pattern: "/api/companies/:slug/mileage", summary: "Registrerer en kørsel (#335)." },
  { method: "GET", pattern: "/api/companies/:slug/assets", summary: "Anlægskartotek — kapitaliserede aktiver + straksafskrivninger (#336)." },
  { method: "POST", pattern: "/api/companies/:slug/assets", summary: "Registrerer et anlæg + lineær afskrivningsplan (#336)." },
  { method: "GET", pattern: "/api/companies/:slug/assets/:id/next-depreciation", summary: "Næste afskrivningsperiode for et anlæg (#336)." },
  { method: "POST", pattern: "/api/companies/:slug/assets/:id/depreciate", summary: "Bogfører næste afskrivningsperiode (#336)." },
  { method: "POST", pattern: "/api/companies/:slug/assets/write-off", summary: "Straksafskriver et småanskaffelse (#336)." },
  { method: "GET", pattern: "/api/companies/:slug/payables", summary: "Leverandørfaktura-arbejdsbord — kreditorliste + modal-data (#340)." },
  { method: "POST", pattern: "/api/companies/:slug/payables", summary: "Registrerer et bilag som leverandørfaktura (#340)." },
  { method: "POST", pattern: "/api/companies/:slug/payables/:id/pay", summary: "Markerer leverandørfaktura betalt fra bankpost (#340)." },
  { method: "GET", pattern: "/api/companies/:slug/agent-suggestions", summary: "Agent-forslag i kø — afventer ejerens godkendelse (#346)." },
  { method: "POST", pattern: "/api/companies/:slug/agent-suggestions/:id/approve", summary: "Ejer godkender agent-forslag — løser undtagelsen med 'Godkendt'-note (#346)." },
  { method: "POST", pattern: "/api/companies/:slug/agent-suggestions/:id/reject", summary: "Ejer afviser agent-forslag — løser undtagelsen med 'Afvist'-note (#346)." },
];

// --------------------------------------------------------------------------
// Dispatch
// --------------------------------------------------------------------------

/**
 * Handles one HTTP request end-to-end. Always resolves to a `Response` —
 * thrown `ApiError`s (and any other error) are mapped to safe JSON here.
 */
export async function handleRequest(
  request: Request,
  config: ServerConfig,
): Promise<Response> {
  try {
    // (1) The single auth seam — runs before any route logic. Phase 2 swaps
    // the body of authMiddleware; this call site never changes.
    authMiddleware(request, config);

    // (2) Route dispatch.
    const url = new URL(request.url);
    const path = url.pathname.replace(/\/+$/, "") || "/";
    const method = request.method.toUpperCase();

    if (path === "/api" || path === "/api/health") {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      return handleHealth(config, ROUTE_CATALOG);
    }

    // #402 — CVR-login status, so the cockpit can offer a friendly path
    // through "Hent fra CVR" instead of letting the owner click a button
    // that fails silently when the credentials are missing.
    if (path === "/api/system/cvr-status") {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      return handleSystemCvrStatus();
    }

    if (path === "/api/portfolio") {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      return handlePortfolio(config, url);
    }

    if (path === "/api/rules") {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      return handleRules();
    }

    if (path === "/api/companies") {
      if (method === "GET") return handleCompanyList(config);
      if (method === "POST") return await handleCompanyCreate(config, request);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    const dashboardMatch = /^\/api\/companies\/([^/]+)\/dashboard$/.exec(path);
    if (dashboardMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(dashboardMatch[1]!);
      return handleCompanyDashboard(config, slug, url);
    }

    const fiscalYearsMatch = /^\/api\/companies\/([^/]+)\/fiscal-years$/.exec(path);
    if (fiscalYearsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(fiscalYearsMatch[1]!);
      return handleCompanyFiscalYears(config, slug);
    }

    const overviewMatch = /^\/api\/companies\/([^/]+)\/overview$/.exec(path);
    if (overviewMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(overviewMatch[1]!);
      return handleCompanyOverview(config, slug, url);
    }

    const retentionMatch = /^\/api\/companies\/([^/]+)\/retention$/.exec(path);
    if (retentionMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(retentionMatch[1]!);
      return handleCompanyRetention(config, slug);
    }

    const integrityMatch = /^\/api\/companies\/([^/]+)\/integrity$/.exec(path);
    if (integrityMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(integrityMatch[1]!);
      return handleCompanyIntegrity(config, slug);
    }

    const accountsMatch = /^\/api\/companies\/([^/]+)\/accounts$/.exec(path);
    if (accountsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(accountsMatch[1]!);
      return handleCompanyAccounts(config, slug);
    }

    const incomeStatementExportMatch =
      /^\/api\/companies\/([^/]+)\/income-statement\/export$/.exec(path);
    if (incomeStatementExportMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(incomeStatementExportMatch[1]!);
      return handleCompanyStatementExport(config, slug, url, "income-statement");
    }

    const incomeStatementMatch =
      /^\/api\/companies\/([^/]+)\/income-statement$/.exec(path);
    if (incomeStatementMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(incomeStatementMatch[1]!);
      return handleCompanyIncomeStatement(config, slug, url);
    }

    const balanceExportMatch = /^\/api\/companies\/([^/]+)\/balance\/export$/.exec(path);
    if (balanceExportMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(balanceExportMatch[1]!);
      return handleCompanyStatementExport(config, slug, url, "balance");
    }

    const balanceMatch = /^\/api\/companies\/([^/]+)\/balance$/.exec(path);
    if (balanceMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(balanceMatch[1]!);
      return handleCompanyBalance(config, slug, url);
    }

    const trialBalanceExportMatch =
      /^\/api\/companies\/([^/]+)\/trial-balance\/export$/.exec(path);
    if (trialBalanceExportMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(trialBalanceExportMatch[1]!);
      return handleCompanyStatementExport(config, slug, url, "trial-balance");
    }

    const trialBalanceMatch =
      /^\/api\/companies\/([^/]+)\/trial-balance$/.exec(path);
    if (trialBalanceMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(trialBalanceMatch[1]!);
      return handleCompanyTrialBalance(config, slug, url);
    }

    const journalExportMatch = /^\/api\/companies\/([^/]+)\/journal\/export$/.exec(path);
    if (journalExportMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(journalExportMatch[1]!);
      return handleCompanyJournalExport(config, slug, url);
    }

    const vatExportMatch = /^\/api\/companies\/([^/]+)\/vat\/export$/.exec(path);
    if (vatExportMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(vatExportMatch[1]!);
      return handleCompanyVatExport(config, slug, url);
    }

    const journalMatch = /^\/api\/companies\/([^/]+)\/journal$/.exec(path);
    if (journalMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(journalMatch[1]!);
      return handleCompanyJournal(config, slug, url);
    }

    const bankMatch = /^\/api\/companies\/([^/]+)\/bank$/.exec(path);
    if (bankMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(bankMatch[1]!);
      return handleCompanyBank(config, slug, url);
    }

    const vatMatch = /^\/api\/companies\/([^/]+)\/vat$/.exec(path);
    if (vatMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(vatMatch[1]!);
      return handleCompanyVat(config, slug, url);
    }

    const documentsMatch = /^\/api\/companies\/([^/]+)\/documents$/.exec(path);
    if (documentsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(documentsMatch[1]!);
      return handleCompanyDocuments(config, slug);
    }

    const documentFileMatch =
      /^\/api\/companies\/([^/]+)\/documents\/(\d+)\/file$/.exec(path);
    if (documentFileMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(documentFileMatch[1]!);
      return handleCompanyDocumentFile(config, slug, documentFileMatch[2]!);
    }

    // The Bogfør-bilag modal pulls its picker rows from this endpoint (#407).
    const documentBookingOptionsMatch =
      /^\/api\/companies\/([^/]+)\/documents\/(\d+)\/booking-options$/.exec(path);
    if (documentBookingOptionsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(documentBookingOptionsMatch[1]!);
      return handleCompanyDocumentBookingOptions(
        config,
        slug,
        documentBookingOptionsMatch[2]!,
      );
    }

    const recurringInvoicesMatch =
      /^\/api\/companies\/([^/]+)\/recurring-invoices$/.exec(path);
    if (recurringInvoicesMatch) {
      const slug = decodeURIComponent(recurringInvoicesMatch[1]!);
      if (method === "GET") return handleCompanyRecurringInvoices(config, slug);
      // #386 — cockpit can create a recurring-invoice template instead of
      // having to drop to the CLI. POSTs through the same write pipeline as
      // the rest of the write-routes (backup lock, localhost gate, actor
      // attribution, requireConfirm) — see `handleCreateRecurringInvoiceTemplate`.
      if (method === "POST")
        return await handleCreateRecurringInvoiceTemplate(config, request, slug);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    const recurringInvoiceGenerateMatch =
      /^\/api\/companies\/([^/]+)\/recurring-invoices\/(\d+)\/generate$/.exec(path);
    if (recurringInvoiceGenerateMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(recurringInvoiceGenerateMatch[1]!);
      return await handleGenerateRecurringInvoice(
        config,
        request,
        slug,
        recurringInvoiceGenerateMatch[2]!,
      );
    }

    // Cockpit write route (#435) — deactivate (retire) a recurring-invoice
    // template so it stops suggesting itself when the underlying contract has
    // ended. Templates are append-only by schema: the trigger forbids
    // unretiring, and identity/payload columns cannot be mutated. Owners who
    // need to change terms create a new template that supersedes the retired
    // one — historical generations on the old template stay intact.
    const recurringInvoiceRetireMatch =
      /^\/api\/companies\/([^/]+)\/recurring-invoices\/(\d+)\/retire$/.exec(path);
    if (recurringInvoiceRetireMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(recurringInvoiceRetireMatch[1]!);
      return await handleRetireRecurringInvoiceTemplate(
        config,
        request,
        slug,
        recurringInvoiceRetireMatch[2]!,
      );
    }

    const archiveMatch =
      /^\/api\/companies\/([^/]+)\/archive\/([^/]+)$/.exec(path);
    if (archiveMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(archiveMatch[1]!);
      const year = decodeURIComponent(archiveMatch[2]!);
      return handleCompanyArchiveYear(config, slug, year);
    }

    const multiYearMatch = /^\/api\/companies\/([^/]+)\/multi-year$/.exec(path);
    if (multiYearMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(multiYearMatch[1]!);
      return handleCompanyMultiYear(config, slug);
    }

    const invoicesMatch = /^\/api\/companies\/([^/]+)\/invoices$/.exec(path);
    if (invoicesMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(invoicesMatch[1]!);
      return handleCompanyInvoices(config, slug, url);
    }

    // Cockpit read route (#378): serve the issued-invoice PDF so the owner
    // can download/forward it without leaving the browser. Re-uses the same
    // `renderIssuedInvoicePdf` core the CLI runs — no new rendering path.
    const invoicePdfMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/(\d+)\/pdf$/.exec(path);
    if (invoicePdfMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(invoicePdfMatch[1]!);
      return handleCompanyInvoicePdf(config, slug, invoicePdfMatch[2]!);
    }

    const contactsMatch = /^\/api\/companies\/([^/]+)\/contacts$/.exec(path);
    if (contactsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(contactsMatch[1]!);
      return handleCompanyContacts(config, slug);
    }

    // Cockpit write routes for contacts (#390) — create + edit kunder/leverandører
    // from the Kontakter page instead of the CLI.
    const createCustomerMatch =
      /^\/api\/companies\/([^/]+)\/customers$/.exec(path);
    if (createCustomerMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(createCustomerMatch[1]!);
      return await handleCreateCustomer(config, request, slug);
    }

    const customerByIdMatch =
      /^\/api\/companies\/([^/]+)\/customers\/(\d+)$/.exec(path);
    if (customerByIdMatch) {
      const slug = decodeURIComponent(customerByIdMatch[1]!);
      if (method === "PATCH") {
        return await handleUpdateCustomer(
          config,
          request,
          slug,
          customerByIdMatch[2]!,
        );
      }
      if (method === "DELETE") {
        return await handleDeleteCustomer(
          config,
          request,
          slug,
          customerByIdMatch[2]!,
        );
      }
      throw ApiError.methodNotAllowed("kun PATCH eller DELETE er understøttet på denne rute");
    }

    const createVendorMatch =
      /^\/api\/companies\/([^/]+)\/vendors$/.exec(path);
    if (createVendorMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(createVendorMatch[1]!);
      return await handleCreateVendor(config, request, slug);
    }

    const vendorByIdMatch =
      /^\/api\/companies\/([^/]+)\/vendors\/(\d+)$/.exec(path);
    if (vendorByIdMatch) {
      const slug = decodeURIComponent(vendorByIdMatch[1]!);
      if (method === "PATCH") {
        return await handleUpdateVendor(
          config,
          request,
          slug,
          vendorByIdMatch[2]!,
        );
      }
      if (method === "DELETE") {
        return await handleDeleteVendor(
          config,
          request,
          slug,
          vendorByIdMatch[2]!,
        );
      }
      throw ApiError.methodNotAllowed("kun PATCH eller DELETE er understøttet på denne rute");
    }

    // CVR lookup helper for the Kontakter modal (#390) — read-only enrichment.
    const cvrLookupMatch =
      /^\/api\/companies\/([^/]+)\/cvr-lookup$/.exec(path);
    if (cvrLookupMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(cvrLookupMatch[1]!);
      return await handleCvrLookup(config, request, slug);
    }

    const companySettingsMatch = /^\/api\/companies\/([^/]+)\/company$/.exec(path);
    if (companySettingsMatch) {
      const slug = decodeURIComponent(companySettingsMatch[1]!);
      if (method === "GET") return handleCompanySettings(config, slug);
      // PATCH edits the company profile + bank/payment details (#284).
      if (method === "PATCH") {
        return await handleCompanyProfile(config, request, slug);
      }
      throw ApiError.methodNotAllowed(
        "kun GET eller PATCH er understøttet på denne rute",
      );
    }

    const syncCvrMatch = /^\/api\/companies\/([^/]+)\/sync-cvr$/.exec(path);
    if (syncCvrMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(syncCvrMatch[1]!);
      return await handleCompanySyncCvr(config, slug);
    }

    const obligationsMatch =
      /^\/api\/companies\/([^/]+)\/obligations$/.exec(path);
    if (obligationsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(obligationsMatch[1]!);
      return handleCompanyObligations(config, slug, url);
    }

    const cashflowMatch = /^\/api\/companies\/([^/]+)\/cashflow$/.exec(path);
    if (cashflowMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(cashflowMatch[1]!);
      return handleCompanyCashflow(config, slug, url);
    }

    // Kørsel (#335). GET lists the register for the selected fiscal year; POST
    // registers one mileage entry through the SAME `createMileageEntry` core
    // function the CLI's `mileage add` and the MCP tool use.
    const mileageMatch = /^\/api\/companies\/([^/]+)\/mileage$/.exec(path);
    if (mileageMatch) {
      const slug = decodeURIComponent(mileageMatch[1]!);
      if (method === "GET") return handleCompanyMileage(config, slug, url);
      if (method === "POST") return await handleMileageCreate(config, request, slug);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    // Budget endpoints (#339). The longer `/budget-vs-actual` route MUST come
    // before `/budget` so the shorter pattern does not shadow it.
    const budgetVsActualMatch =
      /^\/api\/companies\/([^/]+)\/budget-vs-actual$/.exec(path);
    if (budgetVsActualMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(budgetVsActualMatch[1]!);
      return handleCompanyBudgetVsActual(config, slug, url);
    }

    const budgetMatch = /^\/api\/companies\/([^/]+)\/budget$/.exec(path);
    if (budgetMatch) {
      const slug = decodeURIComponent(budgetMatch[1]!);
      if (method === "GET") return handleCompanyBudget(config, slug, url);
      if (method === "POST") return await handleSetBudget(config, request, slug);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    // Exceptions queue read endpoint (#332).
    const exceptionsListMatch =
      /^\/api\/companies\/([^/]+)\/exceptions$/.exec(path);
    if (exceptionsListMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(exceptionsListMatch[1]!);
      return handleCompanyExceptions(config, slug, url);
    }

    // Periods read endpoint (#342). Close/reopen er dækket separat længere nede.
    const periodsListMatch =
      /^\/api\/companies\/([^/]+)\/periods$/.exec(path);
    if (periodsListMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(periodsListMatch[1]!);
      return handleCompanyPeriods(config, slug);
    }

    // Bank-accounts list + create (#345).
    const bankAccountsMatch = /^\/api\/companies\/([^/]+)\/bank-accounts$/.exec(path);
    if (bankAccountsMatch) {
      const slug = decodeURIComponent(bankAccountsMatch[1]!);
      if (method === "GET") return handleCompanyBankAccounts(config, slug);
      if (method === "POST")
        return await handleCreateBankAccount(config, request, slug);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    // GDPR export (read) + erase (write) (#334).
    const gdprExportMatch = /^\/api\/companies\/([^/]+)\/gdpr\/export$/.exec(path);
    if (gdprExportMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(gdprExportMatch[1]!);
      return handleCompanyGdprExport(config, slug, url);
    }

    const gdprEraseMatch = /^\/api\/companies\/([^/]+)\/gdpr\/erase$/.exec(path);
    if (gdprEraseMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(gdprEraseMatch[1]!);
      return await handleGdprErase(config, request, slug);
    }

    // Accruals (periodiseringsregister) read endpoint (#337).
    const accrualsMatch = /^\/api\/companies\/([^/]+)\/accruals$/.exec(path);
    if (accrualsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(accrualsMatch[1]!);
      return handleCompanyAccruals(config, slug);
    }

    // Annual-report builder read endpoint (#338).
    const annualReportMatch = /^\/api\/companies\/([^/]+)\/annual-report$/.exec(path);
    if (annualReportMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(annualReportMatch[1]!);
      return handleCompanyAnnualReport(config, slug, url);
    }

    // Bilagsmail read endpoint (#348/#350/#351).
    const bilagsmailMatch = /^\/api\/companies\/([^/]+)\/bilagsmail$/.exec(path);
    if (bilagsmailMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(bilagsmailMatch[1]!);
      return handleCompanyBilagsmail(config, slug);
    }

    // Bilagsmail IMAP-config write (#348).
    const imapConfigMatch =
      /^\/api\/companies\/([^/]+)\/bilagsmail\/imap-config$/.exec(path);
    if (imapConfigMatch) {
      const slug = decodeURIComponent(imapConfigMatch[1]!);
      if (method === "POST")
        return await handleSaveBilagsmailImapConfig(config, request, slug);
      if (method === "DELETE")
        return await handleDeleteBilagsmailImapConfig(config, request, slug);
      throw ApiError.methodNotAllowed("kun POST eller DELETE er understøttet på denne rute");
    }

    // Bilagsmail alias write (#350).
    const bilagsmailAliasMatch =
      /^\/api\/companies\/([^/]+)\/bilagsmail\/alias$/.exec(path);
    if (bilagsmailAliasMatch) {
      if (method !== "PATCH") throw ApiError.methodNotAllowed("kun PATCH er understøttet på denne rute");
      const slug = decodeURIComponent(bilagsmailAliasMatch[1]!);
      return await handleSetBilagsmailAlias(config, request, slug);
    }

    // Bookkeeping write route (#213, slice 1): resolve an open exception.
    const resolveExceptionMatch =
      /^\/api\/companies\/([^/]+)\/exceptions\/([^/]+)\/resolve$/.exec(path);
    if (resolveExceptionMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(resolveExceptionMatch[1]!);
      const id = decodeURIComponent(resolveExceptionMatch[2]!);
      return await handleResolveException(config, request, slug, id);
    }

    // Bookkeeping write route (#213, slice 2): import a bank-statement CSV.
    const bankImportMatch =
      /^\/api\/companies\/([^/]+)\/bank\/import$/.exec(path);
    if (bankImportMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(bankImportMatch[1]!);
      return await handleBankImport(config, request, slug);
    }

    // Cockpit write route: the generic file-import. Recognises which system
    // an export file came from and routes it to the matching core importer.
    const dataImportMatch = /^\/api\/companies\/([^/]+)\/import$/.exec(path);
    if (dataImportMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(dataImportMatch[1]!);
      return await handleDataImport(config, request, slug);
    }

    // Cockpit write route: the accountant-export download. Generates the
    // accountant-handoff package and streams it back as one .tar file.
    const accountantExportMatch =
      /^\/api\/companies\/([^/]+)\/accountant-export$/.exec(path);
    if (accountantExportMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(accountantExportMatch[1]!);
      return await handleAccountantExport(config, request, slug);
    }

    // Bookkeeping write route (#213, slice 3): ingest a document (bilag).
    const documentIngestMatch =
      /^\/api\/companies\/([^/]+)\/documents\/ingest$/.exec(path);
    if (documentIngestMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(documentIngestMatch[1]!);
      return await handleDocumentIngest(config, request, slug);
    }

    // Bookkeeping write route (#407): book an ingested purchase document
    // (bilag) against an unmatched outgoing bank transaction. Third caller
    // of `bookExpenseFromBank` alongside the CLI's `expense book` and the
    // MCP tool.
    const documentBookExpenseMatch =
      /^\/api\/companies\/([^/]+)\/documents\/book-expense$/.exec(path);
    if (documentBookExpenseMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(documentBookExpenseMatch[1]!);
      return await handleDocumentBookExpense(config, request, slug);
    }

    // Bookkeeping write route (#213, slice 4): issue a sales invoice.
    const invoiceIssueMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/issue$/.exec(path);
    if (invoiceIssueMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoiceIssueMatch[1]!);
      return await handleInvoiceIssue(config, request, slug);
    }

    // Cockpit read+render route (#440): forhåndsvis en faktura — render the
    // customer-facing PDF without writing anything to the ledger so the owner
    // can verify layout/amounts/customer-address BEFORE the irreversible
    // posting. Same body shape as `invoices/issue`; the response is the raw
    // PDF bytes (Content-Type application/pdf). NO sequence draw, NO documents
    // row, NO audit_log entry — the preview is read-only.
    const invoicePreviewMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/preview$/.exec(path);
    if (invoicePreviewMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoicePreviewMatch[1]!);
      return await handleInvoicePreview(config, request, slug);
    }

    // Bookkeeping write route (#213, slice 4): post an issued invoice.
    const invoicePostMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/post$/.exec(path);
    if (invoicePostMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoicePostMatch[1]!);
      return await handleInvoicePost(config, request, slug);
    }

    // Bookkeeping write route (#213, slice 4): settle an invoice from bank.
    const invoiceSettleMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/settle$/.exec(path);
    if (invoiceSettleMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoiceSettleMatch[1]!);
      return await handleInvoiceSettle(config, request, slug);
    }

    // Bookkeeping write route (#412): credit an issued invoice. The Cockpit
    // becomes a third caller of `issueCreditNote`, alongside the CLI's
    // `invoice credit-note` command and the MCP tool.
    const invoiceCreditMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/credit-note$/.exec(path);
    if (invoiceCreditMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoiceCreditMatch[1]!);
      return await handleInvoiceCreditNote(config, request, slug);
    }

    // Bookkeeping write route (#428): send an issued invoice as an
    // e-faktura via NemHandel/PEPPOL. Cockpit becomes a third caller of
    // `submitPublicEInvoicePeppol`, alongside the CLI's
    // `invoice submit-public-peppol` command and the MCP tool. Access-point
    // config is loaded from `RENTEMESTER_PEPPOL_ACCESS_POINT` so credentials
    // never enter core state or the request body.
    const invoiceSendPublicMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/send-public$/.exec(path);
    if (invoiceSendPublicMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoiceSendPublicMatch[1]!);
      return await handleInvoiceSendPublic(config, request, slug);
    }

    // Bookkeeping write route (#429): send an issued invoice to the
    // customer's e-mail with the PDF attached. Cockpit becomes a third
    // caller of `sendInvoiceEmail`, alongside the CLI's `invoice send`
    // command and the MCP tool `invoice_send_email`. SMTP config is read
    // from `config/smtp.json` inside the company directory so credentials
    // never enter core state or the request body.
    const invoiceSendEmailMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/send-email$/.exec(path);
    if (invoiceSendEmailMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoiceSendEmailMatch[1]!);
      return await handleInvoiceSendEmail(config, request, slug);
    }

    // Bookkeeping write route (#434): register + send a payment reminder
    // (rykker) for an overdue invoice. Combines three existing core calls
    // (`registerInvoiceReminder`, `postInvoiceReminderToLedger`,
    // `sendInvoiceEmail` with `kind: 'reminder'`) so the cockpit's
    // "Send rykker" button is a one-click write. Statutory rentel. § 9b
    // limits (max 100 kr/reminder, max 3 reminders, >= 10 days apart) are
    // enforced by the core; a violation is mapped to a 400.
    const invoiceSendReminderMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/send-reminder$/.exec(path);
    if (invoiceSendReminderMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(invoiceSendReminderMatch[1]!);
      return await handleInvoiceSendReminder(config, request, slug);
    }

    // Leverandørfaktura-arbejdsbordet (#340) — match the per-id /pay route
    // first because the bare /payables routes would otherwise consume it.
    const payablePayMatch =
      /^\/api\/companies\/([^/]+)\/payables\/(\d+)\/pay$/.exec(path);
    if (payablePayMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(payablePayMatch[1]!);
      return await handlePayablePay(config, request, slug, payablePayMatch[2]!);
    }

    const payablesMatch = /^\/api\/companies\/([^/]+)\/payables$/.exec(path);
    if (payablesMatch) {
      const slug = decodeURIComponent(payablesMatch[1]!);
      if (method === "GET") return handleCompanyPayables(config, slug, url);
      if (method === "POST") {
        return await handlePayableRegister(config, request, slug);
      }
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    // Bookkeeping write route (#287): close an accounting period — the
    // prerequisite for a momsangivelse.
    const periodCloseMatch =
      /^\/api\/companies\/([^/]+)\/periods\/close$/.exec(path);
    if (periodCloseMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(periodCloseMatch[1]!);
      return await handleClosePeriod(config, request, slug);
    }

    // Bookkeeping write route (#301): reopen a closed accounting period — the
    // controlled, audit-logged recovery path for a period closed too early.
    const periodReopenMatch =
      /^\/api\/companies\/([^/]+)\/periods\/reopen$/.exec(path);
    if (periodReopenMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(periodReopenMatch[1]!);
      return await handleReopenPeriod(config, request, slug);
    }

    // Anlægskartotek read + write routes (#336). The cockpit becomes a third
    // caller of `src/core/assets.ts` alongside the CLI's `asset` sub-commands
    // and the MCP `asset_*` tools — no depreciation arithmetic is reimplemented
    // here. Write routes go through `withCompanyMutation`, so the backup-lock,
    // the localhost gate, actor attribution and the confirm gate all apply.
    const assetsCollectionMatch =
      /^\/api\/companies\/([^/]+)\/assets$/.exec(path);
    if (assetsCollectionMatch) {
      const slug = decodeURIComponent(assetsCollectionMatch[1]!);
      if (method === "GET") return handleCompanyAssets(config, slug);
      if (method === "POST")
        return await handleAssetRegister(config, request, slug);
      throw ApiError.methodNotAllowed("kun GET eller POST er understøttet på denne rute");
    }

    const assetWriteOffMatch =
      /^\/api\/companies\/([^/]+)\/assets\/write-off$/.exec(path);
    if (assetWriteOffMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(assetWriteOffMatch[1]!);
      return await handleAssetWriteOff(config, request, slug);
    }

    const assetNextDepreciationMatch =
      /^\/api\/companies\/([^/]+)\/assets\/(\d+)\/next-depreciation$/.exec(path);
    if (assetNextDepreciationMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(assetNextDepreciationMatch[1]!);
      return handleAssetNextDepreciation(
        config,
        slug,
        assetNextDepreciationMatch[2]!,
      );
    }

    const assetDepreciateMatch =
      /^\/api\/companies\/([^/]+)\/assets\/(\d+)\/depreciate$/.exec(path);
    if (assetDepreciateMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(assetDepreciateMatch[1]!);
      return await handleAssetDepreciate(
        config,
        request,
        slug,
        assetDepreciateMatch[2]!,
      );
    }

    // Agent-forslag → menneskelig godkendelse (#346). The agent loop and the
    // exception sync functions in `core/exceptions.ts` produce open `AGENT_*`
    // rows whenever a deterministic agent run needs a human decision; this
    // surface lists them, approves them, or rejects them. Write routes go
    // through `withCompanyMutation`, so the backup-lock, the localhost gate
    // and actor attribution all apply. Match the per-id /approve and /reject
    // routes BEFORE the bare /agent-suggestions route so the shorter pattern
    // does not consume them.
    const agentSuggestionApproveMatch =
      /^\/api\/companies\/([^/]+)\/agent-suggestions\/(\d+)\/approve$/.exec(
        path,
      );
    if (agentSuggestionApproveMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(agentSuggestionApproveMatch[1]!);
      return await handleApproveAgentSuggestion(
        config,
        request,
        slug,
        agentSuggestionApproveMatch[2]!,
      );
    }

    const agentSuggestionRejectMatch =
      /^\/api\/companies\/([^/]+)\/agent-suggestions\/(\d+)\/reject$/.exec(
        path,
      );
    if (agentSuggestionRejectMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("kun POST er understøttet på denne rute");
      const slug = decodeURIComponent(agentSuggestionRejectMatch[1]!);
      return await handleRejectAgentSuggestion(
        config,
        request,
        slug,
        agentSuggestionRejectMatch[2]!,
      );
    }

    const agentSuggestionsMatch =
      /^\/api\/companies\/([^/]+)\/agent-suggestions$/.exec(path);
    if (agentSuggestionsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
      const slug = decodeURIComponent(agentSuggestionsMatch[1]!);
      return handleCompanyAgentSuggestions(config, slug);
    }

    const companyMatch = /^\/api\/companies\/([^/]+)$/.exec(path);
    if (companyMatch) {
      const slug = decodeURIComponent(companyMatch[1]!);
      if (method === "PATCH") return await handleCompanyUpdate(config, slug, request);
      throw ApiError.methodNotAllowed("kun PATCH er understøttet på denne rute");
    }

    // Anything under /api that did not match a route is a JSON 404. Any other
    // path is a cockpit-SPA route: serve the built app (with the index.html
    // fallback) when it exists, else fall through to the JSON 404.
    if (!path.startsWith("/api")) {
      if (method === "GET" || method === "HEAD") {
        const asset = serveStatic(config.staticRoot, path);
        if (asset) return asset;
        // No SPA built — keep `/` a friendly health probe for API-only runs.
        if (path === "/") return handleHealth(config, ROUTE_CATALOG);
      }
    }

    throw ApiError.notFound("ukendt endpoint");
  } catch (err) {
    // (4) Single error edge. ApiError → its code; anything else → generic 500
    // with no leaked detail.
    const { status, body } = toErrorResponse(err);
    return jsonResponse(body, status);
  }
}
