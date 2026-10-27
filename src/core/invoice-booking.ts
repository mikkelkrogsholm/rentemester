import { runSql } from "./sqlite";
import type { Database } from "bun:sqlite";
import {
  postJournalEntry,
  reverseUnlinkedIssuedInvoiceJournalAfterReplacement,
  type JournalPostResult,
} from "./ledger";
import { addDkk, equalsDkk, roundDkk } from "./money";
import { resolveAccountRole } from "./account-roles";
import { projectVatLines } from "./vat-lines";
import { resolveInvoiceReceivableAccount } from "./invoice-fx-receivable";
import { validateLegacyInvoiceRepairEvidence } from "./invoice-legacy-repair-evidence";
import { validateInvoiceJournalEvidence } from "./invoice-journal-evidence";
import { validateJournalTransactionDate } from "./periods";
import { insertAuditLog } from "./actor";

const RULE_ID = "DK-INVOICE-BOOKKEEPING-001";
const REVERSE_RULE_ID = "DK-INVOICE-BOOKKEEPING-REVERSE-002";

export type PostIssuedInvoiceInput = {
  invoiceDocumentId: number;
  transactionDate?: string;
  receivableAccountNo?: string;
  revenueAccountNo?: string;
  outputVatAccountNo?: string;
  createdBy?: string;
  createdByProgram?: string;
};


function issuedInvoiceJournalLines(doc: { invoice_no: string }, payload: any, grossAmount: number, netAmount: number, vatAmount: number, input: PostIssuedInvoiceInput) {
  if (!input.receivableAccountNo || !input.outputVatAccountNo) throw new Error("resolved debtors and output VAT account roles are required");
  const vatTreatment = payload?.vatTreatment ?? "standard";
  const isDomesticReverseCharge = vatTreatment === "domestic_reverse_charge";
  const isForeignReverseCharge = vatTreatment === "foreign_reverse_charge";
  const isReverseCharge = isDomesticReverseCharge || isForeignReverseCharge;
  // JUR-2/KODE-2: domestic and foreign reverse charge are both VAT-exempt
  // sales but file in DIFFERENT momsangivelse rubrikker. Foreign (EU B2B,
  // momsloven §46 / momsdirektivet art. 196/199) → rubrik B + EU sales list
  // (VIES). Domestic §46 (mobiltelefoner, CPU'er, metalskrot) → rubrik C
  // ("værdi af andet salg uden moms"), and must stay OFF the VIES list. They
  // therefore carry distinct vat codes so buildVatReport can split the bases.
  // Source: SKAT Den juridiske vejledning A.B.3.3.1.5; rubrik C confirmed for
  // domestic §46 reverse charge.
  const reverseChargeVatCode = isDomesticReverseCharge ? "DOMESTIC_REVERSE_CHARGE_EXEMPT" : "REVERSE_CHARGE_EXEMPT";
  const lines: Array<{ accountNo: string; debitAmount?: number; creditAmount?: number; vatCode?: string; text: string }> = [
    { accountNo: input.receivableAccountNo, debitAmount: grossAmount, text: `Receivable ${doc.invoice_no}` },
  ];
  const projection = projectVatLines(payload?.lines, vatTreatment, payload?.totals?.vatRate);
  // Validation at issuance makes this fail-closed check defensive for legacy
  // or directly inserted document rows.
  if (!projection.ok) throw new Error(`invoice ${doc.invoice_no} has inconsistent explicit VAT lines`);
  // Posted amounts are DKK for foreign invoices. Preserve the explicit line
  // split by scaling its bases to the already validated DKK net total.
  const baseScale = projection.netAmount > 0 ? netAmount / projection.netAmount : 1;
  for (const taxLine of projection.lines) {
    const vatCode = taxLine.taxClassification === "taxable"
      ? "DK_SALE_25"
      : taxLine.taxClassification === "reverse_charge"
        ? reverseChargeVatCode
        : "DK_SALE_EXEMPT";
    lines.push({ accountNo: input.revenueAccountNo ?? "1000", creditAmount: roundDkk(taxLine.vatBase * baseScale), vatCode, text: `Revenue ${doc.invoice_no} (${taxLine.taxClassification})` });
  }
  // Per-line FX rounding can leave one øre between the explicit bases and the
  // authoritative DKK net total. Keep the receivable/VAT totals immutable and
  // put that deterministic residual on the final revenue line.
  const revenueLines = lines.filter((line) => line.creditAmount !== undefined && line.vatCode !== undefined);
  const roundedBases = roundDkk(revenueLines.reduce((sum, line) => sum + Number(line.creditAmount), 0));
  if (revenueLines.length > 0 && roundedBases !== netAmount) {
    const last = revenueLines[revenueLines.length - 1];
    last.creditAmount = roundDkk(Number(last.creditAmount) + netAmount - roundedBases);
  }
  if (vatAmount > 0) {
    lines.push({ accountNo: input.outputVatAccountNo, creditAmount: vatAmount, text: `Output VAT ${doc.invoice_no}` });
  }
  return { lines, isReverseCharge: isReverseCharge || projection.lines.some((line) => line.taxClassification === "reverse_charge") };
}

