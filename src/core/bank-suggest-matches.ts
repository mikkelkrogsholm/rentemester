import type { Database } from "bun:sqlite";
import { getInvoiceStatus } from "./invoice-payments";
import { normalizeCurrency, roundDkk, equalsDkk } from "./money";
import { daysBetween } from "./dates";

export type BankMatchSuggestion = {
  // ===== BANK CLUSTER (#182) =====
  // issued_invoice        — incoming customer payment -> issued invoice
  // purchase_sale         — outgoing supplier payment -> purchase document
  // credit_note_refund    — outgoing customer refund  -> its credit note
  // supplier_credit_refund — incoming supplier refund -> the purchase it reverses
  kind: "issued_invoice" | "purchase_sale" | "credit_note_refund" | "supplier_credit_refund";
  // ===== END BANK CLUSTER (#182) =====
  documentId: number;
  invoiceNo: string | null;
  supplierName?: string | null;
  customerName?: string | null;
  confidence: number;
  reasons: string[];
};

export type BankMatchSuggestionRow = {
  bankTransactionId: number;
  date: string;
  text: string;
  amount: number;
  currency: string;
  reference: string | null;
  suggestions: BankMatchSuggestion[];
};

export type SuggestBankMatchesResult = {
  ok: boolean;
  count: number;
  rows: BankMatchSuggestionRow[];
  errors: string[];
};

export type SuggestBankMatchesInput = {
  bankTransactionId?: number;
  max?: number;
};


// Stop-words and company-form tokens that carry no identifying signal. Two
// unrelated "… ApS" customers both contain APS; keeping it as a token would
// inflate confidence on a non-match (see #138). Generic prepositions and the
// DKK currency code are dropped for the same reason.
const STOP_TOKENS = new Set([
  "APS", "A/S", "AS", "IVS", "P/S", "PS", "K/S", "KS", "I/S", "IS",
  "AMBA", "FMBA", "SMBA", "GMBH", "LTD", "INC", "PLC", "LLC", "AB", "OY",
  "DKK", "EUR", "USD", "SEK", "NOK", "GBP",
  "FOR", "OG", "THE", "AND", "VED", "MED", "TIL", "FRA", "DEN", "DET",
]);

function tokenize(value: string | null | undefined) {
  return Array.from(new Set((value ?? "")
    .toUpperCase()
    .split(/[^\p{L}\p{N}-]+/u)
    .map((token) => token.trim())
    .filter((token) => token.length >= 3)
    .filter((token) => !STOP_TOKENS.has(token))));
}

function overlapTokens(left: string | null | undefined, right: string | null | undefined) {
  const leftTokens = new Set(tokenize(left));
  return tokenize(right).filter((token) => leftTokens.has(token));
}

// ===== BANK CLUSTER (#188) =====
// The match string is built from every column that names WHO paid or WHICH
// invoice a payment belongs to: the free text, the bank's reference, the
// counterparty name and the free-text payment message. A profile (#186)
// populates the latter two; generic imports leave them null and the string
// degrades to text + reference, as before.
function combinedBankText(row: {
  text: string;
  reference: string | null;
  counterparty_name?: string | null;
  message?: string | null;
}) {
  return [row.text, row.reference, row.counterparty_name, row.message]
    .filter((part): part is string => typeof part === "string" && part.trim().length > 0)
    .join(" ")
    .trim()
    .toUpperCase();
}
// ===== END BANK CLUSTER (#188) =====

function unmatchedBankTransactions(db: Database, bankTransactionId?: number) {
  return db.query(
    `SELECT bt.id, bt.transaction_date, bt.text, bt.amount, bt.currency, bt.reference,
            bt.counterparty_name, bt.counterparty_account, bt.message
     FROM bank_transactions bt
     LEFT JOIN journal_entries je
       ON je.source_bank_transaction_id = bt.id
      AND je.status = 'posted'
     WHERE je.id IS NULL
       AND (? IS NULL OR bt.id = ?)
     ORDER BY bt.transaction_date DESC, bt.id DESC`
  ).all(bankTransactionId ?? null, bankTransactionId ?? null) as Array<{
    id: number;
    transaction_date: string;
    text: string;
    amount: number;
    currency: string;
    reference: string | null;
    counterparty_name: string | null;
    counterparty_account: string | null;
    message: string | null;
  }>;
}

