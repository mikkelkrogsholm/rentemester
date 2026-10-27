import type { Database } from "bun:sqlite";
import { compareDkk, normalizeCurrency, roundDkk } from "./money";
import { accountRoleCompatibility, resolveAccountRole } from "./account-roles";
import { projectVatLines } from "./vat-lines";

export type ForeignReceivableReliefResult =
  | { ok: true; amountDkk: number; carryingBalanceDkk: number }
  | { ok: false; error: string };

export type InvoiceReceivableAccountResult =
  | { ok: true; accountNo: string; bookingJournalEntryId: number; bookedGrossDkk: number }
  | { ok: false; error: string };

export type SettlementBankAccountResult =
  | { ok: true; accountNo: string; source: "bank-account" | "explicit" | "role" }
  | { ok: false; error: string };

export type InvoiceCreditNoteEvidenceResult =
  | { ok: true }
  | { ok: false; errors: string[] };

/**
 * Resolve the receivable account from the invoice's own active booking entry.
 *
 * A later account-role change must not move settlement to another control
 * account: doing so makes the domain status look paid while the original
 * receivable remains open in the general ledger. The issued-invoice booking is
 * therefore the canonical source of truth.
 */
export function resolveInvoiceReceivableAccount(
  db: Database,
  input: { invoiceDocumentId: number; beforeJournalEntryId?: number },
): InvoiceReceivableAccountResult {
  const cutoff = input.beforeJournalEntryId ?? null;
  const booking = db.query(
    `SELECT p.journal_entry_id AS id, p.booked_gross_dkk,
            p.receivable_account_id, a.account_no, a.type AS account_type,
            a.normal_balance, j.document_id, j.status, j.currency,
            j.transaction_date, j.source_bank_transaction_id,
            j.amount_foreign, j.amount_dkk, j.fx_rate_to_dkk,
            (SELECT reversal.id
               FROM journal_entries reversal
              WHERE reversal.reversal_of_entry_id = j.id
                AND (? IS NULL OR reversal.id < ?)
              ORDER BY reversal.id ASC
              LIMIT 1) AS reversed_by_entry_id
       FROM issued_invoice_postings p
       JOIN journal_entries j ON j.id = p.journal_entry_id
       JOIN accounts a ON a.id = p.receivable_account_id
      WHERE p.invoice_document_id = ?
        AND (? IS NULL OR j.id < ?)`,
  ).get(
    cutoff,
    cutoff,
    input.invoiceDocumentId,
    cutoff,
    cutoff,
  ) as {
    id: number;
    booked_gross_dkk: number;
    receivable_account_id: number;
    account_no: string;
    account_type: string;
    normal_balance: string;
    document_id: number | null;
    status: string;
    currency: string | null;
    transaction_date: string;
    source_bank_transaction_id: number | null;
    amount_foreign: number | null;
    amount_dkk: number | null;
    fx_rate_to_dkk: number | null;
    reversed_by_entry_id: number | null;
  } | null;
  if (!booking) {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} has no active ledger booking before settlement` };
  }

  const invoice = db.query(
    `SELECT invoice_date, amount_inc_vat, vat_amount, currency, payload_json, document_type
       FROM documents
      WHERE id = ?`,
  ).get(input.invoiceDocumentId) as {
    invoice_date: string | null;
    amount_inc_vat: number | null;
    vat_amount: number | null;
    currency: string | null;
    payload_json: string | null;
    document_type: string;
  } | null;
  if (!invoice || invoice.document_type !== "issued_invoice") {
    return { ok: false, error: `invoice posting link ${booking.id} does not reference an issued invoice document` };
  }
  let payload: any;
  try {
    payload = invoice.payload_json ? JSON.parse(invoice.payload_json) : null;
  } catch {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} has invalid booking payload JSON` };
  }
  const currency = normalizeCurrency(invoice.currency);
  const grossForeign = roundDkk(Number(invoice.amount_inc_vat ?? 0));
  const expectedGrossDkk = currency === "DKK"
    ? grossForeign
    : roundDkk(Number(payload?.totals?.grossAmountDkk ?? 0));
  const expectedVatDkk = currency === "DKK"
    ? roundDkk(Number(invoice.vat_amount ?? payload?.totals?.vatAmount ?? 0))
    : roundDkk(Number(payload?.totals?.vatAmountDkk ?? 0));
  const expectedNetDkk = currency === "DKK"
    ? roundDkk(expectedGrossDkk - expectedVatDkk)
    : roundDkk(Number(payload?.totals?.netAmountDkk ?? 0));
  const expectedFxRate = currency === "DKK" ? null : Number(payload?.totals?.fxRateToDkk ?? 0);
  if (!(grossForeign > 0) || !(expectedGrossDkk > 0)) {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} has no positive gross booking basis` };
  }
  if (!(expectedNetDkk > 0) || compareDkk(expectedNetDkk + expectedVatDkk, expectedGrossDkk) !== 0) {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} has inconsistent immutable DKK booking totals` };
  }
  if (booking.document_id !== input.invoiceDocumentId || booking.status !== "posted" || booking.reversed_by_entry_id != null) {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} booking ${booking.id} is not active, posted, and linked to the invoice` };
  }
  if (booking.transaction_date !== invoice.invoice_date || booking.source_bank_transaction_id != null) {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} booking ${booking.id} has invalid invoice-date or bank-source context` };
  }
  if (booking.account_type !== "asset" || booking.normal_balance !== "debit") {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} booking account ${booking.account_no} is not a debit-normal asset` };
  }
  if (compareDkk(Number(booking.booked_gross_dkk), expectedGrossDkk) !== 0) {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} booking ${booking.id} gross ${booking.booked_gross_dkk} DKK does not match invoice gross ${expectedGrossDkk} DKK` };
  }
  if (normalizeCurrency(booking.currency) !== currency) {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} booking ${booking.id} currency does not match the invoice` };
  }
  if (
    currency !== "DKK" &&
    (compareDkk(Number(booking.amount_foreign ?? 0), grossForeign) !== 0 ||
      compareDkk(Number(booking.amount_dkk ?? 0), expectedGrossDkk) !== 0)
  ) {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} booking ${booking.id} foreign/DKK metadata does not match the invoice` };
  }
  if (
    currency !== "DKK" &&
    (expectedFxRate == null || !Number.isFinite(expectedFxRate) || !(expectedFxRate > 0) ||
      Math.abs(Number(booking.fx_rate_to_dkk ?? 0) - expectedFxRate) > 0.0000005)
  ) {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} booking ${booking.id} FX rate metadata does not match the invoice` };
  }

  const totals = db.query(
    `SELECT COALESCE(SUM(debit_amount), 0) AS debit_dkk,
            COALESCE(SUM(credit_amount), 0) AS credit_dkk
       FROM journal_lines
      WHERE journal_entry_id = ?`,
  ).get(booking.id) as { debit_dkk: number; credit_dkk: number };
  if (
    compareDkk(Number(totals.debit_dkk), expectedGrossDkk) !== 0 ||
    compareDkk(Number(totals.credit_dkk), expectedGrossDkk) !== 0
  ) {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} booking ${booking.id} total debits/credits do not equal ${expectedGrossDkk} DKK` };
  }

  const candidates = db.query(
    `SELECT a.account_no,
            SUM(jl.debit_amount) - SUM(jl.credit_amount) AS amount_dkk
       FROM journal_lines jl
       JOIN accounts a ON a.id = jl.account_id
      WHERE jl.journal_entry_id = ?
        AND a.type = 'asset'
        AND a.normal_balance = 'debit'
      GROUP BY a.account_no
     HAVING ROUND(SUM(jl.debit_amount) - SUM(jl.credit_amount), 2) <> 0
      ORDER BY a.account_no ASC`,
  ).all(booking.id) as Array<{ account_no: string; amount_dkk: number }>;
  if (
    candidates.length !== 1 ||
    candidates[0]!.account_no !== booking.account_no ||
    compareDkk(Number(candidates[0]!.amount_dkk), expectedGrossDkk) !== 0
  ) {
    return {
      ok: false,
      error: `invoice document ${input.invoiceDocumentId} booking ${booking.id} does not debit exactly ${expectedGrossDkk} DKK to its linked receivable account`,
    };
  }

  const vatTreatment = payload?.vatTreatment ?? "standard";
  const projection = projectVatLines(payload?.lines, vatTreatment, payload?.totals?.vatRate);
  if (!projection.ok || projection.lines.length === 0 || !(projection.netAmount > 0)) {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} has no valid immutable VAT-line basis for booking ${booking.id}` };
  }
  const isDomesticReverseCharge = vatTreatment === "domestic_reverse_charge";
  const reverseChargeVatCode = isDomesticReverseCharge
    ? "DOMESTIC_REVERSE_CHARGE_EXEMPT"
    : "REVERSE_CHARGE_EXEMPT";
  const baseScale = expectedNetDkk / projection.netAmount;
  const expectedRevenueLines = projection.lines.map((line) => ({
    vatCode: line.taxClassification === "taxable"
      ? "DK_SALE_25"
      : line.taxClassification === "reverse_charge"
        ? reverseChargeVatCode
        : "DK_SALE_EXEMPT",
    amountDkk: roundDkk(line.vatBase * baseScale),
  }));
  const roundedRevenue = roundDkk(expectedRevenueLines.reduce((sum, line) => sum + line.amountDkk, 0));
  if (expectedRevenueLines.length > 0 && compareDkk(roundedRevenue, expectedNetDkk) !== 0) {
    const last = expectedRevenueLines[expectedRevenueLines.length - 1]!;
    last.amountDkk = roundDkk(last.amountDkk + expectedNetDkk - roundedRevenue);
  }
  const expectedRevenueByVatCode = new Map<string, number>();
  for (const line of expectedRevenueLines) {
    expectedRevenueByVatCode.set(
      line.vatCode,
      roundDkk((expectedRevenueByVatCode.get(line.vatCode) ?? 0) + line.amountDkk),
    );
  }

  const counterLines = db.query(
    `SELECT a.type AS account_type, a.normal_balance,
            jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl
       JOIN accounts a ON a.id = jl.account_id
      WHERE jl.journal_entry_id = ?
        AND jl.account_id <> ?
      ORDER BY jl.id ASC`,
  ).all(booking.id, booking.receivable_account_id) as Array<{
    account_type: string;
    normal_balance: string;
    debit_amount: number;
    credit_amount: number;
    vat_code: string | null;
  }>;
  const actualRevenueByVatCode = new Map<string, number>();
  let actualVatDkk = 0;
  for (const line of counterLines) {
    if (line.account_type === "income" && line.normal_balance === "credit" && line.debit_amount === 0 && line.credit_amount > 0 && line.vat_code) {
      actualRevenueByVatCode.set(
        line.vat_code,
        roundDkk((actualRevenueByVatCode.get(line.vat_code) ?? 0) + Number(line.credit_amount)),
      );
      continue;
    }
    if (line.account_type === "vat" && line.normal_balance === "credit" && line.debit_amount === 0 && line.credit_amount > 0 && line.vat_code == null) {
      actualVatDkk = roundDkk(actualVatDkk + Number(line.credit_amount));
      continue;
    }
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} booking ${booking.id} contains an unsupported revenue/VAT counter-line` };
  }
  if (compareDkk(actualVatDkk, expectedVatDkk) !== 0) {
    return { ok: false, error: `invoice document ${input.invoiceDocumentId} booking ${booking.id} output VAT does not match ${expectedVatDkk} DKK` };
  }
  const revenueCodes = new Set([...expectedRevenueByVatCode.keys(), ...actualRevenueByVatCode.keys()]);
  for (const vatCode of revenueCodes) {
    if (compareDkk(actualRevenueByVatCode.get(vatCode) ?? 0, expectedRevenueByVatCode.get(vatCode) ?? 0) !== 0) {
      return { ok: false, error: `invoice document ${input.invoiceDocumentId} booking ${booking.id} revenue for VAT code ${vatCode} does not match immutable invoice lines` };
    }
  }
  return {
    ok: true,
    accountNo: booking.account_no,
    bookingJournalEntryId: booking.id,
    bookedGrossDkk: expectedGrossDkk,
  };
}

