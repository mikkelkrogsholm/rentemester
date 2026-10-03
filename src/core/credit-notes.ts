import { TransactionRejectionError, decodeTransactionRejection } from "./transaction-rejection";
import { runSql } from "./sqlite";
import { createHash } from "node:crypto";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import type { Database } from "bun:sqlite";
import { companyPaths } from "./paths";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { promoteTempFileExclusive, removeIfExists, writeTempFileFor } from "./atomic-file";
import { insertAuditLog } from "./actor";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { fromOre, roundDkk, toOre } from "./money";
import { companySequenceScope, fiscalYearLabelFromDate, nextSequenceValue, reserveSequenceValue } from "./sequences";
import { retainUntilForDate } from "./retention";
import { strengthenGdprErasureAliasesForIdentity } from "./gdpr";
import {
  resolveInvoiceReceivableAccount,
  validateInvoiceCreditNoteEvidence,
} from "./invoice-fx-receivable";

export type IssueCreditNoteInput = {
  originalInvoiceDocumentId: number;
  issueDate: string;
  reason: string;
  grossAmount?: number;
  creditNoteNumber?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type IssueCreditNoteResult = {
  ok: boolean;
  documentId?: number;
  creditNoteNumber?: string;
  originalInvoiceNumber?: string;
  storedPath?: string;
  sha256?: string;
  journalEntryId?: number;
  journalEntryNo?: string;
  appliedRules: string[];
  errors: string[];
};

const RULE_ID = "DK-CREDIT-NOTE-001";
const REVERSE_RULE_ID = "DK-INVOICE-BOOKKEEPING-REVERSE-002";

function sha256(text: string) {
  return createHash("sha256").update(text).digest("hex");
}

function hasCommittedDocumentAtPath(db: Database, storedPath: string): boolean | null {
  try {
    return db.query("SELECT 1 AS present FROM documents WHERE stored_path = ? LIMIT 1").get(storedPath) != null;
  } catch {
    return null;
  }
}


function creditNoteSequenceState(db: Database, issueDate: string) {
  const scope = fiscalYearLabelFromDate(db, issueDate);
  const row = db.query(`SELECT COALESCE(MAX(CAST(substr(invoice_no, -4) AS INTEGER)), 0) AS n FROM documents WHERE document_type = 'credit_note' AND invoice_no GLOB ?`).get(`CN-${scope}-[0-9][0-9][0-9][0-9]`) as { n: number };
  return { scope, currentFloor: Number(row.n ?? 0), sequenceScope: companySequenceScope(db, `CN-${scope}`) };
}

function nextCreditNoteNumber(db: Database, issueDate: string) {
  const { scope, currentFloor, sequenceScope } = creditNoteSequenceState(db, issueDate);
  const nextValue = nextSequenceValue(db, "credit_note", sequenceScope, currentFloor);
  return `CN-${scope}-${String(nextValue).padStart(4, "0")}`;
}

function validateManualCreditNoteNumberScope(db: Database, issueDate: string, creditNoteNumber: string) {
  const { scope } = creditNoteSequenceState(db, issueDate);
  const genericCanonical = /^CN-(\d{4})-(\d{4})$/.exec(creditNoteNumber);
  if (genericCanonical && genericCanonical[1] !== scope) {
    return `manual creditNoteNumber ${creditNoteNumber} does not match current fiscal scope ${scope}`;
  }
  return null;
}

function reserveManualCreditNoteNumber(db: Database, issueDate: string, creditNoteNumber: string) {
  const { scope, currentFloor, sequenceScope } = creditNoteSequenceState(db, issueDate);
  const match = new RegExp(`^CN-${scope.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}-([0-9]{4})$`).exec(creditNoteNumber);
  if (!match) return { ok: true as const };
  const requestedValue = Number(match[1]);
  const reserved = reserveSequenceValue(db, "credit_note", sequenceScope, requestedValue, currentFloor);
  if (!reserved.ok) {
    return { ok: false as const, error: `manual creditNoteNumber ${creditNoteNumber} exceeds næste fortløbende nummer CN-${scope}-${String(reserved.expectedValue).padStart(4, "0")}` };
  }
  return { ok: true as const };
}


function creditNoteLinesFromOriginalJournal(
  db: Database,
  originalInvoiceDocumentId: number,
  originalJournalEntryId: number,
  originalGrossAmount: number,
  cumulativeGrossAmount: number,
  receivableAccountNo: string,
  cumulativeReceivableReliefDkk: number,
) {
  if (!(originalGrossAmount > 0)) return null;

  const originalLines = db.query(
    `SELECT a.account_no, jl.debit_amount, jl.credit_amount, jl.vat_code, jl.text
       FROM journal_lines jl
       JOIN accounts a ON a.id = jl.account_id
      WHERE jl.journal_entry_id = ?
      ORDER BY jl.id ASC`
  ).all(originalJournalEntryId) as Array<{
    account_no: string;
    debit_amount: number;
    credit_amount: number;
    vat_code: string | null;
    text: string | null;
  }>;
  if (originalLines.length === 0) return null;
  const receivableOrigins = originalLines.filter(
    (line) => line.account_no === receivableAccountNo && line.debit_amount > 0 && line.credit_amount === 0,
  );
  if (receivableOrigins.length !== 1 || !(cumulativeReceivableReliefDkk > 0)) return null;

  type Counter = {
    accountNo: string;
    vatCode?: string;
    text?: string;
    originalDkk: number;
    desiredCumulativeDkk: number;
  };
  const counterByKey = new Map<string, Counter>();
  for (const line of originalLines) {
    if (line === receivableOrigins[0]) continue;
    if (!(line.credit_amount > 0) || line.debit_amount !== 0) return null;
    const key = JSON.stringify([line.account_no, line.vat_code]);
    const existing = counterByKey.get(key);
    if (existing) {
      existing.originalDkk = roundDkk(existing.originalDkk + Number(line.credit_amount));
    } else {
      counterByKey.set(key, {
        accountNo: line.account_no,
        vatCode: line.vat_code ?? undefined,
        text: line.text ?? undefined,
        originalDkk: roundDkk(Number(line.credit_amount)),
        desiredCumulativeDkk: 0,
      });
    }
  }
  if (counterByKey.size === 0) return null;

  const isFinalCredit = cumulativeGrossAmount === originalGrossAmount;
  for (const counter of counterByKey.values()) {
    counter.desiredCumulativeDkk = isFinalCredit
      ? counter.originalDkk
      : roundDkk((counter.originalDkk * cumulativeGrossAmount) / originalGrossAmount);
  }
  // Balance the cumulative target, not each note independently. Any one-øre
  // residual lives on the largest revenue line now and is automatically
  // corrected by the next note; a full credit lands exactly on every original
  // revenue/VAT account.
  const desiredDebitOre = [...counterByKey.values()].reduce(
    (sum, counter) => sum + toOre(counter.desiredCumulativeDkk),
    0n,
  );
  const residualOre = desiredDebitOre - toOre(cumulativeReceivableReliefDkk);
  if (residualOre !== 0n) {
    const carrier = [...counterByKey.values()]
      .filter((counter) => counter.vatCode !== undefined)
      .sort((left, right) => right.desiredCumulativeDkk - left.desiredCumulativeDkk)[0];
    if (!carrier) return null;
    const adjusted = fromOre(toOre(carrier.desiredCumulativeDkk) - residualOre);
    if (!(adjusted >= 0)) return null;
    carrier.desiredCumulativeDkk = adjusted;
  }

  const priorRows = db.query(
    `SELECT a.account_no, jl.vat_code,
            SUM(jl.debit_amount) - SUM(jl.credit_amount) AS amount_dkk
       FROM credit_note_postings p
       JOIN journal_lines jl ON jl.journal_entry_id = p.journal_entry_id
       JOIN accounts a ON a.id = jl.account_id
      WHERE p.original_invoice_document_id = ?
        AND a.account_no <> ?
      GROUP BY a.account_no, jl.vat_code`,
  ).all(originalInvoiceDocumentId, receivableAccountNo) as Array<{
    account_no: string;
    vat_code: string | null;
    amount_dkk: number;
  }>;
  const priorByKey = new Map(
    priorRows.map((row) => [JSON.stringify([row.account_no, row.vat_code]), roundDkk(Number(row.amount_dkk))] as const),
  );

  const currentReceivableRelief = roundDkk(
    cumulativeReceivableReliefDkk -
      Number((db.query(
        `SELECT COALESCE(SUM(booked_gross_dkk), 0) AS total
           FROM credit_note_postings
          WHERE original_invoice_document_id = ?`,
      ).get(originalInvoiceDocumentId) as { total: number }).total ?? 0),
  );
  if (!(currentReceivableRelief > 0)) return null;
  const reversedLines: Array<{
    accountNo: string;
    debitAmount?: number;
    creditAmount?: number;
    vatCode?: string;
    text?: string;
  }> = [{
    accountNo: receivableAccountNo,
    creditAmount: currentReceivableRelief,
    text: receivableOrigins[0]!.text ?? undefined,
  }];
  for (const [key, counter] of counterByKey) {
    const current = roundDkk(counter.desiredCumulativeDkk - (priorByKey.get(key) ?? 0));
    if (current < 0) return null;
    if (current === 0) continue;
    reversedLines.push({
      accountNo: counter.accountNo,
      debitAmount: current,
      vatCode: counter.vatCode,
      text: counter.text,
    });
  }
  const debitOre = reversedLines.reduce((sum, line) => sum + toOre(line.debitAmount ?? 0), 0n);
  if (debitOre !== toOre(currentReceivableRelief)) return null;
  return reversedLines;
}

export function issueCreditNote(db: Database, companyRoot: string, input: IssueCreditNoteInput): IssueCreditNoteResult {
  const errors: string[] = [];
  if (!Number.isInteger(input.originalInvoiceDocumentId) || input.originalInvoiceDocumentId <= 0) errors.push("originalInvoiceDocumentId must be a positive integer");
  if (!looksLikeIsoDate(input.issueDate)) errors.push("issueDate must be YYYY-MM-DD");
  if (typeof input.reason !== "string" || input.reason.trim().length === 0) errors.push("reason is required");
  if (errors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors };

  const original = db.query(
    `SELECT id, invoice_no, amount_inc_vat, currency, vat_amount, payload_json, document_type
     FROM documents WHERE id = ?`
  ).get(input.originalInvoiceDocumentId) as any | null;
  if (!original) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice document ${input.originalInvoiceDocumentId} does not exist`] };
  if (original.document_type !== "issued_invoice") return { ok: false, appliedRules: [RULE_ID], errors: [`document ${input.originalInvoiceDocumentId} is not an issued invoice`] };

  const payload = original.payload_json ? JSON.parse(original.payload_json) : null;
  const originalGrossAmount = roundDkk(Number(original.amount_inc_vat ?? payload?.totals?.grossAmount ?? 0));
  const originalVatAmount = roundDkk(Number(original.vat_amount ?? payload?.totals?.vatAmount ?? 0));
  const explicitCreditNoteNumber = input.creditNoteNumber?.trim();
  if (explicitCreditNoteNumber) {
    const scopeError = validateManualCreditNoteNumberScope(db, input.issueDate, explicitCreditNoteNumber);
    if (scopeError) return { ok: false, appliedRules: [RULE_ID], errors: [scopeError] };
  }
  const paths = companyPaths(companyRoot);
  mkdirSync(paths.invoicesIssued, { recursive: true });
  let tempPath: string | undefined;
  let storedPath: string | undefined;
  let storedPathPromoted = false;

  try {
    const result = db.transaction(() => {
      // Booking/reversal state and the cumulative credit cap are mutable. Read
      // and validate them only after BEGIN IMMEDIATE so concurrent issuers
      // cannot both consume the same remaining invoice amount.
      const originalBooking = resolveInvoiceReceivableAccount(db, {
        invoiceDocumentId: original.id,
      });
      if (!originalBooking.ok) {
        return {
          ok: false as const,
          error: `invoice ${original.invoice_no} must have explicit active invoice-posting evidence before a credit note can be issued: ${originalBooking.error}`,
        };
      }
      const priorEvidence = validateInvoiceCreditNoteEvidence(db, {
        invoiceDocumentId: original.id,
      });
      if (!priorEvidence.ok) {
        return { ok: false as const, error: priorEvidence.errors.join("; ") };
      }
      const creditedTotals = db.query(
        `SELECT COALESCE(SUM(c.amount_inc_vat), 0) AS total,
                COALESCE(SUM(c.vat_amount), 0) AS vat_total
           FROM documents c
           JOIN credit_note_postings p ON p.credit_note_document_id = c.id
          WHERE c.document_type = 'credit_note'
            AND p.original_invoice_document_id = ?`,
      ).get(original.id) as { total: number; vat_total: number };
      const creditedSoFar = roundDkk(Number(creditedTotals.total ?? 0));
      const creditedVatSoFar = roundDkk(Number(creditedTotals.vat_total ?? 0));
      const remainingGrossAmount = roundDkk(originalGrossAmount - creditedSoFar);
      if (remainingGrossAmount <= 0) {
        return { ok: false as const, error: `invoice ${original.invoice_no} is already fully credited` };
      }
      const grossAmount = roundDkk(input.grossAmount ?? remainingGrossAmount);
      if (!Number.isFinite(grossAmount) || grossAmount <= 0) {
        return { ok: false as const, error: "grossAmount must be a positive number when present" };
      }
      if (grossAmount > remainingGrossAmount) {
        return { ok: false as const, error: `credit amount ${grossAmount} exceeds remaining creditable amount ${remainingGrossAmount}` };
      }

      const creditedDkkSoFar = roundDkk(Number((db.query(
        `SELECT COALESCE(SUM(booked_gross_dkk), 0) AS total
           FROM credit_note_postings
          WHERE original_invoice_document_id = ?`,
      ).get(original.id) as { total: number }).total ?? 0));
      const cumulativeForeignCredit = roundDkk(creditedSoFar + grossAmount);
      const cumulativeVatCredit = cumulativeForeignCredit === originalGrossAmount
        ? originalVatAmount
        : roundDkk((originalVatAmount * cumulativeForeignCredit) / originalGrossAmount);
      const vatAmount = roundDkk(cumulativeVatCredit - creditedVatSoFar);
      const netAmount = roundDkk(grossAmount - vatAmount);
      const cumulativeDkkRelief = cumulativeForeignCredit === originalGrossAmount
        ? originalBooking.bookedGrossDkk
        : roundDkk((originalBooking.bookedGrossDkk * cumulativeForeignCredit) / originalGrossAmount);
      const bookedCreditDkk = roundDkk(cumulativeDkkRelief - creditedDkkSoFar);
      if (!(bookedCreditDkk > 0)) {
        return { ok: false as const, error: `invoice ${original.invoice_no} credit note has no positive remaining DKK receivable relief` };
      }
      const originalJournalLines = creditNoteLinesFromOriginalJournal(
        db,
        original.id,
        originalBooking.bookingJournalEntryId,
        originalGrossAmount,
        cumulativeForeignCredit,
        originalBooking.accountNo,
        cumulativeDkkRelief,
      );
      if (!originalJournalLines) {
        return { ok: false as const, error: `invoice ${original.invoice_no} booking cannot be reversed into an exact credit-note journal` };
      }
      const actualBookedCreditDkk = roundDkk(originalJournalLines.reduce((sum, line) => {
        if (line.accountNo !== originalBooking.accountNo) return sum;
        return sum + Number(line.creditAmount ?? 0) - Number(line.debitAmount ?? 0);
      }, 0));
      if (actualBookedCreditDkk !== bookedCreditDkk) {
        return { ok: false as const, error: `invoice ${original.invoice_no} credit note does not reduce its booked receivable account` };
      }

      let creditNoteNumber = explicitCreditNoteNumber;
      if (creditNoteNumber) {
        const reserved = reserveManualCreditNoteNumber(db, input.issueDate, creditNoteNumber);
        if (!reserved.ok) return { ok: false as const, error: reserved.error };
      } else {
        creditNoteNumber = nextCreditNoteNumber(db, input.issueDate);
      }

      const creditPayload = {
        type: "credit_note",
        creditNoteNumber,
        originalInvoiceNumber: original.invoice_no,
        originalInvoiceDocumentId: original.id,
        issueDate: input.issueDate,
        reason: input.reason.trim(),
        grossAmount,
        vatAmount,
        netAmount,
        creditedSoFar,
        remainingAfterThisCredit: roundDkk(remainingGrossAmount - grossAmount),
        issuedAt: new Date().toISOString(),
      };
      const serialized = JSON.stringify(creditPayload, null, 2);
      const hash = sha256(serialized);
      storedPath = join(paths.invoicesIssued, `${creditNoteNumber}.json`);
      tempPath = writeTempFileFor(storedPath, serialized);

      const doc = db.query(
        `INSERT INTO documents (
          document_no, source, original_filename, stored_path, mime_type, sha256_hash,
          supplier_name, invoice_no, invoice_date, amount_inc_vat, currency, status,
          document_type, delivery_description, sender_name, sender_address, sender_vat_cvr,
          recipient_name, recipient_address, recipient_vat_cvr, vat_amount, payment_details, exemption_code, payload_json, retain_until
        ) VALUES (?, 'rentemester', ?, ?, 'application/json', ?, ?, ?, ?, ?, ?, 'issued', 'credit_note', ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?)
        RETURNING id`
      ).get(
        creditNoteNumber,
        `${creditNoteNumber}.json`,
        storedPath,
        hash,
        payload?.seller?.name ?? null,
        creditNoteNumber,
        input.issueDate,
        grossAmount,
        original.currency ?? 'DKK',
        `Credit note for ${original.invoice_no}: ${input.reason.trim()}`,
        payload?.seller?.name ?? null,
        payload?.seller?.address ?? null,
        payload?.seller?.vatOrCvr ?? null,
        payload?.buyer?.name ?? null,
        payload?.buyer?.address ?? null,
        payload?.buyer?.vatOrCvr ?? null,
        vatAmount,
        original.invoice_no,
        serialized,
        retainUntilForDate(db, input.issueDate),
      ) as { id: number };

      strengthenGdprErasureAliasesForIdentity(db, {
        name: payload?.seller?.name,
        cvr: payload?.seller?.vatOrCvr,
      });
      strengthenGdprErasureAliasesForIdentity(db, {
        name: payload?.buyer?.name,
        cvr: payload?.buyer?.vatOrCvr,
      });

      const journal = postJournalEntry(db, {
        transactionDate: input.issueDate,
        text: `Credit note ${creditNoteNumber} for invoice ${original.invoice_no}`,
        documentId: doc.id,
        currency: (original.currency ?? "DKK").trim().toUpperCase() === "DKK"
          ? undefined
          : (original.currency ?? "DKK").trim().toUpperCase(),
        amountForeign: (original.currency ?? "DKK").trim().toUpperCase() === "DKK"
          ? undefined
          : grossAmount,
        amountDkk: (original.currency ?? "DKK").trim().toUpperCase() === "DKK"
          ? undefined
          : bookedCreditDkk,
        fxRateToDkk: (original.currency ?? "DKK").trim().toUpperCase() === "DKK"
          ? undefined
          : bookedCreditDkk / grossAmount,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: originalJournalLines,
      });
      if (!journal.ok) throw new TransactionRejectionError({ appliedRules: journal.appliedRules, errors: journal.errors });

      const receivableAccount = db.query(
        "SELECT id FROM accounts WHERE account_no = ?",
      ).get(originalBooking.accountNo) as { id: number } | null;
      if (!receivableAccount || journal.entryId == null) {
        throw new Error(`credit note ${creditNoteNumber} cannot persist its receivable posting evidence`);
      }
      runSql(db,
        `INSERT INTO credit_note_postings
           (credit_note_document_id, original_invoice_document_id, journal_entry_id,
            receivable_account_id, booked_gross_dkk)
         VALUES (?, ?, ?, ?, ?)`,
        doc.id,
        original.id,
        journal.entryId,
        receivableAccount.id,
        bookedCreditDkk,
      );
      const evidence = validateInvoiceCreditNoteEvidence(db, {
        invoiceDocumentId: original.id,
      });
      if (!evidence.ok) {
        throw new TransactionRejectionError({ appliedRules: [RULE_ID], errors: evidence.errors });
      }

      insertAuditLog(db, {
        eventType: "credit_note_issue",
        entityType: "document",
        entityId: doc.id,
        message: `Issued credit note ${creditNoteNumber} for ${original.invoice_no}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      // Keep the legal snapshot and its financial evidence atomic from the
      // caller's perspective: a destination/promotion failure aborts the DB
      // transaction, while a later COMMIT failure removes the published file.
      promoteTempFileExclusive(tempPath!, storedPath!);
      storedPathPromoted = true;

      const treatment = payload?.vatTreatment ?? "standard";
      return { ok: true as const, docId: doc.id, creditNoteNumber, sha256: hash, journal, isReverseCharge: treatment === "domestic_reverse_charge" || treatment === "foreign_reverse_charge" };
    }).immediate();

    if (!result.ok) return { ok: false, appliedRules: [RULE_ID], errors: [result.error] };
    return {
      ok: true,
      documentId: result.docId,
      creditNoteNumber: result.creditNoteNumber,
      originalInvoiceNumber: original.invoice_no,
      storedPath,
      sha256: result.sha256,
      journalEntryId: result.journal.entryId,
      journalEntryNo: result.journal.entryNo,
      appliedRules: [...new Set([RULE_ID, ...(result.journal.appliedRules ?? []), ...(result.isReverseCharge ? [REVERSE_RULE_ID] : [])])],
      errors: [],
    };
  } catch (error) {
    if (tempPath) removeIfExists(tempPath);
    if (
      storedPathPromoted &&
      storedPath &&
      hasCommittedDocumentAtPath(db, storedPath) === false
    ) removeIfExists(storedPath);
    const parsed = decodeTransactionRejection(error);
    return {
      ok: false,
      appliedRules: [...new Set([RULE_ID, ...(parsed?.appliedRules ?? [])])],
      errors: parsed?.errors ?? [String(error)],
    };
  }
}
