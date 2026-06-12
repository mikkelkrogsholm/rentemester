import type { Database } from "bun:sqlite";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { requireCachedViesValidation, normalizeEuVatNumber } from "./vies";
import { addDkk, compareDkk, fromOre, percentOfDkk, roundDkk, subtractDkk, sumDkk, toOre } from "./money";

/** Absolute difference between two DKK amounts, expressed in whole øre. */
function oreDifference(left: number, right: number): number {
  const delta = toOre(left) - toOre(right);
  return Number(delta < 0n ? -delta : delta);
}

export type VatPeriodReport = {
  ok: boolean;
  appliedRules: string[];
  periodStart: string;
  periodEnd: string;
  outputVat: number;
  inputVat: number;
  netVatPayable: number;
  purchaseBase25: number;
  salesBase25: number;
  reverseChargeSalesBase: number;
  reverseChargePurchaseBase: number;
  representationPurchaseBase: number;
  badDebtReliefBase25: number;
  /**
   * Value of VAT-exempt domestic sales (momsloven §13 — vat_code
   * DK_SALE_EXEMPT). These carry no output VAT and feed rubrik C of the
   * momsangivelse; they are kept out of the standard 25% sales base.
   */
  exemptSalesBase: number;
  /**
   * Value of digital-service sales to EU consumers handled under the OSS
   * scheme (vat_code OSS_EU_CONSUMER). These carry no Danish output VAT and
   * are reported via a separate OSS return — they are kept out of every
   * standard momsangivelse rubrik. See src/core/vat-oss.ts.
   */
  ossConsumerSalesBase: number;
  /** Number of journal entries that include an OSS_EU_CONSUMER line. */
  ossConsumerSalesEntryCount: number;
  journalEntryCount: number;
  reversedJournalEntryCount: number;
  reversalJournalEntryCount: number;
  totalJournalEntryCount: number;
  linesConsidered: number;
  reversedLinesConsidered: number;
  reversalLinesConsidered: number;
  totalLinesConsidered: number;
  warnings: string[];
  errors: string[];
};

const RULE_ID = "DK-VAT-REPORT-001";
const REVERSE_CHARGE_RULE_ID = "DK-VAT-REVERSE-CHARGE-001";
const REPRESENTATION_RULE_ID = "DK-VAT-REPRESENTATION-001";

/**
 * SKAT filing/payment deadline for a VAT period (#236).
 *
 * For quarterly VAT (the only cadence Rentemester supports) the momsangivelse
 * must be filed and the moms paid by the 1st day of the third month after the
 * period ends — e.g. Q2 (ends 30-06) is due 1 September. This is the single
 * date that costs money with SKAT if missed, so it is computed here from the
 * period-end date and surfaced on every VAT output.
 *
 * Returns the deadline as a YYYY-MM-DD ISO date, or `null` when `periodEnd`
 * is not a valid ISO date.
 */
export function vatFilingDeadline(periodEnd: string): string | null {
  if (!looksLikeIsoDate(periodEnd)) return null;
  const [yearStr, monthStr] = periodEnd.split("-");
  const year = Number(yearStr);
  const month = Number(monthStr);
  if (!Number.isInteger(year) || !Number.isInteger(month)) return null;
  // The 1st of the third month after the period-end month. month is 1-based;
  // adding 3 and normalising the year keeps quarter-end → deadline correct
  // (06 → 09 same year, 12 → 03 next year).
  let deadlineMonth = month + 3;
  let deadlineYear = year;
  while (deadlineMonth > 12) {
    deadlineMonth -= 12;
    deadlineYear += 1;
  }
  return `${deadlineYear}-${String(deadlineMonth).padStart(2, "0")}-01`;
}

export type ReverseChargePurchaseInput = {
  transactionDate: string;
  text: string;
  documentId: number;
  netAmount: number;
  expenseAccountNo: string;
  paymentAccountNo?: string;
  sourceBankTransactionId?: number;
  currency?: string;
  amountForeign?: number;
  amountDkk?: number;
  fxRateToDkk?: number;
  createdBy?: string;
  createdByProgram?: string;
};

export type RepresentationPurchaseInput = {
  transactionDate: string;
  text: string;
  documentId: number;
  netAmount: number;
  expenseAccountNo?: string;
  paymentAccountNo?: string;
  sourceBankTransactionId?: number;
  currency?: string;
  amountForeign?: number;
  amountDkk?: number;
  fxRateToDkk?: number;
  createdBy?: string;
  createdByProgram?: string;
};