/** Validate every credit-note document as an exact, active principal reduction. */
export function validateInvoiceCreditNoteEvidence(
  db: Database,
  input: { invoiceDocumentId: number },
): InvoiceCreditNoteEvidenceResult {
  const invoice = db.query(
    `SELECT invoice_no, amount_inc_vat, vat_amount, currency, document_type
       FROM documents
      WHERE id = ?`,
  ).get(input.invoiceDocumentId) as {
    invoice_no: string | null;
    amount_inc_vat: number | null;
    vat_amount: number | null;
    currency: string | null;
    document_type: string;
  } | null;
  if (!invoice || invoice.document_type !== "issued_invoice" || !invoice.invoice_no) {
    return { ok: false, errors: [`invoice document ${input.invoiceDocumentId} is not an issued invoice with a number`] };
  }
  const rows = db.query(
    `SELECT c.id, c.amount_inc_vat, c.vat_amount, c.currency, c.invoice_date,
            c.payload_json,
            p.original_invoice_document_id, p.journal_entry_id,
            p.receivable_account_id, p.booked_gross_dkk,
            j.document_id AS journal_document_id, j.status AS journal_status,
            j.transaction_date AS journal_date, j.currency AS journal_currency,
            j.amount_foreign AS journal_amount_foreign,
            j.amount_dkk AS journal_amount_dkk,
            j.fx_rate_to_dkk AS journal_fx_rate_to_dkk,
            j.source_bank_transaction_id,
            (SELECT reversal.id
               FROM journal_entries reversal
              WHERE reversal.reversal_of_entry_id = j.id
              ORDER BY reversal.id ASC
              LIMIT 1) AS reversed_by_entry_id,
            a.account_no, a.type AS account_type, a.normal_balance
       FROM documents c
       LEFT JOIN credit_note_postings p ON p.credit_note_document_id = c.id
       LEFT JOIN journal_entries j ON j.id = p.journal_entry_id
       LEFT JOIN accounts a ON a.id = p.receivable_account_id
      WHERE c.document_type = 'credit_note'
        AND c.payment_details = ?
      ORDER BY CASE WHEN p.journal_entry_id IS NULL THEN 1 ELSE 0 END,
               p.journal_entry_id ASC, c.id ASC`,
  ).all(invoice.invoice_no) as Array<{
    id: number;
    amount_inc_vat: number | null;
    vat_amount: number | null;
    currency: string | null;
    invoice_date: string | null;
    payload_json: string | null;
    original_invoice_document_id: number | null;
    journal_entry_id: number | null;
    receivable_account_id: number | null;
    booked_gross_dkk: number | null;
    journal_document_id: number | null;
    journal_status: string | null;
    journal_date: string | null;
    journal_currency: string | null;
    journal_amount_foreign: number | null;
    journal_amount_dkk: number | null;
    journal_fx_rate_to_dkk: number | null;
    source_bank_transaction_id: number | null;
    reversed_by_entry_id: number | null;
    account_no: string | null;
    account_type: string | null;
    normal_balance: string | null;
  }>;
  const errors: string[] = [];
  const orphanLinks = db.query(
    `SELECT p.credit_note_document_id
       FROM credit_note_postings p
       JOIN documents c ON c.id = p.credit_note_document_id
      WHERE p.original_invoice_document_id = ?
        AND (c.document_type <> 'credit_note' OR c.payment_details IS NOT ?)
      ORDER BY p.credit_note_document_id ASC`,
  ).all(input.invoiceDocumentId, invoice.invoice_no) as Array<{ credit_note_document_id: number }>;
  for (const orphan of orphanLinks) {
    errors.push(`credit note posting ${orphan.credit_note_document_id} does not identify invoice ${invoice.invoice_no}`);
  }
  if (rows.length === 0) {
    return errors.length > 0
      ? { ok: false, errors: [...new Set(errors)] }
      : { ok: true };
  }

  const booking = resolveInvoiceReceivableAccount(db, input);
  if (!booking.ok) return { ok: false, errors: [booking.error] };
  const originalGross = roundDkk(Number(invoice.amount_inc_vat ?? 0));
  if (!(originalGross > 0)) {
    return { ok: false, errors: [`invoice document ${input.invoiceDocumentId} has no positive credit-note basis`] };
  }
  let cumulativeForeignCredit = 0;
  let cumulativeForeignVat = 0;
  let cumulativeDkkRelief = 0;
  const originalCounterLines = db.query(
    `SELECT a.account_no, a.type AS account_type, a.normal_balance,
            jl.debit_amount, jl.credit_amount, jl.vat_code
       FROM journal_lines jl
       JOIN accounts a ON a.id = jl.account_id
      WHERE jl.journal_entry_id = ?
        AND a.account_no <> ?
      ORDER BY jl.id ASC`,
  ).all(booking.bookingJournalEntryId, booking.accountNo) as Array<{
    account_no: string;
    account_type: string;
    normal_balance: string;
    debit_amount: number;
    credit_amount: number;
    vat_code: string | null;
  }>;
  type CreditCounterTarget = {
    accountNo: string;
    accountType: string;
    normalBalance: string;
    vatCode: string | null;
    originalDkk: number;
  };
  const originalCounterByKey = new Map<string, CreditCounterTarget>();
  for (const line of originalCounterLines) {
    const key = JSON.stringify([line.account_no, line.account_type, line.normal_balance, line.vat_code]);
    const existing = originalCounterByKey.get(key);
    if (existing) {
      existing.originalDkk = roundDkk(existing.originalDkk + Number(line.credit_amount));
    } else {
      originalCounterByKey.set(key, {
        accountNo: line.account_no,
        accountType: line.account_type,
        normalBalance: line.normal_balance,
        vatCode: line.vat_code,
        originalDkk: roundDkk(Number(line.credit_amount)),
      });
    }
  }
  const priorActualCounters = new Map<string, number>();
  for (const row of rows) {
    const label = `credit note document ${row.id}`;
    if (
      row.journal_entry_id == null ||
      row.original_invoice_document_id !== input.invoiceDocumentId ||
      row.journal_document_id !== row.id
    ) {
      errors.push(`${label} has no explicit journal evidence linked to invoice document ${input.invoiceDocumentId}`);
      continue;
    }
    if (row.journal_status !== "posted" || row.reversed_by_entry_id != null) {
      errors.push(`${label} journal ${row.journal_entry_id} is not active and posted`);
    }
    const invoiceCurrency = normalizeCurrency(invoice.currency);
    if (normalizeCurrency(row.currency) !== invoiceCurrency) {
      errors.push(`${label} currency does not match invoice ${invoice.invoice_no}`);
    }
    if (row.account_no !== booking.accountNo || row.account_type !== "asset" || row.normal_balance !== "debit") {
      errors.push(`${label} does not reduce the invoice's linked receivable account ${booking.accountNo}`);
      continue;
    }
    const creditForeign = roundDkk(Number(row.amount_inc_vat ?? 0));
    const nextCumulativeForeign = roundDkk(cumulativeForeignCredit + creditForeign);
    const originalVatForeign = roundDkk(Number(invoice.vat_amount ?? 0));
    const nextCumulativeVat = nextCumulativeForeign === originalGross
      ? originalVatForeign
      : roundDkk((originalVatForeign * nextCumulativeForeign) / originalGross);
    const expectedVatForeign = roundDkk(nextCumulativeVat - cumulativeForeignVat);
    const nextCumulativeDkk = nextCumulativeForeign === originalGross
      ? booking.bookedGrossDkk
      : roundDkk((booking.bookedGrossDkk * nextCumulativeForeign) / originalGross);
    const expectedDkk = roundDkk(nextCumulativeDkk - cumulativeDkkRelief);
    cumulativeForeignCredit = nextCumulativeForeign;
    cumulativeForeignVat = nextCumulativeVat;
    cumulativeDkkRelief = nextCumulativeDkk;
    if (compareDkk(cumulativeForeignCredit, originalGross) > 0) {
      errors.push(`${label} makes cumulative credit notes exceed invoice gross ${originalGross}`);
      continue;
    }
    if (!(creditForeign > 0) || !(expectedDkk > 0) || compareDkk(Number(row.booked_gross_dkk ?? 0), expectedDkk) !== 0) {
      errors.push(`${label} booked DKK amount does not match its cumulative invoice principal reduction`);
      continue;
    }
    let creditPayload: any;
    try {
      creditPayload = row.payload_json ? JSON.parse(row.payload_json) : null;
    } catch {
      creditPayload = null;
    }
    if (
      compareDkk(Number(row.vat_amount ?? 0), expectedVatForeign) !== 0 ||
      !creditPayload ||
      compareDkk(Number(creditPayload.grossAmount ?? 0), creditForeign) !== 0 ||
      compareDkk(Number(creditPayload.vatAmount ?? 0), expectedVatForeign) !== 0 ||
      compareDkk(Number(creditPayload.netAmount ?? 0), roundDkk(creditForeign - expectedVatForeign)) !== 0
    ) {
      errors.push(`${label} immutable gross/VAT/net amounts do not match its cumulative credit share`);
      continue;
    }
    const journalContextInvalid = row.journal_date !== row.invoice_date || row.source_bank_transaction_id != null ||
      normalizeCurrency(row.journal_currency) !== invoiceCurrency ||
      (invoiceCurrency === "DKK"
        ? row.journal_amount_foreign != null || row.journal_amount_dkk != null || row.journal_fx_rate_to_dkk != null
        : compareDkk(Number(row.journal_amount_foreign ?? 0), creditForeign) !== 0 ||
          compareDkk(Number(row.journal_amount_dkk ?? 0), expectedDkk) !== 0 ||
          !(Number(row.journal_fx_rate_to_dkk) > 0) ||
          compareDkk(
            roundDkk(creditForeign * Number(row.journal_fx_rate_to_dkk)),
            expectedDkk,
          ) !== 0);
    if (journalContextInvalid) {
      errors.push(`${label} journal ${row.journal_entry_id} has invalid date, currency, FX, or bank context`);
    }
    const journalTotals = db.query(
      `SELECT COALESCE(SUM(debit_amount), 0) AS debit_dkk,
              COALESCE(SUM(credit_amount), 0) AS credit_dkk
         FROM journal_lines
        WHERE journal_entry_id = ?`,
    ).get(row.journal_entry_id) as { debit_dkk: number; credit_dkk: number };
    if (
      compareDkk(Number(journalTotals.debit_dkk), expectedDkk) !== 0 ||
      compareDkk(Number(journalTotals.credit_dkk), expectedDkk) !== 0
    ) {
      errors.push(`${label} journal ${row.journal_entry_id} total debits/credits do not equal ${expectedDkk} DKK`);
      continue;
    }
    const effects = db.query(
      `SELECT a.account_no,
              SUM(jl.debit_amount) - SUM(jl.credit_amount) AS effect_dkk
         FROM journal_lines jl
         JOIN accounts a ON a.id = jl.account_id
        WHERE jl.journal_entry_id = ?
          AND a.type = 'asset'
          AND a.normal_balance = 'debit'
        GROUP BY a.account_no
       HAVING ROUND(SUM(jl.debit_amount) - SUM(jl.credit_amount), 2) <> 0
        ORDER BY a.account_no ASC`,
    ).all(row.journal_entry_id) as Array<{ account_no: string; effect_dkk: number }>;
    if (
      effects.length !== 1 ||
      effects[0]!.account_no !== booking.accountNo ||
      compareDkk(Number(effects[0]!.effect_dkk), -expectedDkk) !== 0
    ) {
      errors.push(`${label} journal ${row.journal_entry_id} does not credit exactly ${expectedDkk} DKK from receivable ${booking.accountNo}`);
      continue;
    }

    const desiredCumulativeByKey = new Map<string, number>();
    for (const [key, target] of originalCounterByKey) {
      desiredCumulativeByKey.set(
        key,
        cumulativeForeignCredit === originalGross
          ? target.originalDkk
          : roundDkk((target.originalDkk * cumulativeForeignCredit) / originalGross),
      );
    }
    const desiredDebit = roundDkk([...desiredCumulativeByKey.values()].reduce((sum, amount) => sum + amount, 0));
    const residual = roundDkk(desiredDebit - cumulativeDkkRelief);
    if (residual !== 0) {
      const carrierEntry = [...originalCounterByKey.entries()]
        .filter(([, target]) => target.accountType === "income" && target.vatCode != null)
        .sort((left, right) => (desiredCumulativeByKey.get(right[0]) ?? 0) - (desiredCumulativeByKey.get(left[0]) ?? 0))[0];
      if (!carrierEntry) {
        errors.push(`${label} cannot reproduce the original booking's credit-note rounding`);
        continue;
      }
      const carrierAmount = roundDkk((desiredCumulativeByKey.get(carrierEntry[0]) ?? 0) - residual);
      if (carrierAmount < 0) {
        errors.push(`${label} cannot reproduce the original booking's credit-note rounding`);
        continue;
      }
      desiredCumulativeByKey.set(carrierEntry[0], carrierAmount);
    }
    const expectedByKey = new Map<string, number>();
    for (const [key, desiredCumulative] of desiredCumulativeByKey) {
      expectedByKey.set(key, roundDkk(desiredCumulative - (priorActualCounters.get(key) ?? 0)));
    }
    const actualCounters = db.query(
      `SELECT a.account_no, a.type AS account_type, a.normal_balance,
              jl.debit_amount, jl.credit_amount, jl.vat_code
         FROM journal_lines jl
         JOIN accounts a ON a.id = jl.account_id
        WHERE jl.journal_entry_id = ?
          AND a.account_no <> ?
        ORDER BY jl.id ASC`,
    ).all(row.journal_entry_id, booking.accountNo) as Array<{
      account_no: string;
      account_type: string;
      normal_balance: string;
      debit_amount: number;
      credit_amount: number;
      vat_code: string | null;
    }>;
    const actualByKey = new Map<string, number>();
    let invalidCounter = false;
    for (const line of actualCounters) {
      if (!(Number(line.debit_amount) > 0) || Number(line.credit_amount) !== 0) {
        invalidCounter = true;
        break;
      }
      const key = JSON.stringify([line.account_no, line.account_type, line.normal_balance, line.vat_code]);
      actualByKey.set(key, roundDkk((actualByKey.get(key) ?? 0) + Number(line.debit_amount)));
    }
    const counterKeys = new Set([...expectedByKey.keys(), ...actualByKey.keys()]);
    if (
      invalidCounter ||
      [...counterKeys].some((key) => compareDkk(actualByKey.get(key) ?? 0, expectedByKey.get(key) ?? 0) !== 0)
    ) {
      errors.push(`${label} journal ${row.journal_entry_id} is not the exact scaled inverse of invoice booking ${booking.bookingJournalEntryId}`);
    }
    for (const [key, amount] of actualByKey) {
      priorActualCounters.set(key, roundDkk((priorActualCounters.get(key) ?? 0) + amount));
    }
  }

  return errors.length > 0
    ? { ok: false, errors: [...new Set(errors)] }
    : { ok: true };
}

