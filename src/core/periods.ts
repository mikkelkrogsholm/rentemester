import type { Database } from "bun:sqlite";
import { insertAuditLog, resolveActor } from "./actor";
import { isValidIsoDate as looksLikeIsoDate, addDays, todayIsoDate, MONTH_NAMES_DA } from "./dates";

export type AccountingPeriodKind = "vat_quarter" | "fiscal_year" | "custom";
export type AccountingPeriodStatus = "open" | "closed" | "reported";

/**
 * #289: the VAT settlement cadence a company is registered for with SKAT.
 * Danish businesses file monthly, quarterly or half-yearly VAT depending on
 * turnover. Rentemester historically hardcoded `quarter`; this type makes the
 * cadence an explicit, per-company setting so VAT periods and their deadlines
 * follow the company's real registration. `quarter` stays the default so
 * companies created before the setting existed are unaffected.
 */
export type VatPeriodType = "month" | "quarter" | "half-year";

/** The default VAT cadence — Rentemester's historical assumption. */
export const DEFAULT_VAT_PERIOD_TYPE: VatPeriodType = "quarter";

const VAT_PERIOD_TYPES = new Set<VatPeriodType>(["month", "quarter", "half-year"]);

/**
 * Validate a VAT-period-type input. Accepts exactly `month`, `quarter` or
 * `half-year`; returns null on anything else so callers can surface a clear
 * error. Note this is the canonical machine value — distinct from the Danish
 * display label ("måned" / "kvartal" / "halvår").
 */
export function normalizeVatPeriodType(value?: string | null): VatPeriodType | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return VAT_PERIOD_TYPES.has(trimmed as VatPeriodType) ? (trimmed as VatPeriodType) : null;
}

/** Danish display label for a VAT period type — used in onboarding/help output. */
export function vatPeriodTypeLabelDa(type: VatPeriodType): string {
  switch (type) {
    case "month":
      return "måned";
    case "half-year":
      return "halvår";
    case "quarter":
    default:
      return "kvartal";
  }
}

/** Number of calendar months a single VAT period spans for each cadence. */
function vatPeriodMonthSpan(type: VatPeriodType): number {
  switch (type) {
    case "month":
      return 1;
    case "half-year":
      return 6;
    case "quarter":
    default:
      return 3;
  }
}

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

export type VatPeriodWindow = {
  /** First day of the VAT period (YYYY-MM-DD). */
  start: string;
  /** Last day of the VAT period (YYYY-MM-DD). */
  end: string;
  /** SKAT filing/payment deadline: 1st of the 3rd month after period end. */
  filingDeadline: string;
  /** The cadence this window was computed for. */
  vatPeriodType: VatPeriodType;
};

/**
 * #289: the VAT period window that contains `isoDate`, for a company on the
 * given VAT cadence. A monthly company gets a one-month window, a quarterly
 * company a three-month window, a half-yearly company a six-month window — so
 * the period (and therefore every VAT deadline derived from it) follows the
 * company's real SKAT registration instead of a hardcoded quarter.
 *
 * The filing deadline is the 1st of the third month after the period ends,
 * consistent across cadences with `core/vat.ts#vatFilingDeadline`.
 */
export function vatPeriodWindowFor(isoDate: string, type: VatPeriodType): VatPeriodWindow {
  const year = Number(isoDate.slice(0, 4));
  const month = Number(isoDate.slice(5, 7)); // 1-based
  const span = vatPeriodMonthSpan(type);
  // The 1-based index of the period within the year, then its first month.
  const periodIndex = Math.floor((month - 1) / span);
  const startMonth = periodIndex * span + 1;
  const endMonth = startMonth + span - 1;
  // Day 0 of the next month is the last day of `endMonth`.
  const lastDay = new Date(Date.UTC(year, endMonth, 0)).getUTCDate();
  // Filing deadline: 1st of the third month after the period-end month.
  let deadlineMonth = endMonth + 3;
  let deadlineYear = year;
  while (deadlineMonth > 12) {
    deadlineMonth -= 12;
    deadlineYear += 1;
  }
  return {
    start: `${year}-${pad2(startMonth)}-01`,
    end: `${year}-${pad2(endMonth)}-${pad2(lastDay)}`,
    filingDeadline: `${deadlineYear}-${pad2(deadlineMonth)}-01`,
    vatPeriodType: type,
  };
}

