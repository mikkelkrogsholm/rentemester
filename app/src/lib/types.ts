// Wire types — the JSON shapes returned by `rentemester serve` (#170).
//
// These mirror `src/server/data.ts` and `src/server/router.ts`. They are kept
// deliberately as a hand-written copy: the SPA is a separate package and does
// not import from the backend's TypeScript sources.

/**
 * #368 — the unified cockpit/MCP/CLI error envelope. `errors[0]` is the
 * human-readable message; `code` is the discrete enum (`bad_request`,
 * `conflict`, …) for programmatic branching.
 */
export type ApiErrorBody = {
  ok: false;
  errors: string[];
  code: string;
};

export type HealthResponse = {
  ok: true;
  service: string;
  workspace: string;
  authRequired: boolean;
};

/**
 * #402 — wire shape for GET /api/system/cvr-status. `configured` is true when
 * the server has both CVR_USERNAME and CVR_PASSWORD set, so the cockpit can
 * tell the owner whether "Hent fra CVR" will actually work before they click.
 */
export type CvrSystemStatus = { configured: boolean };
export type CvrSystemStatusResponse = {
  ok: true;
  cvrStatus: CvrSystemStatus;
};

export type CompanyEntry = {
  slug: string;
  name: string;
  createdAt: string;
  archived: boolean;
};

export type CompanyListResponse = {
  ok: true;
  workspace: string;
  count: number;
  companies: CompanyEntry[];
};

/** One grouped open-task line on a portfolio card. */
export type ExceptionGroup = {
  type: string;
  count: number;
  severity: "low" | "medium" | "high";
  label: string;
  link: string | null;
};

/** The VAT block on a portfolio card — null when no VAT period is known. */
export type CompanyVatSummary = {
  payable: number;
  deadline: string;
  daysRemaining: number;
};

export type CompanySummary = {
  slug: string;
  name: string;
  cvr: string | null;
  archived: boolean;
  ledgerMissing: boolean;
  /** The fiscal year these figures cover, e.g. "2026"; null when unknown. */
  fiscalYear: string | null;
  /** Year-to-date result (resultat), kroner. */
  resultat: number;
  /** Year-to-date revenue (omsætning), kroner. */
  omsaetning: number;
  /** Actual bank balance from the imported statement, kroner; null if unknown. */
  actualBankBalance: number | null;
  /** Current half-year VAT position + deadline; null when unknown. */
  vat: CompanyVatSummary | null;
  /** Open tasks across the company. */
  openTaskCount: number;
  /** The open tasks grouped into Danish summary lines. */
  taskGroups: ExceptionGroup[];
  auditChainOk: boolean;
  // Legacy fields retained for older consumers.
  openInvoiceCount: number;
  openInvoiceTotal: number;
  overdueInvoiceCount: number;
  unlinkedBankCount: number;
  openExceptionCount: number;
  netVatPayable: number;
};

export type PortfolioOverview = {
  workspace: string;
  asOf: string;
  companyCount: number;
  /** Workspace-wide roll-up — how the whole portfolio is doing. */
  rollup: {
    resultat: number;
    liquidity: number;
    vatPayable: number;
    openTaskCount: number;
  };
  totals: {
    openInvoiceCount: number;
    openInvoiceTotal: number;
    overdueInvoiceCount: number;
    unlinkedBankCount: number;
    openExceptionCount: number;
    netVatPayable: number;
  };
  companies: CompanySummary[];
};

export type PortfolioResponse = {
  ok: true;
  portfolio: PortfolioOverview;
};

export type InvoiceRow = {
  invoiceNumber: string;
  customerName?: string;
  issueDate?: string;
  dueDate?: string;
  total?: number;
  openBalance: number;
  [key: string]: unknown;
};

export type ExceptionRow = {
  id: string | number;
  type: string;
  severity: string;
  status: string;
  message: string;
};

export type AuditLogRow = {
  occurredAt?: string;
  action?: string;
  summary?: string;
  [key: string]: unknown;
};

export type CompanyDashboard = {
  slug: string;
  asOf: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
    fiscalYearStartMonth: number | string;
    fiscalYearLabelStrategy: string;
  };
  invoices: { count: number; openTotal: number; rows: InvoiceRow[] };
  overdueInvoices: { count: number; rows: InvoiceRow[] };
  unlinkedBank: { count: number };
  exceptions: { count: number; rows: ExceptionRow[] };
  vat: {
    periodStart: string;
    periodEnd: string;
    netVatPayable: number;
    daysRemaining: number;
    errors: unknown[];
  };
  backup: {
    backupsFound: boolean;
    latestBackupAt: string | null;
    daysSinceLatestBackup: number | null;
    hasActivitySinceBackup: boolean;
  };
  audit: { ok: boolean; entryCount: number; firstError: unknown };
  recentActivity: AuditLogRow[];
};

export type DashboardResponse = {
  ok: true;
  dashboard: CompanyDashboard;
};

// --- fiscal years (GET /api/companies/:slug/fiscal-years) -----------------

export type FiscalYearEntry = {
  label: string;
  start: string | null;
  end: string | null;
  source: "live" | "archive";
};

export type FiscalYearsResponse = {
  ok: true;
  fiscalYears: { slug: string; years: FiscalYearEntry[] };
};

// --- overview (GET /api/companies/:slug/overview?year=) -------------------

export type OverviewMonth = {
  month: number;
  label: string;
  income: number;
  expense: number;
};

export type OverviewExceptionRow = {
  id: number;
  type: string;
  severity: string;
  message: string;
  /**
   * The concrete "what the owner must do" guidance for this exception — the
   * same `requiredAction` the CLI's `exceptions list` shows. Null when the
   * exception carries no recorded action.
   */
  requiredAction: string | null;
};

/**
 * One grouped exception line for the "Opgaver" card — every open exception of
 * one `type` collapsed into a single Danish, actionable summary line.
 */
export type OverviewExceptionGroup = {
  type: string;
  count: number;
  severity: "low" | "medium" | "high";
  /** Danish one-liner, e.g. "362 banktransaktioner mangler afstemning". */
  label: string;
  /** The cockpit sub-view this group links to (e.g. "bank"); null when none. */
  link: string | null;
};

export type OverviewRecentEntry = {
  id: number;
  entryNo: string;
  date: string;
  text: string;
  amount: number;
};

/**
 * The "Overblik" payload. All money fields are kroner (DKK with decimals) —
 * use `formatKroner`, not `formatCurrency` (which expects minor units).
 */
/** The Overblik VAT block — null for an archived year (no VAT data exists). */
export type OverviewVat = {
  periodStart: string;
  periodEnd: string;
  periodLabel: string;
  /**
   * Genuine output VAT on sales for the period, kroner — gross, before any
   * bad-debt relief. The bad-debt adjustment is surfaced separately so a
   * write-off never drags the salgsmoms headline negative (#271).
   */
  outputVat: number;
  /** Bad-debt (debitortab) output-VAT adjustment, ≤ 0; 0 when none, kroner. */
  outputVatAdjustment: number;
  /** 25% of the standard-rated purchase base for the period, kroner. */
  inputVat: number;
  /** outputVat + outputVatAdjustment − inputVat; positive is payable, kroner. */
  payable: number;
  /** The statutory VAT filing/payment deadline, YYYY-MM-DD. */
  deadline: string;
  /** Signed countdown from today to the deadline; negative once passed. */
  daysRemaining: number;
};

