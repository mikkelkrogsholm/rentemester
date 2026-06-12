import type { Database } from "bun:sqlite";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { addDkk, equalsDkk, roundDkk } from "./money";

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
  const vatTreatment = payload?.vatTreatment ?? "standard";
  const isReverseCharge = vatTreatment === "domestic_reverse_charge" || vatTreatment === "foreign_reverse_charge";
  const lines: Array<{ accountNo: string; debitAmount?: number; creditAmount?: number; vatCode?: string; text: string }> = [
    { accountNo: input.receivableAccountNo ?? "1100", debitAmount: grossAmount, text: `Receivable ${doc.invoice_no}` },
    {
      accountNo: input.revenueAccountNo ?? "1000",
      creditAmount: netAmount,
      vatCode: isReverseCharge ? "REVERSE_CHARGE_EXEMPT" : "DK_SALE_25",
      text: `Revenue ${doc.invoice_no}`
    },
  ];
  if (!isReverseCharge && vatAmount > 0) {
    lines.push({ accountNo: input.outputVatAccountNo ?? "1200", creditAmount: vatAmount, text: `Output VAT ${doc.invoice_no}` });
  }
  return { lines, isReverseCharge };
}

export function postIssuedInvoiceToLedger(db: Database, input: PostIssuedInvoiceInput): JournalPostResult {
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["invoiceDocumentId must be a positive integer"] };
  }

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

  const existing = db.query("SELECT id, entry_no FROM journal_entries WHERE document_id = ? AND reversal_of_entry_id IS NULL LIMIT 1").get(input.invoiceDocumentId) as { id: number; entry_no: string } | null;
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

  const posting = issuedInvoiceJournalLines(doc, payload, grossAmountDkk, netAmountDkk, vatAmountDkk, input);
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
    lines: posting.lines,
  });

  return {
    ...journal,
    appliedRules: [...new Set([...(journal.appliedRules ?? []), RULE_ID, ...(posting.isReverseCharge ? [REVERSE_RULE_ID] : [])])],
  };
}
