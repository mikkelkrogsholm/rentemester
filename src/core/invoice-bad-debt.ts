import { TransactionRejectionError, decodeTransactionRejection } from "./transaction-rejection";
import type { Database } from "bun:sqlite";
import { getInvoiceStatus } from "./invoice-payments";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { insertAuditLog } from "./actor";
import { resolveAccountRole } from "./account-roles";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { compareDkk, roundDkk, roundRate6 } from "./money";
import {
  calculateForeignReceivableRelief,
  calculateInvoiceReceivableCarryingBalance,
  resolveInvoiceReceivableAccount,
} from "./invoice-fx-receivable";

const RULE_ID = "DK-INVOICE-BAD-DEBT-WRITEOFF-001";
const VAT_RULE_ID = "DK-VAT-BAD-DEBT-001";

function resolveBadDebtExpenseAccount(db: Database, requestedAccountNo = "3080") {
  const accountNo = requestedAccountNo.trim();
  const account = db.query(
    `SELECT account_no, type, normal_balance, active, allow_direct_posting
       FROM accounts
      WHERE account_no = ?`,
  ).get(accountNo) as {
    account_no: string;
    type: string;
    normal_balance: string;
    active: number;
    allow_direct_posting: number;
  } | null;
  if (!account) return { ok: false as const, error: `bad-debt expense account ${accountNo || "(blank)"} does not exist` };
  if (
    account.active !== 1 ||
    account.allow_direct_posting !== 1 ||
    account.type !== "expense" ||
    account.normal_balance !== "debit"
  ) {
    return {
      ok: false as const,
      error: `bad-debt expense account ${accountNo} must be an active directly-postable debit-normal expense account`,
    };
  }
  return { ok: true as const, accountNo };
}

