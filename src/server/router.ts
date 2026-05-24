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

import { createCompany } from "../core/company";
import {
  findWorkspaceCompany,
  renameWorkspaceCompany,
  setWorkspaceCompanyArchived,
} from "../core/workspace";
import { discoverWorkspaceCompanies } from "./discovery";
import type { ServerConfig } from "./config";
import { authMiddleware } from "./auth";
import { ApiError, toErrorResponse } from "./errors";
import {
  buildCompanyArchiveYear,
  buildCompanyBalance,
  buildCompanyBank,
  buildCompanyCashflow,
  buildCompanyContacts,
  buildCompanyDashboardData,
  buildCompanyDocuments,
  buildCompanyFiscalYears,
  buildCompanyIncomeStatement,
  buildCompanyInvoices,
  buildCompanyJournal,
  buildCompanyMultiYear,
  buildCompanyObligations,
  buildCompanyOverview,
  buildCompanyRecurringInvoices,
  buildCompanySettings,
  buildCompanyTrialBalance,
  buildCompanyVat,
  buildPortfolioOverview,
  resolveAsOfDate,
  resolveCompanyDocumentFile,
  resolveCompanyIssuedInvoicePdf,
  resolvePathYear,
  resolveYearParam,
  syncCompanyCvr,
} from "./data";
import { serveStatic } from "./static";
import {
  handleAccountantExport,
  handleBankImport,
  handleClosePeriod,
  handleCompanyProfile,
  handleDataImport,
  handleDocumentIngest,
  handleGenerateRecurringInvoice,
  handleInvoiceIssue,
  handleInvoicePost,
  handleInvoiceSettle,
  handleReopenPeriod,
  handleResolveException,
} from "./write-handlers";

const JSON_HEADERS = { "content-type": "application/json; charset=utf-8" };

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: JSON_HEADERS });
}

function okResponse(body: Record<string, unknown>, status = 200): Response {
  return jsonResponse({ ok: true, ...body }, status);
}

/** Parses a JSON request body, mapping any failure to a safe 400. */
async function readJsonBody(request: Request): Promise<Record<string, unknown>> {
  let parsed: unknown;
  try {
    parsed = await request.json();
  } catch {
    throw ApiError.badRequest("request body must be valid JSON");
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw ApiError.badRequest("request body must be a JSON object");
  }
  return parsed as Record<string, unknown>;
}

function requireString(
  body: Record<string, unknown>,
  key: string,
): string {
  const value = body[key];
  if (typeof value !== "string" || value.trim().length === 0) {
    throw ApiError.badRequest(`'${key}' is required and must be a non-empty string`);
  }
  return value.trim();
}

