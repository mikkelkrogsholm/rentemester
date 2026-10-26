// Common wire-shape building blocks — types that show up across multiple
// domains (the cockpit error envelope, fiscal-year metadata, the company
// identity block embedded in every statement payload).

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

export type BuildIdentity = {
  version: string;
  gitCommit: string | null;
  builtAt: string | null;
  bunVersion: string | null;
  baseImageDigest: string | null;
};

export type HealthResponse = {
  backend?: "rust";
  setupRequired?: boolean;
  ok: true;
  service: string;
  build: BuildIdentity;
  provenance: {
    product: BuildIdentity;
    schema: { version: number; baselineChecksum: string };
    rules: { digest: string };
  };
  workspace: string;
  authRequired: boolean;
  deploymentProfile?: "local" | "local-container" | "hosted";
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

// --- fiscal years (GET /api/companies/:slug/fiscal-years) -----------------

export type FiscalYearEntry = {
  label: string;
  start: string | null;
  end: string | null;
  source: "live" | "archive";
  /** False when only the dedicated archive source view is supported. */
  selectable?: boolean;
};

export type FiscalYearsResponse = {
  ok: true;
  fiscalYears: { slug: string; years: FiscalYearEntry[] };
};

/** The company identity block shared by every statement payload. */
export type StatementCompany = {
  name: string;
  cvr: string | null;
  country: string;
  currency: string;
  fiscalYearStartMonth: number | string;
  fiscalYearLabelStrategy: string;
};

export type DataCoverage = {
  kind: "current" | "historical" | "scenario" | "incomplete" | "final";
  label: "Aktuel bogføring" | "Historisk kilde" | "Scenarie" | "Ufuldstændigt grundlag" | "Endelig/låst";
  asOfDate: string | null;
  comparison: "available" | "not_comparable";
  provenance: "native" | "imported" | "archived" | "scenario";
  details: string[];
};

/** The three VAT settlement cadences a Danish company can be registered for. */
export type VatPeriodType = "month" | "quarter" | "half-year";

export type ReportMoney = number | string | null;
export type ReportSource = "live" | "archive";
export type ArchiveAccountEvidence = { accountNo: string; name: string; type: string | null; normalBalance: string | null; amount: ReportMoney; sourceRows: Array<{ lineNo: number; sourceAmount: string; amount: ReportMoney }> };
export type ReportSourceMetadata = {
  openingBalanceBasis?: { basis: "live"; cutOverDate: string | null; openingJournalEntryId: number | null; archiveAmountsIncluded: false };
  liveCoverage?: { status: "partial" | "complete"; reason: "native_history_begins_at_explicit_cutover" | null; periodStart: string; periodEnd: string; cutOverDate: string | null };
  ledgerBasis?: "native_ledger_only";
  fiscalYears?: FiscalYearEntry[];
  source?: ReportSource; selectedSource?: ReportSource; declaredCurrency?: string;
  periodStart?: string | null; periodEnd?: string | null;
  evidence?: { importId: number; sourceHash: string; parserContract: string; declaredCurrency: string; sourceSystem: string; classificationSource: string; periodBasis: string; observedPostingStart: string | null; observedPostingEnd: string | null };
  completeness?: { status: "ready" | "incomplete" | "unavailable"; reasons: string[]; hasBalanceFile: boolean; closingBalanceCount: number; unclassifiedAccounts: ArchiveAccountEvidence[] };
  comparison?: { status: "available" | "unavailable"; reason: string | null };
};
