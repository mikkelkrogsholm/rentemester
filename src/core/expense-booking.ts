import type { Database } from "bun:sqlite";
import { getCompanySettings } from "./company";
import { postJournalEntry, postJournalEntryInCurrentTransaction, type JournalLineInput, type JournalPostResult } from "./ledger";
import {
  postEuGoodsAcquisitionPurchase,
  postEuGoodsAcquisitionPurchaseInCurrentTransaction,
  postForeignServiceReverseChargePurchase,
  postForeignServiceReverseChargePurchaseInCurrentTransaction,
  postRepresentationPurchase,
  postRepresentationPurchaseInCurrentTransaction,
} from "./vat";
import { absDkk, compareDkk, fromOre, normalizeCurrency, percentOfDkk, roundDkk, roundRate6, subtractDkk, toOre } from "./money";
import { resolveAccountRole } from "./account-roles";
import { parsePurchaseVatLinesPayload, type PurchaseVatLine } from "./documents";
import { deductibleDanishPurchaseSupplierErrors } from "./supplier-identity";
import { validSimplifiedPurchaseCompanyContext } from "./document-company-context";
import { validIncompleteStandardPurchaseVatEvidenceReview } from "./document-purchase-vat-evidence-review";

/**
 * `non_deductible` (DK-VAT-NON-DEDUCTIBLE-001 / Momsloven § 37) is the
 * treatment for a purchase whose VAT is not deductible (including foreign
 * local tax and purchases by a non-VAT-registered company). The entire VAT is absorbed into the expense cost basis (gross
 * debit on the expense account, no 4000 input-VAT line, nothing for the
 * momsangivelse). The branch mirrors `exempt`'s line shape but accepts
 * `vat_amount > 0` and remains available to VAT-registered companies.
 */
export type ExpenseVatTreatment =
  | "standard"
  | "reverse_charge"
  | "eu_goods_acquisition"
  | "representation"
  | "exempt"
  | "non_deductible";

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
  grossAmountForeign?: number;
  grossAmountDkk?: number;
  netAmountDkk?: number;
  vatAmountDkk?: number;
  fxRateToDkk?: number;
  /** Whether the FX rate was imported with the bank row or derived from its DKK settlement. */
  fxRateSource?: "imported_bank" | "derived_dkk_settlement";
  /** DKK amount reconstructed from the persisted six-decimal rate minus the settlement. */
  fxReconstructionDifferenceDkk?: number;
};

type FxBookingBasis = {
  currency: string;
  grossAmountForeign: number;
  grossAmountDkk: number;
  fxRateToDkk: number;
  fxRateSource?: "imported_bank" | "derived_dkk_settlement";
  fxReconstructionDifferenceDkk?: number;
};

// Internal-only union: "unknown" is never exposed via the public
// ExpenseVatTreatment type — the caller is forced to pass an explicit
// vatTreatment when the account's default_vat_code is null or unmapped.
type InferredVatTreatment = ExpenseVatTreatment | "unknown";

/** Convert mixed purchase bases to DKK while allocating the one-øre FX
 * residual deterministically. Prefer an exempt base so the taxable base keeps
 * its exact 25% relationship to the separately rounded VAT amount. */
