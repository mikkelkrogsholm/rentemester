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

/** The company identity block shared by every statement payload. */
export type StatementCompany = {
  name: string;
  cvr: string | null;
  country: string;
  currency: string;
  fiscalYearStartMonth: number | string;
  fiscalYearLabelStrategy: string;
};

/** The three VAT settlement cadences a Danish company can be registered for. */
export type VatPeriodType = "month" | "quarter" | "half-year";