export type WriteOffInvoiceBadDebtInput = {
  invoiceDocumentId: number;
  writeOffDate: string;
  grossAmount?: number;
  expenseAccountNo?: string;
  receivableAccountNo?: string;
  vatAccountNo?: string;
  note?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type WriteOffInvoiceBadDebtResult = JournalPostResult & {
  writeOffId?: number;
  invoiceNumber?: string;
  grossAmount?: number;
  netAmount?: number;
  vatAmount?: number;
  openBalance?: number;
  claimOpenBalance?: number;
};

function calculateBadDebtVatAllocation(db: Database, input: {
  invoiceDocumentId: number;
  writeOffDate: string;
  grossAmount: number;
  invoiceGrossAmount: number;
  invoiceVatAmount: number;
  currency: string;
}) {
  const prior = db.query(
    `SELECT
       COALESCE((
         SELECT SUM(credit.amount_inc_vat)
           FROM credit_note_postings posting
           JOIN documents credit ON credit.id = posting.credit_note_document_id
          WHERE posting.original_invoice_document_id = ?
            AND credit.document_type = 'credit_note'
            AND credit.invoice_date <= ?
       ), 0) + COALESCE((
         SELECT SUM(gross_amount)
           FROM invoice_bad_debt_writeoffs
          WHERE invoice_document_id = ?
            AND writeoff_date <= ?
       ), 0) AS relieved_gross,
       COALESCE((
         SELECT SUM(credit.vat_amount)
           FROM credit_note_postings posting
           JOIN documents credit ON credit.id = posting.credit_note_document_id
          WHERE posting.original_invoice_document_id = ?
            AND credit.document_type = 'credit_note'
            AND credit.invoice_date <= ?
       ), 0) + COALESCE((
         SELECT SUM(vat_amount)
           FROM invoice_bad_debt_writeoffs
          WHERE invoice_document_id = ?
            AND writeoff_date <= ?
       ), 0) AS relieved_domain_vat,
       COALESCE((
         SELECT SUM(line.credit_amount)
           FROM issued_invoice_postings posting
           JOIN journal_lines line ON line.journal_entry_id = posting.journal_entry_id
           JOIN accounts account ON account.id = line.account_id
          WHERE posting.invoice_document_id = ?
            AND account.type = 'vat'
            AND account.normal_balance = 'credit'
            AND line.debit_amount = 0
            AND line.credit_amount > 0
            AND line.vat_code IS NULL
       ), 0) AS original_output_vat_dkk,
       COALESCE((
         SELECT SUM(line.debit_amount)
           FROM credit_note_postings posting
           JOIN documents credit ON credit.id = posting.credit_note_document_id
           JOIN journal_lines line ON line.journal_entry_id = posting.journal_entry_id
           JOIN accounts account ON account.id = line.account_id
          WHERE posting.original_invoice_document_id = ?
            AND credit.document_type = 'credit_note'
            AND credit.invoice_date <= ?
            AND account.type = 'vat'
            AND account.normal_balance = 'credit'
            AND line.debit_amount > 0
            AND line.credit_amount = 0
            AND line.vat_code IS NULL
       ), 0) + COALESCE((
         SELECT SUM(line.debit_amount)
           FROM invoice_bad_debt_writeoffs writeoff
           JOIN journal_lines line ON line.journal_entry_id = writeoff.journal_entry_id
           JOIN accounts account ON account.id = line.account_id
          WHERE writeoff.invoice_document_id = ?
            AND writeoff.writeoff_date <= ?
            AND account.type = 'vat'
            AND account.normal_balance = 'credit'
            AND line.debit_amount > 0
            AND line.credit_amount = 0
            AND line.vat_code IS NULL
       ), 0) AS relieved_output_vat_dkk`,
  ).get(
    input.invoiceDocumentId, input.writeOffDate,
    input.invoiceDocumentId, input.writeOffDate,
    input.invoiceDocumentId, input.writeOffDate,
    input.invoiceDocumentId, input.writeOffDate,
    input.invoiceDocumentId,
    input.invoiceDocumentId, input.writeOffDate,
    input.invoiceDocumentId, input.writeOffDate,
  ) as {
    relieved_gross: number;
    relieved_domain_vat: number;
    original_output_vat_dkk: number;
    relieved_output_vat_dkk: number;
  };
  const relievedGross = roundDkk(Number(prior.relieved_gross ?? 0));
  const relievedDomainVat = roundDkk(Number(prior.relieved_domain_vat ?? 0));
  const originalOutputVatDkk = roundDkk(Number(prior.original_output_vat_dkk ?? 0));
  const relievedOutputVatDkk = roundDkk(Number(prior.relieved_output_vat_dkk ?? 0));
  const cumulativeRelief = roundDkk(relievedGross + input.grossAmount);
  if (compareDkk(cumulativeRelief, input.invoiceGrossAmount) > 0) {
    return { ok: false as const, error: "bad-debt and credit-note VAT relief exceeds the invoice gross amount" };
  }
  if (!(originalOutputVatDkk > 0)) {
    return { ok: false as const, error: "bad-debt VAT relief cannot reconstruct the booked output VAT" };
  }
  const cumulativeDomainVat = compareDkk(cumulativeRelief, input.invoiceGrossAmount) === 0
    ? input.invoiceVatAmount
    : roundDkk((input.invoiceVatAmount * cumulativeRelief) / input.invoiceGrossAmount);
  const vatAmount = roundDkk(cumulativeDomainVat - relievedDomainVat);
  const cumulativeOutputVatDkk = compareDkk(cumulativeRelief, input.invoiceGrossAmount) === 0
    ? originalOutputVatDkk
    : roundDkk((originalOutputVatDkk * cumulativeRelief) / input.invoiceGrossAmount);
  const vatAmountDkk = input.currency === "DKK"
    ? vatAmount
    : roundDkk(cumulativeOutputVatDkk - relievedOutputVatDkk);
  if (vatAmount < 0 || vatAmount > input.grossAmount || vatAmountDkk < 0) {
    return { ok: false as const, error: "bad-debt VAT relief allocation is outside the remaining invoice basis" };
  }
  return { ok: true as const, vatAmount, vatAmountDkk };
}


export function writeOffInvoiceBadDebt(db: Database, input: WriteOffInvoiceBadDebtInput): WriteOffInvoiceBadDebtResult {
  const errors: string[] = [];
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) errors.push("invoiceDocumentId must be a positive integer");
  if (!looksLikeIsoDate(input.writeOffDate)) errors.push("writeOffDate must be YYYY-MM-DD");
  if (input.grossAmount !== undefined && (!Number.isFinite(input.grossAmount) || input.grossAmount <= 0)) errors.push("grossAmount must be a positive number when present");
  for (const [field, value] of [
    ["expenseAccountNo", input.expenseAccountNo],
    ["receivableAccountNo", input.receivableAccountNo],
    ["vatAccountNo", input.vatAccountNo],
  ] as const) {
    if (value !== undefined && (typeof value !== "string" || value.trim().length === 0)) {
      errors.push(`${field} must be a non-empty account number when present`);
    }
  }
  if (errors.length > 0) return { ok: false, appliedRules: [RULE_ID, VAT_RULE_ID], errors };

  const invoice = db.query(
    `SELECT id, invoice_no, invoice_date, amount_inc_vat, vat_amount, currency, payload_json, document_type
     FROM documents WHERE id = ?`
  ).get(input.invoiceDocumentId) as {
    id: number;
    invoice_no: string;
    invoice_date: string | null;
    amount_inc_vat: number | null;
    vat_amount: number | null;
    currency: string;
    payload_json: string | null;
    document_type: string;
  } | null;
  if (!invoice) return { ok: false, appliedRules: [RULE_ID, VAT_RULE_ID], errors: [`invoice document ${input.invoiceDocumentId} does not exist`] };
  if (invoice.document_type !== "issued_invoice") return { ok: false, appliedRules: [RULE_ID, VAT_RULE_ID], errors: [`document ${input.invoiceDocumentId} is not an issued invoice`] };
  if (!invoice.invoice_date || input.writeOffDate < invoice.invoice_date) {
    return {
      ok: false,
      appliedRules: [RULE_ID, VAT_RULE_ID],
      errors: [`bad-debt writeOffDate ${input.writeOffDate} cannot be before invoice date ${invoice.invoice_date ?? "(missing)"}`],
    };
  }

  const payload = invoice.payload_json ? JSON.parse(invoice.payload_json) : null;
  const currency = (invoice.currency ?? payload?.currency ?? "DKK").trim().toUpperCase();
  if (payload?.vatTreatment !== "standard") {
    return { ok: false, appliedRules: [RULE_ID, VAT_RULE_ID], errors: ["bad-debt VAT relief currently requires a standard-rated issued invoice"] };
  }

  const grossInvoiceAmount = roundDkk(Number(invoice.amount_inc_vat ?? 0));
  const originalVatAmount = roundDkk(Number(invoice.vat_amount ?? 0));
  const fxRateToDkk = currency === "DKK" ? null : Number(payload?.totals?.fxRateToDkk ?? 0);
  const grossInvoiceAmountDkk = currency === "DKK" ? grossInvoiceAmount : roundDkk(Number(payload?.totals?.grossAmountDkk ?? 0));
  const originalVatAmountDkk = currency === "DKK" ? originalVatAmount : roundDkk(Number(payload?.totals?.vatAmountDkk ?? 0));
  if (!(grossInvoiceAmount > 0) || !(originalVatAmount > 0)) {
    return { ok: false, appliedRules: [RULE_ID, VAT_RULE_ID], errors: ["bad-debt VAT relief requires a positive gross invoice amount and VAT amount"] };
  }
  if (currency !== "DKK" && !(grossInvoiceAmountDkk > 0 && originalVatAmountDkk > 0 && Number.isFinite(fxRateToDkk) && fxRateToDkk! > 0)) {
    return { ok: false, appliedRules: [RULE_ID, VAT_RULE_ID], errors: ["non-DKK bad-debt write-offs require deterministic DKK invoice totals"] };
  }

  const status = getInvoiceStatus(db, input.invoiceDocumentId, input.writeOffDate);
  if (!status.ok) return { ok: false, appliedRules: [RULE_ID, VAT_RULE_ID], errors: status.errors };
  const openBalance = roundDkk(Number(status.openBalance ?? 0));
  if (!(openBalance > 0)) return { ok: false, appliedRules: [RULE_ID, VAT_RULE_ID], errors: [`invoice ${invoice.invoice_no} has no open principal balance to write off`] };

  const grossAmount = roundDkk(input.grossAmount ?? openBalance);
  if (grossAmount > openBalance) {
    return { ok: false, appliedRules: [RULE_ID, VAT_RULE_ID], errors: [`bad-debt write-off amount ${grossAmount} exceeds open principal balance ${openBalance}`] };
  }

  const outputVat = resolveAccountRole(db, "output_vat");
  if (!outputVat.ok) return { ok: false, appliedRules: [RULE_ID, VAT_RULE_ID], errors: [outputVat.error] };
  if (input.vatAccountNo !== undefined && input.vatAccountNo.trim() !== outputVat.accountNo) {
    return {
      ok: false,
      appliedRules: [RULE_ID, VAT_RULE_ID],
      errors: [`bad-debt VAT relief must use the confirmed output VAT account ${outputVat.accountNo}, not ${input.vatAccountNo.trim() || "(blank)"}`],
    };
  }
  const expense = resolveBadDebtExpenseAccount(db, input.expenseAccountNo ?? "3080");
  if (!expense.ok) return { ok: false, appliedRules: [RULE_ID, VAT_RULE_ID], errors: [expense.error] };

  try {
    const result = db.transaction(() => {
      const lockedStatus = getInvoiceStatus(db, input.invoiceDocumentId, input.writeOffDate);
      if (!lockedStatus.ok) throw new TransactionRejectionError({ appliedRules: [RULE_ID, VAT_RULE_ID], errors: lockedStatus.errors });
      const lockedOpenBalance = roundDkk(Number(lockedStatus.openBalance ?? 0));
      if (grossAmount > lockedOpenBalance) {
        throw new TransactionRejectionError({
          appliedRules: [RULE_ID, VAT_RULE_ID],
          errors: [`bad-debt write-off amount ${grossAmount} exceeds open principal balance ${lockedOpenBalance}`],
        });
      }
      const receivable = resolveInvoiceReceivableAccount(db, {
        invoiceDocumentId: input.invoiceDocumentId,
      });
      if (!receivable.ok) throw new TransactionRejectionError({ appliedRules: [RULE_ID, VAT_RULE_ID], errors: [receivable.error] });
      if (input.receivableAccountNo && input.receivableAccountNo !== receivable.accountNo) {
        throw new TransactionRejectionError({
          appliedRules: [RULE_ID, VAT_RULE_ID],
          errors: [`invoice ${invoice.invoice_no} must write off its booked receivable account ${receivable.accountNo}, not ${input.receivableAccountNo}`],
        });
      }
      const carryingBalance = calculateInvoiceReceivableCarryingBalance(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        invoiceNumber: invoice.invoice_no,
        receivableAccountNo: receivable.accountNo,
      });
      if (currency === "DKK" && carryingBalance !== lockedOpenBalance) {
        throw new TransactionRejectionError({
          appliedRules: [RULE_ID, VAT_RULE_ID],
          errors: [`invoice ${invoice.invoice_no} domain balance ${lockedOpenBalance} DKK does not match receivable ${receivable.accountNo} carrying balance ${carryingBalance} DKK`],
        });
      }
      const vatAllocation = calculateBadDebtVatAllocation(db, {
        invoiceDocumentId: input.invoiceDocumentId,
        writeOffDate: input.writeOffDate,
        grossAmount,
        invoiceGrossAmount: grossInvoiceAmount,
        invoiceVatAmount: originalVatAmount,
        currency,
      });
      if (!vatAllocation.ok) {
        throw new TransactionRejectionError({
          appliedRules: [RULE_ID, VAT_RULE_ID],
          errors: [vatAllocation.error],
        });
      }
      const vatAmount = vatAllocation.vatAmount;
      const netAmount = roundDkk(grossAmount - vatAmount);
      let grossAmountDkk = grossAmount;
      let writeOffFxRateToDkk: number | undefined;
      if (currency !== "DKK") {
        const relief = calculateForeignReceivableRelief(db, {
          invoiceDocumentId: input.invoiceDocumentId,
          invoiceNumber: invoice.invoice_no,
          receivableAccountNo: receivable.accountNo,
          openForeignBefore: lockedOpenBalance,
          paymentForeign: grossAmount,
        });
        if (!relief.ok) {
          throw new TransactionRejectionError({
            appliedRules: [RULE_ID, VAT_RULE_ID],
            errors: [`foreign bad-debt receivable relief cannot be reconstructed: ${relief.error}`],
          });
        }
        grossAmountDkk = relief.amountDkk;
        writeOffFxRateToDkk = roundRate6(grossAmountDkk / grossAmount);
        if (compareDkk(roundDkk(grossAmount * writeOffFxRateToDkk), grossAmountDkk) !== 0) {
          throw new TransactionRejectionError({
            appliedRules: [RULE_ID, VAT_RULE_ID],
            errors: [
              `foreign bad-debt carrying balance ${grossAmountDkk} DKK cannot be represented by the journal's six-decimal FX basis`,
            ],
          });
        }
      }
      const vatAmountDkk = vatAllocation.vatAmountDkk;
      const netAmountDkk = currency === "DKK" ? netAmount : roundDkk(grossAmountDkk - vatAmountDkk);
      const journal = postJournalEntry(db, {
        transactionDate: input.writeOffDate,
        text: `Bad debt write-off for invoice ${invoice.invoice_no}`,
        documentId: input.invoiceDocumentId,
        currency: currency === "DKK" ? undefined : currency,
        amountForeign: currency === "DKK" ? undefined : grossAmount,
        amountDkk: currency === "DKK" ? undefined : grossAmountDkk,
        fxRateToDkk: currency === "DKK" ? undefined : writeOffFxRateToDkk,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: [
          { accountNo: expense.accountNo, debitAmount: netAmountDkk, vatCode: "DK_BAD_DEBT_25", text: `Bad debt loss basis ${invoice.invoice_no}` },
          ...(vatAmountDkk > 0
            ? [{ accountNo: outputVat.accountNo, debitAmount: vatAmountDkk, text: `Output VAT relief ${invoice.invoice_no}` }]
            : []),
          { accountNo: receivable.accountNo, creditAmount: grossAmountDkk, text: `Write off receivable ${invoice.invoice_no}` },
        ],
      });
      if (!journal.ok) throw new TransactionRejectionError({ appliedRules: journal.appliedRules, errors: journal.errors });

      const writeOff = db.query(
        `INSERT INTO invoice_bad_debt_writeoffs (invoice_document_id, writeoff_date, gross_amount, net_amount, vat_amount, note, journal_entry_id)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         RETURNING id`
      ).get(input.invoiceDocumentId, input.writeOffDate, grossAmount, netAmount, vatAmount, input.note ?? null, journal.entryId!) as { id: number };

      insertAuditLog(db, {
        eventType: "invoice_bad_debt_writeoff",
        entityType: "invoice_bad_debt_writeoff",
        entityId: writeOff.id,
        message: `Wrote off bad debt ${grossAmount} on invoice ${invoice.invoice_no}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      const after = getInvoiceStatus(db, input.invoiceDocumentId, input.writeOffDate);
      if (!after.ok) throw new TransactionRejectionError({ errors: after.errors });

      return {
        ...journal,
        writeOffId: writeOff.id,
        invoiceNumber: invoice.invoice_no,
        grossAmount,
        netAmount,
        vatAmount,
        openBalance: after.openBalance,
        claimOpenBalance: after.claimOpenBalance,
        appliedRules: [...new Set([RULE_ID, VAT_RULE_ID, ...(journal.appliedRules ?? [])])],
      };
    }).immediate();
    return result;
  } catch (error) {
    const parsed = decodeTransactionRejection(error);
    return {
      ok: false,
      appliedRules: [...new Set([RULE_ID, VAT_RULE_ID, ...(parsed?.appliedRules ?? [])])],
      errors: parsed?.errors ?? [String(error)],
    };
  }
}