/**
 * #299: a short Danish display label for the VAT period a window describes.
 * Quarterly periods read "Q1 2026"; monthly periods read "Maj 2026"; half-year
 * periods read "1. halvår 2026" / "2. halvår 2026". A quarterly company keeps
 * the exact "Q<n> <year>" string the cockpit and dashboard have always shown,
 * so back-compat for the default cadence is byte-identical.
 */
export function vatPeriodLabel(window: VatPeriodWindow): string {
  const year = Number(window.start.slice(0, 4));
  const startMonth = Number(window.start.slice(5, 7)); // 1-based
  switch (window.vatPeriodType) {
    case "month":
      return `${MONTH_NAMES_DA[startMonth - 1]} ${year}`;
    case "half-year": {
      const half = startMonth <= 6 ? 1 : 2;
      return `${half}. halvår ${year}`;
    }
    case "quarter":
    default: {
      const quarter = Math.floor((startMonth - 1) / 3) + 1;
      return `Q${quarter} ${year}`;
    }
  }
}

/**
 * #299: every VAT period window that starts inside calendar `year`, for a
 * company on the given cadence — 12 for a monthly company, 4 for a quarterly
 * company, 2 for a half-yearly company. Returned in chronological order.
 *
 * This is the single source of truth for "which VAT periods does a company
 * have in a year" — the cockpit's per-period selection, the obligations list
 * and the dashboard all iterate it instead of hardcoding Q1..Q4.
 */
export function vatPeriodsForYear(year: number, type: VatPeriodType): VatPeriodWindow[] {
  const span = vatPeriodMonthSpan(type);
  const windows: VatPeriodWindow[] = [];
  for (let startMonth = 1; startMonth <= 12; startMonth += span) {
    windows.push(vatPeriodWindowFor(`${year}-${pad2(startMonth)}-01`, type));
  }
  return windows;
}

/**
 * #300: writes the company's VAT settlement cadence (`vat_period_type`) onto
 * the single `companies` row. The cadence is set at `init`/`company add`; this
 * is the supported path to change it afterwards — used by `company set-profile`
 * and the cockpit's PATCH-profile endpoint.
 *
 * The column is ensured (older ledgers and the base schema may lack it) before
 * the write, and a CHECK constraint guards the value, so an invalid cadence is
 * rejected here too. Returns whether the value actually changed.
 */
export function setCompanyVatPeriodType(
  db: Database,
  type: VatPeriodType,
): { ok: boolean; changed: boolean; errors: string[] } {
  if (!VAT_PERIOD_TYPES.has(type)) {
    return {
      ok: false,
      changed: false,
      errors: ["vatPeriodType must be one of month, quarter, half-year"],
    };
  }
  // Ensure the column exists — older ledgers (and the base schema) lack it.
  const cols = db.query("PRAGMA table_info(companies)").all() as Array<{ name: string }>;
  if (!cols.some((col) => col.name === "vat_period_type")) {
    db.exec(
      "ALTER TABLE companies ADD COLUMN vat_period_type TEXT NOT NULL DEFAULT 'quarter' " +
        "CHECK(vat_period_type IN ('month', 'quarter', 'half-year'));",
    );
  }
  const before = db
    .query("SELECT vat_period_type AS t FROM companies WHERE id = 1")
    .get() as { t: string | null } | null;
  if (!before) {
    return {
      ok: false,
      changed: false,
      errors: ["company has not been initialised — run 'rentemester init' first"],
    };
  }
  db.query("UPDATE companies SET vat_period_type = ? WHERE id = 1").run(type);
  return { ok: true, changed: before.t !== type, errors: [] };
}