export type CompanyOverview = {
  slug: string;
  selectedYear: string;
  /** True when the figures are derived from the #197 archive, not the ledger. */
  archived: boolean;
  /** The archive's source system (e.g. "dinero") when archived, else null. */
  archivedSource: string | null;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
    fiscalYearStartMonth: number | string;
    fiscalYearLabelStrategy: string;
  };
  fiscalYears: FiscalYearEntry[];
  profitAndLoss: {
    omsaetning: number;
    udgifter: number;
    resultat: number;
    months: OverviewMonth[];
  };
  bank: {
    /** Booked ledger balance of the bank/cash accounts at the year end, kroner. */
    balance: number;
    /** Actual statement balance (latest imported `balance_after`), kroner; null when unknown. */
    actualBalance: number | null;
    /** balance − actualBalance; the unreconciled gap, kroner; null when unknown. */
    difference: number | null;
  };
  /** Money owed TO the company — open issued-invoice balances at year end. */
  receivables: {
    /** Number of issued invoices still carrying an open balance. */
    openCount: number;
    /** Sum of the open balances, kroner. */
    openTotal: number;
  };
  /** The half-yearly VAT position; null for an archived year. */
  vat: OverviewVat | null;
  exceptions: {
    count: number;
    rows: OverviewExceptionRow[];
    groups: OverviewExceptionGroup[];
  };
  recentEntries: OverviewRecentEntry[];
  /** Transaction date of the most recent posted entry; null when none. */
  lastPostedDate: string | null;
  /** Key ratios as fractions (0–1); each null when its denominator is zero. */
  keyFigures: {
    /** Resultat ÷ omsætning. */
    bruttomargin: number | null;
    /** Egenkapital ÷ balancesum. */
    egenkapitalandel: number | null;
  };
};

export type OverviewResponse = {
  ok: true;
  overview: CompanyOverview;
};

// --- financial statements (cockpit-redesign iteration 2) ------------------
//
// All money fields below are kroner (DKK with decimals) — use `formatKroner`.

/** The company identity block shared by every statement payload. */
export type StatementCompany = {
  name: string;
  cvr: string | null;
  country: string;
  currency: string;
  fiscalYearStartMonth: number | string;
  fiscalYearLabelStrategy: string;
};

// --- income statement (GET .../income-statement?year=) --------------------

export type IncomeStatementLine = {
  accountNo: string;
  name: string;
  amount: number;
  /** The same account's amount in the prior calendar year, kroner. */
  priorAmount: number;
};

export type CompanyIncomeStatement = {
  slug: string;
  selectedYear: string;
  /** True when the figures are derived from the #197 archive. */
  archived: boolean;
  /** The archive's source system when archived, else null. */
  archivedSource: string | null;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  income: IncomeStatementLine[];
  expense: IncomeStatementLine[];
  totalIncome: number;
  totalExpense: number;
  priorTotalIncome: number;
  priorTotalExpense: number;
  result: number;
  priorResult: number;
};

export type IncomeStatementResponse = {
  ok: true;
  incomeStatement: CompanyIncomeStatement;
};

// --- balance sheet (GET .../balance?year=) --------------------------------

export type BalanceLine = {
  accountNo: string;
  name: string;
  amount: number;
  /**
   * The same account's balance one fiscal year earlier, kroner. `null` when
   * there is no prior year in the ledger — the view renders «—». ÅRL § 24
   * kræver sammenligningstal på balancen for regnskabsklasse B.
   */
  priorAmount: number | null;
};

export type BalanceSection = {
  lines: BalanceLine[];
  total: number;
  /** The section total one fiscal year earlier, kroner. `null` when none. */
  priorTotal: number | null;
};

export type CompanyBalance = {
  slug: string;
  selectedYear: string;
  /** True when the figures are derived from the #197 archive. */
  archived: boolean;
  /** The archive's source system when archived, else null. */
  archivedSource: string | null;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  asOfDate: string;
  assets: BalanceSection;
  liabilities: BalanceSection;
  /**
   * Equity including the fiscal year's result: the result is folded in as an
   * "Årets resultat" line so `equity.total` is the equity figure an owner
   * reads — and the same number as the Flerårsoversigt's `egenkapital`.
   */
  equity: BalanceSection;
  /** The fiscal year's result, also folded into the equity section. */
  periodResult: number;
  totalAssets: number;
  totalLiabilitiesAndEquity: number;
  /**
   * The prior year's `totalLiabilitiesAndEquity`, kroner. `null` when no
   * prior year is available — the view renders «—» in that cell. Mirrors the
   * resultatopgørelsens prior-year footer.
   */
  priorTotalLiabilitiesAndEquity: number | null;
  balanced: boolean;
};

export type BalanceResponse = {
  ok: true;
  balance: CompanyBalance;
};

// --- trial balance (GET .../trial-balance?year=) --------------------------

export type TrialBalanceRow = {
  accountNo: string;
  name: string;
  type: string;
  debit: number;
  credit: number;
  balance: number;
};

export type CompanyTrialBalance = {
  slug: string;
  selectedYear: string;
  /** True when the rows are the read-only #197 archived SaldoBalance. */
  archived: boolean;
  /** The archive's source system when archived, else null. */
  archivedSource: string | null;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  rows: TrialBalanceRow[];
  totalDebit: number;
  totalCredit: number;
  balanced: boolean;
};

export type TrialBalanceResponse = {
  ok: true;
  trialBalance: CompanyTrialBalance;
};

// --- journal / Posteringer (GET .../journal?year=) ------------------------
//
// All money fields below are kroner (DKK with decimals) — use `formatKroner`.

export type JournalLine = {
  accountNo: string;
  accountName: string;
  debit: number;
  credit: number;
  text: string | null;
};

export type JournalEntry = {
  id: number;
  entryNo: string;
  date: string;
  text: string;
  /** Sum of the debit side — the entry total, kroner. */
  total: number;
  lines: JournalLine[];
  /**
   * #379 — the id of the document (bilag) backing this entry, when one is
   * linked. `null` when the entry has no underlying document (e.g. a manual
   * kassekladde-post). Used by the UI to surface an "Åbn bilag" link.
   */
  documentId: number | null;
  /** The linked document's `document_no` for display next to the link. */
  documentNo: string | null;
};

export type CompanyJournal = {
  slug: string;
  selectedYear: string;
  /** True when the entries are derived from the #197 archived Posteringer. */
  archived: boolean;
  /** The archive's source system when archived, else null. */
  archivedSource: string | null;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  entries: JournalEntry[];
  /** The account the entries are filtered to, when `?account=` is set. */
  accountFilter: { accountNo: string; name: string } | null;
};

export type JournalResponse = {
  ok: true;
  journal: CompanyJournal;
};

// --- bank / Bank (GET .../bank?year=) -------------------------------------

export type BankAccountInfo = {
  id: number;
  name: string;
  bankName: string | null;
  accountNo: string | null;
  ledgerAccountNo: string | null;
};

