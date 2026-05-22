import type { Database } from "bun:sqlite";
import { formatKronerDa, normalizeCurrency } from "./money";

export type ExceptionSeverity = "low" | "medium" | "high";
export type ExceptionStatus = "open" | "resolved" | "all";

export type RecordExceptionInput = {
  type: string;
  severity?: ExceptionSeverity;
  relatedBankTransactionId?: number | null;
  relatedDocumentId?: number | null;
  message: string;
  requiredAction?: string | null;
  sourceEvidence?: unknown;
  postingPreview?: unknown;
};

export type ListExceptionsInput = {
  status?: ExceptionStatus;
  /**
   * When false (the default), exceptions whose subject falls in an archived
   * period — before the primobalance cut-over date, or inside a closed/reported
   * accounting period — are excluded. They are real historical artifacts but
   * not open tasks, so they must not be counted as such.
   */
  includeArchived?: boolean;
};

export type ResolveExceptionInput = {
  id: number;
  note?: string | null;
  resolvedBy?: string | null;
};

function serializeJson(value: unknown) {
  if (value === undefined) return null;
  return JSON.stringify(value);
}

function parseJson<T = unknown>(value: string | null) {
  if (!value) return null as T | null;
  try {
    return JSON.parse(value) as T;
  } catch {
    return value as T;
  }
}

function normalizeStatus(status?: ExceptionStatus) {
  if (!status || status === "open" || status === "resolved" || status === "all") return status ?? "open";
  return null;
}

export function recordException(db: Database, input: RecordExceptionInput) {
  const errors: string[] = [];
  const severity = input.severity ?? "medium";
  if (!["low", "medium", "high"].includes(severity)) errors.push("severity must be low, medium, or high");
  if (!input.type?.trim()) errors.push("type is required");
  if (!input.message?.trim()) errors.push("message is required");
  if (errors.length > 0) return { ok: false, exceptionId: undefined, duplicate: false, errors };

  const message = input.message.trim();
  const requiredAction = input.requiredAction?.trim() || null;
  const sourceEvidence = serializeJson(input.sourceEvidence);
  const postingPreview = serializeJson(input.postingPreview);

  const existing = db.query(
    `SELECT id
     FROM exceptions
     WHERE status = 'open'
       AND type = ?
       AND COALESCE(related_bank_transaction_id, 0) = COALESCE(?, 0)
       AND COALESCE(related_document_id, 0) = COALESCE(?, 0)
       AND message = ?
       AND COALESCE(required_action, '') = COALESCE(?, '')
     LIMIT 1`
  ).get(
    input.type.trim(),
    input.relatedBankTransactionId ?? null,
    input.relatedDocumentId ?? null,
    message,
    requiredAction,
  ) as { id: number } | null;

  if (existing) return { ok: true, exceptionId: existing.id, duplicate: true, errors: [] };

  const row = db.query(
    `INSERT INTO exceptions (
      type, severity, status, related_bank_transaction_id, related_document_id,
      message, required_action, source_evidence, posting_preview
    ) VALUES (?, ?, 'open', ?, ?, ?, ?, ?, ?)
    RETURNING id`
  ).get(
    input.type.trim(),
    severity,
    input.relatedBankTransactionId ?? null,
    input.relatedDocumentId ?? null,
    message,
    requiredAction,
    sourceEvidence,
    postingPreview,
  ) as { id: number };

  return { ok: true, exceptionId: row.id, duplicate: false, errors: [] };
}

/** The archived boundary of the live ledger — see `isArchivedDate`. */
type ArchivedBoundary = {
  /** Primobalance cut-over date; everything before it is the old system's. */
  cutOverDate: string | null;
  /** Closed/reported accounting periods — exceptions inside them are archived. */
  closedPeriods: Array<{ start: string; end: string }>;
};

function loadArchivedBoundary(db: Database): ArchivedBoundary {
  const opening = db
    .query(`SELECT cut_over_date FROM opening_balances LIMIT 1`)
    .get() as { cut_over_date: string } | null;
  const periods = db
    .query(
      `SELECT period_start, period_end FROM accounting_periods
        WHERE status IN ('closed', 'reported')`,
    )
    .all() as Array<{ period_start: string; period_end: string }>;
  return {
    cutOverDate: opening?.cut_over_date ?? null,
    closedPeriods: periods.map((p) => ({ start: p.period_start, end: p.period_end })),
  };
}