function openIssuedInvoices(db: Database) {
  return db.query(
    `SELECT id, invoice_no, invoice_date, payload_json
     FROM documents
     WHERE document_type = 'issued_invoice'
     ORDER BY id ASC`
  ).all() as Array<{
    id: number;
    invoice_no: string;
    invoice_date: string | null;
    payload_json: string | null;
  }>;
}

function openPurchaseDocuments(db: Database) {
  return db.query(
    `SELECT d.id, d.invoice_no, d.invoice_date, d.amount_inc_vat, d.sender_name, d.payment_details
     FROM documents d
     LEFT JOIN journal_entries je
       ON je.document_id = d.id
      AND je.status = 'posted'
     WHERE d.document_type = 'purchase_sale'
       AND d.currency = 'DKK'
       AND je.id IS NULL
     ORDER BY d.id ASC`
  ).all() as Array<{
    id: number;
    invoice_no: string | null;
    invoice_date: string | null;
    amount_inc_vat: number | null;
    sender_name: string | null;
    payment_details: string | null;
  }>;
}

// ===== BANK CLUSTER (#182) =====
// All credit-note documents. A credit note carries the customer (recipient)
// name, its own number (invoice_no = CN-...) and the original invoice number
// it credits (payment_details). An outgoing customer-refund bank row is matched
// against these.
function creditNoteDocuments(db: Database) {
  return db.query(
    `SELECT id, invoice_no, invoice_date, amount_inc_vat, recipient_name, payment_details, currency
     FROM documents
     WHERE document_type = 'credit_note'
     ORDER BY id ASC`
  ).all() as Array<{
    id: number;
    invoice_no: string | null;
    invoice_date: string | null;
    amount_inc_vat: number | null;
    recipient_name: string | null;
    payment_details: string | null;
    currency: string | null;
  }>;
}

// All purchase documents, regardless of journal status. The refund direction
// matches an incoming supplier credit-note refund against the purchase it
// reverses, and that purchase is typically already booked — so unlike
// openPurchaseDocuments this query does not filter on journal status.
function allPurchaseDocuments(db: Database) {
  return db.query(
    `SELECT id, invoice_no, invoice_date, amount_inc_vat, sender_name, payment_details
     FROM documents
     WHERE document_type = 'purchase_sale'
       AND currency = 'DKK'
     ORDER BY id ASC`
  ).all() as Array<{
    id: number;
    invoice_no: string | null;
    invoice_date: string | null;
    amount_inc_vat: number | null;
    sender_name: string | null;
    payment_details: string | null;
  }>;
}
// ===== END BANK CLUSTER (#182) =====