export type BankTransactionRow = {
  id: number;
  date: string;
  text: string;
  amount: number;
  /** Running balance from the import, kroner; null when the export omits it. */
  runningBalance: number | null;
  reconciliationStatus: "matched" | "unmatched";
  journalEntryNo: string | null;
};

export type CompanyBank = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  accounts: BankAccountInfo[];
  /** Booked ledger balance of the bank/cash accounts at the year end, kroner. */
  bookedBalance: number;
  /** Actual statement balance (latest imported `balance_after`), kroner; null when unknown. */
  actualBalance: number | null;
  /** bookedBalance − actualBalance; the unreconciled gap, kroner; null when unknown. */
  difference: number | null;
  /**
   * Why `actualBalance` is or is not known (#305):
   *  - `known`             — a statement balance was imported and is shown;
   *  - `no-balance-column` — transactions WERE imported, but the CSV carried
   *    no balance column, so the bank saldo is unknown (NOT "not imported");
   *  - `none`              — no bank statement has been imported at all.
   */
  bankStatementStatus: "known" | "no-balance-column" | "none";
  transactions: BankTransactionRow[];
  matchedCount: number;
  unmatchedCount: number;
};

export type BankResponse = {
  ok: true;
  bank: CompanyBank;
};

// --- VAT / Moms (GET .../vat?year=) ---------------------------------------

/**
 * The standard SKAT TastSelv momsangivelse rubrics for a VAT period — the
 * numbers an owner types into the momsangivelse. All amounts are kroner.
 */
export type VatRubrikker = {
  /** Salgsmoms — output VAT on domestic sales (net of bad-debt relief). */
  salgsmoms: number;
  /** Moms af varekøb i udlandet — VAT on goods purchased abroad. */
  momsAfVarekobUdland: number;
  /** Moms af ydelseskøb i udlandet — reverse-charge VAT on foreign services. */
  momsAfYdelseskobUdland: number;
  /** Købsmoms — total deductible input VAT. */
  kobsmoms: number;
  /** Momstilsvar — salgsmoms + udenlandsk moms − købsmoms; positive = owed. */
  momstilsvar: number;
  /** Rubrik A — value of goods/services bought abroad without Danish VAT. */
  rubrikA: number;
  /** Rubrik B — value of goods/services sold abroad without Danish VAT. */
  rubrikB: number;
  /** Rubrik C — value of other VAT-exempt sales. */
  rubrikC: number;
};

export type CompanyVat = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  /**
   * The VAT period label — follows the company's settlement cadence (#299):
   * "Q2 2026" (quarter), "Maj 2026" (month), "1. halvår 2026" (half-year).
   */
  periodLabel: string;
  /**
   * Genuine output VAT on sales (salgsmoms) for the period, kroner — gross,
   * before any bad-debt relief. A bad-debt write-off books a debit on the
   * output-VAT account; surfacing the relief separately keeps this headline
   * from going negative (#271).
   */
  outputVat: number;
  /** Bad-debt (debitortab) output-VAT adjustment, ≤ 0; 0 when none, kroner. */
  outputVatAdjustment: number;
  /** Input VAT (købsmoms) booked for the period, kroner. */
  inputVat: number;
  /** outputVat + outputVatAdjustment − inputVat; positive is payable, kroner. */
  payable: number;
  /** The statutory VAT filing/payment deadline, YYYY-MM-DD. */
  deadline: string;
  /** Signed countdown from today to the deadline; negative once passed. */
  daysRemaining: number;
  /**
   * The VAT period's effective lifecycle state (#303). `open` means the period
   * is NOT yet closed — its figures are provisional and a momsangivelse cannot
   * be filed for it. `closed`/`reported` means the figures are final.
   */
  periodStatus: "open" | "closed" | "reported";
  /**
   * True only when the momsangivelse is filing-ready — i.e. the period is
   * closed or reported. The terminal `vat momsangivelse` refuses an open
   * period, so the cockpit must not present the rubrics as a ready-to-file
   * momsangivelse unless this is true (#303).
   */
  momsangivelseReady: boolean;
  /** The full SKAT TastSelv momsangivelse rubrics for the period. */
  rubrikker: VatRubrikker;
};

export type VatResponse = {
  ok: true;
  vat: CompanyVat;
};

// --- documents / Bilag (GET .../documents) --------------------------------

export type DocumentRow = {
  id: number;
  documentNo: string | null;
  source: string;
  filename: string | null;
  documentType: string;
  supplierName: string | null;
  invoiceNo: string | null;
  invoiceDate: string | null;
  amountIncVat: number | null;
  currency: string;
  status: string;
  voucherRef: string | null;
  journalEntryNo: string | null;
  journalEntryId: number | null;
  /** The linked journal entry's posting text — what the voucher is for. */
  journalEntryText: string | null;
  /** The linked journal entry's total (summed debit side), kroner. */
  journalEntryTotal: number | null;
  /** True when the document has a stored file the cockpit can open. */
  hasFile: boolean;
};

export type CompanyDocuments = {
  slug: string;
  company: StatementCompany;
  documents: DocumentRow[];
  linkedCount: number;
  unlinkedCount: number;
};

export type DocumentsResponse = {
  ok: true;
  documents: CompanyDocuments;
};

// --- archive / Arkiv (GET .../archive/:year) — cockpit-redesign it. 4 -------
//
// All money fields below are kroner (DKK with decimals) — use `formatKroner`.

export type ArchiveBalanceRow = {
  accountNo: string;
  name: string;
  /** Closing balance, kroner, exactly as the Dinero export stored it. */
  amount: number;
};

export type CompanyArchiveYear = {
  slug: string;
  company: StatementCompany;
  /** The archived fiscal-year label, e.g. "2025". */
  year: string;
  /** The accounting system the archive was exported from, e.g. "dinero". */
  sourceSystem: string;
  importedAt: string;
  /** The year's full SaldoBalance — every account's closing balance. */
  saldoBalance: ArchiveBalanceRow[];
  /** A summary of the archived Posteringer for the year. */
  postings: {
    count: number;
    /** Sum of the absolute posting amounts — the gross volume, kroner. */
    grossTotal: number;
  };
};

export type ArchiveResponse = {
  ok: true;
  archive: CompanyArchiveYear;
};

// --- multi-year / Flerårsoversigt (GET .../multi-year) — it. 4 --------------

export type MultiYearRow = {
  /** The fiscal-year label, e.g. "2025". */
  year: string;
  source: "live" | "archive";
  /** Income / omsætning for the year, kroner. */
  omsaetning: number;
  /** Expenses / udgifter for the year, kroner. */
  udgifter: number;
  /** Result (omsætning − udgifter), kroner. */
  resultat: number;
  /** Total assets (balancesum) at the year end, kroner. */
  balancesum: number;
  /** Equity (egenkapital incl. period result) at the year end, kroner. */
  egenkapital: number;
  /** Bruttomargin — resultat ÷ omsætning, a 0–1 fraction; null when no omsætning. */
  bruttomargin: number | null;
  /** Egenkapitalandel — egenkapital ÷ balancesum, a 0–1 fraction; null when balancesum is 0. */
  egenkapitalandel: number | null;
};

export type CompanyMultiYear = {
  slug: string;
  company: StatementCompany;
  /** Key figures per fiscal year, oldest→newest for charting a trend. */
  years: MultiYearRow[];
};

