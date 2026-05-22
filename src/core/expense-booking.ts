import type { Database } from "bun:sqlite";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { postEuServiceReverseChargePurchase, postRepresentationPurchase } from "./vat";
import { absDkk, compareDkk, normalizeCurrency, percentOfDkk, roundDkk, subtractDkk } from "./money";

export type ExpenseVatTreatment = "standard" | "reverse_charge" | "representation" | "exempt";

export type BookExpenseFromBankInput = {
  documentId: number;
  bankTransactionId: number;
  expenseAccountNo: string;
  vatTreatment?: ExpenseVatTreatment;
  paymentAccountNo?: string;
  transactionDate?: string;
  text?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type BookExpenseFromBankResult = JournalPostResult & {
  documentId?: number;
  bankTransactionId?: number;
  grossAmount?: number;
  netAmount?: number;
  vatAmount?: number;
  vatTreatment?: ExpenseVatTreatment;
};

type FxBookingBasis = {
  currency: string;
  grossAmountForeign: number;
  grossAmountDkk: number;
  fxRateToDkk: number;
};

// Internal-only union: "unknown" is never exposed via the public
// ExpenseVatTreatment type — the caller is forced to pass an explicit
// vatTreatment when the account's default_vat_code is null or unmapped.
type InferredVatTreatment = ExpenseVatTreatment | "unknown";

function inferVatTreatment(defaultVatCode: string | null): InferredVatTreatment {
  if (defaultVatCode === "EU_SERVICE_REVERSE_CHARGE") return "reverse_charge";
  if (defaultVatCode === "REPRESENTATION_SPECIAL") return "representation";
  if (defaultVatCode === "DK_PURCHASE_25") return "standard";
  // A null or unrecognised default_vat_code must not be silently downgraded
  // to VAT-exempt — that would under-claim købsmoms with no warning.
  return "unknown";
}

function resolveFxBookingBasis(document: { currency: string; amount_inc_vat: number | null }, bank: {
  id: number;
  amount: number;
  currency: string;
  amount_dkk: number | null;
  fx_rate_to_dkk: number | null;
}): { ok: true; basis: FxBookingBasis } | { ok: false; error: string } {
  const currency = normalizeCurrency(document.currency);
  const grossAmountForeign = roundDkk(Number(document.amount_inc_vat ?? 0));

  if (currency === "DKK") {
    return {
      ok: true,
      basis: {
        currency,
        grossAmountForeign,
        grossAmountDkk: grossAmountForeign,
        fxRateToDkk: 1,
      },
    };
  }

  const bankCurrency = normalizeCurrency(bank.currency);
  const fxRateToDkk = bank.fx_rate_to_dkk == null ? NaN : Number(bank.fx_rate_to_dkk);
  if (!(fxRateToDkk > 0)) {
    if (bankCurrency === "DKK") return { ok: false, error: "foreign-currency expense booking requires bank fx_rate_to_dkk for DKK-settled payments" };
    return { ok: false, error: "foreign-currency expense booking requires bank fx_rate_to_dkk" };
  }

  const expectedAmountDkk = roundDkk(grossAmountForeign * fxRateToDkk);

  if (bankCurrency === "DKK") {
    const grossAmountDkk = roundDkk(Math.abs(Number(bank.amount)));
    if (bank.amount_dkk != null && compareDkk(Number(bank.amount_dkk), grossAmountDkk) !== 0) {
      return { ok: false, error: `bank transaction ${bank.id} amount_dkk ${roundDkk(Number(bank.amount_dkk))} does not match DKK settlement amount ${grossAmountDkk}` };
    }
    if (compareDkk(grossAmountDkk, expectedAmountDkk) !== 0) {
      return { ok: false, error: `bank transaction amount ${grossAmountDkk} DKK does not match document gross amount ${grossAmountForeign} ${currency} at fx_rate_to_dkk ${roundDkk(fxRateToDkk)} (${expectedAmountDkk} DKK)` };
    }
    return {
      ok: true,
      basis: {
        currency,
        grossAmountForeign,
        grossAmountDkk,
        fxRateToDkk,
      },
    };
  }

  if (bankCurrency !== currency) {
    return { ok: false, error: `bank transaction ${bank.id} currency ${bankCurrency} does not match document currency ${currency} or DKK settlement` };
  }

  const paymentAmountForeign = roundDkk(Math.abs(Number(bank.amount)));
  if (compareDkk(paymentAmountForeign, grossAmountForeign) !== 0) {
    return { ok: false, error: `bank transaction amount ${paymentAmountForeign} ${currency} does not match document gross amount ${grossAmountForeign} ${currency}` };
  }

  const grossAmountDkk = roundDkk(Number(bank.amount_dkk ?? 0));
  if (!(grossAmountDkk > 0)) {
    return { ok: false, error: `bank transaction ${bank.id} is missing amount_dkk for foreign-currency settlement` };
  }
  if (compareDkk(grossAmountDkk, expectedAmountDkk) !== 0) {
    return { ok: false, error: `bank transaction amount_dkk ${grossAmountDkk} does not match document gross amount ${grossAmountForeign} ${currency} at fx_rate_to_dkk ${roundDkk(fxRateToDkk)} (${expectedAmountDkk} DKK)` };
  }

  return {
    ok: true,
    basis: {
      currency,
      grossAmountForeign,
      grossAmountDkk,
      fxRateToDkk,
    },
  };
}

export function bookExpenseFromBank(db: Database, input: BookExpenseFromBankInput): BookExpenseFromBankResult {
  const errors: string[] = [];
  if (!Number.isInteger(input.documentId) || input.documentId <= 0) errors.push("documentId must be a positive integer");
  if (!Number.isInteger(input.bankTransactionId) || input.bankTransactionId <= 0) errors.push("bankTransactionId must be a positive integer");
  if (typeof input.expenseAccountNo !== "string" || input.expenseAccountNo.trim().length === 0) errors.push("expenseAccountNo is required");
  if (input.vatTreatment && !["standard", "reverse_charge", "representation", "exempt"].includes(input.vatTreatment)) {
    errors.push("vatTreatment must be one of standard, reverse_charge, representation, exempt when present");
  }
  if (errors.length > 0) return { ok: false, appliedRules: [], errors };

  const account = db.query(`SELECT account_no, type, default_vat_code, active FROM accounts WHERE account_no = ?`).get(input.expenseAccountNo.trim()) as {
    account_no: string;
    type: string;
    default_vat_code: string | null;
    active: number;
  } | null;
  if (!account) return { ok: false, appliedRules: [], errors: [`expense account ${input.expenseAccountNo} does not exist`] };
  if (account.type !== "expense") return { ok: false, appliedRules: [], errors: [`account ${input.expenseAccountNo} is not an expense account`] };
  if (!account.active) return { ok: false, appliedRules: [], errors: [`account ${input.expenseAccountNo} is inactive`] };

  const document = db.query(
    `SELECT id, document_type, invoice_no, invoice_date, amount_inc_vat, vat_amount, currency, sender_name
     FROM documents
     WHERE id = ?`
  ).get(input.documentId) as {
    id: number;
    document_type: string;
    invoice_no: string | null;
    invoice_date: string | null;
    amount_inc_vat: number | null;
    vat_amount: number | null;
    currency: string;
    sender_name: string | null;
  } | null;
  if (!document) return { ok: false, appliedRules: [], errors: [`document ${input.documentId} does not exist`] };
  if (document.document_type !== "purchase_sale" && document.document_type !== "cash_register_receipt") {
    return { ok: false, appliedRules: [], errors: [`document ${input.documentId} is not a purchase document`] };
  }
  const grossAmount = roundDkk(Number(document.amount_inc_vat ?? 0));
  const vatAmount = roundDkk(Number(document.vat_amount ?? 0));
  if (!(grossAmount > 0)) return { ok: false, appliedRules: [], errors: [`document ${input.documentId} must have amount_inc_vat > 0`] };
  if (vatAmount < 0 || vatAmount > grossAmount) return { ok: false, appliedRules: [], errors: [`document ${input.documentId} has invalid vat_amount ${vatAmount}`] };

  const bank = db.query(`SELECT id, transaction_date, amount, text, currency, amount_dkk, fx_rate_to_dkk FROM bank_transactions WHERE id = ?`).get(input.bankTransactionId) as {
    id: number;
    transaction_date: string;
    amount: number;
    text: string;
    currency: string;
    amount_dkk: number | null;
    fx_rate_to_dkk: number | null;
  } | null;
  if (!bank) return { ok: false, appliedRules: [], errors: [`bank transaction ${input.bankTransactionId} does not exist`] };
  if (!(Number(bank.amount) < 0)) return { ok: false, appliedRules: [], errors: [`bank transaction ${input.bankTransactionId} is not an outgoing payment`] };

  const existingJournal = db.query(`SELECT id FROM journal_entries WHERE source_bank_transaction_id = ? LIMIT 1`).get(bank.id) as { id: number } | null;
  if (existingJournal) return { ok: false, appliedRules: [], errors: [`bank transaction ${bank.id} is already linked to journal entry ${existingJournal.id}`] };

  const inferredTreatment = input.vatTreatment ?? inferVatTreatment(account.default_vat_code);
  if (inferredTreatment === "unknown") {
    return {
      ok: false,
      appliedRules: [],
      errors: [`account ${account.account_no} has an unmapped default_vat_code ${account.default_vat_code === null ? "(none)" : account.default_vat_code} — pass an explicit vatTreatment (standard, reverse_charge, representation, exempt)`],
    };
  }
  const vatTreatment: ExpenseVatTreatment = inferredTreatment;
  const transactionDate = input.transactionDate ?? bank.transaction_date;
  // Posting text is read by a Danish owner — keep it fully Danish. The
  // supplier name is used when known; otherwise fall back to a Danish word.
  const supplierName = document.sender_name?.trim();
  const text = input.text?.trim()
    || (supplierName
      ? `Udgift fra ${supplierName} (banktransaktion ${bank.id})`
      : `Udgift (banktransaktion ${bank.id})`);
  const paymentAccountNo = input.paymentAccountNo ?? "2000";
  const fxBasis = resolveFxBookingBasis(document, bank);
  if (!fxBasis.ok) return { ok: false, appliedRules: [], errors: [fxBasis.error] };

  const journalAmount = fxBasis.basis.currency === "DKK" ? roundDkk(Math.abs(Number(bank.amount))) : fxBasis.basis.grossAmountDkk;
  if (fxBasis.basis.currency === "DKK" && compareDkk(journalAmount, grossAmount) !== 0) {
    return { ok: false, appliedRules: [], errors: [`bank transaction amount ${journalAmount} does not match document gross amount ${grossAmount}`] };
  }

  const grossAmountDkk = fxBasis.basis.grossAmountDkk;
  const vatAmountDkk = fxBasis.basis.currency === "DKK" ? vatAmount : roundDkk(vatAmount * fxBasis.basis.fxRateToDkk);
  const netAmountDkk = roundDkk(grossAmountDkk - vatAmountDkk);
  const journalMetadata = fxBasis.basis.currency === "DKK"
    ? {}
    : {
        currency: fxBasis.basis.currency,
        amountForeign: fxBasis.basis.grossAmountForeign,
        amountDkk: fxBasis.basis.grossAmountDkk,
        fxRateToDkk: fxBasis.basis.fxRateToDkk,
      };

  // For 25%-rated treatments the document vat_amount becomes deductible input
  // VAT, so it must be consistent with a 25% rate rather than trusted blindly.
  // A garbled or OCR-extracted vat_amount would otherwise be booked verbatim,
  // over- or under-claiming købsmoms. Validate in the document's native
  // currency (the 25% ratio is currency-independent), allowing 1 øre of
  // rounding slack.
  if (vatTreatment === "standard" || vatTreatment === "representation") {
    const documentNetAmount = subtractDkk(grossAmount, vatAmount);
    const expectedVatAmount = percentOfDkk(documentNetAmount, 25);
    if (compareDkk(absDkk(subtractDkk(vatAmount, expectedVatAmount)), 0.01) > 0) {
      return {
        ok: false,
        appliedRules: [],
        errors: [`document ${input.documentId} vat_amount ${vatAmount} is inconsistent with the 25% rate (expected ~${expectedVatAmount} for net ${documentNetAmount})`],
      };
    }
  }

  if (vatTreatment === "standard") {
    if (!(vatAmount > 0)) return { ok: false, appliedRules: [], errors: ["standard expense booking requires document vat_amount > 0"] };
    const result = postJournalEntry(db, {
      transactionDate,
      text,
      documentId: input.documentId,
      sourceBankTransactionId: input.bankTransactionId,
      createdBy: input.createdBy,
      createdByProgram: input.createdByProgram,
      ...journalMetadata,
      lines: [
        { accountNo: account.account_no, debitAmount: netAmountDkk, vatCode: "DK_PURCHASE_25", text: document.invoice_no ?? "Udgift, grundbeløb" },
        { accountNo: "4000", debitAmount: vatAmountDkk, text: "Købsmoms" },
        { accountNo: paymentAccountNo, creditAmount: grossAmountDkk, text: bank.text },
      ],
    });
    return { ...result, documentId: input.documentId, bankTransactionId: input.bankTransactionId, grossAmount, netAmount: netAmountDkk, vatAmount: vatAmountDkk, vatTreatment };
  }

  if (vatTreatment === "reverse_charge") {
    if (vatAmount !== 0) return { ok: false, appliedRules: [], errors: ["reverse-charge expense booking requires document vat_amount = 0"] };
    const result = postEuServiceReverseChargePurchase(db, {
      transactionDate,
      text,
      documentId: input.documentId,
      netAmount: grossAmountDkk,
      expenseAccountNo: account.account_no,
      paymentAccountNo,
      sourceBankTransactionId: input.bankTransactionId,
      createdBy: input.createdBy,
      createdByProgram: input.createdByProgram,
      ...journalMetadata,
    });
    return { ...result, documentId: input.documentId, bankTransactionId: input.bankTransactionId, grossAmount, netAmount: grossAmountDkk, vatAmount: 0, vatTreatment };
  }

  if (vatTreatment === "representation") {
    if (!(vatAmount > 0)) return { ok: false, appliedRules: [], errors: ["representation expense booking requires document vat_amount > 0"] };
    const result = postRepresentationPurchase(db, {
      transactionDate,
      text,
      documentId: input.documentId,
      netAmount: netAmountDkk,
      expenseAccountNo: account.account_no,
      paymentAccountNo,
      sourceBankTransactionId: input.bankTransactionId,
      createdBy: input.createdBy,
      createdByProgram: input.createdByProgram,
      ...journalMetadata,
    });
    return { ...result, documentId: input.documentId, bankTransactionId: input.bankTransactionId, grossAmount, netAmount: netAmountDkk, vatAmount: vatAmountDkk, vatTreatment };
  }

  if (vatAmount !== 0) return { ok: false, appliedRules: [], errors: ["exempt expense booking requires document vat_amount = 0"] };
  const result = postJournalEntry(db, {
    transactionDate,
    text,
    documentId: input.documentId,
    sourceBankTransactionId: input.bankTransactionId,
    createdBy: input.createdBy,
    createdByProgram: input.createdByProgram,
    ...journalMetadata,
    lines: [
      { accountNo: account.account_no, debitAmount: grossAmountDkk, text: document.invoice_no ?? "Udgift" },
      { accountNo: paymentAccountNo, creditAmount: grossAmountDkk, text: bank.text },
    ],
  });
  return { ...result, documentId: input.documentId, bankTransactionId: input.bankTransactionId, grossAmount, netAmount: grossAmountDkk, vatAmount: 0, vatTreatment };
}