export function postEuServiceReverseChargePurchase(db: Database, input: ReverseChargePurchaseInput): JournalPostResult {
  const errors: string[] = [];
  if (!looksLikeIsoDate(input.transactionDate)) errors.push("transactionDate must be YYYY-MM-DD");
  if (typeof input.text !== "string" || input.text.trim().length === 0) errors.push("text is required");
  if (!Number.isInteger(input.documentId) || input.documentId <= 0) errors.push("documentId must be a positive integer");
  if (!Number.isFinite(input.netAmount) || input.netAmount <= 0) errors.push("netAmount must be a positive number");
  if (typeof input.expenseAccountNo !== "string" || input.expenseAccountNo.trim().length === 0) errors.push("expenseAccountNo is required");
  if (errors.length > 0) return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors };

  const documentRow = db.query(`SELECT sender_vat_cvr FROM documents WHERE id = ?`).get(input.documentId) as { sender_vat_cvr: string | null } | null;
  if (!documentRow) return { ok: false, appliedRules: [REVERSE_CHARGE_RULE_ID], errors: [`documentId ${input.documentId} does not exist`] };
  // EU service reverse charge (momsloven §46) applies only to suppliers in
  // *other* EU member states. A Danish supplier is a domestic purchase and
  // must not be booked as reverse charge.
  const senderVat = normalizeEuVatNumber(documentRow.sender_vat_cvr);
  if (senderVat && senderVat.countryCode === "DK") {
    return {
      ok: false,
      appliedRules: [REVERSE_CHARGE_RULE_ID],
      errors: [`document sender_vat_cvr ${senderVat.normalized} is a Danish supplier — EU service reverse charge applies only to other EU member states; book this as a domestic DK_PURCHASE_25 expense`],
    };
  }
  const viesCheck = requireCachedViesValidation(db, documentRow.sender_vat_cvr, "document sender_vat_cvr");
  if (!viesCheck.ok) return { ok: false, appliedRules: [...new Set([REVERSE_CHARGE_RULE_ID, ...viesCheck.appliedRules])], errors: viesCheck.errors };

  const vatAmount = percentOfDkk(input.netAmount, 25);
  const result = postJournalEntry(db, {
    transactionDate: input.transactionDate,
    text: input.text.trim(),
    documentId: input.documentId,
    sourceBankTransactionId: input.sourceBankTransactionId,
    currency: input.currency,
    amountForeign: input.amountForeign,
    amountDkk: input.amountDkk,
    fxRateToDkk: input.fxRateToDkk,
    createdBy: input.createdBy,
    createdByProgram: input.createdByProgram,
    lines: [
      { accountNo: input.expenseAccountNo, debitAmount: roundDkk(input.netAmount), vatCode: "EU_SERVICE_REVERSE_CHARGE", text: "EU service purchase base" },
      { accountNo: "4000", debitAmount: vatAmount, text: "Deductible reverse-charge input VAT" },
      { accountNo: input.paymentAccountNo ?? "2000", creditAmount: roundDkk(input.netAmount), text: "Payment / liability" },
      { accountNo: "1200", creditAmount: vatAmount, text: "Reverse-charge output VAT" },
    ],
  });

  return {
    ...result,
    appliedRules: result.ok ? [...new Set([...(result.appliedRules ?? []), REVERSE_CHARGE_RULE_ID])] : [...new Set([REVERSE_CHARGE_RULE_ID, ...(result.appliedRules ?? [])])],
  };
}

export function postRepresentationPurchase(db: Database, input: RepresentationPurchaseInput): JournalPostResult {
  const errors: string[] = [];
  if (!looksLikeIsoDate(input.transactionDate)) errors.push("transactionDate must be YYYY-MM-DD");
  if (typeof input.text !== "string" || input.text.trim().length === 0) errors.push("text is required");
  if (!Number.isInteger(input.documentId) || input.documentId <= 0) errors.push("documentId must be a positive integer");
  if (!Number.isFinite(input.netAmount) || input.netAmount <= 0) errors.push("netAmount must be a positive number");
  if (errors.length > 0) return { ok: false, appliedRules: [REPRESENTATION_RULE_ID], errors };

  const fullVatAmount = percentOfDkk(input.netAmount, 25);
  const deductibleVatAmount = percentOfDkk(fullVatAmount, 25);
  const nonDeductibleVatAmount = subtractDkk(fullVatAmount, deductibleVatAmount);
  const grossAmount = addDkk(input.netAmount, fullVatAmount);

  const result = postJournalEntry(db, {
    transactionDate: input.transactionDate,
    text: input.text.trim(),
    documentId: input.documentId,
    sourceBankTransactionId: input.sourceBankTransactionId,
    currency: input.currency,
    amountForeign: input.amountForeign,
    amountDkk: input.amountDkk,
    fxRateToDkk: input.fxRateToDkk,
    createdBy: input.createdBy,
    createdByProgram: input.createdByProgram,
    lines: [
      {
        accountNo: input.expenseAccountNo ?? "3070",
        debitAmount: roundDkk(input.netAmount),
        vatCode: "REPRESENTATION_SPECIAL",
        text: "Representation purchase base"
      },
      {
        accountNo: input.expenseAccountNo ?? "3070",
        debitAmount: nonDeductibleVatAmount,
        text: "Non-deductible representation VAT (75%)"
      },
      { accountNo: "4000", debitAmount: deductibleVatAmount, text: "Deductible representation VAT (25%)" },
      { accountNo: input.paymentAccountNo ?? "2000", creditAmount: grossAmount, text: "Payment / liability" },
    ],
  });

  return {
    ...result,
    appliedRules: result.ok ? [...new Set([...(result.appliedRules ?? []), REPRESENTATION_RULE_ID])] : [...new Set([REPRESENTATION_RULE_ID, ...(result.appliedRules ?? [])])],
  };
}

