/**
 * Kreditorstyring — the accounts-payable open-item register.
 *
 * This is the creditor-side counterpart to the debitor-side invoice register
 * (`src/core/invoice-list.ts`, `src/core/invoice-payments.ts`). A registered
 * supplier bill is an OPEN ITEM: it owes money to a creditor, carries a due
 * date, and is recognised in the append-only ledger as a balanced journal
 * entry (debit expense + købsmoms, credit 7000 Leverandørgæld). Outgoing bank
 * payments are matched against the open item, posting a settlement entry
 * (debit 7000 Leverandørgæld, credit bank). The open balance is the gross
 * amount minus the sum of applied payments.
 *
 * FIRST SLICE — deliberately narrow:
 *  - DKK-only. A registered bill must be a DKK purchase document; foreign
 *    currency is out of scope (the debitor side's fx handling lives elsewhere).
 *  - VAT treatments: standard 25 % deductible input VAT, or exempt (no VAT).
 *    Reverse-charge / representation bills stay on the existing
 *    `expense-booking.ts` path; they are not modelled as payables open items.
 *  - No partial-payment fan-out beyond simple sum-of-payments; over-payment is
 *    rejected, not booked as a creditor over-payment.
 */

import type { Database } from "bun:sqlite";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { insertAuditLog } from "./actor";
import { isValidIsoDate as looksLikeIsoDate, diffDays, todayIsoDate } from "./dates";
import { absDkk, compareDkk, percentOfDkk, roundDkk, subtractDkk, sumDkk } from "./money";

const RULE_ID = "DK-PAYABLE-001";
const PAYMENT_RULE_ID = "DK-PAYABLE-PAYMENT-001";
/** 7000 Leverandørgæld (kreditorer) — the trade-creditor liability account. */
const CREDITOR_ACCOUNT_NO = "7000";
const DEFAULT_PAYMENT_ACCOUNT_NO = "2000";
const DEFAULT_VAT_ACCOUNT_NO = "4000";