function invoiceSuggestion(db: Database, bank: ReturnType<typeof unmatchedBankTransactions>[number], doc: ReturnType<typeof openIssuedInvoices>[number]): BankMatchSuggestion | null {
  // Only incoming customer payments (amount > 0) are matched against issued
  // invoices here. An *outgoing* customer refund (amount < 0) is matched
  // against its credit note by creditNoteRefundSuggestion (#182), a separate
  // matching direction.
  if (!(Number(bank.amount) > 0)) return null;
  const status = getInvoiceStatus(db, doc.id, bank.transaction_date);
  if (!status.ok) return null;
  const openBalance = roundDkk(Number(status.openBalance ?? 0));
  const claimOpenBalance = roundDkk(Number(status.claimOpenBalance ?? 0));
  if (!(claimOpenBalance > 0)) return null;

  const payload = doc.payload_json ? JSON.parse(doc.payload_json) : null;
  // Currency guard (#: bank/suggest cross-currency): the open balance is in the
  // invoice's own currency, and equalsDkk compares raw amounts currency-blind,
  // so a 100 EUR bank row would otherwise "match" a 100,00 DKK invoice. The
  // apply path (applyBankPaymentToInvoice) hard-rejects a currency mismatch, so
  // a cross-currency suggestion is always unactionable — never surface it.
  const invoiceCurrency = normalizeCurrency(typeof payload?.currency === "string" ? payload.currency : "DKK");
  if (normalizeCurrency(bank.currency) !== invoiceCurrency) return null;
  const customerName = typeof payload?.buyer?.name === "string" ? payload.buyer.name : null;
  const bankText = combinedBankText(bank);
  let confidence = 0;
  const reasons: string[] = [];
  // A corroborating signal identifies *which* invoice this is, beyond the
  // amount alone. Without one, two open invoices with the same balance are
  // indistinguishable and an agent could auto-apply the wrong one (#138).
  let corroborated = false;

  // Integer-øre comparison (equalsDkk): bank.amount is a raw float that can be
  // float-distinct from an øre-equal claim balance assembled via addDkk (#148).
  const bankAmount = Number(bank.amount);
  if (equalsDkk(bankAmount, claimOpenBalance)) {
    confidence += 0.6;
    reasons.push(`amount match: ${roundDkk(bankAmount)} vs claim open balance ${claimOpenBalance}`);
  } else if (equalsDkk(bankAmount, openBalance)) {
    confidence += 0.55;
    reasons.push(`amount match: ${roundDkk(bankAmount)} vs principal open balance ${openBalance}`);
  }

  if (doc.invoice_no && bankText.includes(doc.invoice_no.toUpperCase())) {
    confidence += 0.25;
    corroborated = true;
    reasons.push(`invoice number '${doc.invoice_no}' found in bank text/reference`);
  }

  const sharedCustomerTokens = overlapTokens(bankText, customerName).slice(0, 3);
  if (sharedCustomerTokens.length > 0) {
    confidence += Math.min(0.15, sharedCustomerTokens.length * 0.05);
    // A single shared token is weak (a common word can collide); two or more
    // is a strong name match that identifies the customer.
    if (sharedCustomerTokens.length >= 2) corroborated = true;
    reasons.push(`customer token match: ${sharedCustomerTokens.join(", ")}`);
  }

  if (doc.invoice_date) {
    const days = daysBetween(doc.invoice_date, bank.transaction_date);
    if (days <= 7) {
      confidence += 0.1;
      reasons.push(`date within ${days} days of invoice date`);
    }
  }

  // Amount + date proximity alone never crosses the threshold: without an
  // invoice number or strong name match the suggestion stays low-confidence.
  if (!corroborated && confidence >= 0.5) {
    confidence = 0.45;
    reasons.push("low confidence: amount-only match, no invoice number or name corroboration");
  }

  if (confidence < 0.5) return null;
  return {
    kind: "issued_invoice",
    documentId: doc.id,
    invoiceNo: doc.invoice_no,
    customerName,
    confidence: roundDkk(confidence),
    reasons,
  };
}

