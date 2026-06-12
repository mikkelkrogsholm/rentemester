// Period close/reopen + periodelås (#342) wire types.

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