function postIssuedInvoiceToLedgerInternal(
  db: Database,
  input: PostIssuedInvoiceInput,
  ignoredLegacyJournalEntryId?: number,
): JournalPostResult {
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["invoiceDocumentId must be a positive integer"] };
  }
  const debtors = input.receivableAccountNo ? { ok: true as const, accountNo: input.receivableAccountNo } : resolveAccountRole(db, "debtors");
  const outputVat = input.outputVatAccountNo ? { ok: true as const, accountNo: input.outputVatAccountNo } : resolveAccountRole(db, "output_vat");
  if (!debtors.ok) return { ok: false, appliedRules: [RULE_ID], errors: [debtors.error] };
  if (!outputVat.ok) return { ok: false, appliedRules: [RULE_ID], errors: [outputVat.error] };

  const doc = db.query(
    `SELECT id, invoice_no, invoice_date, amount_inc_vat, currency, vat_amount, payload_json, document_type
     FROM documents WHERE id = ?`
  ).get(input.invoiceDocumentId) as {
    id: number;
    invoice_no: string;
    invoice_date: string | null;
    amount_inc_vat: number | null;
    currency: string;
    vat_amount: number | null;
    payload_json: string | null;
    document_type: string;
  } | null;

  if (!doc) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice document ${input.invoiceDocumentId} does not exist`] };
  if (doc.document_type !== "issued_invoice") return { ok: false, appliedRules: [RULE_ID], errors: [`document ${input.invoiceDocumentId} is not an issued invoice`] };
  if (input.transactionDate && input.transactionDate !== doc.invoice_date) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`invoice booking date ${input.transactionDate} must match invoice date ${doc.invoice_date}`] };
  }

  const existing = db.query(
    `SELECT j.id, j.entry_no
       FROM issued_invoice_postings p
       JOIN journal_entries j ON j.id = p.journal_entry_id
      WHERE p.invoice_document_id = ?`,
  ).get(input.invoiceDocumentId) as { id: number; entry_no: string } | null;
  if (existing) {
    // Issue #385: lead with a friendly Danish sentence so the cockpit
    // owner and the CLI user both see plain Danish instead of an internal
    // English phrase. The English suffix is kept verbatim because the
    // conflict heuristic in `withCompanyMutation` and several core tests
    // match on "already has journal entry".
    return {
      ok: false,
      appliedRules: [RULE_ID],
      errors: [
        `Faktura ${doc.invoice_no} er allerede bogført som postering ${existing.entry_no} og kan ikke bogføres igen. (invoice ${doc.invoice_no} already has journal entry ${existing.entry_no})`,
      ],
    };
  }
  const unresolvedLegacy = db.query(
    `SELECT j.id, j.entry_no
       FROM journal_entries j
      WHERE j.document_id = ?
        AND j.status = 'posted'
        AND j.reversal_of_entry_id IS NULL
        AND NOT EXISTS (
          SELECT 1 FROM journal_entries reversal
           WHERE reversal.reversal_of_entry_id = j.id
        )
        AND NOT EXISTS (SELECT 1 FROM issued_invoice_postings p WHERE p.journal_entry_id = j.id)
        AND NOT EXISTS (SELECT 1 FROM invoice_payments p WHERE p.journal_entry_id = j.id)
        AND NOT EXISTS (SELECT 1 FROM invoice_refunds r WHERE r.journal_entry_id = j.id)
        AND NOT EXISTS (SELECT 1 FROM invoice_claim_payments p WHERE p.journal_entry_id = j.id)
        AND NOT EXISTS (SELECT 1 FROM invoice_bad_debt_writeoffs w WHERE w.journal_entry_id = j.id)
        AND NOT EXISTS (SELECT 1 FROM invoice_interest_corrections c WHERE c.journal_entry_id = j.id)
        AND NOT EXISTS (SELECT 1 FROM invoice_reminder_postings p WHERE p.journal_entry_id = j.id)
        AND NOT EXISTS (SELECT 1 FROM invoice_compensation_postings p WHERE p.journal_entry_id = j.id)
        AND NOT EXISTS (SELECT 1 FROM invoice_interest_postings p WHERE p.journal_entry_id = j.id)
        AND (? IS NULL OR j.id <> ?)
      ORDER BY j.id ASC
      LIMIT 1`,
  ).get(
    input.invoiceDocumentId,
    ignoredLegacyJournalEntryId ?? null,
    ignoredLegacyJournalEntryId ?? null,
  ) as { id: number; entry_no: string } | null;
  if (unresolvedLegacy) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      errors: [
        `invoice ${doc.invoice_no} has unresolved legacy journal ${unresolvedLegacy.entry_no} without an explicit issued-invoice posting link; refusing to guess or post a duplicate`,
      ],
    };
  }

  const payload = doc.payload_json ? JSON.parse(doc.payload_json) : null;
  const currency = (doc.currency ?? payload?.currency ?? "DKK").trim().toUpperCase();
  const grossAmount = roundDkk(Number(doc.amount_inc_vat ?? payload?.totals?.grossAmount ?? 0));
  const vatAmount = roundDkk(Number(doc.vat_amount ?? payload?.totals?.vatAmount ?? 0));
  const netAmount = roundDkk(grossAmount - vatAmount);
  const fxRateToDkk = payload?.totals?.fxRateToDkk == null ? null : Number(payload.totals.fxRateToDkk);
  const grossAmountDkk = currency === "DKK"
    ? grossAmount
    : roundDkk(Number(payload?.totals?.grossAmountDkk ?? 0));
  const vatAmountDkk = currency === "DKK"
    ? vatAmount
    : roundDkk(Number(payload?.totals?.vatAmountDkk ?? 0));
  const netAmountDkk = currency === "DKK"
    ? netAmount
    : roundDkk(Number(payload?.totals?.netAmountDkk ?? 0));

  if (!(grossAmount > 0)) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${doc.invoice_no} is missing gross amount`] };
  if (netAmount <= 0) return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${doc.invoice_no} produced invalid net amount`] };
  if (currency !== "DKK" && !(grossAmountDkk > 0 && netAmountDkk > 0 && Number.isFinite(fxRateToDkk) && fxRateToDkk! > 0)) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${doc.invoice_no} is missing deterministic DKK conversion totals`] };
  }

  // Cross-check the amounts that are actually posted: the receivable line is
  // gross while the revenue + output-VAT lines sum to net + vat. If a divergent
  // payload makes those disagree the journal would be unbalanced in DKK, so
  // fail loudly here rather than relying on the ledger balance check.
  if (!equalsDkk(addDkk(netAmount, vatAmount), grossAmount)) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${doc.invoice_no} totals are inconsistent: net + vat (${addDkk(netAmount, vatAmount)}) does not equal gross (${grossAmount})`] };
  }
  if (currency !== "DKK" && !equalsDkk(addDkk(netAmountDkk, vatAmountDkk), grossAmountDkk)) {
    return { ok: false, appliedRules: [RULE_ID], errors: [`invoice ${doc.invoice_no} DKK totals are inconsistent: netDkk + vatDkk (${addDkk(netAmountDkk, vatAmountDkk)}) does not equal grossDkk (${grossAmountDkk})`] };
  }

  let posting: ReturnType<typeof issuedInvoiceJournalLines>;
  try {
    posting = issuedInvoiceJournalLines(doc, payload, grossAmountDkk, netAmountDkk, vatAmountDkk, { ...input, receivableAccountNo: debtors.accountNo, outputVatAccountNo: outputVat.accountNo });
  } catch (error) {
    return { ok: false, appliedRules: [RULE_ID], errors: [String(error)] };
  }
  try {
    return db.transaction(() => {
      const lockedDebtors = input.receivableAccountNo
        ? { ok: true as const, accountNo: input.receivableAccountNo }
        : resolveAccountRole(db, "debtors");
      const lockedOutputVat = input.outputVatAccountNo
        ? { ok: true as const, accountNo: input.outputVatAccountNo }
        : resolveAccountRole(db, "output_vat");
      if (!lockedDebtors.ok) {
        return {
          ok: false,
          appliedRules: [RULE_ID],
          errors: [lockedDebtors.error],
        } satisfies JournalPostResult;
      }
      if (!lockedOutputVat.ok) return {
        ok: false,
        appliedRules: [RULE_ID],
        errors: [lockedOutputVat.error],
      } satisfies JournalPostResult;
      let lockedPosting: ReturnType<typeof issuedInvoiceJournalLines>;
      try {
        lockedPosting = issuedInvoiceJournalLines(doc, payload, grossAmountDkk, netAmountDkk, vatAmountDkk, {
          ...input,
          receivableAccountNo: lockedDebtors.accountNo,
          outputVatAccountNo: lockedOutputVat.accountNo,
        });
      } catch (error) {
        return { ok: false, appliedRules: [RULE_ID], errors: [String(error)] } satisfies JournalPostResult;
      }
      const linked = db.query(
        `SELECT j.entry_no
           FROM issued_invoice_postings p
           JOIN journal_entries j ON j.id = p.journal_entry_id
          WHERE p.invoice_document_id = ?`,
      ).get(input.invoiceDocumentId) as { entry_no: string } | null;
      if (linked) {
        return {
          ok: false,
          appliedRules: [RULE_ID],
          errors: [
            `Faktura ${doc.invoice_no} er allerede bogført som postering ${linked.entry_no} og kan ikke bogføres igen. (invoice ${doc.invoice_no} already has journal entry ${linked.entry_no})`,
          ],
        } satisfies JournalPostResult;
      }
      const unresolved = db.query(
        `SELECT j.entry_no
           FROM journal_entries j
          WHERE j.document_id = ?
            AND j.status = 'posted'
            AND j.reversal_of_entry_id IS NULL
            AND NOT EXISTS (
              SELECT 1 FROM journal_entries reversal
               WHERE reversal.reversal_of_entry_id = j.id
            )
            AND NOT EXISTS (SELECT 1 FROM issued_invoice_postings p WHERE p.journal_entry_id = j.id)
            AND NOT EXISTS (SELECT 1 FROM invoice_payments p WHERE p.journal_entry_id = j.id)
            AND NOT EXISTS (SELECT 1 FROM invoice_refunds r WHERE r.journal_entry_id = j.id)
            AND NOT EXISTS (SELECT 1 FROM invoice_claim_payments p WHERE p.journal_entry_id = j.id)
            AND NOT EXISTS (SELECT 1 FROM invoice_bad_debt_writeoffs w WHERE w.journal_entry_id = j.id)
            AND NOT EXISTS (SELECT 1 FROM invoice_interest_corrections c WHERE c.journal_entry_id = j.id)
            AND NOT EXISTS (SELECT 1 FROM invoice_reminder_postings p WHERE p.journal_entry_id = j.id)
            AND NOT EXISTS (SELECT 1 FROM invoice_compensation_postings p WHERE p.journal_entry_id = j.id)
            AND NOT EXISTS (SELECT 1 FROM invoice_interest_postings p WHERE p.journal_entry_id = j.id)
            AND (? IS NULL OR j.id <> ?)
          ORDER BY j.id ASC
          LIMIT 1`,
      ).get(
        input.invoiceDocumentId,
        ignoredLegacyJournalEntryId ?? null,
        ignoredLegacyJournalEntryId ?? null,
      ) as { entry_no: string } | null;
      if (unresolved) {
        return {
          ok: false,
          appliedRules: [RULE_ID],
          errors: [`invoice ${doc.invoice_no} has unresolved legacy journal ${unresolved.entry_no} without an explicit issued-invoice posting link; refusing to guess or post a duplicate`],
        } satisfies JournalPostResult;
      }

      const journal = postJournalEntry(db, {
        transactionDate: input.transactionDate ?? doc.invoice_date ?? payload?.issueDate,
        text: `Faktura ${doc.invoice_no} udstedt`,
        documentId: input.invoiceDocumentId,
        currency: currency === "DKK" ? undefined : currency,
        amountForeign: currency === "DKK" ? undefined : grossAmount,
        amountDkk: currency === "DKK" ? undefined : grossAmountDkk,
        fxRateToDkk: currency === "DKK" ? undefined : fxRateToDkk ?? undefined,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: lockedPosting.lines,
      });
      if (!journal.ok || journal.entryId == null) return {
        ...journal,
        appliedRules: [...new Set([...(journal.appliedRules ?? []), RULE_ID, ...(lockedPosting.isReverseCharge ? [REVERSE_RULE_ID] : [])])],
      };

      const receivableAccount = db.query(
        "SELECT id FROM accounts WHERE account_no = ?",
      ).get(lockedDebtors.accountNo) as { id: number } | null;
      if (!receivableAccount) throw new Error(`resolved receivable account ${lockedDebtors.accountNo} disappeared before invoice posting`);
      runSql(db,
        `INSERT INTO issued_invoice_postings
           (invoice_document_id, journal_entry_id, receivable_account_id, booked_gross_dkk)
         VALUES (?, ?, ?, ?)`,
        input.invoiceDocumentId,
        journal.entryId,
        receivableAccount.id,
        grossAmountDkk,
      );

      const evidence = resolveInvoiceReceivableAccount(db, {
        invoiceDocumentId: input.invoiceDocumentId,
      });
      if (!evidence.ok) throw new Error(evidence.error);
      return {
        ...journal,
        appliedRules: [...new Set([...(journal.appliedRules ?? []), RULE_ID, ...(lockedPosting.isReverseCharge ? [REVERSE_RULE_ID] : [])])],
      };
    }).immediate();
  } catch (error) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      errors: [String(error)],
    };
  }
}

export function postIssuedInvoiceToLedger(db: Database, input: PostIssuedInvoiceInput): JournalPostResult {
  return postIssuedInvoiceToLedgerInternal(db, input);
}

export type RepairUnlinkedIssuedInvoiceBookingInput = PostIssuedInvoiceInput & {
  legacyJournalEntryId: number;
  reason: string;
};

export type RepairUnlinkedIssuedInvoiceBookingResult = JournalPostResult & {
  invoiceDocumentId?: number;
  legacyJournalEntryId?: number;
  replacementJournalEntryId?: number;
  replacementJournalEntryNo?: string;
  reversalJournalEntryId?: number;
  reversalJournalEntryNo?: string;
};

class InvoiceBookingRepairFailure extends Error {
  constructor(readonly repairErrors: string[], readonly appliedRules: string[] = [RULE_ID]) {
    super(repairErrors.join("; "));
  }
}

/**
 * Atomically supersede one explicitly named, dependency-free legacy invoice
 * journal with the canonical booking. The old journal is reversed on its own
 * immutable date and the replacement is posted on the invoice date, so every
 * accounting period retains the correct net effect. Closed periods must be
 * explicitly reopened by an authorised operator before this repair can run.
 */
export function repairUnlinkedIssuedInvoiceBooking(
  db: Database,
  input: RepairUnlinkedIssuedInvoiceBookingInput,
): RepairUnlinkedIssuedInvoiceBookingResult {
  const inputErrors: string[] = [];
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    inputErrors.push("invoiceDocumentId must be a positive integer");
  }
  if (!Number.isInteger(input.legacyJournalEntryId) || input.legacyJournalEntryId <= 0) {
    inputErrors.push("legacyJournalEntryId must be a positive integer");
  }
  if (typeof input.reason !== "string" || input.reason.trim().length === 0) {
    inputErrors.push("reason is required");
  }
  if (inputErrors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors: inputErrors };

  try {
    return db.transaction(() => {
      const before = validateLegacyInvoiceRepairEvidence(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        legacyJournalEntryId: input.legacyJournalEntryId,
      });
      if (!before.ok) throw new InvoiceBookingRepairFailure(before.errors);

      const periodErrors = [
        ...validateJournalTransactionDate(db, before.state.invoiceDate)
          .map((error) => `canonical invoice date ${before.state.invoiceDate}: ${error}`),
        ...validateJournalTransactionDate(db, before.state.legacyTransactionDate)
          .map((error) => `legacy journal date ${before.state.legacyTransactionDate}: ${error}`),
      ];
      if (periodErrors.length > 0) {
        throw new InvoiceBookingRepairFailure([...new Set(periodErrors)]);
      }

      const replacement = postIssuedInvoiceToLedgerInternal(
        db,
        {
          invoiceDocumentId: input.invoiceDocumentId,
          receivableAccountNo: input.receivableAccountNo,
          revenueAccountNo: input.revenueAccountNo,
          outputVatAccountNo: input.outputVatAccountNo,
          createdBy: input.createdBy,
          createdByProgram: input.createdByProgram,
        },
        input.legacyJournalEntryId,
      );
      if (!replacement.ok || replacement.entryId == null || !replacement.entryNo) {
        throw new InvoiceBookingRepairFailure(replacement.errors, replacement.appliedRules);
      }

      const transition = validateLegacyInvoiceRepairEvidence(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        legacyJournalEntryId: input.legacyJournalEntryId,
        replacementJournalEntryId: replacement.entryId,
      });
      if (!transition.ok) {
        throw new InvoiceBookingRepairFailure(transition.errors, replacement.appliedRules);
      }

      const reversal = reverseUnlinkedIssuedInvoiceJournalAfterReplacement(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        legacyJournalEntryId: input.legacyJournalEntryId,
        replacementJournalEntryId: replacement.entryId,
        reason: input.reason.trim(),
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });
      if (!reversal.ok || reversal.entryId == null || !reversal.entryNo) {
        throw new InvoiceBookingRepairFailure(
          reversal.errors,
          [...new Set([...replacement.appliedRules, ...reversal.appliedRules])],
        );
      }

      const finalState = db.query(
        `SELECT p.journal_entry_id AS replacement_journal_entry_id,
                (SELECT r.id FROM journal_entries r
                  WHERE r.reversal_of_entry_id = ?
                  ORDER BY r.id ASC LIMIT 1) AS reversal_journal_entry_id
           FROM issued_invoice_postings p
          WHERE p.invoice_document_id = ?`,
      ).get(input.legacyJournalEntryId, input.invoiceDocumentId) as {
        replacement_journal_entry_id: number;
        reversal_journal_entry_id: number | null;
      } | null;
      if (
        !finalState ||
        finalState.replacement_journal_entry_id !== replacement.entryId ||
        finalState.reversal_journal_entry_id !== reversal.entryId
      ) {
        throw new InvoiceBookingRepairFailure(["legacy invoice repair did not reach one linked replacement plus one exact reversal"]);
      }
      const booking = resolveInvoiceReceivableAccount(db, {
        invoiceDocumentId: input.invoiceDocumentId,
      });
      if (!booking.ok) throw new InvoiceBookingRepairFailure([booking.error]);
      const evidence = validateInvoiceJournalEvidence(db, {
        invoiceDocumentId: input.invoiceDocumentId,
      });
      if (!evidence.ok) throw new InvoiceBookingRepairFailure(evidence.errors);

      insertAuditLog(db, {
        eventType: "invoice_booking_repair",
        entityType: "document",
        entityId: input.invoiceDocumentId,
        message: `Repaired invoice ${before.state.invoiceNumber}: legacy journal ${before.state.legacyEntryNo} (${input.legacyJournalEntryId}) was reversed by ${reversal.entryNo} (${reversal.entryId}) and superseded by ${replacement.entryNo} (${replacement.entryId}); reason: ${input.reason.trim()}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      return {
        ok: true,
        invoiceDocumentId: input.invoiceDocumentId,
        legacyJournalEntryId: input.legacyJournalEntryId,
        replacementJournalEntryId: replacement.entryId,
        replacementJournalEntryNo: replacement.entryNo,
        reversalJournalEntryId: reversal.entryId,
        reversalJournalEntryNo: reversal.entryNo,
        entryId: replacement.entryId,
        entryNo: replacement.entryNo,
        entryHash: replacement.entryHash,
        appliedRules: [...new Set([...replacement.appliedRules, ...reversal.appliedRules, RULE_ID])],
        errors: [],
      };
    }).immediate();
  } catch (error) {
    if (error instanceof InvoiceBookingRepairFailure) {
      return {
        ok: false,
        invoiceDocumentId: input.invoiceDocumentId,
        legacyJournalEntryId: input.legacyJournalEntryId,
        appliedRules: error.appliedRules,
        errors: error.repairErrors,
      };
    }
    return {
      ok: false,
      invoiceDocumentId: input.invoiceDocumentId,
      legacyJournalEntryId: input.legacyJournalEntryId,
      appliedRules: [RULE_ID],
      errors: [String(error)],
    };
  }
}