/**
 * Whether an exception's subject date falls in an archived period. ISO dates
 * compare correctly as strings. A null date (an exception with no dated
 * subject) is never archived — it stays an open task.
 */
function isArchivedDate(subjectDate: string | null, boundary: ArchivedBoundary): boolean {
  if (!subjectDate) return false;
  if (boundary.cutOverDate && subjectDate < boundary.cutOverDate) return true;
  return boundary.closedPeriods.some((p) => subjectDate >= p.start && subjectDate <= p.end);
}

export function listExceptions(db: Database, input: ListExceptionsInput = {}) {
  const status = normalizeStatus(input.status);
  if (!status) return { ok: false, count: 0, rows: [], errors: ["status must be open, resolved, or all when present"] };

  const boundary = loadArchivedBoundary(db);
  // The subject date is the date of the bank transaction / document the
  // exception concerns — that is what places it in (or out of) an archived
  // period. An exception with no dated subject has no archived classification.
  const rawRows = db.query(
    `SELECT e.id, e.type, e.severity, e.status, e.related_bank_transaction_id, e.related_document_id,
            e.message, e.required_action, e.source_evidence, e.posting_preview,
            e.created_at, e.resolved_at, e.resolved_by, e.resolution_note,
            COALESCE(bt.transaction_date, d.invoice_date, substr(d.upload_datetime, 1, 10)) AS subject_date
     FROM exceptions e
     LEFT JOIN bank_transactions bt ON bt.id = e.related_bank_transaction_id
     LEFT JOIN documents d ON d.id = e.related_document_id
     WHERE (? = 'all' OR e.status = ?)
     ORDER BY e.id DESC`
  ).all(status, status) as Array<any>;

  const includeArchived = input.includeArchived === true;
  const rows = rawRows
    .map((row) => ({
      id: row.id,
      type: row.type,
      severity: row.severity,
      status: row.status,
      relatedBankTransactionId: row.related_bank_transaction_id,
      relatedDocumentId: row.related_document_id,
      message: row.message,
      requiredAction: row.required_action,
      sourceEvidence: parseJson(row.source_evidence),
      postingPreview: parseJson(row.posting_preview),
      createdAt: row.created_at,
      resolvedAt: row.resolved_at,
      resolvedBy: row.resolved_by,
      resolutionNote: row.resolution_note,
      /** True when the exception belongs to an archived/closed period. */
      archived: isArchivedDate(row.subject_date, boundary),
    }))
    .filter((row) => includeArchived || !row.archived);

  return { ok: true, count: rows.length, rows, errors: [] };
}

export function resolveException(db: Database, input: ResolveExceptionInput) {
  const errors: string[] = [];
  if (!Number.isInteger(input.id) || input.id <= 0) errors.push("id must be a positive integer");
  if (errors.length > 0) return { ok: false, resolved: false, errors };

  const row = db.query(`SELECT id, status FROM exceptions WHERE id = ? LIMIT 1`).get(input.id) as { id: number; status: string } | null;
  if (!row) return { ok: false, resolved: false, errors: [`exception ${input.id} does not exist`] };
  if (row.status === "resolved") return { ok: true, resolved: false, errors: [] };

  db.run(
    `UPDATE exceptions
     SET status = 'resolved', resolved_at = CURRENT_TIMESTAMP, resolved_by = ?, resolution_note = ?
     WHERE id = ?`,
    input.resolvedBy ?? null,
    input.note?.trim() || null,
    input.id,
  );

  return { ok: true, resolved: true, errors: [] };
}