function optionalString(
  body: Record<string, unknown>,
  key: string,
): string | undefined {
  const value = body[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string") {
    throw ApiError.badRequest(`'${key}' must be a string when present`);
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

// --------------------------------------------------------------------------
// Route handlers — reads + workspace management only.
// --------------------------------------------------------------------------

function handleHealth(config: ServerConfig): Response {
  return okResponse({
    service: "rentemester-cockpit",
    workspace: config.workspaceRoot,
    authRequired: config.authRequired,
  });
}

function handlePortfolio(config: ServerConfig, url: URL): Response {
  const asOf = resolveAsOfDate(url.searchParams.get("asOf"));
  const overview = buildPortfolioOverview(config.workspaceRoot, asOf);
  return okResponse({ portfolio: overview });
}

function handleCompanyList(config: ServerConfig): Response {
  // Discover-and-adopt any present-but-unlisted company directory before
  // listing (#256): an owner who set a company up via the CLI then opened the
  // cockpit must see that real company, not "0 virksomheder" + a blank create.
  const companies = discoverWorkspaceCompanies(config.workspaceRoot);
  return okResponse({
    workspace: config.workspaceRoot,
    count: companies.length,
    companies: companies.map((c) => ({
      slug: c.slug,
      name: c.name,
      createdAt: c.createdAt,
      archived: c.archived,
    })),
  });
}

function handleCompanyDashboard(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const asOf = resolveAsOfDate(url.searchParams.get("asOf"));
  const data = buildCompanyDashboardData(config.workspaceRoot, slug, asOf);
  return okResponse({ dashboard: data });
}

function handleCompanyFiscalYears(config: ServerConfig, slug: string): Response {
  const data = buildCompanyFiscalYears(config.workspaceRoot, slug);
  return okResponse({ fiscalYears: data });
}

function handleCompanyOverview(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyOverview(config.workspaceRoot, slug, year);
  return okResponse({ overview: data });
}

function handleCompanyIncomeStatement(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyIncomeStatement(config.workspaceRoot, slug, year);
  return okResponse({ incomeStatement: data });
}

function handleCompanyBalance(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyBalance(config.workspaceRoot, slug, year);
  return okResponse({ balance: data });
}

function handleCompanyTrialBalance(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyTrialBalance(config.workspaceRoot, slug, year);
  return okResponse({ trialBalance: data });
}

function handleCompanyJournal(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const account = url.searchParams.get("account");
  const data = buildCompanyJournal(config.workspaceRoot, slug, year, account);
  return okResponse({ journal: data });
}

function handleCompanyBank(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyBank(config.workspaceRoot, slug, year);
  return okResponse({ bank: data });
}

function handleCompanyVat(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyVat(config.workspaceRoot, slug, year);
  return okResponse({ vat: data });
}

function handleCompanyDocuments(config: ServerConfig, slug: string): Response {
  const data = buildCompanyDocuments(config.workspaceRoot, slug);
  return okResponse({ documents: data });
}

function handleCompanyRecurringInvoices(
  config: ServerConfig,
  slug: string,
): Response {
  const data = buildCompanyRecurringInvoices(config.workspaceRoot, slug);
  return okResponse({ recurringInvoices: data });
}

/**
 * GET /api/companies/:slug/documents/:id/file — serves the stored bilag file
 * so a human can open it in the cockpit. A read route, so it does not run the
 * mutation pipeline; an unknown company or document is a 404.
 */
function handleCompanyDocumentFile(
  config: ServerConfig,
  slug: string,
  idRaw: string,
): Response {
  const id = Number(idRaw);
  if (!Number.isInteger(id) || id <= 0) {
    throw ApiError.badRequest("document id must be a positive integer");
  }
  const file = resolveCompanyDocumentFile(config.workspaceRoot, slug, id);
  // PDFs and images render safely inline; anything else (txt/json/unknown) is
  // sent as a download so the browser never renders it inside the cockpit's
  // own origin. `nosniff` stops the browser re-sniffing the body as HTML.
  // `filename*` carries the (possibly non-ASCII) name per RFC 5987.
  const inline =
    file.mimeType === "application/pdf" || file.mimeType.startsWith("image/");
  return new Response(Bun.file(file.path), {
    headers: {
      "content-type": file.mimeType,
      "content-disposition": `${inline ? "inline" : "attachment"}; filename*=UTF-8''${encodeURIComponent(file.filename)}`,
      "x-content-type-options": "nosniff",
      "cache-control": "private, no-store",
    },
  });
}

/**
 * GET /api/companies/:slug/invoices/:id/pdf — serves the issued-invoice PDF so
 * the owner can download or forward it without leaving the cockpit (#378). The
 * bytes come from the same `renderIssuedInvoicePdf` core the CLI uses, so the
 * PDF is byte-identical to `bun run cli invoice render <id>`.
 */
function handleCompanyInvoicePdf(
  config: ServerConfig,
  slug: string,
  idRaw: string,
): Response {
  const id = Number(idRaw);
  if (!Number.isInteger(id) || id <= 0) {
    throw ApiError.badRequest("invoice id must be a positive integer");
  }
  const file = resolveCompanyIssuedInvoicePdf(config.workspaceRoot, slug, id);
  return new Response(Bun.file(file.path), {
    headers: {
      "content-type": file.mimeType,
      "content-disposition": `inline; filename*=UTF-8''${encodeURIComponent(file.filename)}`,
      "x-content-type-options": "nosniff",
      "cache-control": "private, no-store",
    },
  });
}

function handleCompanyArchiveYear(
  config: ServerConfig,
  slug: string,
  yearRaw: string,
): Response {
  const year = resolvePathYear(yearRaw);
  const data = buildCompanyArchiveYear(config.workspaceRoot, slug, year);
  return okResponse({ archive: data });
}

function handleCompanyMultiYear(config: ServerConfig, slug: string): Response {
  const data = buildCompanyMultiYear(config.workspaceRoot, slug);
  return okResponse({ multiYear: data });
}

function handleCompanyInvoices(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyInvoices(config.workspaceRoot, slug, year);
  return okResponse({ invoices: data });
}

function handleCompanyContacts(config: ServerConfig, slug: string): Response {
  const data = buildCompanyContacts(config.workspaceRoot, slug);
  return okResponse({ contacts: data });
}

function handleCompanySettings(config: ServerConfig, slug: string): Response {
  const data = buildCompanySettings(config.workspaceRoot, slug);
  return okResponse({ company: data });
}

/**
 * Refreshes a company's CVR-register stamdata. The CVR lookup runs server-side
 * so the CVR credentials never reach the browser. A failed lookup (missing
 * credentials, unknown CVR) is reported inside `sync.ok`, not as an HTTP error.
 */
async function handleCompanySyncCvr(
  config: ServerConfig,
  slug: string,
): Promise<Response> {
  const data = await syncCompanyCvr(config.workspaceRoot, slug);
  return okResponse({ sync: data });
}

function handleCompanyObligations(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyObligations(config.workspaceRoot, slug, year);
  return okResponse({ obligations: data });
}

function handleCompanyCashflow(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyCashflow(config.workspaceRoot, slug, year);
  return okResponse({ cashflow: data });
}

/**
 * Parses the optional `payment` body field on the create-company form (#284)
 * into a core `CompanyPaymentInput`. Every sub-field is optional — `createCompany`
 * only creates the primary bank account when at least one carries information.
 */
function parseCreatePayment(
  body: Record<string, unknown>,
): { bankName?: string; registrationNo?: string; accountNo?: string; iban?: string } | undefined {
  const raw = body.payment;
  if (raw === undefined || raw === null) return undefined;
  if (typeof raw !== "object" || Array.isArray(raw)) {
    throw ApiError.badRequest("'payment' must be an object when present");
  }
  const p = raw as Record<string, unknown>;
  const payment: {
    bankName?: string;
    registrationNo?: string;
    accountNo?: string;
    iban?: string;
  } = {};
  for (const field of ["bankName", "registrationNo", "accountNo", "iban"] as const) {
    const value = optionalString(p, field);
    if (value !== undefined) payment[field] = value;
  }
  return Object.keys(payment).length > 0 ? payment : undefined;
}

async function handleCompanyCreate(
  config: ServerConfig,
  request: Request,
): Promise<Response> {
  const body = await readJsonBody(request);
  const name = requireString(body, "name");
  const payment = parseCreatePayment(body);
  let result;
  try {
    result = createCompany(config.workspaceRoot, {
      name,
      slug: optionalString(body, "slug"),
      cvr: optionalString(body, "cvr") ?? null,
      fiscalYearStartMonth: optionalString(body, "fiscalYearStartMonth"),
      fiscalYearLabelStrategy: optionalString(body, "fiscalYearLabelStrategy"),
      // #300: the VAT settlement cadence. `initialiseCompanyVolume` validates
      // it and throws on an unknown value — re-mapped to a 400 below.
      vatPeriodType: optionalString(body, "vatPeriodType"),
      ...(payment ? { payment } : {}),
    });
  } catch (err) {
    // createCompany throws plain Errors for invalid slug / duplicate. Re-map
    // them to a safe code; the messages it produces are curated (no paths)
    // — except `companyRoot`, which createCompany only embeds for the
    // "already exists" case, so collapse that to a generic conflict.
    const message = err instanceof Error ? err.message : String(err);
    if (/already exists|already registered/i.test(message)) {
      throw ApiError.conflict("a company with that slug already exists");
    }
    throw ApiError.badRequest(message);
  }
  return okResponse(
    {
      company: { slug: result.slug, name: result.name },
    },
    201,
  );
}

/**
 * Updates a registered company's mutable workspace metadata: the display
 * `name` and/or the `archived` flag. This never touches the slug or the
 * ledger — there is deliberately NO destructive delete of ledger data.
 */
async function handleCompanyUpdate(
  config: ServerConfig,
  slug: string,
  request: Request,
): Promise<Response> {
  if (!findWorkspaceCompany(config.workspaceRoot, slug)) {
    throw ApiError.notFound(`no company with slug '${slug}' in the workspace`);
  }
  const body = await readJsonBody(request);
  const name = optionalString(body, "name");
  const archivedRaw = body.archived;
  if (archivedRaw !== undefined && typeof archivedRaw !== "boolean") {
    throw ApiError.badRequest("'archived' must be a boolean when present");
  }
  if (name === undefined && archivedRaw === undefined) {
    throw ApiError.badRequest("provide 'name' and/or 'archived' to update");
  }
  try {
    let entry = findWorkspaceCompany(config.workspaceRoot, slug)!;
    if (name !== undefined) {
      entry = renameWorkspaceCompany(config.workspaceRoot, slug, name);
    }
    if (typeof archivedRaw === "boolean") {
      entry = setWorkspaceCompanyArchived(config.workspaceRoot, slug, archivedRaw);
    }
    return okResponse({
      company: {
        slug: entry.slug,
        name: entry.name,
        createdAt: entry.createdAt,
        archived: entry.archived,
      },
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw ApiError.badRequest(message);
  }
}

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
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      return handleHealth(config);
    }

    if (path === "/api/portfolio") {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      return handlePortfolio(config, url);
    }

    if (path === "/api/companies") {
      if (method === "GET") return handleCompanyList(config);
      if (method === "POST") return await handleCompanyCreate(config, request);
      throw ApiError.methodNotAllowed("GET or POST required");
    }

    const dashboardMatch = /^\/api\/companies\/([^/]+)\/dashboard$/.exec(path);
    if (dashboardMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(dashboardMatch[1]!);
      return handleCompanyDashboard(config, slug, url);
    }

    const fiscalYearsMatch = /^\/api\/companies\/([^/]+)\/fiscal-years$/.exec(path);
    if (fiscalYearsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(fiscalYearsMatch[1]!);
      return handleCompanyFiscalYears(config, slug);
    }

    const overviewMatch = /^\/api\/companies\/([^/]+)\/overview$/.exec(path);
    if (overviewMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(overviewMatch[1]!);
      return handleCompanyOverview(config, slug, url);
    }

    const incomeStatementMatch =
      /^\/api\/companies\/([^/]+)\/income-statement$/.exec(path);
    if (incomeStatementMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(incomeStatementMatch[1]!);
      return handleCompanyIncomeStatement(config, slug, url);
    }

    const balanceMatch = /^\/api\/companies\/([^/]+)\/balance$/.exec(path);
    if (balanceMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(balanceMatch[1]!);
      return handleCompanyBalance(config, slug, url);
    }

    const trialBalanceMatch =
      /^\/api\/companies\/([^/]+)\/trial-balance$/.exec(path);
    if (trialBalanceMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(trialBalanceMatch[1]!);
      return handleCompanyTrialBalance(config, slug, url);
    }

    const journalMatch = /^\/api\/companies\/([^/]+)\/journal$/.exec(path);
    if (journalMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(journalMatch[1]!);
      return handleCompanyJournal(config, slug, url);
    }

    const bankMatch = /^\/api\/companies\/([^/]+)\/bank$/.exec(path);
    if (bankMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(bankMatch[1]!);
      return handleCompanyBank(config, slug, url);
    }

    const vatMatch = /^\/api\/companies\/([^/]+)\/vat$/.exec(path);
    if (vatMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(vatMatch[1]!);
      return handleCompanyVat(config, slug, url);
    }

    const documentsMatch = /^\/api\/companies\/([^/]+)\/documents$/.exec(path);
    if (documentsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(documentsMatch[1]!);
      return handleCompanyDocuments(config, slug);
    }

    const documentFileMatch =
      /^\/api\/companies\/([^/]+)\/documents\/(\d+)\/file$/.exec(path);
    if (documentFileMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(documentFileMatch[1]!);
      return handleCompanyDocumentFile(config, slug, documentFileMatch[2]!);
    }

    const recurringInvoicesMatch =
      /^\/api\/companies\/([^/]+)\/recurring-invoices$/.exec(path);
    if (recurringInvoicesMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(recurringInvoicesMatch[1]!);
      return handleCompanyRecurringInvoices(config, slug);
    }

    const recurringInvoiceGenerateMatch =
      /^\/api\/companies\/([^/]+)\/recurring-invoices\/(\d+)\/generate$/.exec(path);
    if (recurringInvoiceGenerateMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("POST required");
      const slug = decodeURIComponent(recurringInvoiceGenerateMatch[1]!);
      return await handleGenerateRecurringInvoice(
        config,
        request,
        slug,
        recurringInvoiceGenerateMatch[2]!,
      );
    }

    const archiveMatch =
      /^\/api\/companies\/([^/]+)\/archive\/([^/]+)$/.exec(path);
    if (archiveMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(archiveMatch[1]!);
      const year = decodeURIComponent(archiveMatch[2]!);
      return handleCompanyArchiveYear(config, slug, year);
    }

    const multiYearMatch = /^\/api\/companies\/([^/]+)\/multi-year$/.exec(path);
    if (multiYearMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(multiYearMatch[1]!);
      return handleCompanyMultiYear(config, slug);
    }

    const invoicesMatch = /^\/api\/companies\/([^/]+)\/invoices$/.exec(path);
    if (invoicesMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(invoicesMatch[1]!);
      return handleCompanyInvoices(config, slug, url);
    }

    // Cockpit read route (#378): serve the issued-invoice PDF so the owner
    // can download/forward it without leaving the browser. Re-uses the same
    // `renderIssuedInvoicePdf` core the CLI runs — no new rendering path.
    const invoicePdfMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/(\d+)\/pdf$/.exec(path);
    if (invoicePdfMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(invoicePdfMatch[1]!);
      return handleCompanyInvoicePdf(config, slug, invoicePdfMatch[2]!);
    }

    const contactsMatch = /^\/api\/companies\/([^/]+)\/contacts$/.exec(path);
    if (contactsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(contactsMatch[1]!);
      return handleCompanyContacts(config, slug);
    }

    const companySettingsMatch = /^\/api\/companies\/([^/]+)\/company$/.exec(path);
    if (companySettingsMatch) {
      const slug = decodeURIComponent(companySettingsMatch[1]!);
      if (method === "GET") return handleCompanySettings(config, slug);
      // PATCH edits the company profile + bank/payment details (#284).
      if (method === "PATCH") {
        return await handleCompanyProfile(config, request, slug);
      }
      throw ApiError.methodNotAllowed("GET or PATCH required");
    }

    const syncCvrMatch = /^\/api\/companies\/([^/]+)\/sync-cvr$/.exec(path);
    if (syncCvrMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("POST required");
      const slug = decodeURIComponent(syncCvrMatch[1]!);
      return await handleCompanySyncCvr(config, slug);
    }

    const obligationsMatch =
      /^\/api\/companies\/([^/]+)\/obligations$/.exec(path);
    if (obligationsMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(obligationsMatch[1]!);
      return handleCompanyObligations(config, slug, url);
    }

    const cashflowMatch = /^\/api\/companies\/([^/]+)\/cashflow$/.exec(path);
    if (cashflowMatch) {
      if (method !== "GET") throw ApiError.methodNotAllowed("GET required");
      const slug = decodeURIComponent(cashflowMatch[1]!);
      return handleCompanyCashflow(config, slug, url);
    }

    // Bookkeeping write route (#213, slice 1): resolve an open exception.
    const resolveExceptionMatch =
      /^\/api\/companies\/([^/]+)\/exceptions\/([^/]+)\/resolve$/.exec(path);
    if (resolveExceptionMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("POST required");
      const slug = decodeURIComponent(resolveExceptionMatch[1]!);
      const id = decodeURIComponent(resolveExceptionMatch[2]!);
      return await handleResolveException(config, request, slug, id);
    }

    // Bookkeeping write route (#213, slice 2): import a bank-statement CSV.
    const bankImportMatch =
      /^\/api\/companies\/([^/]+)\/bank\/import$/.exec(path);
    if (bankImportMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("POST required");
      const slug = decodeURIComponent(bankImportMatch[1]!);
      return await handleBankImport(config, request, slug);
    }

    // Cockpit write route: the generic file-import. Recognises which system
    // an export file came from and routes it to the matching core importer.
    const dataImportMatch = /^\/api\/companies\/([^/]+)\/import$/.exec(path);
    if (dataImportMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("POST required");
      const slug = decodeURIComponent(dataImportMatch[1]!);
      return await handleDataImport(config, request, slug);
    }

    // Cockpit write route: the accountant-export download. Generates the
    // accountant-handoff package and streams it back as one .tar file.
    const accountantExportMatch =
      /^\/api\/companies\/([^/]+)\/accountant-export$/.exec(path);
    if (accountantExportMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("POST required");
      const slug = decodeURIComponent(accountantExportMatch[1]!);
      return await handleAccountantExport(config, request, slug);
    }

    // Bookkeeping write route (#213, slice 3): ingest a document (bilag).
    const documentIngestMatch =
      /^\/api\/companies\/([^/]+)\/documents\/ingest$/.exec(path);
    if (documentIngestMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("POST required");
      const slug = decodeURIComponent(documentIngestMatch[1]!);
      return await handleDocumentIngest(config, request, slug);
    }

    // Bookkeeping write route (#213, slice 4): issue a sales invoice.
    const invoiceIssueMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/issue$/.exec(path);
    if (invoiceIssueMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("POST required");
      const slug = decodeURIComponent(invoiceIssueMatch[1]!);
      return await handleInvoiceIssue(config, request, slug);
    }

    // Bookkeeping write route (#213, slice 4): post an issued invoice.
    const invoicePostMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/post$/.exec(path);
    if (invoicePostMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("POST required");
      const slug = decodeURIComponent(invoicePostMatch[1]!);
      return await handleInvoicePost(config, request, slug);
    }

    // Bookkeeping write route (#213, slice 4): settle an invoice from bank.
    const invoiceSettleMatch =
      /^\/api\/companies\/([^/]+)\/invoices\/settle$/.exec(path);
    if (invoiceSettleMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("POST required");
      const slug = decodeURIComponent(invoiceSettleMatch[1]!);
      return await handleInvoiceSettle(config, request, slug);
    }

    // Bookkeeping write route (#287): close an accounting period — the
    // prerequisite for a momsangivelse.
    const periodCloseMatch =
      /^\/api\/companies\/([^/]+)\/periods\/close$/.exec(path);
    if (periodCloseMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("POST required");
      const slug = decodeURIComponent(periodCloseMatch[1]!);
      return await handleClosePeriod(config, request, slug);
    }

    // Bookkeeping write route (#301): reopen a closed accounting period — the
    // controlled, audit-logged recovery path for a period closed too early.
    const periodReopenMatch =
      /^\/api\/companies\/([^/]+)\/periods\/reopen$/.exec(path);
    if (periodReopenMatch) {
      if (method !== "POST") throw ApiError.methodNotAllowed("POST required");
      const slug = decodeURIComponent(periodReopenMatch[1]!);
      return await handleReopenPeriod(config, request, slug);
    }

    const companyMatch = /^\/api\/companies\/([^/]+)$/.exec(path);
    if (companyMatch) {
      const slug = decodeURIComponent(companyMatch[1]!);
      if (method === "PATCH") return await handleCompanyUpdate(config, slug, request);
      throw ApiError.methodNotAllowed("PATCH required");
    }

    // Anything under /api that did not match a route is a JSON 404. Any other
    // path is a cockpit-SPA route: serve the built app (with the index.html
    // fallback) when it exists, else fall through to the JSON 404.
    if (!path.startsWith("/api")) {
      if (method === "GET" || method === "HEAD") {
        const asset = serveStatic(config.staticRoot, path);
        if (asset) return asset;
        // No SPA built — keep `/` a friendly health probe for API-only runs.
        if (path === "/") return handleHealth(config);
      }
    }

    throw ApiError.notFound("no such endpoint");
  } catch (err) {
    // (4) Single error edge. ApiError → its code; anything else → generic 500
    // with no leaked detail.
    const { status, body } = toErrorResponse(err);
    return jsonResponse(body, status);
  }
}