export type MultiYearResponse = {
  ok: true;
  multiYear: CompanyMultiYear;
};

// --- invoices / Fakturaer (GET .../invoices?year=) — cockpit-redesign it. 5 --
//
// All money fields below are kroner (DKK with decimals) — use `formatKroner`.

export type InvoiceStatus =
  | "open"
  | "paid"
  | "credited"
  | "refunded"
  | "overpaid"
  | "written_off"
  | "overdue";

/**
 * Cockpit-facing PEPPOL/e-faktura status (#428) — `null` when the invoice
 * has never been sent as an e-faktura. `prepared` means an envelope has been
 * recorded; `acknowledged` means the access point confirmed receipt.
 */
export type InvoicePeppolStatus = {
  status: "prepared" | "acknowledged";
  submissionReference: string;
  transmissionId: string | null;
  acknowledgedAt: string | null;
};

export type CompanyInvoiceRow = {
  documentId: number;
  invoiceNo: string;
  invoiceDate: string | null;
  customerName: string | null;
  /**
   * Customer's e-mail when set on the kontaktkort (#429). The cockpit row
   * offers "Send på mail" only when this is present so the dialog can
   * prefill the recipient without a second round-trip.
   */
  customerEmail: string | null;
  /**
   * Buyer's EAN-number (13 digits) when set on the invoice payload. The
   * cockpit row offers "Send som e-faktura" only when this is present.
   */
  buyerEanNumber: string | null;
  /** True when the buyer is marked as a public recipient. */
  buyerPublicRecipient: boolean;
  /** Latest PEPPOL submission/transmission, or `null` when never sent. */
  peppolStatus: InvoicePeppolStatus | null;
  /**
   * Timestamp (ISO-8601) of the most recent `email_send_log` row for this
   * invoice (#429), or `null` when the invoice has never been emailed from
   * the cockpit. Surfaced so the row can show "Sendt {dato}" beside the
   * settlement status.
   */
  lastEmailedAt: string | null;
  /**
   * Timestamp (ISO-8601) of the most recently registered payment reminder
   * (#434), or `null` when no reminder has been sent yet. Surfaced so the row
   * can show "{n}. rykker sendt {dato}" under the status flag and the
   * "Send rykker" action knows whether further reminders are still allowed.
   */
  lastReminderAt: string | null;
  /**
   * Count of reminders that have been registered against the invoice (#434).
   * 0 when no reminder has been sent. The cockpit hides the "Send rykker"
   * action once this reaches the statutory cap of 3 (rentel. § 9b).
   */
  lastReminderSequence: number;
  /** Gross amount inc. VAT, kroner. */
  grossAmount: number;
  /** Still-outstanding balance on the invoice, kroner. */
  openBalance: number;
  currency: string;
  status: InvoiceStatus;
  effectiveDueDate: string | null;
  overdueDays: number;
};

export type CompanyInvoices = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  invoices: CompanyInvoiceRow[];
  totalGross: number;
  totalOpen: number;
  overdueCount: number;
};

export type InvoicesResponse = {
  ok: true;
  invoices: CompanyInvoices;
};

// --- contacts / Kontakter (GET .../contacts) — cockpit-redesign it. 5 --------

export type ContactCustomerRow = {
  id: number;
  name: string;
  vatOrCvr: string | null;
  email: string | null;
  paymentTermsDays: number;
  defaultCurrency: string;
  // #390 — full stamdata so the edit-modal can prefill without another fetch.
  address: string | null;
  phone: string | null;
  website: string | null;
  eanNumber: string | null;
  notes: string | null;
  /**
   * #439 — aggregated åbent tilgodehavende på tværs af alle år, kroner.
   * Server-side derivat fra samme ledger-kilde som `/invoices`-endpointet.
   * `0` når kunden ingen åbne fakturaer har.
   */
  openBalance: number;
  /** #439 — antal åbne (endnu ikke fuldt betalte) fakturaer for kunden. */
  openInvoiceCount: number;
  /**
   * #439 — antal af kundens åbne fakturaer der er løbet over forfaldsdato.
   * `> 0` udløser den røde flag-styling i Kontakter-tabellen.
   */
  overdueCount: number;
};

export type ContactVendorRow = {
  id: number;
  name: string;
  vatOrCvr: string | null;
  defaultExpenseAccount: string | null;
  defaultVatTreatment: string | null;
  // #390 — full stamdata so the edit-modal can prefill without another fetch.
  address: string | null;
  email: string | null;
  phone: string | null;
  website: string | null;
  notes: string | null;
};

// --- contact create/update payloads (#390) ----------------------------------

export type CustomerInput = {
  name: string;
  address?: string | null;
  vatOrCvr?: string | null;
  email?: string | null;
  phone?: string | null;
  website?: string | null;
  eanNumber?: string | null;
  paymentTermsDays?: number;
  defaultCurrency?: string;
  notes?: string | null;
};

export type VendorInput = {
  name: string;
  address?: string | null;
  vatOrCvr?: string | null;
  email?: string | null;
  phone?: string | null;
  website?: string | null;
  defaultExpenseAccount?: string | null;
  defaultVatTreatment?: string | null;
  notes?: string | null;
};

/** CVR-lookup result the cockpit modal uses to prefill name + address. */
export type CvrLookupResult = {
  ok: boolean;
  cached: boolean;
  company: {
    cvr: string;
    name: string;
    address?: string | null;
    postalCode?: string | null;
    city?: string | null;
    email?: string | null;
    phone?: string | null;
    website?: string | null;
  } | null;
  errors: string[];
};

export type CompanyContacts = {
  slug: string;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  customers: ContactCustomerRow[];
  vendors: ContactVendorRow[];
};

export type ContactsResponse = {
  ok: true;
  contacts: CompanyContacts;
};

// --- obligations / Forpligtelser (GET .../obligations?year=) — it. 7 --------
//
// All money fields below are kroner (DKK with decimals) — use `formatKroner`.

export type ObligationKind =
  | "vat"
  | "corporation-tax"
  | "annual-report"
  | "creditors"
  | "auditor"
  | "other";

export type ObligationRow = {
  kind: ObligationKind;
  /** A human Danish label, e.g. "Moms — Q2 2026". */
  label: string;
  /** The amount owed, kroner; positive is payable. */
  amount: number;
  /** The filing/payment deadline as YYYY-MM-DD, or null when none is known. */
  dueDate: string | null;
  /** Signed countdown from today to `dueDate`; null when `dueDate` is null. */
  daysRemaining: number | null;
  /** The ledger account the figure was read from, when one applies. */
  accountNo: string | null;
};

export type CompanyObligations = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  /** Payables sorted by due date, soonest first; dateless rows last. */
  obligations: ObligationRow[];
  /** Sum of every obligation's amount, kroner. */
  totalOwed: number;
};

export type ObligationsResponse = {
  ok: true;
  obligations: CompanyObligations;
};

// --- cash flow / Likviditet (GET .../cashflow?year=) ----------------------

export type CashflowMonth = {
  /** 1–12. */
  month: number;
  label: string;
  /** Money in: sum of positive bank-transaction amounts, kroner. */
  indbetalinger: number;
  /** Money out: sum of negative amounts as a positive figure, kroner. */
  udbetalinger: number;
  /** indbetalinger − udbetalinger, kroner. */
  netto: number;
};