function purchaseSuggestion(bank: ReturnType<typeof unmatchedBankTransactions>[number], doc: ReturnType<typeof openPurchaseDocuments>[number]): BankMatchSuggestion | null {
  // Only outgoing supplier payments (amount < 0) are matched against purchase
  // documents here. An *incoming* supplier credit-note refund (amount > 0) is
  // matched against the purchase it reverses by supplierCreditRefundSuggestion
  // (#182), a separate matching direction.
  if (!(Number(bank.amount) < 0)) return null;
  // Purchase documents are DKK-only (openPurchaseDocuments filters currency =
  // 'DKK'), so a non-DKK bank row can never be a real match — its raw amount is
  // a foreign figure that equalsDkk would compare currency-blind. (#bank/suggest)
  if (normalizeCurrency(bank.currency) !== "DKK") return null;
  const grossAmount = roundDkk(Number(doc.amount_inc_vat ?? 0));
  if (!(grossAmount > 0)) return null;
  const paymentAmount = Math.abs(Number(bank.amount));
  const bankText = combinedBankText(bank);
  let confidence = 0;
  const reasons: string[] = [];
  // See invoiceSuggestion: a corroborating signal identifies which purchase
  // document this payment belongs to beyond the amount alone (#138).
  let corroborated = false;

  // Integer-øre comparison (equalsDkk) — see #148.
  if (equalsDkk(paymentAmount, grossAmount)) {
    confidence += 0.55;
    reasons.push(`amount match: ${roundDkk(paymentAmount)} vs purchase gross amount ${grossAmount}`);
  }

  if (doc.invoice_no && bankText.includes(doc.invoice_no.toUpperCase())) {
    confidence += 0.2;
    corroborated = true;
    reasons.push(`invoice number '${doc.invoice_no}' found in bank text/reference`);
  }

  const paymentDetailsTokens = overlapTokens(bankText, doc.payment_details).slice(0, 2);
  if (paymentDetailsTokens.length > 0) {
    confidence += 0.1;
    corroborated = true;
    reasons.push(`payment-details token match: ${paymentDetailsTokens.join(", ")}`);
  }

  const sharedSupplierTokens = overlapTokens(bankText, doc.sender_name).slice(0, 3);
  if (sharedSupplierTokens.length > 0) {
    confidence += Math.min(0.2, sharedSupplierTokens.length * 0.1);
    if (sharedSupplierTokens.length >= 2) corroborated = true;
    reasons.push(`supplier token match: ${sharedSupplierTokens.join(", ")}`);
  }

  if (doc.invoice_date) {
    const days = daysBetween(doc.invoice_date, bank.transaction_date);
    if (days <= 7) {
      confidence += 0.05;
      reasons.push(`date within ${days} days of purchase invoice date`);
    }
  }

  // Amount + date proximity alone never crosses the threshold without an
  // invoice number, payment-reference token or strong supplier-name match.
  if (!corroborated && confidence >= 0.5) {
    confidence = 0.45;
    reasons.push("low confidence: amount-only match, no invoice number or supplier corroboration");
  }

  if (confidence < 0.5) return null;
  return {
    kind: "purchase_sale",
    documentId: doc.id,
    invoiceNo: doc.invoice_no,
    supplierName: doc.sender_name,
    confidence: roundDkk(confidence),
    reasons,
  };
}

// ===== BANK CLUSTER (#182) =====
/**
 * Matches an *outgoing* customer-refund bank row (amount < 0) against a credit
 * note. #154 documented that such rows had no matching path and stayed
 * permanently unmatched; this is that path.
 *
 * Matching stays conservative: amount + date proximity alone never crosses the
 * 0.5 threshold. A crossing-threshold suggestion requires a corroborating
 * signal — the credit-note number, the credited invoice number, or a strong
 * (>= 2 token) customer-name overlap — so an agent never auto-applies a wrong
 * refund. Amounts are compared in integer øre (equalsDkk).
 */