export function resolveOpenExceptionsForBankTransaction(db: Database, bankTransactionId: number, note?: string | null, resolvedBy?: string | null) {
  if (!Number.isInteger(bankTransactionId) || bankTransactionId <= 0) return { ok: false, resolvedCount: 0, errors: ["bankTransactionId must be a positive integer"] };
  const openRows = db.query(
    `SELECT id
     FROM exceptions
     WHERE type = 'UNMATCHED_BANK_TRANSACTION'
       AND status = 'open'
       AND related_bank_transaction_id = ?`
  ).all(bankTransactionId) as Array<{ id: number }>;

  for (const row of openRows) {
    db.run(
      `UPDATE exceptions
       SET status = 'resolved', resolved_at = CURRENT_TIMESTAMP, resolved_by = ?, resolution_note = ?
       WHERE id = ?`,
      resolvedBy ?? null,
      note?.trim() || "Resolved automatically when bank transaction was linked to a posted journal entry",
      row.id,
    );
  }

  return { ok: true, resolvedCount: openRows.length, errors: [] };
}

/**
 * A purchase/cash-register document that has not yet been booked to a journal
 * entry, found by exact gross-amount match against an unmatched bank line.
 * Used to make the unmatched-bank-transaction exception concrete in Danish:
 * we can name the bilag and say what the owner must add to book it (#224).
 */
type CandidateBilag = {
  documentId: number;
  documentNo: string | null;
  documentType: string;
  supplierName: string | null;
  deliveryDescription: string | null;
};

/** Crude representation/bevirtning detector — restaurant, café, bar, kro … */
function looksLikeRepresentation(bilag: CandidateBilag, bankText: string): boolean {
  const haystack = [
    bilag.documentType,
    bilag.supplierName ?? "",
    bilag.deliveryDescription ?? "",
    bankText,
  ]
    .join(" ")
    .toLowerCase();
  if (bilag.documentType === "cash_register_receipt") return true;
  return /restaurant|café|cafe|bistro|kro|brasserie|brasseri|spisehus|diner|bevirtning|frokost|middag/.test(
    haystack,
  );
}

/**
 * Finds the single unbooked purchase/cash-register bilag whose gross amount
 * equals the absolute bank-line amount. Returns null on no match or on an
 * ambiguous match (more than one candidate) — ambiguity is never guessed.
 */
function findCandidateBilag(db: Database, bankTransactionId: number, amount: number): CandidateBilag | null {
  if (!(Math.abs(amount) > 0)) return null;
  // Round to integer øre to compare a raw float bank amount against the
  // document gross safely (matches the comparison the matcher uses).
  const ore = Math.round(Math.abs(amount) * 100);
  const rows = db
    .query(
      `SELECT d.id, d.document_no, d.document_type, d.supplier_name, d.delivery_description
       FROM documents d
       LEFT JOIN journal_entries je ON je.document_id = d.id AND je.status = 'posted'
       WHERE d.document_type IN ('purchase_sale', 'cash_register_receipt')
         AND je.id IS NULL
         AND CAST(ROUND(d.amount_inc_vat * 100) AS INTEGER) = ?
       LIMIT 2`,
    )
    .all(ore) as Array<{
      id: number;
      document_no: string | null;
      document_type: string;
      supplier_name: string | null;
      delivery_description: string | null;
    }>;
  if (rows.length !== 1) return null;
  const row = rows[0]!;
  return {
    documentId: row.id,
    documentNo: row.document_no,
    documentType: row.document_type,
    supplierName: row.supplier_name,
    deliveryDescription: row.delivery_description,
  };
}

/**
 * Danish-locale rendering of a bank-line amount in its own currency. A DKK
 * line renders as kroner ("1.205,00 kr."). A foreign-currency line renders in
 * that currency ("100,00 EUR") — never as a DKK-formatted figure with "kr."
 * and the code tacked on, which would imply a wrong, far larger kroner value
 * (#269: 100 EUR is ~745 kr., so "100,00 kr. EUR" is numerically misleading).
 *
 * #314: the DKK branch delegates to the single canonical formatter
 * `formatKronerDa` in `core/money.ts` (using its absolute value, since the
 * surrounding wording already carries the in/out direction), so a kroner bank
 * amount renders the identical string as every other human-facing surface.
 */
