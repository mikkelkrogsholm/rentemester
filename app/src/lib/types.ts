// Wire types — the JSON shapes returned by `rentemester serve` (#170).
//
// These mirror `src/server/data.ts` and `src/server/router.ts`. They are kept
// deliberately as a hand-written copy: the SPA is a separate package and does
// not import from the backend's TypeScript sources.

export type ApiErrorBody = {
  ok: false;
  error: { code: string; message: string };
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
   * Buyer's EAN-number (13 digits) when set on the invoice payload. The
   * cockpit row offers "Send som e-faktura" only when this is present.
   */
  buyerEanNumber: string | null;
  /** True when the buyer is marked as a public recipient. */
  buyerPublicRecipient: boolean;
  /** Latest PEPPOL submission/transmission, or `null` when never sent. */
  peppolStatus: InvoicePeppolStatus | null;
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

export type CompanyRecurringInvoices = {
  slug: string;
  templates: RecurringInvoiceTemplateRow[];
};

export type RecurringInvoicesResponse = {
  ok: true;
  recurringInvoices: CompanyRecurringInvoices;
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