/** Resolve the concrete ledger account for a selected bank transaction. */
export function resolveSettlementBankAccount(
  db: Database,
  input: { bankTransactionId: number; requestedAccountNo?: string },
): SettlementBankAccountResult {
  const row = db.query(
    `SELECT bt.bank_account_id, bt.currency AS transaction_currency,
            ba.currency AS bank_account_currency, ba.ledger_account_no
       FROM bank_transactions bt
       LEFT JOIN bank_accounts ba ON ba.id = bt.bank_account_id
      WHERE bt.id = ?`,
  ).get(input.bankTransactionId) as {
    bank_account_id: number | null;
    transaction_currency: string | null;
    bank_account_currency: string | null;
    ledger_account_no: string | null;
  } | null;
  if (!row) return { ok: false, error: `bank transaction ${input.bankTransactionId} does not exist` };

  const requested = input.requestedAccountNo?.trim() || undefined;
  if (row.bank_account_id != null) {
    const concrete = row.ledger_account_no?.trim() || "";
    if (!concrete) {
      return {
        ok: false,
        error: `bank transaction ${input.bankTransactionId} belongs to bank account ${row.bank_account_id}, which has no ledger account mapping`,
      };
    }
    if (normalizeCurrency(row.transaction_currency) !== normalizeCurrency(row.bank_account_currency)) {
      return {
        ok: false,
        error: `bank transaction ${input.bankTransactionId} currency ${normalizeCurrency(row.transaction_currency)} does not match bank account ${row.bank_account_id} currency ${normalizeCurrency(row.bank_account_currency)}`,
      };
    }
    const compatible = accountRoleCompatibility(db, "bank", concrete);
    if (!compatible.ok) return { ok: false, error: compatible.error };
    if (requested && requested !== concrete) {
      return {
        ok: false,
        error: `bank transaction ${input.bankTransactionId} must post to its bank account ledger ${concrete}, not ${requested}`,
      };
    }
    return { ok: true, accountNo: concrete, source: "bank-account" };
  }

  if (requested) {
    const compatible = accountRoleCompatibility(db, "bank", requested);
    if (!compatible.ok) return { ok: false, error: compatible.error };
    return { ok: true, accountNo: requested, source: "explicit" };
  }
  const role = resolveAccountRole(db, "bank");
  if (!role.ok) return { ok: false, error: role.error };
  return { ok: true, accountNo: role.accountNo, source: "role" };
}