export type CashflowBalancePoint = {
  date: string;
  /** The imported running balance at this point, kroner. */
  balance: number;
};

export type CompanyCashflow = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  /** False when the company has no bank transactions in the year. */
  hasTransactions: boolean;
  months: CashflowMonth[];
  /** The real bank-balance trajectory, oldest-first. */
  balanceSeries: CashflowBalancePoint[];
  /** Actual balance the day before the year starts, kroner; null when unknown. */
  openingBalance: number | null;
  /** Actual balance at the year end, kroner; null when unknown. */
  closingBalance: number | null;
  /** Total money in across the year, kroner. */
  totalIn: number;
  /** Total money out across the year, kroner. */
  totalOut: number;
};

export type CashflowResponse = {
  ok: true;
  cashflow: CompanyCashflow;
};

// --- mileage / Kørsel (GET .../mileage?year=) — #335 ----------------------

export type MileageEntryRow = {
  id: number;
  /** Sequence number `MIL-{year}-{6 digits}` assigned by the core. */
  entryNo: string;
  tripDate: string;
  purpose: string;
  fromLocation: string;
  toLocation: string;
  /** Whole kilometres driven on the trip. */
  kilometers: number;
  vehicle: string;
  driver: string;
  /** User-supplied per-kilometre rate (kr). The mileage core never owns a tax rate. */
  ratePerKm: number;
  /** `kilometers * ratePerKm`, rounded to øre. Documentation only — never a posted amount. */
  amountBasis: number;
  /** Free-text, source-backed basis the user confirms (which official rate table). */
  rateBasis: string;
  rateSource: string | null;
  notes: string | null;
  createdAt: string;
};

export type MileageMonthRow = {
  /** 1–12. */
  month: number;
  /** Danish month abbreviation (`jan`, `feb`, …). */
  label: string;
  tripCount: number;
  kilometers: number;
  amountBasis: number;
};

export type CompanyMileage = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  /** Newest trip first. */
  entries: MileageEntryRow[];
  totalKilometers: number;
  totalAmountBasis: number;
  tripCount: number;
  /** Twelve rows, jan…dec; months with no trips appear with zero values. */
  months: MileageMonthRow[];
};

export type MileageResponse = {
  ok: true;
  mileage: CompanyMileage;
};

/** Input for `api.createMileageEntry` — mirrors `CreateMileageEntryInput` in the core. */
export type MileageEntryInput = {
  tripDate: string;
  purpose: string;
  fromLocation: string;
  toLocation: string;
  kilometers: number;
  vehicle: string;
  driver: string;
  ratePerKm: number;
  rateBasis: string;
  rateSource?: string;
  notes?: string;
};

/** The create result the server echoes back. */
export type MileageEntrySummary = {
  mileageEntryId: number | null;
  entryNo: string | null;
  amountBasis: number | null;
};

// --- budget / Budget (GET .../budget?year=, .../budget-vs-actual) — #339 ----
//
// All money fields below are kroner (DKK with decimals) — use `formatKroner`.

/** One effective (latest-revision) budget line for a company. */
export type CompanyBudgetLine = {
  id: number;
  accountNo: string;
  accountName: string | null;
  /** `YYYY-MM` calendar month. */
  period: string;
  amount: number;
  notes: string | null;
  createdAt: string;
};

export type CompanyBudget = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  /** First calendar month covered, `YYYY-MM`. */
  periodStart: string;
  /** Last calendar month covered, `YYYY-MM`. */
  periodEnd: string;
  /** Every calendar month inside the fiscal year, chronological. */
  periods: string[];
  /** Effective budget lines, ordered period→account. */
  lines: CompanyBudgetLine[];
  /** Sum of every line's amount, kroner. */
  totalBudget: number;
};

export type BudgetResponse = {
  ok: true;
  budget: CompanyBudget;
};

/** One row in the budget-vs-faktisk comparison. */
export type CompanyBudgetVsActualLine = {
  accountNo: string;
  accountName: string | null;
  accountType: string | null;
  period: string;
  budget: number;
  actual: number;
  /**
   * Signed variance — see core/budget.ts for the per-account-type sign
   * convention. Positive is "good" (under budget for expense, over target
   * for income), negative is "bad".
   */
  variance: number;
  /** Variance as a fraction of `budget`; null when `budget` is 0. */
  variancePercent: number | null;
};

export type CompanyBudgetVsActual = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  lines: CompanyBudgetVsActualLine[];
  totalBudget: number;
  totalActual: number;
  totalVariance: number;
};

export type BudgetVsActualResponse = {
  ok: true;
  budgetVsActual: CompanyBudgetVsActual;
};

/** Input for `api.setBudget` — append a budget revision for one cell. */
export type SetBudgetInput = {
  accountNo: string;
  period: string;
  amount: number;
  notes?: string;
};


/** The three VAT settlement cadences a Danish company can be registered for. */
export type VatPeriodType = "month" | "quarter" | "half-year";

export type CreateCompanyInput = {
  name: string;
  slug?: string;
  cvr?: string;
  fiscalYearStartMonth?: string;
  fiscalYearLabelStrategy?: string;
  /** The VAT settlement cadence (#300). Defaults to `quarter` server-side. */
  vatPeriodType?: VatPeriodType;
  /** Optional bank/payment details — sets up the primary bank account (#284). */
  payment?: CompanyPaymentInput;
};

export type UpdateCompanyInput = {
  name?: string;
  archived?: boolean;
};

/** Optional bank fields on the create-company form (#284). */
export type CompanyPaymentInput = {
  bankName?: string;
  registrationNo?: string;
  accountNo?: string;
  iban?: string;
};

/**
 * The editable company profile fields (#284) — what `PATCH .../company` accepts.
 * Only the fields present are changed; the rest keep their current value.
 */
export type CompanyProfileInput = {
  name?: string;
  cvr?: string;
  address?: string;
  postalCode?: string;
  city?: string;
  paymentTermsDays?: number;
  /** The VAT settlement cadence (#300) — month / quarter / half-year. */
  vatPeriodType?: VatPeriodType;
  payment?: CompanyPaymentInput;
};

// --- company settings + CVR sync (GET .../company, POST .../sync-cvr) -------

/**
 * The company's payment/bank details — the primary bank account every issued
 * invoice's payment block reads from. Null on `CompanySettings` when no bank
 * account is configured yet.
 */
export type CompanyPaymentDetails = {
  bankName: string | null;
  registrationNo: string | null;
  accountNo: string | null;
  iban: string | null;
};

/** The full companies row, including CVR-register stamdata + bank details. */
export type CompanySettings = {
  id: number;
  name: string;
  country: string;
  currency: string;
  cvr: string | null;
  fiscalYearStartMonth: number;
  fiscalYearLabelStrategy: string;
  address: string | null;
  postalCode: string | null;
  city: string | null;
  companyForm: string | null;
  industryCode: string | null;
  industryText: string | null;
  cvrStatus: string | null;
  auditWaived: boolean | null;
  /** ISO timestamp the CVR stamdata was last synced; null when never. */
  cvrSyncedAt: string | null;
  /** The VAT settlement cadence the company is registered for with SKAT (#300). */
  vatPeriodType: VatPeriodType;
  /** The company's own bank/payment details; null when none is configured. */
  payment: CompanyPaymentDetails | null;
};

