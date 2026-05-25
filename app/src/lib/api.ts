// The single HTTP seam between the cockpit SPA and `rentemester serve` (#170).
//
// Every backend response is `{ok:true,...}` or
// `{ok:false, errors:[...], code:"..."}` — the unified envelope shared with
// MCP + CLI (#368). `request` normalises both into a typed value or a thrown
// `ApiError`, so views only ever deal with data or a single error type.

import type {
  AssetsResponse,
  AssetNextDepreciationResponse,
  AssetRegisterInput,
  AssetRegisterSummary,
  AssetDepreciateInput,
  AssetDepreciateSummary,
  AssetWriteOffInput,
  AssetWriteOffSummary,
  ArchiveResponse,
  BalanceResponse,
  BankResponse,
  BudgetResponse,
  BudgetVsActualResponse,
  CashflowResponse,
  ClosePeriodInput,
  ClosePeriodResponse,
  ReopenPeriodInput,
  ReopenPeriodResponse,
  CompanyListResponse,
  CompanyProfileInput,
  CompanySettingsResponse,
  ContactsResponse,
  CreateCompanyInput,
  CustomerInput,
  CvrLookupResult,
  CvrSystemStatusResponse,
  DashboardResponse,
  DocumentsResponse,
  FiscalYearsResponse,
  HealthResponse,
  IncomeStatementResponse,
  InvoicesResponse,
  JournalResponse,
  MileageEntryInput,
  MileageEntrySummary,
  MileageResponse,
  MultiYearResponse,
  ObligationsResponse,
  OverviewResponse,
  PayableListStatusFilter,
  PayablePayInput,
  PayablePaySummary,
  PayableRegisterInput,
  PayableRegisterSummary,
  PayablesResponse,
  PortfolioResponse,
  RecurringInvoiceGenerationResult,
  RecurringInvoiceTemplateCreatedResult,
  RecurringInvoiceTemplateInput,
  RecurringInvoicesResponse,
  SetBudgetInput,
  AccountsResponse,
  BankAccountsResponse,
  CompanyAccrualsResponse,
  CompanyAnnualReportResponse,
  ExceptionsResponse,
  GdprErasureResult,
  GdprResponse,
  IntegrityResponse,
  PeriodsResponse,
  RetentionResponse,
  RulesResponse,
  SyncCvrResponse,
  TrialBalanceResponse,
  UpdateCompanyInput,
  VatResponse,
  VendorInput,
} from "./types";

/** A failed API call — carries the backend error code for precise handling. */
export class ApiError extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, message: string, status: number) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.status = status;
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(path, {
      ...init,
      headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
    });
  } catch {
    throw new ApiError(
      "network",
      "Kunne ikke nå serveren. Kører `rentemester serve`?",
      0,
    );
  }

  let body: unknown;
  try {
    body = await res.json();
  } catch {
    throw new ApiError("internal", "Serveren gav et ugyldigt svar.", res.status);
  }

  if (body && typeof body === "object" && (body as { ok?: unknown }).ok === false) {
    // #368: cockpit, MCP and CLI all return the same shape now —
    // `{ ok:false, errors:[string], code?:string }`. The human-readable
    // message lives in `errors[0]`; `code` is the discrete enum
    // (`bad_request`, `conflict`, …) for programmatic branching.
    const env = body as { errors?: unknown; code?: unknown };
    const errors = Array.isArray(env.errors)
      ? env.errors.map((e) => String(e))
      : [];
    const code = typeof env.code === "string" ? env.code : "internal";
    const message = errors[0] ?? "Ukendt serverfejl.";
    throw new ApiError(code, message, res.status);
  }
  if (!res.ok) {
    throw new ApiError("internal", `HTTP ${res.status}`, res.status);
  }
  return body as T;
}