export type RegisterPayableInput = {
  documentId: number;
  billDate: string;
  dueDate: string;
  expenseAccountNo: string;
  /** "standard" books 25 % deductible input VAT; "exempt" books no VAT. */
  vatTreatment?: "standard" | "exempt";
  vendorId?: number;
  vatAccountNo?: string;
  note?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type RegisterPayableResult = JournalPostResult & {
  payableId?: number;
  documentId?: number;
  supplierName?: string | null;
  billNo?: string | null;
  grossAmount?: number;
  netAmount?: number;
  vatAmount?: number;
  dueDate?: string;
};

export type PayPayableInput = {
  payableId: number;
  bankTransactionId: number;
  paymentDate?: string;
  amount?: number;
  paymentAccountNo?: string;
  note?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type PayPayableResult = {
  ok: boolean;
  paymentId?: number;
  journalEntryId?: number;
  payableId?: number;
  openBalance?: number;
  appliedRules: string[];
  errors: string[];
};

export type PayableStatus = "open" | "paid";

export type PayableStatusResult = {
  ok: boolean;
  payableId?: number;
  documentId?: number;
  supplierName?: string | null;
  billNo?: string | null;
  billDate?: string;
  dueDate?: string;
  grossAmount?: number;
  netAmount?: number;
  vatAmount?: number;
  currency?: string;
  paidAmount?: number;
  openBalance?: number;
  status?: PayableStatus;
  asOfDate?: string;
  isOverdue?: boolean;
  overdueDays?: number;
  payments?: Array<{
    paymentId: number;
    paymentDate: string;
    amount: number;
    bankTransactionId: number | null;
    journalEntryId: number | null;
    note: string | null;
  }>;
  errors: string[];
};

/** Aging bucket for an open creditor item, by days past the due date. */
export type PayableAgingBucket = "not-due" | "0-30" | "31-60" | "61-90" | "90+";

export type PayableQueryStatus = "open" | "paid" | "overdue" | "all";

export type PayablesListFilters = {
  status?: PayableQueryStatus;
  asOfDate?: string;
  vendorId?: number;
  supplier?: string;
  from?: string;
  to?: string;
  minDays?: number;
};

export type PayablesListRow = {
  payableId: number;
  documentId: number;
  billNo: string | null;
  billDate: string;
  dueDate: string;
  supplierName: string | null;
  vendorId: number | null;
  grossAmount: number;
  currency: string;
  paidAmount: number;
  openBalance: number;
  status: PayableStatus;
  isOverdue: boolean;
  overdueDays: number;
  agingBucket: PayableAgingBucket;
};

export type PayablesListResult = {
  ok: boolean;
  count: number;
  status: PayableQueryStatus;
  asOfDate: string;
  totalOpenBalance: number;
  overdueOpenBalance: number;
  notYetDueOpenBalance: number;
  rows: PayablesListRow[];
  errors: string[];
};

type PayableRow = {
  id: number;
  document_id: number;
  vendor_id: number | null;
  supplier_name: string | null;
  bill_no: string | null;
  bill_date: string;
  due_date: string;
  gross_amount: number;
  net_amount: number;
  vat_amount: number;
  currency: string;
  journal_entry_id: number;
  note: string | null;
};

function agingBucket(isOverdue: boolean, overdueDays: number): PayableAgingBucket {
  if (!isOverdue) return "not-due";
  if (overdueDays <= 30) return "0-30";
  if (overdueDays <= 60) return "31-60";
  if (overdueDays <= 90) return "61-90";
  return "90+";
}

function getPayableRow(db: Database, payableId: number): PayableRow | null {
  return db.query(
    `SELECT id, document_id, vendor_id, supplier_name, bill_no, bill_date, due_date,
            gross_amount, net_amount, vat_amount, currency, journal_entry_id, note
     FROM payables WHERE id = ?`,
  ).get(payableId) as PayableRow | null;
}

/**
 * Registers a booked supplier bill as an open creditor item and posts the
 * bill-recognition journal entry (debit expense + købsmoms, credit 7000
 * Leverandørgæld). Idempotent on `documentId`: a second registration of the
 * same purchase document is rejected.
 */
export function registerPayable(db: Database, input: RegisterPayableInput): RegisterPayableResult {
  const errors: string[] = [];
  if (!Number.isInteger(input.documentId) || input.documentId <= 0) errors.push("documentId must be a positive integer");
  if (!looksLikeIsoDate(input.billDate)) errors.push("billDate must be YYYY-MM-DD");
  if (!looksLikeIsoDate(input.dueDate)) errors.push("dueDate must be YYYY-MM-DD");
  if (typeof input.expenseAccountNo !== "string" || input.expenseAccountNo.trim().length === 0) errors.push("expenseAccountNo is required");
  if (input.vatTreatment && !["standard", "exempt"].includes(input.vatTreatment)) {
    errors.push("vatTreatment must be one of standard, exempt when present");
  }
  if (input.vendorId !== undefined && (!Number.isInteger(input.vendorId) || input.vendorId <= 0)) {
    errors.push("vendorId must be a positive integer when present");
  }
  if (errors.length === 0 && input.dueDate < input.billDate) errors.push("dueDate must not be before billDate");
  if (errors.length > 0) return { ok: false, appliedRules: [RULE_ID], errors };

  const expenseAccountNo = input.expenseAccountNo.trim();
  const account = db.query(
    `SELECT account_no, type, active FROM accounts WHERE account_no = ?`,
  ).get(expenseAccountNo) as { account_no: string; type: string; active: number } | null;
  if (!account) return { ok: false, appliedRules: [RULE_ID], errors: [`expense account ${expenseAccountNo} does not exist`] };
  if (account.type !== "expense") return { ok: false, appliedRules: [RULE_ID], errors: [`account ${expenseAccountNo} is not an expense account`] };
  if (!account.active) return { ok: false, appliedRules: [RULE_ID], errors: [`account ${expenseAccountNo} is inactive`] };

  const document = db.query(
    `SELECT id, document_type, invoice_no, amount_inc_vat, vat_amount, currency, sender_name
     FROM documents WHERE id = ?`,
  ).get(input.documentId) as {
    id: number;
    document_type: string;
    invoice_no: string | null;
    amount_inc_vat: number | null;
    vat_amount: number | null;
    currency: string;
    sender_name: string | null;
  } | null;
  if (!document) return { ok: false, appliedRules: [RULE_ID], errors: [`document ${input.documentId} does not exist`] };
  if (document.document_type !== "purchase_sale" && document.document_type !== "cash_register_receipt") {
    return { ok: false, appliedRules: [RULE_ID], errors: [`document ${input.documentId} is not a purchase document`] };
  }
  if ((document.currency ?? "DKK").trim().toUpperCase() !== "DKK") {
    return { ok: false, appliedRules: [RULE_ID], errors: [`document ${input.documentId} is not in DKK — foreign-currency payables are out of scope for this slice`] };
  }

  const grossAmount = roundDkk(Number(document.amount_inc_vat ?? 0));
  const vatAmount = roundDkk(Number(document.vat_amount ?? 0));
  if (!(grossAmount > 0)) return { ok: false, appliedRules: [RULE_ID], errors: [`document ${input.documentId} must have amount_inc_vat > 0`] };
  if (vatAmount < 0 || vatAmount > grossAmount) return { ok: false, appliedRules: [RULE_ID], errors: [`document ${input.documentId} has invalid vat_amount ${vatAmount}`] };

  const existing = db.query(`SELECT id FROM payables WHERE document_id = ? LIMIT 1`).get(input.documentId) as { id: number } | null;
  if (existing) return { ok: false, appliedRules: [RULE_ID], errors: [`document ${input.documentId} is already registered as payable ${existing.id}`] };

  const vatTreatment = input.vatTreatment ?? (vatAmount > 0 ? "standard" : "exempt");
  if (vatTreatment === "exempt" && vatAmount !== 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["exempt payable registration requires document vat_amount = 0"] };
  }
  if (vatTreatment === "standard") {
    if (!(vatAmount > 0)) return { ok: false, appliedRules: [RULE_ID], errors: ["standard payable registration requires document vat_amount > 0"] };
    // The document vat_amount becomes deductible input VAT — it must be
    // consistent with the 25 % rate rather than trusted blindly (a garbled or
    // OCR-extracted amount would otherwise be booked verbatim). 1 øre slack.
    const documentNetAmount = subtractDkk(grossAmount, vatAmount);
    const expectedVatAmount = percentOfDkk(documentNetAmount, 25);
    if (compareDkk(absDkk(subtractDkk(vatAmount, expectedVatAmount)), 0.01) > 0) {
      return {
        ok: false,
        appliedRules: [RULE_ID],
        errors: [`document ${input.documentId} vat_amount ${vatAmount} is inconsistent with the 25% rate (expected ~${expectedVatAmount} for net ${documentNetAmount})`],
      };
    }
  }

  const netAmount = subtractDkk(grossAmount, vatAmount);
  const vatAccountNo = input.vatAccountNo?.trim() || DEFAULT_VAT_ACCOUNT_NO;
  const supplierName = document.sender_name?.trim() || null;
  const text = supplierName
    ? `Kreditorpost: bilag fra ${supplierName} (bilag ${input.documentId})`
    : `Kreditorpost (bilag ${input.documentId})`;

  const lines = vatTreatment === "standard"
    ? [
        { accountNo: expenseAccountNo, debitAmount: netAmount, vatCode: "DK_PURCHASE_25", text: document.invoice_no ?? "Udgift, grundbeløb" },
        { accountNo: vatAccountNo, debitAmount: vatAmount, text: "Købsmoms" },
        { accountNo: CREDITOR_ACCOUNT_NO, creditAmount: grossAmount, text: supplierName ? `Leverandørgæld ${supplierName}` : "Leverandørgæld" },
      ]
    : [
        { accountNo: expenseAccountNo, debitAmount: grossAmount, text: document.invoice_no ?? "Udgift" },
        { accountNo: CREDITOR_ACCOUNT_NO, creditAmount: grossAmount, text: supplierName ? `Leverandørgæld ${supplierName}` : "Leverandørgæld" },
      ];

  try {
    return db.transaction(() => {
      const journal = postJournalEntry(db, {
        transactionDate: input.billDate,
        text,
        documentId: input.documentId,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines,
      });
      if (!journal.ok || journal.entryId == null) {
        throw new Error(JSON.stringify({ appliedRules: journal.appliedRules, errors: journal.errors }));
      }

      const inserted = db.query(
        `INSERT INTO payables
           (document_id, vendor_id, supplier_name, bill_no, bill_date, due_date,
            gross_amount, net_amount, vat_amount, currency, journal_entry_id, note)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'DKK', ?, ?)
         RETURNING id`,
      ).get(
        input.documentId,
        input.vendorId ?? null,
        supplierName,
        document.invoice_no ?? null,
        input.billDate,
        input.dueDate,
        grossAmount,
        netAmount,
        vatAmount,
        journal.entryId,
        input.note ?? null,
      ) as { id: number };

      insertAuditLog(db, {
        eventType: "payable_register",
        entityType: "payable",
        entityId: inserted.id,
        message: `Registered payable ${grossAmount} DKK${supplierName ? ` to ${supplierName}` : ""} (due ${input.dueDate})`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      return {
        ok: true,
        payableId: inserted.id,
        documentId: input.documentId,
        supplierName,
        billNo: document.invoice_no ?? null,
        grossAmount,
        netAmount,
        vatAmount,
        dueDate: input.dueDate,
        entryId: journal.entryId,
        entryNo: journal.entryNo,
        entryHash: journal.entryHash,
        appliedRules: [RULE_ID, ...journal.appliedRules],
        errors: [],
      } satisfies RegisterPayableResult;
    })();
  } catch (error) {
    const parsed = parseTransactionError(error);
    return {
      ok: false,
      appliedRules: [...new Set([RULE_ID, ...(parsed?.appliedRules ?? [])])],
      errors: parsed?.errors ?? [String(error)],
    };
  }
}

/** Open balance (gross minus applied payments) for a single payable. */
function openBalanceOf(db: Database, payable: PayableRow): number {
  const payments = db.query(
    `SELECT amount FROM payable_payments WHERE payable_id = ?`,
  ).all(payable.id) as Array<{ amount: number }>;
  const paid = sumDkk(payments.map((p) => Number(p.amount)));
  return subtractDkk(roundDkk(Number(payable.gross_amount)), paid);
}

export function getPayableStatus(db: Database, payableId: number, asOfDate?: string): PayableStatusResult {
  if (!Number.isInteger(payableId) || payableId <= 0) {
    return { ok: false, errors: ["payableId must be a positive integer"] };
  }
  const payable = getPayableRow(db, payableId);
  if (!payable) return { ok: false, errors: [`payable ${payableId} does not exist`] };

  const payments = db.query(
    `SELECT id, payment_date, amount, bank_transaction_id, journal_entry_id, note
     FROM payable_payments WHERE payable_id = ? ORDER BY id ASC`,
  ).all(payableId) as Array<{ id: number; payment_date: string; amount: number; bank_transaction_id: number | null; journal_entry_id: number; note: string | null }>;

  const grossAmount = roundDkk(Number(payable.gross_amount));
  const paidAmount = sumDkk(payments.map((p) => Number(p.amount)));
  const openBalance = subtractDkk(grossAmount, paidAmount);
  const comparisonDate = asOfDate ?? payable.due_date;
  const overdueDays = openBalance > 0 ? Math.max(0, diffDays(payable.due_date, comparisonDate)) : 0;
  const isOverdue = overdueDays > 0;
  const status: PayableStatus = openBalance > 0 ? "open" : "paid";

  return {
    ok: true,
    payableId,
    documentId: payable.document_id,
    supplierName: payable.supplier_name,
    billNo: payable.bill_no,
    billDate: payable.bill_date,
    dueDate: payable.due_date,
    grossAmount,
    netAmount: roundDkk(Number(payable.net_amount)),
    vatAmount: roundDkk(Number(payable.vat_amount)),
    currency: payable.currency,
    paidAmount,
    openBalance,
    status,
    asOfDate: comparisonDate,
    isOverdue,
    overdueDays,
    payments: payments.map((p) => ({
      paymentId: p.id,
      paymentDate: p.payment_date,
      amount: roundDkk(Number(p.amount)),
      bankTransactionId: p.bank_transaction_id,
      journalEntryId: Number(p.journal_entry_id),
      note: p.note,
    })),
    errors: [],
  };
}

/**
 * Matches an outgoing bank payment against an open payable and posts the
 * settlement entry (debit 7000 Leverandørgæld, credit bank). The bank
 * transaction must be an outgoing payment (negative amount), in DKK, and not
 * already linked to a payable payment or any journal entry.
 */
export function payPayableFromBank(db: Database, input: PayPayableInput): PayPayableResult {
  const errors: string[] = [];
  if (!Number.isInteger(input.payableId) || input.payableId <= 0) errors.push("payableId must be a positive integer");
  if (!Number.isInteger(input.bankTransactionId) || input.bankTransactionId <= 0) errors.push("bankTransactionId must be a positive integer");
  if (input.paymentDate !== undefined && !looksLikeIsoDate(input.paymentDate)) errors.push("paymentDate must be YYYY-MM-DD when present");
  if (input.amount !== undefined && (!Number.isFinite(input.amount) || input.amount <= 0)) errors.push("amount must be a positive number when present");
  if (errors.length > 0) return { ok: false, appliedRules: [PAYMENT_RULE_ID], errors };

  const payable = getPayableRow(db, input.payableId);
  if (!payable) return { ok: false, appliedRules: [PAYMENT_RULE_ID], errors: [`payable ${input.payableId} does not exist`] };

  const bank = db.query(
    `SELECT id, transaction_date, amount, text, currency FROM bank_transactions WHERE id = ?`,
  ).get(input.bankTransactionId) as { id: number; transaction_date: string; amount: number; text: string; currency: string } | null;
  if (!bank) return { ok: false, appliedRules: [PAYMENT_RULE_ID], errors: [`bank transaction ${input.bankTransactionId} does not exist`] };
  if (!(Number(bank.amount) < 0)) return { ok: false, appliedRules: [PAYMENT_RULE_ID], errors: [`bank transaction ${input.bankTransactionId} is not an outgoing payment`] };
  if ((bank.currency ?? "DKK").trim().toUpperCase() !== "DKK") {
    return { ok: false, appliedRules: [PAYMENT_RULE_ID], errors: [`bank transaction ${input.bankTransactionId} is not in DKK — foreign-currency payable settlement is out of scope for this slice`] };
  }

  const existingJournal = db.query(`SELECT id FROM journal_entries WHERE source_bank_transaction_id = ? LIMIT 1`).get(bank.id) as { id: number } | null;
  if (existingJournal) return { ok: false, appliedRules: [PAYMENT_RULE_ID], errors: [`bank transaction ${bank.id} is already linked to journal entry ${existingJournal.id}`] };
  const existingPayment = db.query(`SELECT id FROM payable_payments WHERE bank_transaction_id = ? LIMIT 1`).get(bank.id) as { id: number } | null;
  if (existingPayment) return { ok: false, appliedRules: [PAYMENT_RULE_ID], errors: [`bank transaction ${bank.id} is already applied to payable payment ${existingPayment.id}`] };

  const openBalance = openBalanceOf(db, payable);
  if (!(openBalance > 0)) return { ok: false, appliedRules: [PAYMENT_RULE_ID], errors: [`payable ${input.payableId} has no open balance`] };

  const bankAmount = roundDkk(Math.abs(Number(bank.amount)));
  const amount = roundDkk(input.amount ?? bankAmount);
  if (input.amount !== undefined && compareDkk(amount, bankAmount) !== 0) {
    return { ok: false, appliedRules: [PAYMENT_RULE_ID], errors: [`payment amount ${amount} does not match bank transaction amount ${bankAmount}`] };
  }
  if (compareDkk(amount, openBalance) > 0) {
    return { ok: false, appliedRules: [PAYMENT_RULE_ID], errors: [`payment amount ${amount} exceeds open payable balance ${openBalance}`] };
  }

  const paymentDate = input.paymentDate ?? bank.transaction_date;
  const paymentAccountNo = input.paymentAccountNo?.trim() || DEFAULT_PAYMENT_ACCOUNT_NO;
  const text = payable.supplier_name
    ? `Betaling af kreditorpost til ${payable.supplier_name} (banktransaktion ${bank.id})`
    : `Betaling af kreditorpost (banktransaktion ${bank.id})`;

  try {
    return db.transaction(() => {
      const journal = postJournalEntry(db, {
        transactionDate: paymentDate,
        text,
        sourceBankTransactionId: input.bankTransactionId,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: [
          { accountNo: CREDITOR_ACCOUNT_NO, debitAmount: amount, text: payable.supplier_name ? `Leverandørgæld ${payable.supplier_name}` : "Leverandørgæld" },
          { accountNo: paymentAccountNo, creditAmount: amount, text: bank.text },
        ],
      });
      if (!journal.ok || journal.entryId == null) {
        throw new Error(JSON.stringify({ appliedRules: journal.appliedRules, errors: journal.errors }));
      }

      const inserted = db.query(
        `INSERT INTO payable_payments
           (payable_id, bank_transaction_id, journal_entry_id, payment_date, amount, currency, note)
         VALUES (?, ?, ?, ?, ?, 'DKK', ?)
         RETURNING id`,
      ).get(input.payableId, input.bankTransactionId, journal.entryId, paymentDate, amount, input.note ?? null) as { id: number };

      insertAuditLog(db, {
        eventType: "payable_payment_apply",
        entityType: "payable_payment",
        entityId: inserted.id,
        message: `Applied payment ${amount} DKK to payable ${input.payableId}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      const after = getPayableStatus(db, input.payableId);
      return {
        ok: true,
        paymentId: inserted.id,
        journalEntryId: journal.entryId,
        payableId: input.payableId,
        openBalance: after.openBalance,
        appliedRules: [PAYMENT_RULE_ID, ...journal.appliedRules],
        errors: [],
      } satisfies PayPayableResult;
    })();
  } catch (error) {
    const parsed = parseTransactionError(error);
    return {
      ok: false,
      appliedRules: [...new Set([PAYMENT_RULE_ID, ...(parsed?.appliedRules ?? [])])],
      errors: parsed?.errors ?? [String(error)],
    };
  }
}

/**
 * Builds the kreditorliste: every registered payable with its open balance and
 * aging bucket, symmetric to the debitor-side `buildInvoiceList`. Sorted with
 * the most overdue creditor items first.
 */
export function buildPayablesList(db: Database, filters: PayablesListFilters = {}): PayablesListResult {
  const status = filters.status ?? "all";
  const errors: string[] = [];
  if (filters.asOfDate !== undefined && !looksLikeIsoDate(filters.asOfDate)) errors.push("asOfDate must be YYYY-MM-DD when present");
  if (filters.from !== undefined && !looksLikeIsoDate(filters.from)) errors.push("from must be YYYY-MM-DD when present");
  if (filters.to !== undefined && !looksLikeIsoDate(filters.to)) errors.push("to must be YYYY-MM-DD when present");
  if (!["open", "paid", "overdue", "all"].includes(status)) errors.push("status must be one of open, paid, overdue, all");
  if (filters.vendorId !== undefined && (!Number.isInteger(filters.vendorId) || filters.vendorId <= 0)) {
    errors.push("vendorId must be a positive integer when present");
  }
  // Default til dagens dato så et kald uden asOfDate giver et brugbart svar:
  // status="overdue" returnerer reelt forfaldne poster i stedet for at aldre
  // mod en epoch-dato hvor intet nogensinde er forfaldent.
  const asOfDate = filters.asOfDate ?? todayIsoDate();
  if (errors.length > 0) {
    return { ok: false, count: 0, status, asOfDate, totalOpenBalance: 0, overdueOpenBalance: 0, notYetDueOpenBalance: 0, rows: [], errors };
  }

  const supplierNeedle = filters.supplier?.trim().toLocaleLowerCase() ?? "";
  const minDays = Number.isFinite(filters.minDays) ? Math.max(0, Number(filters.minDays)) : 0;

  const payables = db.query(
    `SELECT id, document_id, vendor_id, supplier_name, bill_no, bill_date, due_date,
            gross_amount, net_amount, vat_amount, currency, journal_entry_id, note
     FROM payables ORDER BY due_date ASC, id ASC`,
  ).all() as PayableRow[];

  const rows: PayablesListRow[] = [];
  for (const payable of payables) {
    if (filters.from && payable.bill_date < filters.from) continue;
    if (filters.to && payable.bill_date > filters.to) continue;
    if (filters.vendorId !== undefined && payable.vendor_id !== filters.vendorId) continue;
    if (supplierNeedle && !(payable.supplier_name ?? "").toLocaleLowerCase().includes(supplierNeedle)) continue;

    const grossAmount = roundDkk(Number(payable.gross_amount));
    const openBalance = openBalanceOf(db, payable);
    const itemStatus: PayableStatus = openBalance > 0 ? "open" : "paid";
    const overdueDays = openBalance > 0 ? Math.max(0, diffDays(payable.due_date, asOfDate)) : 0;
    const isOverdue = overdueDays > 0;

    if (status === "open" && itemStatus !== "open") continue;
    if (status === "paid" && itemStatus !== "paid") continue;
    if (status === "overdue" && (!isOverdue || overdueDays < minDays)) continue;

    rows.push({
      payableId: payable.id,
      documentId: payable.document_id,
      billNo: payable.bill_no,
      billDate: payable.bill_date,
      dueDate: payable.due_date,
      supplierName: payable.supplier_name,
      vendorId: payable.vendor_id,
      grossAmount,
      currency: payable.currency,
      paidAmount: subtractDkk(grossAmount, openBalance),
      openBalance,
      status: itemStatus,
      isOverdue,
      overdueDays,
      agingBucket: agingBucket(isOverdue, overdueDays),
    });
  }

  rows.sort((a, b) => {
    if (b.overdueDays !== a.overdueDays) return b.overdueDays - a.overdueDays;
    if (a.dueDate !== b.dueDate) return a.dueDate.localeCompare(b.dueDate);
    return a.payableId - b.payableId;
  });

  const totalOpenBalance = sumDkk(rows.map((r) => r.openBalance));
  const overdueOpenBalance = sumDkk(rows.filter((r) => r.isOverdue).map((r) => r.openBalance));
  const notYetDueOpenBalance = sumDkk(rows.filter((r) => !r.isOverdue).map((r) => r.openBalance));

  return {
    ok: true,
    count: rows.length,
    status,
    asOfDate,
    totalOpenBalance,
    overdueOpenBalance,
    notYetDueOpenBalance,
    rows,
    errors: [],
  };
}

function parseTransactionError(error: unknown): { appliedRules?: string[]; errors?: string[] } | null {
  if (typeof error === "object" && error && "message" in error) {
    try {
      return JSON.parse(String((error as { message: unknown }).message));
    } catch {
      return null;
    }
  }
  return null;
}