export type CompanySettingsResponse = {
  ok: true;
  company: CompanySettings;
};

/** The result of `POST /api/companies/:slug/periods/close` (#287). */
export type ClosePeriodResult = {
  id: number | null;
  periodStart: string | null;
  periodEnd: string | null;
  kind: string | null;
  status: string | null;
  reference: string | null;
};

export type ClosePeriodResponse = {
  ok: true;
  period: ClosePeriodResult;
};

/** Input for `api.closePeriod`. */
export type ClosePeriodInput = {
  periodStart: string;
  periodEnd: string;
  kind?: "vat_quarter" | "fiscal_year" | "custom";
  reference?: string;
};

/** The result of `POST /api/companies/:slug/periods/reopen` (#301). */
export type ReopenPeriodResult = {
  id: number | null;
  periodStart: string | null;
  periodEnd: string | null;
  kind: string | null;
  /** The period's effective state after the reopen — `open` on success. */
  effectiveStatus: "open" | "closed" | "reported" | null;
  reopenedBy: string | null;
  reason: string | null;
};

export type ReopenPeriodResponse = {
  ok: true;
  period: ReopenPeriodResult;
};

/** Input for `api.reopenPeriod` (#301). `reason` is recorded in the audit log. */
export type ReopenPeriodInput = {
  periodStart: string;
  periodEnd: string;
  kind?: "vat_quarter" | "fiscal_year" | "custom";
  reason: string;
};

export type CvrManagementMember = { name: string; role: string };

/** A normalised CVR-register snapshot for one company. */
export type CvrCompanyInfo = {
  cvr: string;
  name: string;
  address: string | null;
  postalCode: string | null;
  city: string | null;
  municipalityCode: number | null;
  companyFormCode: number | null;
  companyFormShort: string | null;
  companyFormLong: string | null;
  status: string | null;
  industryCode: string | null;
  industryText: string | null;
  email: string | null;
  phone: string | null;
  website: string | null;
  startDate: string | null;
  fiscalYearStart: string | null;
  fiscalYearEnd: string | null;
  fiscalYearStartMonth: number | null;
  auditWaived: boolean | null;
  shareCapital: number | null;
  shareCapitalCurrency: string | null;
  employees: number | null;
  advertisingProtected: boolean;
  management: CvrManagementMember[];
};

/** The result of `POST /api/companies/:slug/sync-cvr`. */
export type SyncCvrResult = {
  ok: boolean;
  cvr?: string;
  company?: CvrCompanyInfo;
  cached?: boolean;
  /** Names of the company fields whose value changed. */
  updatedFields?: string[];
  /** Configured vs. CVR-registered fiscal-year start month. */
  fiscalYearStartMonth?: { current: number; cvr: number | null; matches: boolean };
  errors: string[];
};

export type SyncCvrResponse = {
  ok: true;
  sync: SyncCvrResult;
};

/** One previously generated invoice for a recurring-invoice template. */
export type RecurringInvoiceGenerationRow = {
  id: number;
  periodIndex: number;
  invoiceNumber: string;
  issueDate: string;
  documentId: number;
  deliveryPeriodStart: string | null;
  deliveryPeriodEnd: string | null;
};

/** One recurring-invoice template plus the invoices it has already issued. */
export type RecurringInvoiceTemplateRow = {
  id: number;
  name: string;
  interval: "monthly" | "quarterly" | "yearly";
  firstIssueDate: string;
  nextIssueDate: string;
  paymentTermsDays: number;
  deliveryPeriodMode: "issue_month" | "interval_window" | "none";
  notes: string | null;
  active: boolean;
  createdAt: string;
  generations: RecurringInvoiceGenerationRow[];
};

/** Public alias used by the create modal (#386). */
export type RecurringInterval = "monthly" | "quarterly" | "yearly";
export type DeliveryPeriodMode = "issue_month" | "interval_window" | "none";

/**
 * Minimal create-template payload the cockpit POSTs (#386). The server
 * computes line totals + net/moms/brutto via `computeInvoiceAmounts` and runs
 * the same `createRecurringInvoiceTemplate` core function the CLI calls —
 * the cockpit never hand-builds an `InvoicePayload`.
 */
export type RecurringInvoiceTemplateInput = {
  name: string;
  interval: RecurringInterval;
  firstIssueDate: string;
  paymentTermsDays: number;
  deliveryPeriodMode?: DeliveryPeriodMode;
  notes?: string;
  vatRatePercent: number;
  currency?: string;
  /** When set, server back-fills the buyer from stored customer master-data. */
  customerId?: number;
  buyer?: { name?: string; address?: string; vatOrCvr?: string };
  lines: Array<{
    description: string;
    quantity: number;
    unitPriceExVat: number;
  }>;
};

/** Server's echo of a successful create. */
export type RecurringInvoiceTemplateCreatedResult = {
  templateId: number;
  name: string;
  interval: RecurringInterval;
  firstIssueDate: string;
};

export type CompanyRecurringInvoices = {
  slug: string;
  templates: RecurringInvoiceTemplateRow[];
};

export type RecurringInvoicesResponse = {
  ok: true;
  recurringInvoices: CompanyRecurringInvoices;
};

// --- payables / Leverandørfaktura (GET .../payables) — #340 ----------------
//
// The cockpit's Leverandørfaktura-arbejdsbord (#340). All money fields in
// kroner; use `formatKroner`.

export type PayableStatusWire = "open" | "paid";
export type PayableAgingBucketWire =
  | "not-due"
  | "0-30"
  | "31-60"
  | "61-90"
  | "90+";
export type PayableListStatusFilter = "open" | "paid" | "overdue" | "all";

export type CompanyPayableRow = {
  payableId: number;
  documentId: number;
  billNo: string | null;
  billDate: string;
  dueDate: string;
  supplierName: string | null;
  vendorId: number | null;
  grossAmount: number;
  currency: string;
  paidAmount: number;
  openBalance: number;
  status: PayableStatusWire;
  isOverdue: boolean;
  overdueDays: number;
  agingBucket: PayableAgingBucketWire;
};

export type UnregisteredPurchaseDocumentRow = {
  id: number;
  documentNo: string | null;
  invoiceNo: string | null;
  invoiceDate: string | null;
  supplierName: string | null;
  amountIncVat: number | null;
  vatAmount: number | null;
  currency: string;
};

export type PayableExpenseAccountOption = {
  accountNo: string;
  name: string;
  defaultVatCode: string | null;
};

export type PayableVendorOption = {
  id: number;
  name: string;
  defaultExpenseAccount: string | null;
  defaultVatTreatment: string | null;
};

export type CompanyPayables = {
  slug: string;
  asOfDate: string;
  status: PayableListStatusFilter;
  company: StatementCompany;
  fiscalYears: FiscalYearEntry[];
  rows: CompanyPayableRow[];
  count: number;
  totalOpenBalance: number;
  overdueOpenBalance: number;
  notYetDueOpenBalance: number;
  unregisteredDocuments: UnregisteredPurchaseDocumentRow[];
  expenseAccounts: PayableExpenseAccountOption[];
  vendors: PayableVendorOption[];
};

export type PayablesResponse = {
  ok: true;
  payables: CompanyPayables;
};