export const api = {
  health: () => request<HealthResponse>("/api/health"),

  portfolio: (asOf?: string) =>
    request<PortfolioResponse>(
      `/api/portfolio${asOf ? `?asOf=${encodeURIComponent(asOf)}` : ""}`,
    ).then((r) => r.portfolio),

  /**
   * #347 — Lovgrundlag-viewer. Henter alle aktive regler i `rules/dk/*.yaml`
   * sammen med deres SHA-256-citationer og legal-sources, så cockpittet kan
   * vise SMB-ejeren hvilke regler der styrer bogføringen og hvor de er hentet
   * fra.
   */
  rules: () => request<RulesResponse>("/api/rules"),

  /**
   * #343 — 5-års retention-status pr. data-domæne for én virksomhed.
   */
  retention: (slug: string) =>
    request<RetentionResponse>(
      `/api/companies/${encodeURIComponent(slug)}/retention`,
    ).then((r) => r.retention),

  /**
   * #333 — Audit chain + backup status panel. Idempotent: serveren kører
   * `verifyAuditChain` (read-only) hver gang og returnerer den aktuelle
   * status sammen med backup-compliance og destinations.
   */
  integrity: (slug: string) =>
    request<IntegrityResponse>(
      `/api/companies/${encodeURIComponent(slug)}/integrity`,
    ).then((r) => r.integrity),

  /**
   * #344 — kontoplan (read-only). Bygger på den eksisterende `accounts`-tabel
   * som seedAccounts + reconcileChartOfAccounts populerer.
   */
  accounts: (slug: string) =>
    request<AccountsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/accounts`,
    ).then((r) => r.accounts),

  /**
   * #332 — Exceptions queue list. Default status er 'open' så cockpittet
   * altid starter på det aktive arbejde.
   */
  exceptions: (slug: string, status?: "open" | "resolved" | "all") => {
    const params = new URLSearchParams();
    if (status) params.set("status", status);
    const qs = params.toString();
    return request<ExceptionsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/exceptions${qs ? `?${qs}` : ""}`,
    ).then((r) => r.exceptions);
  },

  /**
   * POST /api/companies/:slug/exceptions/:id/resolve — closes an open
   * exception. Returns `{ resolved: boolean }`.
   */
  /**
   * #338 — Annual report (regnskabsklasse-B) builder.
   */
  annualReport: (slug: string, fiscalYearStart: string, fiscalYearEnd: string) => {
    const params = new URLSearchParams({ fiscalYearStart, fiscalYearEnd });
    return request<CompanyAnnualReportResponse>(
      `/api/companies/${encodeURIComponent(slug)}/annual-report?${params.toString()}`,
    ).then((r) => r.annualReport);
  },

  /**
   * #337 — Periodiseringsregister (read).
   */
  accruals: (slug: string) =>
    request<CompanyAccrualsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/accruals`,
    ).then((r) => r.accruals),

  /**
   * #334 — GDPR-indsigt (read). cvr ELLER name skal sættes.
   */
  gdprExport: (
    slug: string,
    key: { cvr?: string; name?: string; asOf?: string },
  ) => {
    const params = new URLSearchParams();
    if (key.cvr) params.set("cvr", key.cvr);
    if (key.name) params.set("name", key.name);
    if (key.asOf) params.set("asOf", key.asOf);
    return request<GdprResponse>(
      `/api/companies/${encodeURIComponent(slug)}/gdpr/export?${params.toString()}`,
    ).then((r) => r.gdpr);
  },

  /**
   * #334 — GDPR-anonymisering (write). Skriver append-only tombstones.
   */
  gdprErase: (
    slug: string,
    body: { cvr?: string; name?: string; asOf?: string },
  ) =>
    request<{ ok: true; gdprErasure: GdprErasureResult }>(
      `/api/companies/${encodeURIComponent(slug)}/gdpr/erase`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      },
    ).then((r) => r.gdprErasure),

  /**
   * #345 — Bankkonti + CSV-mapping-profiler (read).
   */
  bankAccounts: (slug: string) =>
    request<BankAccountsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/bank-accounts`,
    ).then((r) => r.bankAccounts),

  /**
   * #345 — Opretter en bankkonto.
   */
  createBankAccount: (
    slug: string,
    body: {
      name: string;
      slug?: string;
      bankName?: string;
      registrationNo?: string;
      accountNo?: string;
      iban?: string;
      currency?: string;
      ledgerAccountNo?: string;
    },
  ) =>
    request<{ ok: true; bankAccount: { id: number; slug: string } }>(
      `/api/companies/${encodeURIComponent(slug)}/bank-accounts`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      },
    ).then((r) => r.bankAccount),

  /**
   * #342 — Periodelås-liste (read).
   */
  periods: (slug: string) =>
    request<PeriodsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/periods`,
    ).then((r) => r.periods),

  /**
   * #342 — close a period. Body shape matches CLI's `period close`.
   */
  closePeriod: (
    slug: string,
    body: {
      periodStart: string;
      periodEnd: string;
      kind: "vat_quarter" | "fiscal_year" | "custom";
      status?: "closed" | "reported";
      reference?: string;
    },
  ) =>
    request<{ ok: true; period: { id: number; effectiveStatus?: string } }>(
      `/api/companies/${encodeURIComponent(slug)}/periods/close`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      },
    ).then((r) => r.period),

  /**
   * #342 — reopen a closed period; `reason` is mandatory and recorded verbatim
   * in the audit log.
   */
  reopenPeriod: (
    slug: string,
    body: {
      periodStart: string;
      periodEnd: string;
      kind: "vat_quarter" | "fiscal_year" | "custom";
      reason: string;
    },
  ) =>
    request<{ ok: true; period: { id: number; effectiveStatus?: string } }>(
      `/api/companies/${encodeURIComponent(slug)}/periods/reopen`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      },
    ).then((r) => r.period),

  resolveException: (
    slug: string,
    id: number,
    body: { note?: string } = {},
  ) =>
    request<{ ok: true; exception: { id: number; resolved: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/exceptions/${id}/resolve`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      },
    ).then((r) => r.exception),

  companies: () =>
    request<CompanyListResponse>("/api/companies").then((r) => r.companies),

  dashboard: (slug: string, asOf?: string) =>
    request<DashboardResponse>(
      `/api/companies/${encodeURIComponent(slug)}/dashboard${
        asOf ? `?asOf=${encodeURIComponent(asOf)}` : ""
      }`,
    ).then((r) => r.dashboard),

  fiscalYears: (slug: string) =>
    request<FiscalYearsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/fiscal-years`,
    ).then((r) => r.fiscalYears.years),

  overview: (slug: string, year?: string) =>
    request<OverviewResponse>(
      `/api/companies/${encodeURIComponent(slug)}/overview${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.overview),

  incomeStatement: (slug: string, year?: string) =>
    request<IncomeStatementResponse>(
      `/api/companies/${encodeURIComponent(slug)}/income-statement${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.incomeStatement),

  balance: (slug: string, year?: string) =>
    request<BalanceResponse>(
      `/api/companies/${encodeURIComponent(slug)}/balance${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.balance),

  trialBalance: (slug: string, year?: string) =>
    request<TrialBalanceResponse>(
      `/api/companies/${encodeURIComponent(slug)}/trial-balance${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.trialBalance),

  journal: (slug: string, year?: string, account?: string) => {
    const params = new URLSearchParams();
    if (year) params.set("year", year);
    if (account) params.set("account", account);
    const query = params.toString();
    return request<JournalResponse>(
      `/api/companies/${encodeURIComponent(slug)}/journal${
        query ? `?${query}` : ""
      }`,
    ).then((r) => r.journal);
  },

  bank: (slug: string, year?: string) =>
    request<BankResponse>(
      `/api/companies/${encodeURIComponent(slug)}/bank${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.bank),

  vat: (slug: string, year?: string) =>
    request<VatResponse>(
      `/api/companies/${encodeURIComponent(slug)}/vat${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.vat),

  documents: (slug: string) =>
    request<DocumentsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/documents`,
    ).then((r) => r.documents),

  /**
   * URL of a stored bilag file — opened directly in a new browser tab, so it
   * is a plain URL builder rather than a fetch. The server serves the file
   * inline with its recorded MIME type.
   */
  documentFileUrl: (slug: string, id: number) =>
    `/api/companies/${encodeURIComponent(slug)}/documents/${id}/file`,

  /**
   * URL of an issued invoice's PDF — opened directly in a new browser tab so
   * the owner can download or forward it. URL builder rather than a fetch:
   * the server serves the file inline (#378).
   */
  invoicePdfUrl: (slug: string, id: number) =>
    `/api/companies/${encodeURIComponent(slug)}/invoices/${id}/pdf`,

  /**
   * URL of the Resultatopgørelse, Balance eller Saldobalance som CSV-fil
   * (#372). Endpointet returnerer en `text/csv`-attachment med stabile
   * danske kolonnenavne og semikolon-separator, så browseren downloader
   * filen direkte når URL'en åbnes (typisk via et `<a href download>`).
   * PDF følger i et opfølger-issue.
   */
  statementCsvUrl: (
    slug: string,
    report: "income-statement" | "balance" | "trial-balance",
    year?: string,
  ) => {
    const params = new URLSearchParams({ format: "csv" });
    if (year) params.set("year", year);
    return `/api/companies/${encodeURIComponent(slug)}/${report}/export?${params.toString()}`;
  },

  /**
   * #463 — Statement PDF download URL. Samme endpoint som CSV, bare med
   * format=pdf. Deterministisk Helvetica/WinAnsi PDF.
   */
  statementPdfUrl: (
    slug: string,
    report: "income-statement" | "balance" | "trial-balance",
    year?: string,
  ) => {
    const params = new URLSearchParams({ format: "pdf" });
    if (year) params.set("year", year);
    return `/api/companies/${encodeURIComponent(slug)}/${report}/export?${params.toString()}`;
  },

  /**
   * Moms-rapport som PDF-download (#464). Indeholder SKAT-rubrikker
   * (salgsmoms, købsmoms, momstilsvar, A/B/C), periode-bounds og
   * betalingsfristen — én printbar arbejdsudskrift.
   */
  vatPdfUrl: (slug: string, year?: string) => {
    const params = new URLSearchParams({ format: "pdf" });
    if (year) params.set("year", year);
    return `/api/companies/${encodeURIComponent(slug)}/vat/export?${params.toString()}`;
  },

  /**
   * Posteringer (kassekladde) som CSV-download (#465). Samme deterministiske
   * UTF-8 BOM + semikolon-CSV som de tre kerne-rapporter; et valgfrit
   * `account` filtrerer drilldown'et til netop den konto.
   */
  journalCsvUrl: (slug: string, year?: string, account?: string | null) => {
    const params = new URLSearchParams({ format: "csv" });
    if (year) params.set("year", year);
    if (account) params.set("account", account);
    return `/api/companies/${encodeURIComponent(slug)}/journal/export?${params.toString()}`;
  },

  /** Recurring-invoice templates + their past generations for a company. */
  recurringInvoices: (slug: string) =>
    request<RecurringInvoicesResponse>(
      `/api/companies/${encodeURIComponent(slug)}/recurring-invoices`,
    ).then((r) => r.recurringInvoices),

  /**
   * Creates a recurring-invoice template (#386). The cockpit hands the server
   * a minimal `{name, interval, firstIssueDate, paymentTermsDays,
   * deliveryPeriodMode, notes, vatRatePercent, currency, customerId, buyer,
   * lines}`; the server runs `computeInvoiceAmounts` + `resolveInvoiceMasterData`
   * and then `createRecurringInvoiceTemplate` — the same core path the CLI's
   * `recurring-invoice create` uses. Write-irreversible (a template that
   * cannot be edited or deleted, only retired), so the body carries
   * `confirm: true` to match the surrounding write-routes.
   */
  createRecurringInvoiceTemplate: (
    slug: string,
    input: RecurringInvoiceTemplateInput,
  ) =>
    request<{ ok: true; template: RecurringInvoiceTemplateCreatedResult }>(
      `/api/companies/${encodeURIComponent(slug)}/recurring-invoices`,
      {
        method: "POST",
        body: JSON.stringify({ ...input, confirm: true }),
      },
    ).then((r) => r.template),

  /**
   * Generates the next invoice from a template for the given `asOfDate`. The
   * core is idempotent: a second call for the same period returns the existing
   * generation with `created: false`. Write-irreversible (issues an invoice
   * document), so the body carries `confirm: true`.
   */
  generateRecurringInvoice: (slug: string, templateId: number, asOfDate: string) =>
    request<{ ok: true; generation: RecurringInvoiceGenerationResult }>(
      `/api/companies/${encodeURIComponent(slug)}/recurring-invoices/${templateId}/generate`,
      {
        method: "POST",
        body: JSON.stringify({ asOfDate, confirm: true }),
      },
    ).then((r) => r.generation),

  /**
   * Retires (deactivates) a recurring-invoice template (#435). Templates are
   * append-only by schema, so this flips `active` from 1 -> 0 and audit-logs
   * the change; the cockpit hides the generate button on retired templates.
   * The trigger forbids reactivation — an owner who needs to change terms
   * creates a new template that supersedes the retired one.
   */
  retireRecurringInvoiceTemplate: (
    slug: string,
    templateId: number,
    reason?: string,
  ) =>
    request<{ ok: true; template: { id: number; retired: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/recurring-invoices/${templateId}/retire`,
      {
        method: "POST",
        body: JSON.stringify({
          confirm: true,
          ...(reason ? { reason } : {}),
        }),
      },
    ).then((r) => r.template),

  archive: (slug: string, year: string) =>
    request<ArchiveResponse>(
      `/api/companies/${encodeURIComponent(slug)}/archive/${encodeURIComponent(
        year,
      )}`,
    ).then((r) => r.archive),

  multiYear: (slug: string) =>
    request<MultiYearResponse>(
      `/api/companies/${encodeURIComponent(slug)}/multi-year`,
    ).then((r) => r.multiYear),

  invoices: (slug: string, year?: string) =>
    request<InvoicesResponse>(
      `/api/companies/${encodeURIComponent(slug)}/invoices${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.invoices),

  contacts: (slug: string) =>
    request<ContactsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/contacts`,
    ).then((r) => r.contacts),

  obligations: (slug: string, year?: string) =>
    request<ObligationsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/obligations${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.obligations),

  cashflow: (slug: string, year?: string) =>
    request<CashflowResponse>(
      `/api/companies/${encodeURIComponent(slug)}/cashflow${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.cashflow),

  /** Mileage register (Kørsel, #335) for the selected fiscal year. */
  mileage: (slug: string, year?: string) =>
    request<MileageResponse>(
      `/api/companies/${encodeURIComponent(slug)}/mileage${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.mileage),

  /**
   * Registers a single mileage entry (Kørsel, #335). Calls the same
   * `createMileageEntry` core function the CLI's `mileage add` and the MCP
   * tool use. The mileage register is append-only audit data, so the body
   * carries `confirm: true`.
   */
  createMileageEntry: (slug: string, input: MileageEntryInput) =>
    request<{ ok: true; mileage: MileageEntrySummary }>(
      `/api/companies/${encodeURIComponent(slug)}/mileage`,
      {
        method: "POST",
        body: JSON.stringify({
          ...input,
          ...(input.rateSource ? { rateSource: input.rateSource } : {}),
          ...(input.notes ? { notes: input.notes } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.mileage),

  /**
   * #339 — the effective (latest-revision) budget lines for one fiscal year.
   * Drives the Budget input grid in the cockpit. The server collapses every
   * (account, period) pair to the newest revision before returning.
   */
  budget: (slug: string, year?: string) =>
    request<BudgetResponse>(
      `/api/companies/${encodeURIComponent(slug)}/budget${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.budget),

  /**
   * #339 — the budget-vs-faktisk comparison for one fiscal year. Third caller
   * of the same `buildBudgetVsActual` core the CLI report uses, so every
   * surface gets the same numbers and the same sign convention.
   */
  budgetVsActual: (slug: string, year?: string) =>
    request<BudgetVsActualResponse>(
      `/api/companies/${encodeURIComponent(slug)}/budget-vs-actual${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.budgetVsActual),

  /**
   * #339 — append a budget revision for one (account, period) cell. The core
   * is append-only: a second call for the same pair inserts a new revision
   * and the latest wins, so re-saving the grid is always safe.
   */
  setBudget: (slug: string, input: SetBudgetInput) =>
    request<{
      ok: true;
      budget: {
        id: number | null;
        accountNo: string | null;
        period: string | null;
        amount: number | null;
      };
    }>(`/api/companies/${encodeURIComponent(slug)}/budget`, {
      method: "POST",
      body: JSON.stringify({
        accountNo: input.accountNo,
        period: input.period,
        amount: input.amount,
        ...(input.notes ? { notes: input.notes } : {}),
      }),
    }).then((r) => r.budget),

  createCompany: (input: CreateCompanyInput) =>
    request<{ ok: true; company: { slug: string; name: string } }>(
      "/api/companies",
      { method: "POST", body: JSON.stringify(input) },
    ).then((r) => r.company),

  updateCompany: (slug: string, input: UpdateCompanyInput) =>
    request<{ ok: true; company: { slug: string; name: string; archived: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}`,
      { method: "PATCH", body: JSON.stringify(input) },
    ).then((r) => r.company),

  companySettings: (slug: string) =>
    request<CompanySettingsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/company`,
    ).then((r) => r.company),

  /**
   * Updates the editable company profile + bank/payment details (#284). Only
   * the fields present in `input` are changed. The primary bank account it
   * creates is the one every issued-invoice payment block reads from.
   */
  updateCompanyProfile: (slug: string, input: CompanyProfileInput) =>
    request<CompanySettingsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/company`,
      { method: "PATCH", body: JSON.stringify(input) },
    ).then((r) => r.company),

  syncCvr: (slug: string) =>
    request<SyncCvrResponse>(
      `/api/companies/${encodeURIComponent(slug)}/sync-cvr`,
      { method: "POST" },
    ).then((r) => r.sync),

  /**
   * #402 — whether the server has CVR_USERNAME/CVR_PASSWORD set, so the
   * cockpit can disable "Hent fra CVR" with a friendly note instead of
   * letting the owner trigger a silent failure.
   */
  cvrStatus: () =>
    request<CvrSystemStatusResponse>("/api/system/cvr-status").then(
      (r) => r.cvrStatus,
    ),

  /**
   * Closes an accounting period (#287) — the prerequisite for a momsangivelse.
   * Calls the same `closeAccountingPeriod` core the CLI's `period close` uses.
   * Write-irreversible-shaped, so the server's pipeline requires `confirm`.
   */
  closePeriod: (slug: string, input: ClosePeriodInput) =>
    request<ClosePeriodResponse>(
      `/api/companies/${encodeURIComponent(slug)}/periods/close`,
      {
        method: "POST",
        body: JSON.stringify({
          periodStart: input.periodStart,
          periodEnd: input.periodEnd,
          ...(input.kind ? { kind: input.kind } : {}),
          ...(input.reference ? { reference: input.reference } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.period),

  /**
   * Reopens a closed accounting period (#301) — the controlled, audit-logged
   * recovery path for a period closed too early. `reason` is recorded verbatim
   * in the audit log. Calls the same `reopenAccountingPeriod` core the CLI's
   * `period reopen` uses; the server's pipeline requires `confirm`.
   */
  reopenPeriod: (slug: string, input: ReopenPeriodInput) =>
    request<ReopenPeriodResponse>(
      `/api/companies/${encodeURIComponent(slug)}/periods/reopen`,
      {
        method: "POST",
        body: JSON.stringify({
          periodStart: input.periodStart,
          periodEnd: input.periodEnd,
          ...(input.kind ? { kind: input.kind } : {}),
          reason: input.reason,
          confirm: true,
        }),
      },
    ).then((r) => r.period),

  // --- write actions (#213) -----------------------------------------------
  //
  // Bookkeeping mutations. The server routes these through the
  // `withCompanyMutation` pipeline (backup lock, actor attribution); a 409
  // means the backup lock is engaged — callers render that kindly.

  /** Resolves an open exception. `note` is optional free text. */
  resolveException: (slug: string, id: number, note?: string) =>
    request<{ ok: true; exception: { id: number; resolved: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/exceptions/${id}/resolve`,
      {
        method: "POST",
        body: JSON.stringify(note ? { note } : {}),
      },
    ).then((r) => r.exception),

  /**
   * Imports a bank-statement CSV (#213, slice 2). The browser reads the file
   * and passes its text as `csvContent`; the server writes it to a temp file
   * and calls the same core import the CLI/MCP use. Destructive, so the body
   * carries `confirm: true`.
   */
  importBank: (slug: string, input: BankImportInput) =>
    request<{ ok: true; import: BankImportSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/bank/import`,
      {
        method: "POST",
        body: JSON.stringify({
          csvContent: input.csvContent,
          ...(input.account ? { account: input.account } : {}),
          ...(input.profile ? { profile: input.profile } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.import),

  /**
   * Generates the accountant-handoff package and returns it as a downloadable
   * .tar blob — the cockpit's "share with revisor" action. The server packs
   * the same files the CLI's `system export-accountant` produces; the browser
   * triggers a download from the returned blob.
   */
  accountantExport: async (
    slug: string,
    input: { periodStart: string; periodEnd: string },
  ): Promise<AccountantExportResult> => {
    const res = await fetch(
      `/api/companies/${encodeURIComponent(slug)}/accountant-export`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ ...input, confirm: true }),
      },
    );
    if (!res.ok) {
      let message = `HTTP ${res.status}`;
      let code = "internal";
      try {
        // #368: unified envelope — `errors[0]` is the message, `code` is the
        // top-level enum.
        const body = (await res.json()) as {
          errors?: unknown;
          code?: unknown;
        };
        if (Array.isArray(body.errors) && body.errors.length > 0) {
          message = String(body.errors[0]);
        }
        if (typeof body.code === "string") code = body.code;
      } catch {}
      throw new ApiError(code, message, res.status);
    }
    const filename =
      parseFilenameFromContentDisposition(res.headers.get("content-disposition")) ??
      `revisor-eksport-${slug}-${input.periodStart}-${input.periodEnd}.tar`;
    return {
      blob: await res.blob(),
      filename,
      journalEntryCount: Number(
        res.headers.get("x-rentemester-journal-entries") ?? 0,
      ),
      documentCount: Number(res.headers.get("x-rentemester-documents") ?? 0),
      bankTransactionCount: Number(
        res.headers.get("x-rentemester-bank-transactions") ?? 0,
      ),
    };
  },

  /**
   * Imports an export file from another accounting system. The browser reads
   * the chosen file and passes its text as `content`; the server recognises
   * which system the file came from and routes it to the matching importer.
   * Destructive (it appends master data), so the body carries `confirm: true`.
   */
  importData: (slug: string, input: DataImportInput) =>
    request<{ ok: true; import: DataImportSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/import`,
      {
        method: "POST",
        body: JSON.stringify({
          fileName: input.fileName,
          content: input.content,
          enrichCvr: input.enrichCvr === true,
          confirm: true,
        }),
      },
    ).then((r) => r.import),

  /**
   * Ingests a document/voucher (bilag) (#213, slice 3). The browser reads the
   * (possibly binary) file and passes it base64-encoded; the server decodes it
   * to a temp file and calls the same core ingest the CLI/MCP use. Destructive,
   * so the body carries `confirm: true`.
   */
  ingestDocument: (slug: string, input: DocumentIngestInput) =>
    request<{
      ok: true;
      document: { id: number | null; documentNo: string | null };
    }>(`/api/companies/${encodeURIComponent(slug)}/documents/ingest`, {
      method: "POST",
      body: JSON.stringify({
        fileName: input.fileName,
        fileBase64: input.fileBase64,
        metadata: input.metadata,
        ...(input.vendorId ? { vendorId: input.vendorId } : {}),
        ...(input.force ? { force: true } : {}),
        confirm: true,
      }),
    }).then((r) => r.document),

  /**
   * #440 — forhåndsviser en faktura uden at udstede. Same body as
   * `issueInvoice`, but the server runs `previewIssuedInvoicePdf` instead of
   * `issueInvoice`: no sequence draw, no `documents` row, no `audit_log`
   * entry. The response is the raw PDF (Content-Type application/pdf) so the
   * cockpit can open it in a new tab via `URL.createObjectURL`. Errors come
   * back as the regular `{ok:false,error}` envelope (400 for validation,
   * 409 for unknown customer/master-data, 404 for unknown company).
   */
  previewInvoice: async (
    slug: string,
    input: InvoiceIssueInput,
  ): Promise<Blob> => {
    const body = {
      issueDate: input.issueDate,
      lines: input.lines,
      ...(input.vatRatePercent !== undefined
        ? { vatRatePercent: input.vatRatePercent }
        : {}),
      ...(input.customerId ? { customerId: input.customerId } : {}),
      ...(input.invoiceNumber ? { invoiceNumber: input.invoiceNumber } : {}),
      ...(input.dueDate ? { dueDate: input.dueDate } : {}),
      ...(input.currency ? { currency: input.currency } : {}),
      ...(input.seller ? { seller: input.seller } : {}),
      ...(input.buyer ? { buyer: input.buyer } : {}),
    };
    let res: Response;
    try {
      res = await fetch(
        `/api/companies/${encodeURIComponent(slug)}/invoices/preview`,
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(body),
        },
      );
    } catch {
      throw new ApiError(
        "network",
        "Kunne ikke nå serveren. Kører `rentemester serve`?",
        0,
      );
    }
    if (!res.ok) {
      let message = `HTTP ${res.status}`;
      let code = "internal";
      try {
        const errBody = (await res.json()) as {
          error?: { code?: string; message?: string };
        };
        message = errBody.error?.message ?? message;
        code = errBody.error?.code ?? code;
      } catch {}
      throw new ApiError(code, message, res.status);
    }
    return await res.blob();
  },

  /**
   * Issues a sales invoice (#213, slice 4). The human enters the customer and
   * the line items; the server COMPUTES every line total, net, VAT and gross
   * via the same core path the CLI's `invoice create` uses — the human never
   * does invoice arithmetic. Issuing is a kladde (no journal entry yet), so no
   * `confirm` is required.
   */
  issueInvoice: (slug: string, input: InvoiceIssueInput) =>
    request<{ ok: true; invoice: InvoiceIssueSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/invoices/issue`,
      {
        method: "POST",
        body: JSON.stringify({
          issueDate: input.issueDate,
          lines: input.lines,
          ...(input.vatRatePercent !== undefined
            ? { vatRatePercent: input.vatRatePercent }
            : {}),
          ...(input.customerId ? { customerId: input.customerId } : {}),
          ...(input.invoiceNumber ? { invoiceNumber: input.invoiceNumber } : {}),
          ...(input.dueDate ? { dueDate: input.dueDate } : {}),
          ...(input.currency ? { currency: input.currency } : {}),
          ...(input.seller ? { seller: input.seller } : {}),
          ...(input.buyer ? { buyer: input.buyer } : {}),
        }),
      },
    ).then((r) => r.invoice),

  /**
   * Creates a new customer (#390). Mirrors the CLI's `customer add`. The
   * Cockpit's Kontakter page calls this directly so the owner never has to
   * leave the browser for daily master-data maintenance.
   */
  createCustomer: (slug: string, input: CustomerInput) =>
    request<{ ok: true; customer: { id: number } }>(
      `/api/companies/${encodeURIComponent(slug)}/customers`,
      { method: "POST", body: JSON.stringify(input) },
    ).then((r) => r.customer),

  /** Updates an existing customer — only the fields present in `input` change. */
  updateCustomer: (slug: string, id: number, input: Partial<CustomerInput>) =>
    request<{ ok: true; customer: { id: number; ok: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/customers/${id}`,
      { method: "PATCH", body: JSON.stringify(input) },
    ).then((r) => r.customer),

  /**
   * #430 — sletter en kunde fra master data. Sletningen blokeres server-side
   * (returnerer ApiError) hvis kunden er i brug på en åben (ikke-betalt)
   * udstedt faktura; cockpittet viser fejlbeskeden verbatim. Bogførte
   * fakturaer beholder deres navne-snapshot — historikken er intakt.
   * Write-irreversibel, så body'en bærer `confirm: true`.
   */
  deleteCustomer: (slug: string, id: number) =>
    request<{ ok: true; customer: { id: number; deleted: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/customers/${id}`,
      { method: "DELETE", body: JSON.stringify({ confirm: true }) },
    ).then((r) => r.customer),

  /** Creates a new vendor (#390). */
  createVendor: (slug: string, input: VendorInput) =>
    request<{ ok: true; vendor: { id: number } }>(
      `/api/companies/${encodeURIComponent(slug)}/vendors`,
      { method: "POST", body: JSON.stringify(input) },
    ).then((r) => r.vendor),

  /** Updates an existing vendor — only the fields present in `input` change. */
  updateVendor: (slug: string, id: number, input: Partial<VendorInput>) =>
    request<{ ok: true; vendor: { id: number; ok: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/vendors/${id}`,
      { method: "PATCH", body: JSON.stringify(input) },
    ).then((r) => r.vendor),

  /**
   * #430 — sletter en leverandør. Blokeres hvis leverandøren er i brug på en
   * åben gæld (`payables` med `vendor_id`-FK). Write-irreversibel.
   */
  deleteVendor: (slug: string, id: number) =>
    request<{ ok: true; vendor: { id: number; deleted: boolean } }>(
      `/api/companies/${encodeURIComponent(slug)}/vendors/${id}`,
      { method: "DELETE", body: JSON.stringify({ confirm: true }) },
    ).then((r) => r.vendor),

  /**
   * Looks an 8-digit Danish CVR number up in the CVR register (#390). The
   * credentials live on the server; the browser only ever sees the resolved
   * snapshot. A missing-credentials response is returned with `ok:false`
   * inside the envelope so the modal can show a calm hint, not an error.
   */
  cvrLookup: (slug: string, cvr: string) =>
    request<{ ok: true; cvr: CvrLookupResult }>(
      `/api/companies/${encodeURIComponent(slug)}/cvr-lookup?cvr=${encodeURIComponent(cvr)}`,
    ).then((r) => r.cvr),

  /**
   * Settles an issued invoice against a bank payment (#213, slice 4). The
   * human identifies the bank receipt by its transaction id or reference.
   * Write-irreversible, so the body carries `confirm: true`.
   */
  settleInvoice: (slug: string, input: InvoiceSettleInput) =>
    request<{ ok: true; settlement: InvoiceSettleSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/invoices/settle`,
      {
        method: "POST",
        body: JSON.stringify({
          invoiceDocumentId: input.invoiceDocumentId,
          ...(input.bankTransactionId
            ? { bankTransactionId: input.bankTransactionId }
            : {}),
          ...(input.bankTransactionReference
            ? { bankTransactionReference: input.bankTransactionReference }
            : {}),
          ...(input.paymentDate ? { paymentDate: input.paymentDate } : {}),
          ...(input.amount !== undefined ? { amount: input.amount } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.settlement),

  /**
   * Issues a credit note for an already-issued sales invoice (#412). The
   * human supplies the source invoice + a mandatory reason; Rentemester
   * computes the credit amount from the original gross unless `grossAmount`
   * is given. Write-irreversible (it inserts a credit-note document AND
   * appends a reversal journal entry), so the body carries `confirm: true`.
   */
  creditInvoice: (slug: string, input: InvoiceCreditNoteInput) =>
    request<{ ok: true; creditNote: InvoiceCreditNoteSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/invoices/credit-note`,
      {
        method: "POST",
        body: JSON.stringify({
          invoiceDocumentId: input.invoiceDocumentId,
          issueDate: input.issueDate,
          reason: input.reason,
          ...(input.grossAmount !== undefined
            ? { grossAmount: input.grossAmount }
            : {}),
          ...(input.creditNoteNumber
            ? { creditNoteNumber: input.creditNoteNumber }
            : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.creditNote),

  /**
   * #429 — sends an issued invoice to the customer's e-mail with the PDF
   * attached, from the cockpit. Third caller of the SAME `sendInvoiceEmail`
   * core function the CLI's `invoice send` command and the MCP tool
   * `invoice_send_email` use, so the cockpit and the terminal produce a
   * byte-identical MIME message and `email_send_log` row.
   *
   * SMTP CONFIG (host/port/fromAddress + optional username/password) is read
   * server-side from `config/smtp.json` in the company directory — credentials
   * never enter the request body. Idempotent: an identical send collapses onto
   * the existing send-log row instead of re-transmitting. Write-irreversible
   * (an `email_send_log` row + an `audit_log` entry), so the body carries
   * `confirm: true`.
   */
  sendInvoiceByEmail: (slug: string, input: InvoiceSendEmailInput) =>
    request<{ ok: true; delivery: InvoiceSendEmailSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/invoices/send-email`,
      {
        method: "POST",
        body: JSON.stringify({
          invoiceDocumentId: input.invoiceDocumentId,
          to: input.to,
          ...(input.kind ? { kind: input.kind } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.delivery),

  /**
   * #434 — sends a payment reminder (betalingspaamindelse) for an overdue
   * issued invoice. Single endpoint that combines three core calls so the
   * cockpit's "Send rykker" action is a one-click write:
   *   1. `registerInvoiceReminder` — records the reminder + statutory fee
   *      (max 100 kr/reminder, max 3 reminders per rentel. § 9b),
   *   2. (optional, when `bookFee` is true) `postInvoiceReminderToLedger` —
   *      journals the fee against the customer receivable,
   *   3. `sendInvoiceEmail` with `kind: 'reminder'` — same SMTP transport as
   *      "Send på mail", so the message is byte-identical to a CLI send.
   *
   * Write-irreversible (it inserts an `invoice_reminders` row, may append a
   * journal entry, and always appends an `email_send_log` + `audit_log`
   * row), so the body carries `confirm: true`.
   */
  sendInvoiceReminder: (slug: string, input: InvoiceSendReminderInput) =>
    request<{ ok: true; reminder: InvoiceSendReminderSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/invoices/send-reminder`,
      {
        method: "POST",
        body: JSON.stringify({
          invoiceDocumentId: input.invoiceDocumentId,
          to: input.to,
          bookFee: input.bookFee,
          ...(input.feeAmount !== undefined ? { feeAmount: input.feeAmount } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.reminder),

  /**
   * #428 — sends an issued invoice as an e-faktura (NemHandel/PEPPOL) via
   * the cockpit. Third caller of the SAME `submitPublicEInvoicePeppol` core
   * function the CLI / MCP use; the server loads its access-point config
   * from `RENTEMESTER_PEPPOL_ACCESS_POINT` so credentials never enter the
   * body. Write-irreversible (it appends a `peppol_submissions` row + an
   * `audit_log` entry), so the body carries `confirm: true`.
   */
  sendInvoiceAsEInvoice: (slug: string, input: InvoiceSendEInvoiceInput) =>
    request<{ ok: true; submission: InvoiceSendEInvoiceSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/invoices/send-public`,
      {
        method: "POST",
        body: JSON.stringify({
          invoiceDocumentId: input.invoiceDocumentId,
          confirm: true,
        }),
      },
    ).then((r) => r.submission),

  /**
   * #407 — read-side data backing the Bogfør-bilag modal: the bilag's fields
   * to prefill, the bookable expense accounts and the unmatched outgoing
   * bank transactions the owner can pair the bilag with.
   */
  documentBookingOptions: (slug: string, documentId: number) =>
    request<{ ok: true; options: DocumentBookingOptions }>(
      `/api/companies/${encodeURIComponent(slug)}/documents/${documentId}/booking-options`,
    ).then((r) => r.options),

  /**
   * #407 — books an ingested purchase document (bilag) as an expense against
   * an unmatched outgoing bank transaction. Third caller of the SAME
   * `bookExpenseFromBank` core function the CLI's `expense book` and the MCP
   * tool use. Write-irreversible (it appends a journal entry that links both
   * the document and the bank transaction), so the body carries
   * `confirm: true`.
   */
  bookDocumentExpense: (slug: string, input: DocumentBookExpenseInput) =>
    request<{ ok: true; booking: DocumentBookExpenseSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/documents/book-expense`,
      {
        method: "POST",
        body: JSON.stringify({
          documentId: input.documentId,
          bankTransactionId: input.bankTransactionId,
          expenseAccountNo: input.expenseAccountNo,
          ...(input.vatTreatment ? { vatTreatment: input.vatTreatment } : {}),
          ...(input.paymentAccountNo
            ? { paymentAccountNo: input.paymentAccountNo }
            : {}),
          ...(input.transactionDate
            ? { transactionDate: input.transactionDate }
            : {}),
          ...(input.text ? { text: input.text } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.booking),

  // --- Anlæg (fixed assets) — #336 ----------------------------------------
  //
  // The cockpit becomes a THIRD caller of `src/core/assets.ts` alongside the
  // CLI's `asset` sub-commands and the MCP `asset_*` tools — no depreciation
  // arithmetic crosses the wire. Write actions carry `confirm: true`; the
  // backend's `withCompanyMutation` enforces backup-lock + actor attribution.

  /** GET /api/companies/:slug/assets — the anlægskartotek. */
  assets: (slug: string) =>
    request<AssetsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/assets`,
    ).then((r) => r.assets),

  /** GET .../assets/:id/next-depreciation — what posts on a "Beregn afskrivning". */
  assetNextDepreciation: (slug: string, assetId: number) =>
    request<AssetNextDepreciationResponse>(
      `/api/companies/${encodeURIComponent(slug)}/assets/${assetId}/next-depreciation`,
    ).then((r) => r.nextDepreciation),

  /** POST /api/companies/:slug/assets — registers a capitalised anlæg. */
  registerAsset: (slug: string, input: AssetRegisterInput) =>
    request<{ ok: true; asset: AssetRegisterSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/assets`,
      {
        method: "POST",
        body: JSON.stringify({
          name: input.name,
          category: input.category,
          acquisitionDate: input.acquisitionDate,
          cost: input.cost,
          usefulLifeMonths: input.usefulLifeMonths,
          purchaseDocumentId: input.purchaseDocumentId,
          ...(input.assetAccountNo
            ? { assetAccountNo: input.assetAccountNo }
            : {}),
          ...(input.depreciationExpenseAccountNo
            ? {
                depreciationExpenseAccountNo:
                  input.depreciationExpenseAccountNo,
              }
            : {}),
          ...(input.accumulatedDepreciationAccountNo
            ? {
                accumulatedDepreciationAccountNo:
                  input.accumulatedDepreciationAccountNo,
              }
            : {}),
          ...(input.note ? { note: input.note } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.asset),

  /** POST .../assets/:id/depreciate — posts the next depreciation period. */
  depreciateAsset: (
    slug: string,
    assetId: number,
    input: AssetDepreciateInput = {},
  ) =>
    request<{ ok: true; depreciation: AssetDepreciateSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/assets/${assetId}/depreciate`,
      {
        method: "POST",
        body: JSON.stringify({
          ...(input.transactionDate
            ? { transactionDate: input.transactionDate }
            : {}),
          ...(input.periodIndex !== undefined
            ? { periodIndex: input.periodIndex }
            : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.depreciation),

  /** POST .../assets/write-off — books a straksafskrivning. */
  writeOffAsset: (slug: string, input: AssetWriteOffInput) =>
    request<{ ok: true; writeOff: AssetWriteOffSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/assets/write-off`,
      {
        method: "POST",
        body: JSON.stringify({
          name: input.name,
          category: input.category,
          acquisitionDate: input.acquisitionDate,
          transactionDate: input.transactionDate,
          cost: input.cost,
          purchaseDocumentId: input.purchaseDocumentId,
          expenseAccountNo: input.expenseAccountNo,
          thresholdRuleSource: input.thresholdRuleSource,
          ...(input.paymentAccountNo
            ? { paymentAccountNo: input.paymentAccountNo }
            : {}),
          ...(input.note ? { note: input.note } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.writeOff),

  // --- Leverandørfaktura (payables) — #340 --------------------------------

  /**
   * #340 — Leverandørfaktura-arbejdsbordet. Returns the kreditorliste from
   * `core/payables.ts#buildPayablesList` plus the modal picker rows
   * (unregistered purchase documents, expense accounts, vendors).
   */
  payables: (slug: string, status?: PayableListStatusFilter, asOf?: string) => {
    const params = new URLSearchParams();
    if (status) params.set("status", status);
    if (asOf) params.set("asOf", asOf);
    const qs = params.toString();
    return request<PayablesResponse>(
      `/api/companies/${encodeURIComponent(slug)}/payables${qs ? `?${qs}` : ""}`,
    ).then((r) => r.payables);
  },

  /**
   * #340 — registers an existing ingested purchase document as a
   * leverandørfaktura. Write-irreversible (it appends a journal entry), so
   * the body carries `confirm: true`.
   */
  registerPayable: (slug: string, input: PayableRegisterInput) =>
    request<{ ok: true; payable: PayableRegisterSummary }>(
      `/api/companies/${encodeURIComponent(slug)}/payables`,
      {
        method: "POST",
        body: JSON.stringify({
          documentId: input.documentId,
          billDate: input.billDate,
          dueDate: input.dueDate,
          expenseAccountNo: input.expenseAccountNo,
          ...(input.vatTreatment ? { vatTreatment: input.vatTreatment } : {}),
          ...(input.vendorId !== undefined ? { vendorId: input.vendorId } : {}),
          ...(input.note ? { note: input.note } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.payable),

  /**
   * #340 — applies an outgoing bank payment to an open leverandørfaktura.
   * Write-irreversible (it appends a settlement journal entry + a
   * `payable_payments` row), so the body carries `confirm: true`.
   */
  payPayable: (slug: string, input: PayablePayInput) =>
    request<{ ok: true; payment: PayablePaySummary }>(
      `/api/companies/${encodeURIComponent(slug)}/payables/${input.payableId}/pay`,
      {
        method: "POST",
        body: JSON.stringify({
          bankTransactionId: input.bankTransactionId,
          ...(input.paymentDate ? { paymentDate: input.paymentDate } : {}),
          ...(input.amount !== undefined ? { amount: input.amount } : {}),
          ...(input.paymentAccountNo
            ? { paymentAccountNo: input.paymentAccountNo }
            : {}),
          ...(input.note ? { note: input.note } : {}),
          confirm: true,
        }),
      },
    ).then((r) => r.payment),

  // --- Agent-forslag → menneskelig godkendelse (#346) ----------------------
  //
  // The agent loop + the exception sync functions raise `AGENT_*` exceptions
  // whenever a deterministic agent run needs a human decision. These API
  // methods drive the dedicated cockpit view that lists them, approves them,
  // or rejects them. Approve/reject NEVER post on their own — they only
  // resolve the underlying exception with a Danish decision note, so the
  // audit trail records WHO decided and WHY. The actual ledger action
  // (e.g. "Beregn afskrivning", "payable pay") lives on the deep-linked view.

  /** GET /api/companies/:slug/agent-suggestions — the open agent-forslag queue. */
  agentSuggestions: (slug: string) =>
    request<import("./types").AgentSuggestionsResponse>(
      `/api/companies/${encodeURIComponent(slug)}/agent-suggestions`,
    ).then((r) => r.agentSuggestions),

  /** POST .../agent-suggestions/:id/approve — owner accepts the suggestion. */
  approveAgentSuggestion: (slug: string, exceptionId: number, note?: string) =>
    request<import("./types").AgentSuggestionDecisionResponse>(
      `/api/companies/${encodeURIComponent(slug)}/agent-suggestions/${exceptionId}/approve`,
      {
        method: "POST",
        body: JSON.stringify(note ? { note } : {}),
      },
    ).then((r) => r.suggestion),

  /** POST .../agent-suggestions/:id/reject — owner declines the suggestion. */
  rejectAgentSuggestion: (slug: string, exceptionId: number, note?: string) =>
    request<import("./types").AgentSuggestionDecisionResponse>(
      `/api/companies/${encodeURIComponent(slug)}/agent-suggestions/${exceptionId}/reject`,
      {
        method: "POST",
        body: JSON.stringify(note ? { note } : {}),
      },
    ).then((r) => r.suggestion),
};

/** Wire type for one bookable expense account (#407). */
export type ExpenseAccountOption = {
  accountNo: string;
  name: string;
  defaultVatCode: string | null;
};

/** Wire type for one unmatched outgoing bank transaction (#407). */
export type UnmatchedBankOption = {
  id: number;
  date: string;
  text: string;
  amount: number;
  currency: string;
  reference: string | null;
};

/** Wire type for the bilag fields the modal prefills its form from (#407). */
export type DocumentBookingOptionsDocument = {
  id: number;
  documentNo: string | null;
  documentType: string;
  invoiceNo: string | null;
  invoiceDate: string | null;
  supplierName: string | null;
  amountIncVat: number | null;
  vatAmount: number | null;
  currency: string;
};

/** Combined read-side response for the Bogfør-bilag modal (#407). */
export type DocumentBookingOptions = {
  document: DocumentBookingOptionsDocument;
  expenseAccounts: ExpenseAccountOption[];
  unmatchedOutgoingBank: UnmatchedBankOption[];
};

/** Allowed VAT treatments for an expense booking (#407). */
export type ExpenseVatTreatment =
  | "standard"
  | "reverse_charge"
  | "representation"
  | "exempt";

/** Input for `api.bookDocumentExpense` (#407). */
export type DocumentBookExpenseInput = {
  documentId: number;
  bankTransactionId: number;
  expenseAccountNo: string;
  vatTreatment?: ExpenseVatTreatment;
  paymentAccountNo?: string;
  transactionDate?: string;
  text?: string;
};

/** The booking result the server echoes back (#407). */
export type DocumentBookExpenseSummary = {
  entryId: number | null;
  documentId: number | null;
  bankTransactionId: number | null;
  grossAmount: number | null;
  netAmount: number | null;
  vatAmount: number | null;
  vatTreatment: string | null;
};

/** A single invoice line as the human enters it — Rentemester computes totals. */
export type InvoiceLineInput = {
  description: string;
  quantity: number;
  unitPriceExVat: number;
};

/** An invoice party (seller / buyer) — all fields optional. */
export type InvoicePartyInput = {
  name?: string;
  address?: string;
  vatOrCvr?: string;
};

/** Input for `api.issueInvoice`. */
export type InvoiceIssueInput = {
  issueDate: string;
  lines: InvoiceLineInput[];
  vatRatePercent?: number;
  customerId?: number;
  invoiceNumber?: string;
  dueDate?: string;
  currency?: string;
  seller?: InvoicePartyInput;
  buyer?: InvoicePartyInput;
};

/** The issue result the server echoes back — every amount Rentemester computed. */
export type InvoiceIssueSummary = {
  documentId: number | null;
  invoiceNumber: string | null;
  netAmount: number;
  vatRate: number;
  vatAmount: number;
  grossAmount: number;
  lines: Array<InvoiceLineInput & { lineTotalExVat: number }>;
};

/** Input for `api.settleInvoice`. */
export type InvoiceSettleInput = {
  invoiceDocumentId: number;
  bankTransactionId?: number;
  bankTransactionReference?: string;
  paymentDate?: string;
  amount?: number;
};

/** The settlement result the server echoes back. */
export type InvoiceSettleSummary = {
  entryId: number | null;
  paymentId: number | null;
  principalAmount: number;
  claimAmount: number;
  invoiceNumber: string | null;
  openBalance: number | null;
};

/** Input for `api.creditInvoice` (#412). */
export type InvoiceCreditNoteInput = {
  invoiceDocumentId: number;
  issueDate: string;
  reason: string;
  /** Optional partial credit. When omitted, the remaining gross is credited. */
  grossAmount?: number;
  creditNoteNumber?: string;
};

/** Input for `api.sendInvoiceAsEInvoice` (#428). */
export type InvoiceSendEInvoiceInput = {
  invoiceDocumentId: number;
};

/** The PEPPOL submission result the server echoes back (#428). */
export type InvoiceSendEInvoiceSummary = {
  invoiceNumber: string | null;
  submissionReference: string | null;
  status: "prepared" | "acknowledged" | null;
  duplicate: boolean;
  envelopeSha256: string | null;
  oioublSha256: string | null;
};

/** Input for `api.sendInvoiceByEmail` (#429). */
export type InvoiceSendEmailInput = {
  invoiceDocumentId: number;
  /** Recipient email address — prefilled from the customer but editable. */
  to: string;
  /** What to send (#429): the invoice itself or a payment reminder. */
  kind?: "invoice" | "reminder";
};

/** Input for `api.sendInvoiceReminder` (#434). */
export type InvoiceSendReminderInput = {
  invoiceDocumentId: number;
  /** Recipient email address — prefilled from the customer but editable. */
  to: string;
  /**
   * Whether to also book the statutory reminder fee (max 100 kr) against the
   * receivable account. When false the reminder is only registered + emailed.
   */
  bookFee: boolean;
  /** Optional override of the default 100 kr reminder fee. */
  feeAmount?: number;
};

/** The reminder-send result the server echoes back (#434). */
export type InvoiceSendReminderSummary = {
  invoiceNumber: string | null;
  recipient: string | null;
  /** 1, 2 or 3 — which reminder in the statutory series this one was. */
  reminderSequence: number | null;
  /** Fee amount registered (kroner). */
  feeAmount: number | null;
  /** True when the fee was also booked to the receivable. */
  feeBooked: boolean;
  /** Journal entry number when the fee was booked, else null. */
  journalEntryNo: string | null;
  /** Message-id of the reminder e-mail. */
  messageId: string | null;
  /** True when this is a re-send that collapsed onto an existing send-log row. */
  duplicate: boolean;
};

/** The email-delivery result the server echoes back (#429). */
export type InvoiceSendEmailSummary = {
  invoiceNumber: string | null;
  recipient: string | null;
  subject: string | null;
  messageId: string | null;
  duplicate: boolean;
};

/** The credit-note result the server echoes back. */
export type InvoiceCreditNoteSummary = {
  documentId: number | null;
  creditNoteNumber: string | null;
  originalInvoiceNumber: string | null;
  journalEntryId: number | null;
  journalEntryNo: string | null;
};

/** Input for `api.importBank` — the CSV text plus optional account/profile. */
export type BankImportInput = {
  csvContent: string;
  account?: string;
  profile?: string;
};

/** The accountant-export result the server echoes back, plus the tar blob. */
export type AccountantExportResult = {
  /** The .tar archive as a Blob — the browser triggers a download from this. */
  blob: Blob;
  /** Suggested download filename derived from the response. */
  filename: string;
  /** Number of journal entries included — surfaced in the UI receipt. */
  journalEntryCount: number;
  /** Number of supporting documents included. */
  documentCount: number;
  /** Number of bank transactions included. */
  bankTransactionCount: number;
};

/** Picks the filename from a `filename*=UTF-8''…` content-disposition header. */
function parseFilenameFromContentDisposition(cd: string | null): string | null {
  if (!cd) return null;
  const m = cd.match(/filename\*=UTF-8''([^;]+)/i);
  if (!m || !m[1]) return null;
  try {
    return decodeURIComponent(m[1]);
  } catch {
    return null;
  }
}

/** Input for `api.importData` — the file name + text, plus the CVR-enrich opt-in. */
export type DataImportInput = {
  fileName: string;
  content: string;
  enrichCvr?: boolean;
};

/** The source system + data type the server recognised an imported file as. */
export type DataImportDetected = {
  id: string;
  label: string;
  system: string;
  dataType: string;
};

/** The contact-import counts the server echoes back. */
export type DataImportCounts = {
  parsed: number;
  customersCreated: number;
  vendorsCreated: number;
  skipped: number;
  enriched: number;
  enrichmentFailures: number;
};

/** The file-import result the server echoes back. */
export type DataImportSummary = {
  detected: DataImportDetected | null;
  summary: DataImportCounts;
  errors: string[];
};

/** The bank-import result the server echoes back. */
export type BankImportSummary = {
  importBatchId?: string;
  imported: number;
  skippedDuplicates: number;
  skippedDuplicateRows: string[];
  bankAccountSlug?: string;
  profile?: string;
  balanceWarnings: string[];
  exceptionsCreated: number;
};

/** Document metadata for `api.ingestDocument` — amounts are kroner (decimal DKK). */
export type DocumentIngestMetadata = {
  source: string;
  documentType?: "purchase_sale" | "cash_register_receipt";
  issueDate?: string;
  invoiceNo?: string;
  deliveryDescription?: string;
  amountIncVat?: number;
  currency?: string;
  sender?: { name?: string; address?: string; vatOrCvr?: string };
  recipient?: { name?: string; address?: string; vatOrCvr?: string };
  vatAmount?: number;
  paymentDetails?: string;
};

/** Input for `api.ingestDocument` — the base64 file plus its metadata. */
export type DocumentIngestInput = {
  fileName: string;
  fileBase64: string;
  metadata: DocumentIngestMetadata;
  vendorId?: number;
  force?: boolean;
};