export type CloseAccountingPeriodInput = {
  periodStart: string;
  periodEnd: string;
  kind?: AccountingPeriodKind;
  status?: Exclude<AccountingPeriodStatus, "open">;
  reference?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type CloseAccountingPeriodResult = {
  ok: boolean;
  periodId?: number;
  periodStart?: string;
  periodEnd?: string;
  kind?: AccountingPeriodKind;
  status?: Exclude<AccountingPeriodStatus, "open">;
  reference?: string;
  appliedRules: string[];
  errors: string[];
};

const PERIOD_RULE_ID = "DK-BOOKKEEPING-PERIOD-LOCK-001";
const PERIOD_KINDS = new Set<AccountingPeriodKind>(["vat_quarter", "fiscal_year", "custom"]);
const PERIOD_STATUSES = new Set<Exclude<AccountingPeriodStatus, "open">>(["closed", "reported"]);

/**
 * Audit-log event type that drives a controlled reopen. The *latest*
 * lifecycle event for an `accounting_period` entity is the authoritative
 * state — that is how `period reopen` works without ever mutating the
 * (immutable) period row: a reopen is an appended `period_reopen` fact, a
 * re-close a fresh `period_close` fact. See `effectivePeriodState`.
 */
const PERIOD_REOPEN_EVENT = "period_reopen";

/**
 * The append-only lifecycle of an accounting period.
 *
 * The `accounting_periods_guard_update` schema trigger makes a closed/reported
 * period row immutable — a row can never be moved back to `open`. A controlled
 * reopen (#247) therefore cannot mutate the row; instead it appends a
 * `period_reopen` event to the append-only, actor-attributed `audit_log`.
 *
 * `effectivePeriodState` replays those audit events for one period and returns
 * its current effective state: a period whose most recent lifecycle event is a
 * `period_reopen` is effectively `open` again (new postings allowed) even
 * though its row still reads `closed`. A later `period close` appends a fresh
 * `period_close` event and locks it again. Nothing is ever deleted, so the
 * full close/reopen history stays auditable forever.
 */
export type EffectivePeriodState = "open" | "closed" | "reported";

/**
 * Replays the append-only audit lifecycle for a single accounting period and
 * returns its effective state. `rowStatus` is the period row's stored status,
 * used as the baseline when no lifecycle audit events exist (older periods,
 * or periods closed before audit logging covered them).
 */
export function effectivePeriodState(
  db: Database,
  periodId: number,
  rowStatus: AccountingPeriodStatus,
): EffectivePeriodState {
  const events = db
    .query(
      `SELECT event_type
         FROM audit_log
        WHERE entity_type = 'accounting_period'
          AND entity_id = ?
        ORDER BY id ASC`,
    )
    .all(String(periodId)) as Array<{ event_type: string }>;

  let state: EffectivePeriodState = rowStatus;
  for (const ev of events) {
    if (ev.event_type === "period_report") state = "reported";
    else if (ev.event_type === "period_close") state = "closed";
    else if (ev.event_type === PERIOD_REOPEN_EVENT) state = "open";
  }
  return state;
}


function maxFutureDays() {
  const raw = process.env.RENTEMESTER_MAX_FUTURE_DAYS;
  if (raw === undefined) return 45;
  const parsed = Number(raw);
  return Number.isInteger(parsed) && parsed >= 0 ? parsed : 45;
}

export function validateJournalTransactionDate(db: Database, transactionDate: string, fieldName = "transactionDate") {
  const errors: string[] = [];
  if (!looksLikeIsoDate(transactionDate)) return errors;

  const latestAllowedDate = addDays(todayIsoDate(), maxFutureDays());
  if (transactionDate > latestAllowedDate) {
    errors.push(`${fieldName} ${transactionDate} cannot be later than ${latestAllowedDate}`);
  }

  // Every closed/reported period that covers the date. The row status alone
  // is not authoritative — a period may have been reopened via the
  // append-only `period reopen` path (#247), which leaves the row reading
  // `closed` while its effective state is `open`. Such a period must NOT
  // block new postings, so each candidate is replayed through its audit
  // lifecycle and only those still effectively locked are reported.
  const candidates = db.query(
    `SELECT id, period_start, period_end, kind, status, reference
       FROM accounting_periods
      WHERE period_start <= ?
        AND period_end >= ?
        AND status IN ('closed', 'reported')
      ORDER BY period_end ASC, id ASC`
  ).all(transactionDate, transactionDate) as Array<{
    id: number;
    period_start: string;
    period_end: string;
    kind: AccountingPeriodKind;
    status: Exclude<AccountingPeriodStatus, "open">;
    reference: string | null;
  }>;

  const blocking = candidates.find(
    (period) => effectivePeriodState(db, period.id, period.status) !== "open",
  );

  if (blocking) {
    const effective = effectivePeriodState(db, blocking.id, blocking.status);
    const referenceText = blocking.reference ? ` ref ${blocking.reference}` : "";
    errors.push(
      `${fieldName} ${transactionDate} falls in ${effective} period ${blocking.kind} ${blocking.period_start}..${blocking.period_end}${referenceText}`
    );
  }

  return errors;
}

export function closeAccountingPeriod(db: Database, input: CloseAccountingPeriodInput): CloseAccountingPeriodResult {
  const appliedRules = [PERIOD_RULE_ID];
  const errors: string[] = [];
  const periodStart = input.periodStart?.trim();
  const periodEnd = input.periodEnd?.trim();
  const kind = (input.kind ?? "vat_quarter").trim() as AccountingPeriodKind;
  const status = (input.status ?? "closed").trim() as Exclude<AccountingPeriodStatus, "open">;
  const reference = input.reference?.trim();

  if (!looksLikeIsoDate(periodStart)) errors.push("periodStart must be YYYY-MM-DD");
  if (!looksLikeIsoDate(periodEnd)) errors.push("periodEnd must be YYYY-MM-DD");
  if (looksLikeIsoDate(periodStart) && looksLikeIsoDate(periodEnd) && periodStart > periodEnd) {
    errors.push("periodStart must be before or equal to periodEnd");
  }
  if (!PERIOD_KINDS.has(kind)) errors.push("kind must be one of vat_quarter, fiscal_year, custom");
  if (!PERIOD_STATUSES.has(status)) errors.push("status must be closed or reported");

  if (errors.length > 0) return { ok: false, appliedRules, errors };

  const overlap = db.query(
    `SELECT id, period_start, period_end, kind, status
       FROM accounting_periods
      WHERE kind = ?
        AND NOT (period_end < ? OR period_start > ?)
      LIMIT 1`
  ).get(kind, periodStart, periodEnd) as
    | { id: number; period_start: string; period_end: string; kind: AccountingPeriodKind; status: AccountingPeriodStatus }
    | null;

  if (overlap) {
    // An overlapping period that is *exactly* this period and has been
    // reopened (#247) is not a real conflict — it is the same period being
    // closed again. Re-close it by appending a fresh `period_close` event;
    // the immutable row keeps its original `closed`/`reported` status.
    const isSamePeriod = overlap.period_start === periodStart && overlap.period_end === periodEnd;
    const overlapEffective = effectivePeriodState(db, overlap.id, overlap.status);
    if (isSamePeriod && overlapEffective === "open") {
      insertAuditLog(db, {
        eventType: status === "reported" ? "period_report" : "period_close",
        entityType: "accounting_period",
        entityId: overlap.id,
        message:
          `Re-closed ${kind} period ${periodStart}..${periodEnd}` +
          `${reference ? ` (${reference})` : ""} after a controlled reopen`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });
      return {
        ok: true,
        periodId: overlap.id,
        periodStart,
        periodEnd,
        kind,
        status,
        reference,
        appliedRules,
        errors,
      };
    }
    return {
      ok: false,
      appliedRules,
      errors: [`${kind} period ${periodStart}..${periodEnd} overlaps existing period ${overlap.period_start}..${overlap.period_end}`],
    };
  }

  const inserted = db.query(
    `INSERT INTO accounting_periods (
      period_start, period_end, kind, status, closed_at, closed_by, reported_at, reference
    ) VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, ?, ?, ?)
    RETURNING id`
  ).get(
    periodStart,
    periodEnd,
    kind,
    status,
    input.createdBy ?? null,
    status === "reported" ? new Date().toISOString() : null,
    reference ?? null,
  ) as { id: number };

  insertAuditLog(db, {
    eventType: status === "reported" ? "period_report" : "period_close",
    entityType: "accounting_period",
    entityId: inserted.id,
    message: `${status === "reported" ? "Marked" : "Closed"} ${kind} period ${periodStart}..${periodEnd}${reference ? ` (${reference})` : ""}`,
    createdBy: input.createdBy,
    createdByProgram: input.createdByProgram,
  });

  return {
    ok: true,
    periodId: inserted.id,
    periodStart,
    periodEnd,
    kind,
    status,
    reference,
    appliedRules,
    errors,
  };
}

export type ReopenAccountingPeriodInput = {
  periodStart: string;
  periodEnd: string;
  kind?: AccountingPeriodKind;
  /** Mandatory free-text justification — recorded verbatim in the audit log. */
  reason: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type ReopenAccountingPeriodResult = {
  ok: boolean;
  periodId?: number;
  periodStart?: string;
  periodEnd?: string;
  kind?: AccountingPeriodKind;
  /** The period's effective state AFTER the reopen — `open` on success. */
  effectiveStatus?: EffectivePeriodState;
  /** The audit actor the reopen was attributed to. */
  reopenedBy?: string;
  reason?: string;
  appliedRules: string[];
  errors: string[];
};

/**
 * Controlled, fully audit-logged reopen of a closed accounting period (#247).
 *
 * The previous `period close` help honestly stated there was no reopen path
 * and that an early close "kun [kan] rettes ved at redigere ledger-databasen
 * direkte" — a dead end for a non-technical owner. This is the supported
 * recovery path.
 *
 * Integrity is kept intact precisely *because* nothing is overwritten:
 *  - The `accounting_periods` row is never mutated — the schema trigger keeps
 *    it immutable, and its `closed_at`/`closed_by`/bounds stay as historical
 *    record.
 *  - The reopen is a NEW row appended to the append-only `audit_log`, carrying
 *    the actor, timestamp and the mandatory `reason`. It is therefore fully
 *    attributable and can never be silently undone.
 *  - A `reported` period (already submitted to SKAT/Erhvervsstyrelsen) is
 *    refused — undoing an authority filing is not a bookkeeping operation.
 *
 * After a reopen, postings with a transaction date inside the period are
 * accepted again (`validateJournalTransactionDate` replays the audit
 * lifecycle). A later `period close` re-locks the same period.
 */
export function reopenAccountingPeriod(
  db: Database,
  input: ReopenAccountingPeriodInput,
): ReopenAccountingPeriodResult {
  const appliedRules = [PERIOD_RULE_ID];
  const errors: string[] = [];
  const periodStart = input.periodStart?.trim();
  const periodEnd = input.periodEnd?.trim();
  const kind = (input.kind ?? "vat_quarter").trim() as AccountingPeriodKind;
  const reason = input.reason?.trim();

  if (!looksLikeIsoDate(periodStart)) errors.push("periodStart must be YYYY-MM-DD");
  if (!looksLikeIsoDate(periodEnd)) errors.push("periodEnd must be YYYY-MM-DD");
  if (looksLikeIsoDate(periodStart) && looksLikeIsoDate(periodEnd) && periodStart > periodEnd) {
    errors.push("periodStart must be before or equal to periodEnd");
  }
  if (!PERIOD_KINDS.has(kind)) errors.push("kind must be one of vat_quarter, fiscal_year, custom");
  if (!reason) errors.push("reason is required: a reopen must record why the period is being reopened");

  if (errors.length > 0) return { ok: false, appliedRules, errors };

  const period = db.query(
    `SELECT id, period_start, period_end, kind, status
       FROM accounting_periods
      WHERE period_start = ? AND period_end = ? AND kind = ?
      LIMIT 1`
  ).get(periodStart, periodEnd, kind) as
    | { id: number; period_start: string; period_end: string; kind: AccountingPeriodKind; status: AccountingPeriodStatus }
    | null;

  if (!period) {
    return {
      ok: false,
      appliedRules,
      errors: [`no ${kind} period ${periodStart}..${periodEnd} exists to reopen`],
    };
  }

  const effective = effectivePeriodState(db, period.id, period.status);

  if (effective === "open") {
    return {
      ok: false,
      appliedRules,
      errors: [`${kind} period ${periodStart}..${periodEnd} is already open — nothing to reopen`],
    };
  }

  if (effective === "reported") {
    return {
      ok: false,
      appliedRules,
      errors: [
        `${kind} period ${periodStart}..${periodEnd} is reported (already submitted to the authority) ` +
          `and cannot be reopened; correct it with a new posting in an open period instead`,
      ],
    };
  }

  // effective === 'closed' — append the reopen fact. The actor resolves the
  // same way every mutating command does (--actor / RENTEMESTER_ACTOR / env),
  // so the reopen is always clearly attributable.
  const actor = resolveActor({ createdBy: input.createdBy, createdByProgram: input.createdByProgram });
  insertAuditLog(db, {
    eventType: PERIOD_REOPEN_EVENT,
    entityType: "accounting_period",
    entityId: period.id,
    message: `Reopened ${kind} period ${periodStart}..${periodEnd} — reason: ${reason}`,
    createdBy: input.createdBy,
    createdByProgram: input.createdByProgram,
  });

  return {
    ok: true,
    periodId: period.id,
    periodStart,
    periodEnd,
    kind,
    effectiveStatus: "open",
    reopenedBy: actor.auditActor,
    reason,
    appliedRules,
    errors,
  };
}