function creditNoteRefundSuggestion(bank: ReturnType<typeof unmatchedBankTransactions>[number], doc: ReturnType<typeof creditNoteDocuments>[number]): BankMatchSuggestion | null {
  if (!(Number(bank.amount) < 0)) return null;
  // Only match a refund row against a credit note of the same currency — the
  // gross is in the credit note's currency and equalsDkk is currency-blind.
  // (#bank/suggest cross-currency)
  if (normalizeCurrency(bank.currency) !== normalizeCurrency(doc.currency)) return null;
  const grossAmount = roundDkk(Number(doc.amount_inc_vat ?? 0));
  if (!(grossAmount > 0)) return null;
  const refundAmount = Math.abs(Number(bank.amount));
  const bankText = combinedBankText(bank);
  let confidence = 0;
  const reasons: string[] = [];
  let corroborated = false;

  if (equalsDkk(refundAmount, grossAmount)) {
    confidence += 0.55;
    reasons.push(`amount match: ${roundDkk(refundAmount)} vs credit-note gross amount ${grossAmount}`);
  }

  if (doc.invoice_no && bankText.includes(doc.invoice_no.toUpperCase())) {
    confidence += 0.25;
    corroborated = true;
    reasons.push(`credit-note number '${doc.invoice_no}' found in bank text/reference`);
  }

  // payment_details on a credit note is the original invoice number it credits.
  if (doc.payment_details && bankText.includes(doc.payment_details.toUpperCase())) {
    confidence += 0.15;
    corroborated = true;
    reasons.push(`credited invoice number '${doc.payment_details}' found in bank text/reference`);
  }

  const sharedCustomerTokens = overlapTokens(bankText, doc.recipient_name).slice(0, 3);
  if (sharedCustomerTokens.length > 0) {
    confidence += Math.min(0.15, sharedCustomerTokens.length * 0.05);
    if (sharedCustomerTokens.length >= 2) corroborated = true;
    reasons.push(`customer token match: ${sharedCustomerTokens.join(", ")}`);
  }

  if (doc.invoice_date) {
    const days = daysBetween(doc.invoice_date, bank.transaction_date);
    if (days <= 14) {
      confidence += 0.1;
      reasons.push(`date within ${days} days of credit-note date`);
    }
  }

  // Amount + date proximity alone never crosses the threshold without a
  // credit-note/invoice number or strong customer-name match.
  if (!corroborated && confidence >= 0.5) {
    confidence = 0.45;
    reasons.push("low confidence: amount-only match, no credit-note number or customer corroboration");
  }

  if (confidence < 0.5) return null;
  return {
    kind: "credit_note_refund",
    documentId: doc.id,
    invoiceNo: doc.invoice_no,
    customerName: doc.recipient_name,
    confidence: roundDkk(confidence),
    reasons,
  };
}

/**
 * Matches an *incoming* supplier credit-note refund (amount > 0) against the
 * purchase document it reverses. #154 documented that such rows had no
 * matching path; this is that path.
 *
 * The purchase is typically already booked, so allPurchaseDocuments is used
 * (no journal-status filter). Matching stays conservative the same way: a
 * crossing-threshold suggestion needs a corroborating signal — the purchase
 * invoice number, a payment-reference token, or a strong supplier-name overlap.
 */
function supplierCreditRefundSuggestion(bank: ReturnType<typeof unmatchedBankTransactions>[number], doc: ReturnType<typeof allPurchaseDocuments>[number]): BankMatchSuggestion | null {
  if (!(Number(bank.amount) > 0)) return null;
  // allPurchaseDocuments is DKK-only, so a non-DKK bank row cannot be a real
  // supplier-refund match. (#bank/suggest cross-currency)
  if (normalizeCurrency(bank.currency) !== "DKK") return null;
  const grossAmount = roundDkk(Number(doc.amount_inc_vat ?? 0));
  if (!(grossAmount > 0)) return null;
  const refundAmount = Number(bank.amount);
  const bankText = combinedBankText(bank);
  let confidence = 0;
  const reasons: string[] = [];
  let corroborated = false;

  // A supplier refund is usually a partial or full reversal of the purchase:
  // accept an exact gross match, never a larger amount (that is not a refund).
  if (equalsDkk(refundAmount, grossAmount)) {
    confidence += 0.5;
    reasons.push(`amount match: ${roundDkk(refundAmount)} vs purchase gross amount ${grossAmount}`);
  } else if (refundAmount < grossAmount) {
    confidence += 0.3;
    reasons.push(`partial-refund amount: ${roundDkk(refundAmount)} of purchase gross amount ${grossAmount}`);
  } else {
    return null;
  }

  // A credit-note / refund row almost always says so explicitly; requiring a
  // refund cue keeps an ordinary incoming customer payment from being mistaken
  // for a supplier refund.
  const looksLikeRefund = /\b(KREDIT|KREDITNOTA|REFUSION|REFUND|TILBAGEBETALING|CREDIT)\b/.test(bankText);

  if (doc.invoice_no && bankText.includes(doc.invoice_no.toUpperCase())) {
    confidence += 0.25;
    corroborated = true;
    reasons.push(`purchase invoice number '${doc.invoice_no}' found in bank text/reference`);
  }

  const paymentDetailsTokens = overlapTokens(bankText, doc.payment_details).slice(0, 2);
  if (paymentDetailsTokens.length > 0) {
    confidence += 0.1;
    corroborated = true;
    reasons.push(`payment-details token match: ${paymentDetailsTokens.join(", ")}`);
  }

  const sharedSupplierTokens = overlapTokens(bankText, doc.sender_name).slice(0, 3);
  if (sharedSupplierTokens.length > 0) {
    confidence += Math.min(0.2, sharedSupplierTokens.length * 0.1);
    if (sharedSupplierTokens.length >= 2) corroborated = true;
    reasons.push(`supplier token match: ${sharedSupplierTokens.join(", ")}`);
  }

  if (looksLikeRefund) {
    confidence += 0.1;
    reasons.push("bank text identifies the row as a credit-note / refund");
  }

  // A supplier refund needs BOTH a document-identifying corroboration and an
  // explicit refund cue: without the cue an incoming payment that happens to
  // name a supplier would be wrongly offered as a refund.
  if (!(corroborated && looksLikeRefund) && confidence >= 0.5) {
    confidence = 0.45;
    reasons.push("low confidence: not corroborated as a supplier credit-note refund");
  }

  if (confidence < 0.5) return null;
  return {
    kind: "supplier_credit_refund",
    documentId: doc.id,
    invoiceNo: doc.invoice_no,
    supplierName: doc.sender_name,
    confidence: roundDkk(confidence),
    reasons,
  };
}
// ===== END BANK CLUSTER (#182) =====