export function buildVatReport(db: Database, periodStart: string, periodEnd: string): VatPeriodReport {
  const errors: string[] = [];
  if (!looksLikeIsoDate(periodStart)) errors.push("periodStart must be YYYY-MM-DD");
  if (!looksLikeIsoDate(periodEnd)) errors.push("periodEnd must be YYYY-MM-DD");
  if (errors.length === 0 && periodStart > periodEnd) errors.push("periodStart must be before or equal to periodEnd");
  if (errors.length > 0) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      periodStart,
      periodEnd,
      outputVat: 0,
      inputVat: 0,
      netVatPayable: 0,
      purchaseBase25: 0,
      salesBase25: 0,
      reverseChargeSalesBase: 0,
      reverseChargePurchaseBase: 0,
      representationPurchaseBase: 0,
      badDebtReliefBase25: 0,
      exemptSalesBase: 0,
      ossConsumerSalesBase: 0,
      ossConsumerSalesEntryCount: 0,
      journalEntryCount: 0,
      reversedJournalEntryCount: 0,
      reversalJournalEntryCount: 0,
      totalJournalEntryCount: 0,
      linesConsidered: 0,
      reversedLinesConsidered: 0,
      reversalLinesConsidered: 0,
      totalLinesConsidered: 0,
      warnings: [],
      errors,
    };
  }

  const rows = db.query(
    `SELECT je.id as entry_id, je.status, je.reversal_of_entry_id, a.account_no, a.type as account_type, jl.debit_amount, jl.credit_amount, jl.vat_code
     FROM journal_entries je
     JOIN journal_lines jl ON jl.journal_entry_id = je.id
     JOIN accounts a ON a.id = jl.account_id
     WHERE je.transaction_date >= ? AND je.transaction_date <= ?
     ORDER BY je.id ASC, jl.id ASC`
  ).all(periodStart, periodEnd) as Array<{
    entry_id: number;
    status: string;
    reversal_of_entry_id: number | null;
    account_no: string;
    account_type: string;
    debit_amount: number;
    credit_amount: number;
    vat_code: string | null;
  }>;

  let outputVat = 0;
  let inputVat = 0;
  let purchaseBase25 = 0;
  let salesBase25 = 0;
  let reverseChargeSalesBase = 0;
  let reverseChargePurchaseBase = 0;
  let representationPurchaseBase = 0;
  let badDebtReliefBase25 = 0;
  let exemptSalesBase = 0;
  let ossConsumerSalesBase = 0;
  // OSS sales are counted per *entry*, not per line, so a multi-line OSS
  // invoice still counts as one entry.
  const ossConsumerSalesEntryIds = new Set<number>();
  // Count VAT-bearing base lines per category. 25% of a period-summed base is
  // not equal to the sum of per-line 25%-rounded VAT when amounts have odd
  // øre, so each base line can drift the aggregate by up to 1 øre. We allow a
  // (lineCount - 1)-øre tolerance on the reconciliation cross-check below.
  let outputVatBaseLines = 0;
  let inputVatBaseLines = 0;
  const activeEntryIds = new Set<number>();
  const reversedEntryIds = new Set<number>();
  const reversalEntryIds = new Set<number>();
  let activeLinesConsidered = 0;
  let reversedLinesConsidered = 0;
  let reversalLinesConsidered = 0;
  const reversedByInPeriodReversal = new Set(rows.filter((row) => row.reversal_of_entry_id != null).map((row) => row.reversal_of_entry_id as number));

  for (const row of rows) {
    const isReversalEntry = row.reversal_of_entry_id != null;
    const isReversedEntry = !isReversalEntry && reversedByInPeriodReversal.has(row.entry_id);

    if (isReversalEntry) {
      reversalEntryIds.add(row.entry_id);
      reversalLinesConsidered += 1;
    } else if (isReversedEntry) {
      reversedEntryIds.add(row.entry_id);
      reversedLinesConsidered += 1;
    } else {
      activeEntryIds.add(row.entry_id);
      activeLinesConsidered += 1;
    }

    const debit = roundDkk(Number(row.debit_amount ?? 0));
    const credit = roundDkk(Number(row.credit_amount ?? 0));

    if (row.account_no === "1200") outputVat += credit - debit;
    if (row.account_no === "4000") inputVat += debit - credit;

    if (row.vat_code === "DK_PURCHASE_25") { purchaseBase25 += debit - credit; inputVatBaseLines += 1; }
    if (row.vat_code === "DK_SALE_25") { salesBase25 += credit - debit; outputVatBaseLines += 1; }
    if (row.vat_code === "REVERSE_CHARGE_EXEMPT") reverseChargeSalesBase += credit - debit;
    if (row.vat_code === "EU_SERVICE_REVERSE_CHARGE") {
      reverseChargePurchaseBase += debit - credit;
      // Reverse charge contributes to both output and input VAT.
      inputVatBaseLines += 1;
      outputVatBaseLines += 1;
    }
    if (row.vat_code === "REPRESENTATION_SPECIAL") { representationPurchaseBase += debit - credit; inputVatBaseLines += 1; }
    if (row.vat_code === "DK_BAD_DEBT_25") { badDebtReliefBase25 += debit - credit; outputVatBaseLines += 1; }
    // VAT-exempt domestic sales (momsloven §13) and OSS consumer sales carry
    // NO Danish output VAT, so they are tracked in their own bases and are
    // deliberately NOT added to outputVatBaseLines (the output-VAT
    // reconciliation must not expect 25% of them).
    if (row.vat_code === "DK_SALE_EXEMPT") exemptSalesBase += credit - debit;
    if (row.vat_code === "OSS_EU_CONSUMER") {
      ossConsumerSalesBase += credit - debit;
      ossConsumerSalesEntryIds.add(row.entry_id);
    }
  }

  outputVat = roundDkk(outputVat);
  inputVat = roundDkk(inputVat);
  purchaseBase25 = roundDkk(purchaseBase25);
  salesBase25 = roundDkk(salesBase25);
  reverseChargeSalesBase = roundDkk(reverseChargeSalesBase);
  reverseChargePurchaseBase = roundDkk(reverseChargePurchaseBase);
  representationPurchaseBase = roundDkk(representationPurchaseBase);
  badDebtReliefBase25 = roundDkk(badDebtReliefBase25);
  exemptSalesBase = roundDkk(exemptSalesBase);
  ossConsumerSalesBase = roundDkk(ossConsumerSalesBase);

  const expectedOutputVat = subtractDkk(addDkk(percentOfDkk(salesBase25, 25), percentOfDkk(reverseChargePurchaseBase, 25)), percentOfDkk(badDebtReliefBase25, 25));
  const expectedInputVat = addDkk(addDkk(percentOfDkk(purchaseBase25, 25), percentOfDkk(reverseChargePurchaseBase, 25)), percentOfDkk(percentOfDkk(representationPurchaseBase, 25), 25));
  const warnings: string[] = [];
  // Each VAT-bearing base line is rounded to øre independently when booked,
  // so the booked aggregate can differ from "25% of the summed base" by up to
  // 1 øre per line. Only the *first* line establishes the aggregate; the
  // remaining (n-1) lines can each drift it, so the tolerance is (n-1) øre.
  // A genuine mis-booking exceeds this small bound and still warns.
  const outputVatTolerance = Math.max(0, outputVatBaseLines - 1);
  const inputVatTolerance = Math.max(0, inputVatBaseLines - 1);
  if (oreDifference(outputVat, expectedOutputVat) > outputVatTolerance) {
    warnings.push(`output VAT mismatch: booked ${outputVat}, expected from base × rate ${expectedOutputVat}`);
  }
  if (oreDifference(inputVat, expectedInputVat) > inputVatTolerance) {
    warnings.push(`input VAT mismatch: booked ${inputVat}, expected from base × rate ${expectedInputVat}`);
  }

  return {
    ok: true,
    appliedRules: [RULE_ID],
    periodStart,
    periodEnd,
    outputVat,
    inputVat,
    netVatPayable: subtractDkk(outputVat, inputVat),
    purchaseBase25,
    salesBase25,
    reverseChargeSalesBase,
    reverseChargePurchaseBase,
    representationPurchaseBase,
    badDebtReliefBase25,
    exemptSalesBase,
    ossConsumerSalesBase,
    ossConsumerSalesEntryCount: ossConsumerSalesEntryIds.size,
    journalEntryCount: activeEntryIds.size,
    reversedJournalEntryCount: reversedEntryIds.size,
    reversalJournalEntryCount: reversalEntryIds.size,
    totalJournalEntryCount: activeEntryIds.size + reversedEntryIds.size + reversalEntryIds.size,
    linesConsidered: activeLinesConsidered,
    reversedLinesConsidered,
    reversalLinesConsidered,
    totalLinesConsidered: rows.length,
    warnings,
    errors: [],
  };
}