export function calculateInvoiceReceivableCarryingBalance(
  db: Database,
  input: {
    invoiceDocumentId: number;
    invoiceNumber: string;
    receivableAccountNo: string;
    beforeJournalEntryId?: number;
  },
) {
  const cutoff = input.beforeJournalEntryId ?? null;
  const row = db.query(
    `WITH principal_entries(id) AS (
       SELECT p.journal_entry_id
         FROM issued_invoice_postings p
        WHERE p.invoice_document_id = ?
          AND (? IS NULL OR p.journal_entry_id < ?)
       UNION
       SELECT p.journal_entry_id
         FROM credit_note_postings p
        WHERE p.original_invoice_document_id = ?
          AND (? IS NULL OR p.journal_entry_id < ?)
       UNION
       SELECT journal_entry_id
         FROM invoice_payments
        WHERE invoice_document_id = ?
          AND journal_entry_id IS NOT NULL
          AND (? IS NULL OR journal_entry_id < ?)
       UNION
       SELECT journal_entry_id
         FROM invoice_refunds
        WHERE invoice_document_id = ?
          AND journal_entry_id IS NOT NULL
          AND (? IS NULL OR journal_entry_id < ?)
       UNION
       SELECT journal_entry_id
         FROM invoice_bad_debt_writeoffs
        WHERE invoice_document_id = ?
          AND (? IS NULL OR journal_entry_id < ?)
     ), effective_entries(id) AS (
       SELECT id FROM principal_entries
       UNION
       SELECT reversal.id
         FROM journal_entries reversal
         JOIN principal_entries original
           ON original.id = reversal.reversal_of_entry_id
        WHERE (? IS NULL OR reversal.id < ?)
     )
     SELECT COALESCE(SUM(jl.debit_amount), 0) -
            COALESCE(SUM(jl.credit_amount), 0) AS carrying_balance_dkk
       FROM effective_entries evidence
       JOIN journal_lines jl ON jl.journal_entry_id = evidence.id
       JOIN accounts a ON a.id = jl.account_id
      WHERE a.account_no = ?`,
  ).get(
    input.invoiceDocumentId,
    cutoff,
    cutoff,
    input.invoiceDocumentId,
    cutoff,
    cutoff,
    input.invoiceDocumentId,
    cutoff,
    cutoff,
    input.invoiceDocumentId,
    cutoff,
    cutoff,
    input.invoiceDocumentId,
    cutoff,
    cutoff,
    cutoff,
    cutoff,
    input.receivableAccountNo,
  ) as { carrying_balance_dkk: number | null } | null;
  return roundDkk(Number(row?.carrying_balance_dkk ?? 0));
}