/** Input for `api.registerPayable` (#340). */
export type PayableRegisterInput = {
  documentId: number;
  billDate: string;
  dueDate: string;
  expenseAccountNo: string;
  vatTreatment?: "standard" | "exempt";
  vendorId?: number;
  note?: string;
};

/** The register result the server echoes back (#340). */
export type PayableRegisterSummary = {
  payableId: number | null;
  documentId: number | null;
  supplierName: string | null;
  billNo: string | null;
  grossAmount: number;
  netAmount: number;
  vatAmount: number;
  dueDate: string | null;
  entryId: number | null;
  entryNo: string | null;
};

/** Input for `api.payPayable` (#340). */
export type PayablePayInput = {
  payableId: number;
  bankTransactionId: number;
  paymentDate?: string;
  amount?: number;
  paymentAccountNo?: string;
  note?: string;
};

/** The pay result the server echoes back (#340). */
export type PayablePaySummary = {
  paymentId: number | null;
  journalEntryId: number | null;
  payableId: number;
  openBalance: number | null;
};

/** The generate-from-template result the server echoes back. */
export type RecurringInvoiceGenerationResult = {
  /** True for a freshly-issued invoice, false for an idempotent re-run. */
  created: boolean;
  templateId: number | null;
  periodIndex: number | null;
  documentId: number | null;
  invoiceNumber: string | null;
  issueDate: string | null;
  dueDate: string | null;
  deliveryPeriodStart: string | null;
  deliveryPeriodEnd: string | null;
};

// --- Anlæg (fixed assets) — #336 -----------------------------------------

/** One row in the Anlæg list — a capitalised asset and its derived status. */
export type AssetRow = {
  assetId: number;
  name: string;
  category: string;
  acquisitionDate: string;
  cost: number;
  usefulLifeMonths: number;
  postedPeriods: number;
  accumulatedDepreciation: number;
  netBookValue: number;
  status: "active" | "fully-depreciated";
  remainingPeriods: number;
};

/** One row in the straksafskrivning history list. */
export type AssetWriteOffRow = {
  id: number;
  name: string;
  category: string;
  acquisitionDate: string;
  writeOffDate: string;
  cost: number;
  expenseAccountNo: string;
  thresholdDkk: number;
  thresholdRuleSource: string;
  note: string | null;
  purchaseDocumentId: number;
  journalEntryId: number;
};

/** Backs `GET /api/companies/:slug/assets` — the Anlæg page payload. */
export type CompanyAssets = {
  slug: string;
  company: StatementCompany;
  assets: AssetRow[];
  writeOffs: AssetWriteOffRow[];
  totals: {
    cost: number;
    accumulatedDepreciation: number;
    netBookValue: number;
    activeCount: number;
    fullyDepreciatedCount: number;
    writeOffCount: number;
    writeOffTotal: number;
  };
};

export type AssetsResponse = {
  ok: true;
  assets: CompanyAssets;
};

/** The next-depreciation-period preview for a single asset. */
export type AssetNextDepreciation = {
  assetId: number;
  totalPeriods: number;
  postedPeriods: number;
  remainingPeriods: number;
  nextPeriodIndex: number | null;
  nextPeriodAmount: number | null;
};

export type AssetNextDepreciationResponse = {
  ok: true;
  nextDepreciation: AssetNextDepreciation;
};

/** Input for `api.registerAsset`. */
export type AssetRegisterInput = {
  name: string;
  category: string;
  acquisitionDate: string;
  cost: number;
  usefulLifeMonths: number;
  purchaseDocumentId: number;
  assetAccountNo?: string;
  depreciationExpenseAccountNo?: string;
  accumulatedDepreciationAccountNo?: string;
  note?: string;
};

export type AssetRegisterSummary = {
  assetId: number | null;
  totalPeriods: number | null;
  periodAmount: number | null;
};

/** Input for `api.depreciateAsset`. `periodIndex` is derived server-side. */
export type AssetDepreciateInput = {
  transactionDate?: string;
  periodIndex?: number;
};

export type AssetDepreciateSummary = {
  entryId: number | null;
  assetId: number | null;
  periodIndex: number | null;
  periodAmount: number | null;
};

/** Input for `api.writeOffAsset` — books a straksafskrivning. */
export type AssetWriteOffInput = {
  name: string;
  category: string;
  acquisitionDate: string;
  transactionDate: string;
  cost: number;
  purchaseDocumentId: number;
  expenseAccountNo: string;
  thresholdRuleSource: string;
  paymentAccountNo?: string;
  note?: string;
};

export type AssetWriteOffSummary = {
  writeOffId: number | null;
  entryId: number | null;
  cost: number | null;
  thresholdDkk: number | null;
};

// ---------------------------------------------------------------------------
// Agent-forslag → menneskelig godkendelse (#346)
// ---------------------------------------------------------------------------

/**
 * One agent suggestion waiting on the owner's approve/reject decision. Mirrors
 * `AgentSuggestionRow` in `src/server/data/agent-suggestions.ts` — the cockpit
 * never re-derives the rule id, severity or kind label.
 */
export type AgentSuggestionRow = {
  exceptionId: number;
  type: string;
  kindLabel: string;
  severity: "low" | "medium" | "high";
  rationale: string;
  requiredAction: string | null;
  ruleId: string | null;
  sourceEvidence: unknown;
  postingPreview: unknown;
  agentActor: string | null;
  agentProgram: string | null;
  createdAt: string;
  relatedDocumentId: number | null;
  relatedBankTransactionId: number | null;
  /** Cockpit deep-link target ("anlaeg", "leverandoerfaktura", …); may be null. */
  link: string | null;
};

export type CompanyAgentSuggestions = {
  slug: string;
  company: StatementCompany;
  rows: AgentSuggestionRow[];
  count: number;
  bySeverity: {
    high: number;
    medium: number;
    low: number;
  };
};

export type AgentSuggestionsResponse = {
  ok: true;
  agentSuggestions: CompanyAgentSuggestions;
};

/** Result of an approve/reject decision — the resolved-id pair the cockpit echoes. */
export type AgentSuggestionDecisionResult = {
  id: number;
  decision: "approved" | "rejected";
  resolved: boolean;
};

export type AgentSuggestionDecisionResponse = {
  ok: true;
  suggestion: AgentSuggestionDecisionResult;
};

// ---------------------------------------------------------------------------
// #347 — Lovgrundlag-viewer (read-only).
// ---------------------------------------------------------------------------

export type RuleBundleSummary = {
  name: string;
  version: string;
  ruleCount: number;
  sources: string[];
  vatCodes: string[];
};

export type RuleProvisionCitation = {
  ref: string;
  textHash: string;
};

export type RuleSummary = {
  ruleId: string;
  bundle: string;
  sourceId: string;
  name: string;
  explanation: string;
  severity: string;
  category: string;
  provisions: RuleProvisionCitation[];
};

export type LegalSource = {
  id: string;
  title: string;
  authority: string;
  category: string;
  url: string;
  xmlUrl?: string;
  notes?: string;
};

export type RulesResponse = {
  ok: true;
  ruleBundles: RuleBundleSummary[];
  rules: RuleSummary[];
  legalSources: LegalSource[];
};