function formatBankAmount(amount: number, currency: string | null | undefined): string {
  if (normalizeCurrency(currency) === "DKK") return formatKronerDa(Math.abs(amount));
  const formatted = Math.abs(amount).toLocaleString("da-DK", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
  return `${formatted} ${normalizeCurrency(currency)}`;
}

/**
 * The bookkeeping direction of a bank line, derived from the amount's sign.
 * A positive amount is money IN (a customer payment / payout — income); a
 * negative amount is money OUT (a supplier payment — an expense). A zero
 * amount has no direction and is treated as an expense for wording purposes.
 */
type BankDirection = "income" | "expense";

function bankDirection(amount: number): BankDirection {
  return amount > 0 ? "income" : "expense";
}

/**
 * Builds the human-facing Danish message + required action for an unmatched
 * bank transaction (#224, #237). The text names the transaction concretely,
 * is sign-aware — an inbound payment is described as income, never as a
 * deductible expense — and, when a likely bilag is found, says exactly what
 * is still missing and what the owner must do, in plain Danish without
 * technical command jargon.
 */
function describeUnmatchedBankTransaction(
  bank: { id: number; transaction_date: string; text: string | null; amount: number; currency: string },
  bilag: CandidateBilag | null,
): { message: string; requiredAction: string } {
  const beløb = formatBankAmount(bank.amount, bank.currency);
  const tekst = (bank.text ?? "").trim() || "(ingen tekst)";
  const direction = bankDirection(bank.amount);
  // Sign-aware nouns: an inbound payment is income (an "indbetaling"), an
  // outbound one an expense (a "betaling"/"udgift"). #237: a money-in line
  // must never be described as a deductible expense.
  const kindNoun = direction === "income" ? "indbetalingen" : "betalingen";
  const linje =
    direction === "income"
      ? `Indbetalingen "${tekst}" den ${bank.transaction_date} på ${beløb} er endnu ikke bogført`
      : `Banktransaktionen "${tekst}" den ${bank.transaction_date} på ${beløb} er endnu ikke bogført`;

  // No bilag at all — the owner is missing the underlying voucher.
  if (!bilag) {
    if (direction === "income") {
      return {
        message:
          `${linje}. Der er ikke fundet et bilag (faktura eller anden indtægtsdokumentation), ` +
          `der passer til beløbet.`,
        requiredAction:
          "Find fakturaen eller dokumentationen for denne indbetaling og læg den i bogføringen. " +
          "Uden et bilag kan indtægten ikke bogføres korrekt, og evt. salgsmoms kan ikke afregnes.",
      };
    }
    return {
      message: `${linje}. Der er ikke fundet et bilag (kvittering eller faktura), der passer til beløbet.`,
      requiredAction:
        "Find kvitteringen eller fakturaen for denne betaling og læg den i bogføringen. " +
        "Uden et bilag kan udgiften ikke bogføres og momsen ikke fratrækkes.",
    };
  }

  const bilagNavn = bilag.documentNo ? `bilag ${bilag.documentNo}` : `et bilag (id ${bilag.documentId})`;
  const leverandør = bilag.supplierName ? ` fra ${bilag.supplierName}` : "";

  // Representation / restaurant voucher missing its purpose + attendees — the
  // exact case from the README: "Restaurantbilag mangler formål og deltagere".
  // Representation is by definition an outbound expense; the wording stays
  // expense-shaped here regardless of any odd sign.
  if (looksLikeRepresentation(bilag, tekst)) {
    return {
      message:
        `${linje}. Den passer til ${bilagNavn}${leverandør}, som ligner et restaurant-/bevirtningsbilag. ` +
        `Bilaget mangler formål og deltagere — det skal fremgå, hvem der deltog, og hvad anledningen var.`,
      requiredAction:
        "Skriv formålet med bevirtningen og navnene på deltagerne på bilaget (fx \"kundemøde, " +
        "Anders Jensen, Acme A/S\"). Formål og deltagere er et lovkrav for, at udgiften kan " +
        "fradrages som repræsentation. Bogfør derefter bilaget mod betalingen.",
    };
  }

  // A matching bilag exists but the link is not yet posted. #237: do NOT
  // claim "no bilag found" here — name the real reason it still needs
  // attention (the bilag is not yet booked against the bank line) and keep
  // the income/expense wording aligned with the amount's sign.
  if (direction === "income") {
    return {
      message:
        `${linje}. Den passer til ${bilagNavn}${leverandør}, men bilaget er endnu ikke bogført mod indbetalingen.`,
      requiredAction:
        `Kontroller, at ${bilagNavn} hører til denne indbetaling, og bogfør indtægten mod banktransaktionen.`,
    };
  }
  return {
    message:
      `${linje}. Den passer til ${bilagNavn}${leverandør}, men bilaget er endnu ikke bogført mod ${kindNoun}.`,
    requiredAction:
      `Kontroller, at ${bilagNavn} hører til denne betaling, og bogfør udgiften mod banktransaktionen.`,
  };
}

export function syncUnmatchedBankTransactionExceptions(db: Database) {
  // Every bank transaction with no posted journal entry — together with its
  // currently-open UNMATCHED_BANK_TRANSACTION exception (if any). A new
  // transaction has no exception yet; one synced before its bilag was ingested
  // has a stale one. Both are handled below: missing ⇒ create, stale ⇒ the old
  // exception is resolved and a fresh, corrected one opened in its place.
  const unmatchedRows = db.query(
    `SELECT bt.id, bt.transaction_date, bt.booking_date, bt.text, bt.amount, bt.currency, bt.reference, bt.import_batch_id,
            ex.id AS exception_id, ex.message AS exception_message, ex.required_action AS exception_required_action
     FROM bank_transactions bt
     LEFT JOIN journal_entries je
       ON je.source_bank_transaction_id = bt.id
      AND je.status = 'posted'
     LEFT JOIN exceptions ex
       ON ex.related_bank_transaction_id = bt.id
      AND ex.type = 'UNMATCHED_BANK_TRANSACTION'
      AND ex.status = 'open'
     WHERE je.id IS NULL
     ORDER BY bt.transaction_date ASC, bt.id ASC`
  ).all() as Array<any>;

  let created = 0;
  let refreshed = 0;
  let resolvedStale = 0;
  for (const row of unmatchedRows) {
    const amount = Number(row.amount);
    const bilag = findCandidateBilag(db, row.id, amount);
    const { message, requiredAction } = describeUnmatchedBankTransaction(
      {
        id: row.id,
        transaction_date: row.transaction_date,
        text: row.text,
        amount,
        currency: row.currency,
      },
      bilag,
    );
    const sourceEvidence = JSON.stringify({
      bankTransactionId: row.id,
      transactionDate: row.transaction_date,
      bookingDate: row.booking_date,
      text: row.text,
      amount,
      currency: row.currency,
      reference: row.reference,
      importBatchId: row.import_batch_id,
      candidateBilagId: bilag?.documentId ?? null,
      candidateBilagNo: bilag?.documentNo ?? null,
    });

    if (row.exception_id != null) {
      // An exception already exists. When its Danish text is still current
      // there is nothing to do — leaving it untouched keeps the sync
      // idempotent, so re-importing overlapping bank rows never touches the
      // immutable row.
      const stale =
        row.exception_message !== message ||
        (row.exception_required_action ?? "") !== requiredAction;
      if (!stale) continue;

      // The text has gone stale — typically because the matching bilag has
      // since been ingested, so the generic "no bilag found" message can now
      // name the bilag. Exception rows are append-only (the immutability
      // trigger rejects ANY mutation, including a corrected message or a
      // newly-linked related_document_id), so we cannot rewrite the row.
      // Instead we resolve the stale exception and fall through to open a
      // fresh, corrected one — the audit chain stays intact and we stay
      // within the existing open → resolved lifecycle.
      resolveException(db, {
        id: row.exception_id,
        note: "Erstattet automatisk: beskrivelsen blev opdateret, da et passende bilag blev tilføjet bogføringen",
        resolvedBy: "system:sync-unmatched-bank-transactions",
      });
      refreshed += 1;
      resolvedStale += 1;
      // fall through — recordException below opens the corrected exception.
    }

    const result = recordException(db, {
      type: "UNMATCHED_BANK_TRANSACTION",
      severity: "medium",
      relatedBankTransactionId: row.id,
      relatedDocumentId: bilag?.documentId ?? null,
      message,
      requiredAction,
      sourceEvidence: JSON.parse(sourceEvidence),
      postingPreview: {
        bankTransactionId: row.id,
        candidateBilagId: bilag?.documentId ?? null,
      },
    });
    if (result.ok && !result.duplicate) created += 1;
  }

  return { ok: true, created, refreshed, resolvedStale, errors: [] };
}