/**
 * Reconstruct the principal receivable that actually existed immediately
 * before a foreign-currency payment journal was posted.
 *
 * Credit notes and write-offs round their own DKK journal lines. Reading those
 * real lines avoids manufacturing a one-øre FX result by re-scaling the
 * invoice's original DKK total a second time.
 */
export function calculateForeignReceivableRelief(
  db: Database,
  input: {
    invoiceDocumentId: number;
    invoiceNumber: string;
    receivableAccountNo: string;
    openForeignBefore: number;
    paymentForeign: number;
    beforeJournalEntryId?: number;
  },
): ForeignReceivableReliefResult {
  if (
    !Number.isFinite(input.openForeignBefore) ||
    !Number.isFinite(input.paymentForeign) ||
    !(input.openForeignBefore > 0) ||
    !(input.paymentForeign > 0) ||
    compareDkk(input.paymentForeign, input.openForeignBefore) > 0
  ) {
    return { ok: false, error: "foreign payment exceeds the reconstructable principal balance" };
  }

  const carryingBalanceDkk = calculateInvoiceReceivableCarryingBalance(db, input);
  if (!(carryingBalanceDkk > 0)) {
    return {
      ok: false,
      error: `no positive principal receivable remains on account ${input.receivableAccountNo}`,
    };
  }

  const amountDkk = compareDkk(input.paymentForeign, input.openForeignBefore) === 0
    ? carryingBalanceDkk
    : roundDkk(
      (carryingBalanceDkk * input.paymentForeign) / input.openForeignBefore,
    );
  if (!(amountDkk > 0) || compareDkk(amountDkk, carryingBalanceDkk) > 0) {
    return { ok: false, error: "foreign receivable relief is outside the carrying balance" };
  }
  return { ok: true, amountDkk, carryingBalanceDkk };
}
