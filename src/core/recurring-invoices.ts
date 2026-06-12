/**
 * Recurring invoice templates (#118).
 *
 * A recurring-invoice *template* is a reusable, append-only specification of an
 * invoice that repeats on a fixed monthly/quarterly/yearly cadence. Generation
 * is an *explicit* deterministic step: given a template and an `asOfDate`, the
 * core materializes exactly the invoice for the period currently due — never
 * more, never any background scheduling.
 *
 * Determinism rules (the whole point of this slice):
 *  - The period a generation belongs to is identified by an integer
 *    `periodIndex` (0-based) counted from the template's `firstIssueDate`.
 *  - The issue date and delivery period are derived purely from
 *    `firstIssueDate` + intervalMonths * periodIndex — no wall clock.
 *  - `recurring_invoice_generations` has UNIQUE(template_id, period_index), so
 *    re-running generation for the same period is idempotent: it returns the
 *    already-generated invoice instead of issuing a duplicate.
 *
 * Reminders / settlement stay on the generated invoice objects (the
 * `documents` row), never on the template.
 */

import type { Database } from "bun:sqlite";
import { isValidIsoDate, addDays } from "./dates";
import { validateInvoice, type InvoicePayload } from "./invoice";
import { issueInvoice } from "./issued-invoices";
import { insertAuditLog } from "./actor";
import { asDocumentId, type DocumentId } from "./ids";

export type RecurringInterval = "monthly" | "quarterly" | "yearly";

/** How the generated invoice's delivery period is derived from its issue date. */
export type DeliveryPeriodMode = "issue_month" | "interval_window" | "none";