// ---------------------------------------------------------------------------
// #343 — Retention status view.
// ---------------------------------------------------------------------------

export type RetentionStatusRow = {
  table: "documents" | "journal_entries" | "bank_transactions";
  total: number;
  expired: number;
  nextExpiry: string | null;
  oldestExpired: string | null;
};

export type CompanyRetention = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  report: {
    ok: boolean;
    asOf: string;
    appliedRules: string[];
    rows: RetentionStatusRow[];
    errors: string[];
  };
  legalCitation: {
    sourceId: string;
    note: string;
  };
};

export type RetentionResponse = {
  ok: true;
  retention: CompanyRetention;
};

// ---------------------------------------------------------------------------
// #333 — Integritet & backup-panel.
// ---------------------------------------------------------------------------

export type AuditChainStatus = {
  ok: boolean;
  entries: number;
  errors: string[];
};

export type BackupStatusSummary = {
  ok: boolean;
  latestBackupAt: string | null;
  latestBackupId: string | null;
  backupDue: boolean;
  hasActivitySinceBackup: boolean;
  daysSinceLatestBackup: number | null;
  backupsFound: number;
  requiredBy: string | null;
  checkedAt: string;
};

export type BackupDestinationSummary = {
  id: string;
  label: string;
  kind: string;
  location: string;
  inEeaOrEu: boolean;
  country: string | null;
  meetsRecognisedStandards: boolean | null;
  nonRelatedParty: boolean;
  lastPlacementAt: string | null;
};

export type CompanyIntegrity = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  auditChain: AuditChainStatus;
  backup: BackupStatusSummary;
  destinations: BackupDestinationSummary[];
  legalCitation: { sourceId: string; note: string };
};

export type IntegrityResponse = {
  ok: true;
  integrity: CompanyIntegrity;
};

// ---------------------------------------------------------------------------
// #344 — Kontoplan-view (read-only).
// ---------------------------------------------------------------------------

export type AccountRow = {
  accountNo: string;
  name: string;
  type: string;
  normalBalance: string;
  defaultVatCode: string | null;
  hasPostings: boolean;
};

export type CompanyAccounts = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  accounts: AccountRow[];
  byType: Record<string, number>;
};

export type AccountsResponse = {
  ok: true;
  accounts: CompanyAccounts;
};

// ---------------------------------------------------------------------------
// #332 — Exceptions queue (read-only liste).
// ---------------------------------------------------------------------------

export type ExceptionRow = {
  id: number;
  type: string;
  severity: "low" | "medium" | "high";
  status: "open" | "resolved";
  relatedBankTransactionId: number | null;
  relatedDocumentId: number | null;
  message: string;
  requiredAction: string | null;
  sourceEvidence: unknown;
  postingPreview: unknown;
  createdAt: string;
  resolvedAt: string | null;
  resolvedBy: string | null;
  resolutionNote: string | null;
  archived: boolean;
};

export type CompanyExceptions = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  status: "open" | "resolved" | "all";
  rows: ExceptionRow[];
  bySeverity: { high: number; medium: number; low: number };
  count: number;
};

export type ExceptionsResponse = {
  ok: true;
  exceptions: CompanyExceptions;
};

// ---------------------------------------------------------------------------
// #342 — Periodelås.
// ---------------------------------------------------------------------------

export type AccountingPeriodKind = "vat_quarter" | "fiscal_year" | "custom";
export type AccountingPeriodStatus = "open" | "closed" | "reported";

export type AccountingPeriodRow = {
  id: number;
  periodStart: string;
  periodEnd: string;
  kind: AccountingPeriodKind;
  rowStatus: AccountingPeriodStatus;
  effectiveStatus: AccountingPeriodStatus;
  closedAt: string | null;
  closedBy: string | null;
  reference: string | null;
};

export type CompanyPeriods = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  periods: AccountingPeriodRow[];
  byStatus: { open: number; closed: number; reported: number };
};

export type PeriodsResponse = {
  ok: true;
  periods: CompanyPeriods;
};

// ---------------------------------------------------------------------------
// #345 — Bankkonti + CSV-mapping-profiler.
// ---------------------------------------------------------------------------

export type BankAccount = {
  id: number;
  slug: string;
  name: string;
  bankName: string | null;
  registrationNo: string | null;
  accountNo: string | null;
  iban: string | null;
  currency: string;
  ledgerAccountNo: string | null;
  active: boolean;
  createdAt: string;
};

export type BankImportProfile = {
  name: string;
  bankName?: string;
  separator?: string;
  encoding?: string;
  dateOrder?: "dmy" | "mdy" | "ymd" | "iso";
  columns?: Record<string, string>;
};

export type CompanyBankAccounts = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  accounts: BankAccount[];
  profiles: BankImportProfile[];
};

export type BankAccountsResponse = {
  ok: true;
  bankAccounts: CompanyBankAccounts;
};

// ---------------------------------------------------------------------------
// #334 — GDPR-export og forget UI.
// ---------------------------------------------------------------------------

export type GdprPersonalData = {
  name: string | null;
  address: string | null;
  email: string | null;
  vatOrCvr: string | null;
};

export type GdprExportRecord = {
  source: "customers" | "vendors" | "documents" | "bank_transactions";
  sourceRowId: number;
  label: string | null;
  personalData: GdprPersonalData;
  retainUntil: string | null;
  underRetention: boolean;
  erased: boolean;
};

export type GdprSubjectExport = {
  ok: boolean;
  asOf: string;
  appliedRules: string[];
  subject: { cvr: string | null; name: string | null };
  records: GdprExportRecord[];
  errors: string[];
};

export type CompanyGdpr = {
  slug: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  export: GdprSubjectExport;
};

export type GdprResponse = {
  ok: true;
  gdpr: CompanyGdpr;
};

// ---------------------------------------------------------------------------
// #337 — Periodisering / accrual register.
// ---------------------------------------------------------------------------

export type AccrualRegisterRow = {
  accrualId: number;
  accrualType: "prepaid_expense" | "accrued_expense" | "deferred_revenue";
  description: string;
  totalAmount: number;
  recognitionPeriods: number;
  recognizedPeriods: number;
  recognizedAmount: number;
  remainingAmount: number;
  fullyRecognized: boolean;
  balanceAccountNo: string;
  resultAccountNo: string;
  firstRecognitionDate: string;
  periodStepMonths: number;
};

export type AccrualRegisterReport = {
  ok: boolean;
  accruals: AccrualRegisterRow[];
  totals: { totalAmount: number; recognizedAmount: number; remainingAmount: number };
  errors: string[];
};

export type CompanyAccrualsResponse = {
  ok: true;
  accruals: {
    slug: string;
    company: { name: string; cvr: string | null; country: string; currency: string };
    report: AccrualRegisterReport;
  };
};

export type GdprErasureResult = {
  ok: boolean;
  asOf: string;
  subject: { cvr: string | null; name: string | null };
  erasedCount: number;
  refusedCount: number;
  alreadyErasedCount: number;
  erased: Array<{
    source: string;
    sourceRowId: number;
    label: string | null;
    redactedFields: string[];
  }>;
  refused: Array<{
    source: string;
    sourceRowId: number;
    label: string | null;
    retainUntil: string;
    reason: string;
  }>;
  errors: string[];
};