function scalePurchaseVatNetAmounts(
  lines: PurchaseVatLine[],
  scale: number,
  grossAmountDkk: number,
  vatAmountDkk: number,
):
  | { ok: true; lines: Array<{ line: PurchaseVatLine; netAmountDkk: number }> }
  | { ok: false; error: string } {
  const scaled = lines.map((line) => ({ line, netAmountDkk: roundDkk(line.netAmount * scale) }));
  const targetNetOre = toOre(subtractDkk(grossAmountDkk, vatAmountDkk));
  const currentNetOre = scaled.reduce((sum, item) => sum + toOre(item.netAmountDkk), 0n);
  const residual = targetNetOre - currentNetOre;
  if (residual === 0n || scaled.length === 0) return { ok: true, lines: scaled };

  const exemptIndexes = scaled.flatMap((item, index) => item.line.classification === "exempt" ? [index] : []);
  const candidates = exemptIndexes.length > 0 ? exemptIndexes : scaled.map((_, index) => index);
  const targetIndex = candidates.reduce((best, index) =>
    toOre(scaled[index]!.netAmountDkk) > toOre(scaled[best]!.netAmountDkk) ? index : best,
  candidates[0]!);
  const adjustedOre = toOre(scaled[targetIndex]!.netAmountDkk) + residual;
  if (adjustedOre < 0n) return { ok: false, error: "FX residual allocation would make a purchase VAT base negative" };
  scaled[targetIndex] = { ...scaled[targetIndex]!, netAmountDkk: fromOre(adjustedOre) };
  return { ok: true, lines: scaled };
}

function inferVatTreatment(
  defaultVatCode: string | null,
  companyIsVatRegistered: boolean,
): InferredVatTreatment {
  // For a NOT VAT-registered company, § 37 grants no input-VAT deduction, so
  // every domestic VAT-bearing treatment collapses to `non_deductible` (gross
  // to the expense, no 4000 line):
  //   - DK 25 % purchase  → non_deductible (absorb the full VAT).
  //   - representation     → non_deductible. The § 42 partial deduction is a
  //     registered-business relief; with no deduction at all the full VAT is a
  //     cost, so absorbing it is the correct booking (not the partial path).
  // EU service reverse charge is the exception: it is NOT absorbed, because for
  // a non-registered company it triggers a separate § 50 b erhvervelsesmoms
  // registration owed to SKAT (out of scope). We keep inferring `reverse_charge`
  // so the core post path refuses it with the § 50 b guidance rather than
  // silently hiding an owed-VAT liability.
  if (defaultVatCode === "EU_SERVICE_REVERSE_CHARGE") return "reverse_charge";
  if (defaultVatCode === "EU_GOODS_ACQUISITION") return "eu_goods_acquisition";
  if (defaultVatCode === "REPRESENTATION_SPECIAL") {
    return companyIsVatRegistered ? "representation" : "non_deductible";
  }
  if (defaultVatCode === "DK_PURCHASE_25") {
    return companyIsVatRegistered ? "standard" : "non_deductible";
  }
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
  if (bankCurrency === "DKK") {
    const grossAmountDkk = roundDkk(Math.abs(Number(bank.amount)));
    if (bank.amount_dkk != null && compareDkk(Math.abs(Number(bank.amount_dkk)), grossAmountDkk) !== 0) {
      return { ok: false, error: `bank transaction ${bank.id} amount_dkk ${roundDkk(Math.abs(Number(bank.amount_dkk)))} does not match DKK settlement amount ${grossAmountDkk}` };
    }
    const importedFxRate = bank.fx_rate_to_dkk == null ? null : Number(bank.fx_rate_to_dkk);
    if (importedFxRate !== null && !(importedFxRate > 0)) {
      return { ok: false, error: `bank transaction ${bank.id} fx_rate_to_dkk must be positive when provided` };
    }
    const fxRateSource = importedFxRate !== null ? "imported_bank" as const : "derived_dkk_settlement" as const;
    if (!(grossAmountForeign > 0)) return { ok: false, error: `document foreign gross amount must be positive to derive DKK settlement FX rate` };
    // A DKK bank row is itself the settlement evidence. When the import did
    // not include a rate, derive one once and retain precisely the six-decimal
    // value that journal metadata persists. Reject rates whose stored precision
    // cannot reproduce the settlement to the øre: silently persisting a rate
    // that changes the documented payment would undermine reconciliation.
    const fxRateToDkk = importedFxRate !== null
      ? importedFxRate
      : roundRate6(grossAmountDkk / grossAmountForeign);
    const expectedAmountDkk = roundDkk(grossAmountForeign * fxRateToDkk);
    const fxReconstructionDifferenceDkk = subtractDkk(expectedAmountDkk, grossAmountDkk);
    if (compareDkk(grossAmountDkk, expectedAmountDkk) !== 0) {
      return { ok: false, error: fxRateSource === "derived_dkk_settlement"
        ? `derived fx_rate_to_dkk ${fxRateToDkk} cannot reconstruct DKK settlement ${grossAmountDkk} from document gross amount ${grossAmountForeign} ${currency} (${expectedAmountDkk} DKK)`
        : `bank transaction amount ${grossAmountDkk} DKK does not match document gross amount ${grossAmountForeign} ${currency} at fx_rate_to_dkk ${roundDkk(fxRateToDkk)} (${expectedAmountDkk} DKK)` };
    }
    return {
      ok: true,
      basis: {
        currency,
        grossAmountForeign,
        grossAmountDkk,
        fxRateToDkk,
        fxRateSource,
        fxReconstructionDifferenceDkk,
      },
    };
  }

  const fxRateToDkk = bank.fx_rate_to_dkk == null ? NaN : Number(bank.fx_rate_to_dkk);
  if (!(fxRateToDkk > 0)) return { ok: false, error: "foreign-currency expense booking requires bank fx_rate_to_dkk" };
  const expectedAmountDkk = roundDkk(grossAmountForeign * fxRateToDkk);

  if (bankCurrency !== currency) {
    return { ok: false, error: `bank transaction ${bank.id} currency ${bankCurrency} does not match document currency ${currency} or DKK settlement` };
  }

  const paymentAmountForeign = roundDkk(Math.abs(Number(bank.amount)));
  if (compareDkk(paymentAmountForeign, grossAmountForeign) !== 0) {
    return { ok: false, error: `bank transaction amount ${paymentAmountForeign} ${currency} does not match document gross amount ${grossAmountForeign} ${currency}` };
  }

  // Bank imports retain cash-flow signs: outgoing amount and amount_dkk are
  // both negative. Journal metadata stores the positive gross valuation, just
  // like the absolute foreign payment amount above.
  const grossAmountDkk = roundDkk(Math.abs(Number(bank.amount_dkk ?? 0)));
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
      fxRateSource: "imported_bank",
      fxReconstructionDifferenceDkk: subtractDkk(expectedAmountDkk, grossAmountDkk),
    },
  };
}