export type RecurringInvoiceTemplateInput = {
  name: string;
  interval: RecurringInterval;
  /** ISO date of the first invoice this template should produce. */
  firstIssueDate: string;
  /** The invoice payload reused for every generation (issue/delivery dates are derived). */
  invoice: InvoicePayload;
  /** Payment terms in days; the generated invoice's dueDate = issueDate + this. */
  paymentTermsDays?: number;
  deliveryPeriodMode?: DeliveryPeriodMode;
  notes?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type RecurringInvoiceTemplateResult = {
  ok: boolean;
  templateId?: number;
  appliedRules: string[];
  errors: string[];
};

export type GenerateRecurringInvoiceInput = {
  templateId: number;
  /** Deterministic clock: the period due as of this ISO date is materialized. */
  asOfDate: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type GenerateRecurringInvoiceResult = {
  ok: boolean;
  /** true when a new invoice was issued, false when an existing one was returned. */
  created?: boolean;
  templateId?: number;
  periodIndex?: number;
  documentId?: number;
  invoiceNumber?: string;
  issueDate?: string;
  dueDate?: string;
  deliveryPeriodStart?: string;
  deliveryPeriodEnd?: string;
  appliedRules: string[];
  errors: string[];
};

const TEMPLATE_RULE_ID = "DK-RECURRING-INVOICE-TEMPLATE-001";
const GENERATE_RULE_ID = "DK-RECURRING-INVOICE-GENERATE-001";

const INTERVAL_MONTHS: Record<RecurringInterval, number> = {
  monthly: 1,
  quarterly: 3,
  yearly: 12,
};

const DELIVERY_MODES = new Set<DeliveryPeriodMode>([
  "issue_month",
  "interval_window",
  "none",
]);

function hasText(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

/**
 * Add `months` to a YYYY-MM-DD date, clamping the day to the last valid day of
 * the resulting month (Jan 31 + 1 month -> Feb 28/29). UTC-based and pure.
 */
export function addMonths(isoDate: string, months: number): string {
  const [yearText, monthText, dayText] = isoDate.split("-");
  const year = Number(yearText);
  const monthIndex = Number(monthText) - 1;
  const day = Number(dayText);
  const totalMonths = year * 12 + monthIndex + months;
  const targetYear = Math.floor(totalMonths / 12);
  const targetMonthIndex = totalMonths - targetYear * 12;
  // Day 0 of the next month is the last day of the target month.
  const lastDayOfMonth = new Date(Date.UTC(targetYear, targetMonthIndex + 1, 0)).getUTCDate();
  const clampedDay = Math.min(day, lastDayOfMonth);
  const result = new Date(Date.UTC(targetYear, targetMonthIndex, clampedDay));
  return result.toISOString().slice(0, 10);
}

/** First day of the month containing `isoDate`. */
function startOfMonth(isoDate: string): string {
  return `${isoDate.slice(0, 7)}-01`;
}

/** Last day of the month containing `isoDate`. */
function endOfMonth(isoDate: string): string {
  const [yearText, monthText] = isoDate.split("-");
  const lastDay = new Date(Date.UTC(Number(yearText), Number(monthText), 0)).getUTCDate();
  return `${yearText}-${monthText}-${String(lastDay).padStart(2, "0")}`;
}

/**
 * The 0-based period index due as of `asOfDate` for a template starting at
 * `firstIssueDate` with `intervalMonths` cadence. Returns -1 when nothing is
 * yet due (asOfDate precedes the first issue date).
 *
 * Counted by walking interval boundaries forward — no floating-point math, so
 * the result is identical on every rerun for the same inputs.
 */
export function periodIndexAsOf(
  firstIssueDate: string,
  intervalMonths: number,
  asOfDate: string,
): number {
  if (asOfDate < firstIssueDate) return -1;
  let index = 0;
  // Advance while the *next* period's issue date is still <= asOfDate.
  while (addMonths(firstIssueDate, intervalMonths * (index + 1)) <= asOfDate) {
    index += 1;
  }
  return index;
}

function deliveryWindow(
  mode: DeliveryPeriodMode,
  issueDate: string,
  intervalMonths: number,
): { start?: string; end?: string } {
  if (mode === "issue_month") {
    return { start: startOfMonth(issueDate), end: endOfMonth(issueDate) };
  }
  if (mode === "interval_window") {
    // The delivery period is the interval window beginning on the issue date.
    return { start: issueDate, end: addMonths(issueDate, intervalMonths) };
  }
  return {};
}

export function createRecurringInvoiceTemplate(
  db: Database,
  input: RecurringInvoiceTemplateInput,
): RecurringInvoiceTemplateResult {
  const appliedRules = [TEMPLATE_RULE_ID];
  const errors: string[] = [];

  const name = hasText(input.name) ? input.name.trim() : null;
  if (!name) errors.push("name is required");
  if (!(input.interval in INTERVAL_MONTHS)) {
    errors.push("interval must be one of monthly, quarterly, yearly");
  }
  if (!isValidIsoDate(input.firstIssueDate)) {
    errors.push("firstIssueDate must be a YYYY-MM-DD date");
  }
  const deliveryPeriodMode: DeliveryPeriodMode = input.deliveryPeriodMode ?? "issue_month";
  if (!DELIVERY_MODES.has(deliveryPeriodMode)) {
    errors.push("deliveryPeriodMode must be one of issue_month, interval_window, none");
  }
  const paymentTermsDays =
    input.paymentTermsDays === undefined
      ? 30
      : Number(input.paymentTermsDays);
  if (
    !Number.isInteger(paymentTermsDays) ||
    paymentTermsDays < 0 ||
    paymentTermsDays > 365
  ) {
    errors.push("paymentTermsDays must be an integer between 0 and 365 when present");
  }
  if (input.invoice === undefined || input.invoice === null || typeof input.invoice !== "object") {
    errors.push("invoice payload is required");
  }

  if (errors.length > 0) return { ok: false, appliedRules, errors };

  // The embedded payload must already be a valid invoice apart from the
  // date/number fields that generation supplies. Validate it with a sentinel
  // issueDate so a structurally broken template is rejected up front.
  const probe: InvoicePayload = {
    ...input.invoice,
    issueDate: input.invoice.issueDate ?? input.firstIssueDate,
    invoiceNumber: undefined,
    dueDate: undefined,
    deliveryDate: undefined,
    deliveryPeriodStart: undefined,
    deliveryPeriodEnd: undefined,
  };
  const validation = validateInvoice(probe);
  if (!validation.ok) {
    return { ok: false, appliedRules: [...new Set([...appliedRules, ...validation.appliedRules])], errors: validation.errors };
  }

  // Persist the payload stripped of derived date/number fields so generation
  // is the single source of truth for those.
  const storedPayload: InvoicePayload = {
    ...input.invoice,
    issueDate: undefined,
    invoiceNumber: undefined,
    dueDate: undefined,
    deliveryDate: undefined,
    deliveryPeriodStart: undefined,
    deliveryPeriodEnd: undefined,
  };

  const inserted = db.transaction(() => {
    const row = db.query(
      `INSERT INTO recurring_invoice_templates (
        name, interval, first_issue_date, next_issue_date, payment_terms_days,
        delivery_period_mode, payload_json, notes
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      RETURNING id`,
    ).get(
      name,
      input.interval,
      input.firstIssueDate,
      input.firstIssueDate,
      paymentTermsDays,
      deliveryPeriodMode,
      JSON.stringify(storedPayload),
      hasText(input.notes) ? input.notes!.trim() : null,
    ) as { id: number };

    insertAuditLog(db, {
      eventType: "recurring_invoice_template_create",
      entityType: "recurring_invoice_template",
      entityId: row.id,
      message: `Created recurring invoice template ${row.id} ${name} (${input.interval})`,
      createdBy: input.createdBy,
      createdByProgram: input.createdByProgram,
    });

    return row;
  }, { immediate: true })();

  return { ok: true, templateId: inserted.id, appliedRules, errors: [] };
}

type TemplateRow = {
  id: number;
  name: string;
  interval: RecurringInterval;
  first_issue_date: string;
  next_issue_date: string;
  payment_terms_days: number;
  delivery_period_mode: DeliveryPeriodMode;
  payload_json: string;
  notes: string | null;
  active: number;
  created_at: string;
};

export function listRecurringInvoiceTemplates(
  db: Database,
  options: { includeInactive?: boolean } = {},
) {
  const rows = db.query(
    `SELECT id, name, interval, first_issue_date, next_issue_date, payment_terms_days,
            delivery_period_mode, payload_json, notes, active, created_at
       FROM recurring_invoice_templates
      WHERE active = CASE WHEN ? THEN active ELSE 1 END
      ORDER BY id ASC`,
  ).all(options.includeInactive ? 1 : 0) as TemplateRow[];

  return {
    ok: true,
    count: rows.length,
    rows: rows.map((row) => ({
      id: row.id,
      name: row.name,
      interval: row.interval,
      firstIssueDate: row.first_issue_date,
      nextIssueDate: row.next_issue_date,
      paymentTermsDays: row.payment_terms_days,
      deliveryPeriodMode: row.delivery_period_mode,
      notes: row.notes,
      active: Boolean(row.active),
      createdAt: row.created_at,
    })),
    errors: [],
  };
}

type GenerationRow = {
  id: number;
  template_id: number;
  period_index: number;
  document_id: number;
  invoice_number: string;
  issue_date: string;
  delivery_period_start: string | null;
  delivery_period_end: string | null;
  created_at: string;
};

export function listRecurringInvoiceGenerations(db: Database, templateId: number) {
  const rows = db.query(
    `SELECT id, template_id, period_index, document_id, invoice_number, issue_date,
            delivery_period_start, delivery_period_end, created_at
       FROM recurring_invoice_generations
      WHERE template_id = ?
      ORDER BY period_index ASC`,
  ).all(templateId) as GenerationRow[];

  return {
    ok: true,
    count: rows.length,
    rows: rows.map((row) => ({
      id: row.id,
      templateId: row.template_id,
      periodIndex: row.period_index,
      documentId: row.document_id,
      invoiceNumber: row.invoice_number,
      issueDate: row.issue_date,
      deliveryPeriodStart: row.delivery_period_start,
      deliveryPeriodEnd: row.delivery_period_end,
      createdAt: row.created_at,
    })),
    errors: [],
  };
}

export type RetireRecurringInvoiceTemplateInput = {
  templateId: number;
  /** Optional human-supplied reason; recorded on the audit log entry. */
  reason?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type RetireRecurringInvoiceTemplateResult = {
  ok: boolean;
  templateId?: number;
  appliedRules: string[];
  errors: string[];
};

// Retire shares the template lifecycle rule (DK-RECURRING-INVOICE-TEMPLATE-001);
// it is the same append-only contract — only the `active` column may flip 1->0.
const RETIRE_RULE_ID = TEMPLATE_RULE_ID;

/**
 * Retire (deactivate) a recurring invoice template — sets `active = 0`.
 *
 * Retiring is the cockpit-facing maintenance action for templates whose
 * underlying contract has ended or changed. The append-only schema trigger
 * (`recurring_invoice_templates_guard_update`) forbids unretiring (0 -> 1) and
 * forbids mutating the immutable identity columns — that is by design: a
 * retired template's historical generations remain intact, and a new template
 * is created when terms change (see issue #435).
 *
 * Idempotent: retiring an already-retired template is a no-op success.
 */
export function retireRecurringInvoiceTemplate(
  db: Database,
  input: RetireRecurringInvoiceTemplateInput,
): RetireRecurringInvoiceTemplateResult {
  const appliedRules = [RETIRE_RULE_ID];

  if (!Number.isInteger(input.templateId) || input.templateId <= 0) {
    return { ok: false, appliedRules, errors: ["templateId must be a positive integer"] };
  }

  const template = db.query(
    `SELECT id, name, active FROM recurring_invoice_templates WHERE id = ? LIMIT 1`,
  ).get(input.templateId) as { id: number; name: string; active: number } | null;

  if (!template) {
    return {
      ok: false,
      appliedRules,
      errors: [`recurring invoice template ${input.templateId} does not exist`],
    };
  }

  if (!template.active) {
    // Idempotent: already retired.
    return { ok: true, templateId: template.id, appliedRules, errors: [] };
  }

  db.transaction(() => {
    db.run(
      `UPDATE recurring_invoice_templates SET active = 0 WHERE id = ?`,
      template.id,
    );
    const reasonSuffix = hasText(input.reason) ? ` — ${input.reason!.trim()}` : "";
    insertAuditLog(db, {
      eventType: "recurring_invoice_template_retire",
      entityType: "recurring_invoice_template",
      entityId: template.id,
      message: `Retired recurring invoice template ${template.id} ${template.name}${reasonSuffix}`,
      createdBy: input.createdBy,
      createdByProgram: input.createdByProgram,
    });
  }, { immediate: true })();

  return { ok: true, templateId: template.id, appliedRules, errors: [] };
}

export function generateRecurringInvoice(
  db: Database,
  companyRoot: string,
  input: GenerateRecurringInvoiceInput,
): GenerateRecurringInvoiceResult {
  const appliedRules = [GENERATE_RULE_ID];

  if (!Number.isInteger(input.templateId) || input.templateId <= 0) {
    return { ok: false, appliedRules, errors: ["templateId must be a positive integer"] };
  }
  if (!isValidIsoDate(input.asOfDate)) {
    return { ok: false, appliedRules, errors: ["asOfDate must be a YYYY-MM-DD date"] };
  }

  const template = db.query(
    `SELECT id, name, interval, first_issue_date, next_issue_date, payment_terms_days,
            delivery_period_mode, payload_json, notes, active, created_at
       FROM recurring_invoice_templates WHERE id = ? LIMIT 1`,
  ).get(input.templateId) as TemplateRow | null;
  if (!template) {
    return { ok: false, appliedRules, errors: [`recurring invoice template ${input.templateId} does not exist`] };
  }
  if (!template.active) {
    return { ok: false, appliedRules, errors: [`recurring invoice template ${input.templateId} is inactive`] };
  }

  const intervalMonths = INTERVAL_MONTHS[template.interval];
  const periodIndex = periodIndexAsOf(template.first_issue_date, intervalMonths, input.asOfDate);
  if (periodIndex < 0) {
    return {
      ok: false,
      appliedRules,
      errors: [
        `recurring invoice template ${template.id} is not yet due as of ${input.asOfDate} (first issue date ${template.first_issue_date})`,
      ],
    };
  }

  const issueDate = addMonths(template.first_issue_date, intervalMonths * periodIndex);
  const window = deliveryWindow(template.delivery_period_mode, issueDate, intervalMonths);
  const dueDate = addDays(issueDate, template.payment_terms_days);

  // Idempotency gate: if this template/period was already generated, return
  // the existing invoice instead of issuing a duplicate.
  const existing = db.query(
    `SELECT id, template_id, period_index, document_id, invoice_number, issue_date,
            delivery_period_start, delivery_period_end, created_at
       FROM recurring_invoice_generations
      WHERE template_id = ? AND period_index = ? LIMIT 1`,
  ).get(template.id, periodIndex) as GenerationRow | null;
  if (existing) {
    return {
      ok: true,
      created: false,
      templateId: template.id,
      periodIndex,
      documentId: existing.document_id,
      invoiceNumber: existing.invoice_number,
      issueDate: existing.issue_date,
      dueDate,
      deliveryPeriodStart: existing.delivery_period_start ?? undefined,
      deliveryPeriodEnd: existing.delivery_period_end ?? undefined,
      appliedRules,
      errors: [],
    };
  }

  const basePayload: InvoicePayload = JSON.parse(template.payload_json);
  const payload: InvoicePayload = {
    ...basePayload,
    issueDate,
    invoiceNumber: undefined,
    dueDate,
    deliveryDate: undefined,
    deliveryPeriodStart: window.start,
    deliveryPeriodEnd: window.end,
  };

  const issued = issueInvoice(db, companyRoot, payload);
  const mergedRules = [...new Set([...appliedRules, ...issued.appliedRules])];
  if (!issued.ok || issued.documentId === undefined || issued.invoiceNumber === undefined) {
    return { ok: false, appliedRules: mergedRules, errors: issued.errors };
  }

  const documentId: DocumentId = asDocumentId(Number(issued.documentId));
  const invoiceNumber = String(issued.invoiceNumber);

  // Record the audit link generated-invoice -> template, and advance the
  // template's next_issue_date marker. The UNIQUE(template_id, period_index)
  // constraint is the hard backstop against double-generation under races.
  db.transaction(() => {
    db.query(
      `INSERT INTO recurring_invoice_generations (
        template_id, period_index, document_id, invoice_number, issue_date,
        delivery_period_start, delivery_period_end
      ) VALUES (?, ?, ?, ?, ?, ?, ?)`,
    ).run(
      template.id,
      periodIndex,
      documentId,
      invoiceNumber,
      issueDate,
      window.start ?? null,
      window.end ?? null,
    );

    const nextIssueDate = addMonths(template.first_issue_date, intervalMonths * (periodIndex + 1));
    if (nextIssueDate > template.next_issue_date) {
      db.run(
        `UPDATE recurring_invoice_templates SET next_issue_date = ? WHERE id = ?`,
        nextIssueDate,
        template.id,
      );
    }

    insertAuditLog(db, {
      eventType: "recurring_invoice_generate",
      entityType: "document",
      entityId: documentId,
      message: `Generated invoice ${invoiceNumber} from recurring template ${template.id} period ${periodIndex}`,
      createdBy: input.createdBy,
      createdByProgram: input.createdByProgram,
    });
  }, { immediate: true })();

  return {
    ok: true,
    created: true,
    templateId: template.id,
    periodIndex,
    documentId,
    invoiceNumber,
    issueDate,
    dueDate,
    deliveryPeriodStart: window.start,
    deliveryPeriodEnd: window.end,
    appliedRules: mergedRules,
    errors: [],
  };
}