export function suggestBankMatches(db: Database, input: SuggestBankMatchesInput = {}): SuggestBankMatchesResult {
  const errors: string[] = [];
  if (input.bankTransactionId !== undefined && (!Number.isInteger(input.bankTransactionId) || input.bankTransactionId <= 0)) {
    errors.push("bankTransactionId must be a positive integer when present");
  }
  const max = input.max ?? 5;
  if (!Number.isInteger(max) || max <= 0) errors.push("max must be a positive integer when present");
  if (errors.length > 0) return { ok: false, count: 0, rows: [], errors };

  const bankRows = unmatchedBankTransactions(db, input.bankTransactionId);
  if (input.bankTransactionId !== undefined && bankRows.length === 0) {
    return { ok: false, count: 0, rows: [], errors: [`unmatched bank transaction ${input.bankTransactionId} does not exist`] };
  }

  const invoiceDocs = openIssuedInvoices(db);
  const purchaseDocs = openPurchaseDocuments(db);
  // ===== BANK CLUSTER (#182) =====
  const creditNoteDocs = creditNoteDocuments(db);
  const allPurchaseDocs = allPurchaseDocuments(db);
  // ===== END BANK CLUSTER (#182) =====

  const rows: BankMatchSuggestionRow[] = bankRows.map((bank) => {
    const suggestions = [
      ...invoiceDocs.map((doc) => invoiceSuggestion(db, bank, doc)).filter((value): value is BankMatchSuggestion => Boolean(value)),
      ...purchaseDocs.map((doc) => purchaseSuggestion(bank, doc)).filter((value): value is BankMatchSuggestion => Boolean(value)),
      // ===== BANK CLUSTER (#182) =====
      ...creditNoteDocs.map((doc) => creditNoteRefundSuggestion(bank, doc)).filter((value): value is BankMatchSuggestion => Boolean(value)),
      ...allPurchaseDocs.map((doc) => supplierCreditRefundSuggestion(bank, doc)).filter((value): value is BankMatchSuggestion => Boolean(value)),
      // ===== END BANK CLUSTER (#182) =====
    ]
      .sort((a, b) => b.confidence - a.confidence || a.documentId - b.documentId)
      .slice(0, max);

    return {
      bankTransactionId: bank.id,
      date: bank.transaction_date,
      text: bank.text,
      amount: roundDkk(Number(bank.amount)),
      currency: bank.currency,
      reference: bank.reference,
      suggestions,
    };
  });

  return { ok: true, count: rows.length, rows, errors: [] };
}