function bookExpenseFromBankInternal(db: Database, input: BookExpenseFromBankInput, inCurrentTransaction: boolean): BookExpenseFromBankResult {
  const post = inCurrentTransaction ? postJournalEntryInCurrentTransaction : postJournalEntry;
  const postForeignService = inCurrentTransaction ? postForeignServiceReverseChargePurchaseInCurrentTransaction : postForeignServiceReverseChargePurchase;
  const postEuGoods = inCurrentTransaction ? postEuGoodsAcquisitionPurchaseInCurrentTransaction : postEuGoodsAcquisitionPurchase;
  const postRepresentation = inCurrentTransaction ? postRepresentationPurchaseInCurrentTransaction : postRepresentationPurchase;
  const errors: string[] = [];
  if (!Number.isInteger(input.documentId) || input.documentId <= 0) errors.push("documentId must be a positive integer");
  if (!Number.isInteger(input.bankTransactionId) || input.bankTransactionId <= 0) errors.push("bankTransactionId must be a positive integer");
  if (typeof input.expenseAccountNo !== "string" || input.expenseAccountNo.trim().length === 0) errors.push("expenseAccountNo is required");
  if (input.vatTreatment && !["standard", "reverse_charge", "eu_goods_acquisition", "representation", "exempt", "non_deductible"].includes(input.vatTreatment)) {
    errors.push("vatTreatment must be one of standard, reverse_charge, eu_goods_acquisition, representation, exempt, non_deductible when present");
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
    `SELECT d.id, d.document_type, d.invoice_no, d.invoice_date,
            d.amount_inc_vat, d.vat_amount, d.currency, d.sender_name,
            d.payload_json, d.sender_vat_cvr, d.recipient_vat_cvr, d.supplier_country_code,
            d.supplier_identifier_kind, d.supplier_identity_status,
            ive.bank_transaction_id AS evidence_bank_transaction_id
     FROM documents d
     LEFT JOIN internal_voucher_evidence ive ON ive.document_id = d.id
     WHERE d.id = ?`
  ).get(input.documentId) as {
    id: number;
    document_type: string;
    invoice_no: string | null;
    invoice_date: string | null;
    amount_inc_vat: number | null;
    vat_amount: number | null;
    currency: string;
    sender_name: string | null;
    payload_json: string | null;
    sender_vat_cvr: string | null;
    recipient_vat_cvr: string | null;
    supplier_country_code: string | null;
    supplier_identifier_kind: string | null;
    supplier_identity_status: string | null;
    evidence_bank_transaction_id: number | null;
  } | null;
  if (!document) return { ok: false, appliedRules: [], errors: [`document ${input.documentId} does not exist`] };
  if (
    document.document_type !== "purchase_sale" &&
    document.document_type !== "cash_register_receipt" &&
    document.document_type !== "internal_voucher"
  ) {
    return { ok: false, appliedRules: [], errors: [`document ${input.documentId} is not a purchase document`] };
  }
  if (
    document.document_type === "internal_voucher" &&
    document.evidence_bank_transaction_id !== input.bankTransactionId
  ) {
    return {
      ok: false,
      appliedRules: [],
      errors: [
        document.evidence_bank_transaction_id === null
          ? `internal voucher document ${input.documentId} has no bank-statement evidence`
          : `internal voucher document ${input.documentId} is bound to bank transaction ${document.evidence_bank_transaction_id}, not ${input.bankTransactionId}`,
      ],
    };
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

  const existingJournal = db.query(`SELECT journal_entry_id AS id FROM bank_journal_reconciliations WHERE bank_transaction_id = ? LIMIT 1`).get(bank.id) as { id: number } | null;
  if (existingJournal) return { ok: false, appliedRules: [], errors: [`bank transaction ${bank.id} is already linked to journal entry ${existingJournal.id}`] };

  const companySettings = getCompanySettings(db);
  const companyIsVatRegistered = companySettings.vatPeriodType !== null;
  const inferredTreatment =
    input.vatTreatment ?? inferVatTreatment(account.default_vat_code, companyIsVatRegistered);
  if (inferredTreatment === "unknown") {
    return {
      ok: false,
      appliedRules: [],
      errors: [`account ${account.account_no} has an unmapped default_vat_code ${account.default_vat_code === null ? "(none)" : account.default_vat_code} — pass an explicit vatTreatment (standard, reverse_charge, representation, exempt, non_deductible)`],
    };
  }
  const vatTreatment: ExpenseVatTreatment = inferredTreatment;
  if (document.document_type === "internal_voucher" && vatTreatment !== "exempt") {
    return {
      ok: false,
      appliedRules: [],
      errors: ["internal voucher expense booking requires explicit vatTreatment exempt"],
    };
  }
  if (vatTreatment === "standard" || vatTreatment === "representation") {
    const supplierErrors = deductibleDanishPurchaseSupplierErrors({
      supplierVatOrCvr: document.sender_vat_cvr,
      supplierCountryCode: document.supplier_country_code,
      supplierIdentifierKind: document.supplier_identifier_kind,
      supplierIdentityStatus: document.supplier_identity_status,
    });
    if (supplierErrors.length > 0) return { ok: false, appliedRules: [], errors: supplierErrors };
  }
  // A stated simplified-invoice fact can only support standard purchase VAT
  // through a separately hash-bound company context. It never replaces the
  // supplier identity checks above and ordinary documents get no exception.
  if (vatTreatment === "standard") {
    try {
      const payload = document.payload_json ? JSON.parse(document.payload_json) as Record<string, unknown> : {};
      if (payload.incompleteStandardPurchaseInvoice === true) {
        if (!validIncompleteStandardPurchaseVatEvidenceReview(db, input.documentId)) return { ok: false, appliedRules: [], errors: ["incomplete standard invoice requires a valid hash-bound VAT evidence review before input-VAT deduction"] };
      }
      const invoiceStatesCompany = typeof document.recipient_vat_cvr === "string" && document.recipient_vat_cvr.trim().length > 0;
      const contextIsValid = (payload.danishSimplifiedPurchaseInvoice === true && validSimplifiedPurchaseCompanyContext(db, input.documentId))
        || (payload.incompleteStandardPurchaseInvoice === true && validIncompleteStandardPurchaseVatEvidenceReview(db, input.documentId));
      if (document.document_type === "purchase_sale" && !invoiceStatesCompany && !contextIsValid) {
        return { ok: false, appliedRules: [], errors: ["standard purchase VAT requires invoice-stated recipient identity or a valid hash-bound simplified-invoice company context"] };
      }
    } catch { return { ok: false, appliedRules: [], errors: ["document payload_json is not valid JSON"] }; }
  }
  const transactionDate = input.transactionDate ?? bank.transaction_date;
  if (
    document.document_type === "internal_voucher" &&
    transactionDate !== bank.transaction_date
  ) {
    return {
      ok: false,
      appliedRules: [],
      errors: [
        `internal voucher transaction date ${transactionDate} must match bank transaction date ${bank.transaction_date}`,
      ],
    };
  }
  // Posting text is read by a Danish owner — keep it fully Danish. The
  // supplier name is used when known; otherwise fall back to a Danish word.
  const supplierName = document.sender_name?.trim();
  const text = input.text?.trim()
    || (supplierName
      ? `Udgift fra ${supplierName} (banktransaktion ${bank.id})`
      : `Udgift (banktransaktion ${bank.id})`);
  const payment = input.paymentAccountNo ? { ok: true as const, accountNo: input.paymentAccountNo } : resolveAccountRole(db, "bank");
  if (!payment.ok) return { ok: false, appliedRules: [], errors: [payment.error] };
  const inputVat = vatTreatment === "standard" ? resolveAccountRole(db, "input_vat") : null;
  if (inputVat && !inputVat.ok) return { ok: false, appliedRules: [], errors: [inputVat.error] };
  const paymentAccountNo = payment.accountNo;
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
  const fxSummary = {
    grossAmountForeign: fxBasis.basis.grossAmountForeign,
    grossAmountDkk: fxBasis.basis.grossAmountDkk,
    fxRateToDkk: fxBasis.basis.fxRateToDkk,
    ...(fxBasis.basis.currency === "DKK" ? {} : {
      fxRateSource: fxBasis.basis.fxRateSource,
      fxReconstructionDifferenceDkk: fxBasis.basis.fxReconstructionDifferenceDkk,
    }),
  };

  const parsedPurchaseVatLines = parsePurchaseVatLinesPayload(document.payload_json, {
    amountIncVat: document.amount_inc_vat,
    vatAmount: document.vat_amount,
  });
  if (parsedPurchaseVatLines.status === "invalid") {
    return { ok: false, appliedRules: [], errors: parsedPurchaseVatLines.errors.map((error) => `document ${input.documentId} has invalid persisted purchaseVatLines: ${error}`) };
  }
  const purchaseVatLines = parsedPurchaseVatLines.lines;
  if (purchaseVatLines && (vatTreatment === "reverse_charge" || vatTreatment === "representation")) {
    return { ok: false, appliedRules: [], errors: [`${vatTreatment} expense booking does not support structured purchaseVatLines; use a dedicated unsplit document or human resolution`] };
  }
  if (purchaseVatLines && vatTreatment === "standard") {
    const scale = fxBasis.basis.currency === "DKK" ? 1 : fxBasis.basis.fxRateToDkk;
    const scaledPurchaseVatLines = scalePurchaseVatNetAmounts(purchaseVatLines, scale, grossAmountDkk, vatAmountDkk);
    if (!scaledPurchaseVatLines.ok) return { ok: false, appliedRules: [], errors: [scaledPurchaseVatLines.error] };
    const lines: JournalLineInput[] = scaledPurchaseVatLines.lines.flatMap(({ line, netAmountDkk }) => {
      if (line.classification === "dk_purchase_25") return [{ accountNo: account.account_no, debitAmount: netAmountDkk, vatCode: "DK_PURCHASE_25", text: document.invoice_no ?? "Udgift, momspligtigt grundbeløb" }];
      return [{ accountNo: account.account_no, debitAmount: netAmountDkk, vatCode: "DK_PURCHASE_EXEMPT", text: document.invoice_no ?? "Udgift, momsfrit grundbeløb" }];
    });
    lines.push({ accountNo: inputVat!.accountNo, debitAmount: vatAmountDkk, text: "Købsmoms" }, { accountNo: paymentAccountNo, creditAmount: grossAmountDkk, text: bank.text });
    const result = post(db, { transactionDate, text, documentId: input.documentId, sourceBankTransactionId: input.bankTransactionId, createdBy: input.createdBy, createdByProgram: input.createdByProgram, ...journalMetadata, lines });
    return { ...result, documentId: input.documentId, bankTransactionId: input.bankTransactionId, grossAmount, netAmount: netAmountDkk, vatAmount: vatAmountDkk, vatTreatment, ...fxSummary, netAmountDkk, vatAmountDkk };
  }

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
    const result = post(db, {
      transactionDate,
      text,
      documentId: input.documentId,
      sourceBankTransactionId: input.bankTransactionId,
      createdBy: input.createdBy,
      createdByProgram: input.createdByProgram,
      ...journalMetadata,
      lines: [
        { accountNo: account.account_no, debitAmount: netAmountDkk, vatCode: "DK_PURCHASE_25", text: document.invoice_no ?? "Udgift, grundbeløb" },
        { accountNo: inputVat!.accountNo, debitAmount: vatAmountDkk, text: "Købsmoms" },
        { accountNo: paymentAccountNo, creditAmount: grossAmountDkk, text: bank.text },
      ],
    });
    return { ...result, documentId: input.documentId, bankTransactionId: input.bankTransactionId, grossAmount, netAmount: netAmountDkk, vatAmount: vatAmountDkk, vatTreatment, ...fxSummary, netAmountDkk, vatAmountDkk };
  }

  if (vatTreatment === "reverse_charge") {
    if (vatAmount !== 0) return { ok: false, appliedRules: [], errors: ["reverse-charge expense booking requires document vat_amount = 0"] };
    const result = postForeignService(db, {
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
    return { ...result, documentId: input.documentId, bankTransactionId: input.bankTransactionId, grossAmount, netAmount: grossAmountDkk, vatAmount: 0, vatTreatment, ...fxSummary, netAmountDkk: grossAmountDkk, vatAmountDkk: 0 };
  }

  if (vatTreatment === "eu_goods_acquisition") {
    if (vatAmount !== 0) return { ok: false, appliedRules: [], errors: ["EU-goods acquisition expense booking requires document vat_amount = 0"] };
    const result = postEuGoods(db, { transactionDate, text, documentId: input.documentId, netAmount: grossAmountDkk, expenseAccountNo: account.account_no, paymentAccountNo, sourceBankTransactionId: input.bankTransactionId, createdBy: input.createdBy, createdByProgram: input.createdByProgram, ...journalMetadata });
    return { ...result, documentId: input.documentId, bankTransactionId: input.bankTransactionId, grossAmount, netAmount: grossAmountDkk, vatAmount: 0, vatTreatment, ...fxSummary, netAmountDkk: grossAmountDkk, vatAmountDkk: 0 };
  }

  if (vatTreatment === "representation") {
    if (!(vatAmount > 0)) return { ok: false, appliedRules: [], errors: ["representation expense booking requires document vat_amount > 0"] };
    const result = postRepresentation(db, {
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
    return { ...result, documentId: input.documentId, bankTransactionId: input.bankTransactionId, grossAmount, netAmount: netAmountDkk, vatAmount: vatAmountDkk, vatTreatment, ...fxSummary, netAmountDkk, vatAmountDkk };
  }

  if (vatTreatment === "exempt") {
    if (vatAmount !== 0) return { ok: false, appliedRules: [], errors: ["exempt expense booking requires document vat_amount = 0"] };
    const result = post(db, {
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
    return { ...result, documentId: input.documentId, bankTransactionId: input.bankTransactionId, grossAmount, netAmount: grossAmountDkk, vatAmount: 0, vatTreatment, ...fxSummary, netAmountDkk: grossAmountDkk, vatAmountDkk: 0 };
  }

  if (vatTreatment === "non_deductible") {
    // The same two-line shape as `exempt` (gross debit on expense, gross
    // credit on payment), but `vat_amount > 0` is allowed because the VAT
    // is on the bilag — it just can't be reclaimed (§ 37) so it is
    // absorbed into the cost basis. No 4000 line is written; no `vatCode`
    // is attached, so the lines never feed any momsangivelse rubrik. The
    // 25 %-ratio sanity check is skipped because non-deductible VAT is not
    // part of any input-VAT total, and a non-25 % bilag (e.g. a foreign-VAT
    // receipt) is legitimately bookable this way.
    const result = post(db, {
      transactionDate,
      text,
      documentId: input.documentId,
      sourceBankTransactionId: input.bankTransactionId,
      createdBy: input.createdBy,
      createdByProgram: input.createdByProgram,
      ...journalMetadata,
      lines: [
        { accountNo: account.account_no, debitAmount: grossAmountDkk, text: document.invoice_no ?? "Udgift inkl. moms (ikke-fradragsberettiget)" },
        { accountNo: paymentAccountNo, creditAmount: grossAmountDkk, text: bank.text },
      ],
    });
    return { ...result, documentId: input.documentId, bankTransactionId: input.bankTransactionId, grossAmount, netAmount: grossAmountDkk, vatAmount: 0, vatTreatment, ...fxSummary, netAmountDkk: grossAmountDkk, vatAmountDkk: 0 };
  }

  // Exhaustiveness: every value of `ExpenseVatTreatment` is handled above.
  // If the union grows without a matching branch, the `never` assignment
  // forces a compile-time error rather than a silent runtime fall-through.
  const _exhaustive: never = vatTreatment;
  throw new Error(`unhandled vatTreatment: ${_exhaustive}`);
}

export function bookExpenseFromBank(db: Database, input: BookExpenseFromBankInput): BookExpenseFromBankResult {
  return db.transaction(() => bookExpenseFromBankInternal(db, input, true)).immediate();
}

/** Exact path for #583's outer BEGIN IMMEDIATE. */
export function bookExpenseFromBankInCurrentTransaction(db: Database, input: BookExpenseFromBankInput): BookExpenseFromBankResult {
  return bookExpenseFromBankInternal(db, input, true);
}

/**
 * Runs the exact expense-booking path in an immediate transaction that is
 * always rolled back. This keeps preview validation, journal metadata,
 * reconciliation and audit behavior aligned with an applied booking without
 * exposing a second booking implementation or retaining any writes.
 */
export function previewBookExpenseFromBank(db: Database, input: BookExpenseFromBankInput): BookExpenseFromBankResult {
  let result: BookExpenseFromBankResult | undefined;
  const rollback = new Error("expense booking preview rollback");
  try {
    db.transaction(() => {
      result = bookExpenseFromBankInCurrentTransaction(db, input);
      throw rollback;
    }).immediate();
  } catch (error) {
    if (error !== rollback) throw error;
  }
  if (!result) throw new Error("expense booking preview completed without a result");
  return result;
}
