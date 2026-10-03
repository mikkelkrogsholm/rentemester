import type { Database } from "bun:sqlite";
import { validSimplifiedPurchaseCompanyContext } from "./document-company-context";
import { validIncompleteStandardPurchaseVatEvidenceReview } from "./document-purchase-vat-evidence-review";
import { absDkk, compareDkk, percentOfDkk, subtractDkk } from "./money";
import { deductibleDanishPurchaseSupplierErrors } from "./supplier-identity";

export function deductiblePurchaseSupplierErrors(document: {
  sender_vat_cvr: string | null;
  supplier_country_code: string | null;
  supplier_identifier_kind: string | null;
  supplier_identity_status: string | null;
}): string[] {
  return deductibleDanishPurchaseSupplierErrors({
    supplierVatOrCvr: document.sender_vat_cvr,
    supplierCountryCode: document.supplier_country_code,
    supplierIdentifierKind: document.supplier_identifier_kind,
    supplierIdentityStatus: document.supplier_identity_status,
  });
}

/** Preserve each workflow's incomplete-invoice gate alongside hash-bound company context. */
export function standardPurchaseCompanyContextErrors(db: Database, documentId: number, document: {
  document_type: string;
  recipient_vat_cvr: string | null;
  payload_json: string | null;
}, options: { requireIncompleteReview: boolean }): string[] {
  try {
    const payload = document.payload_json ? JSON.parse(document.payload_json) as Record<string, unknown> : {};
    // Direct-bank expenses require this review even with a printed recipient.
    // Payable registration accepts it as an alternative to recipient/context.
    if (options.requireIncompleteReview && payload.incompleteStandardPurchaseInvoice === true &&
        !validIncompleteStandardPurchaseVatEvidenceReview(db, documentId)) {
      return ["incomplete standard invoice requires a valid hash-bound VAT evidence review before input-VAT deduction"];
    }
    const invoiceStatesCompany = typeof document.recipient_vat_cvr === "string" && document.recipient_vat_cvr.trim().length > 0;
    const contextIsValid = payload.danishSimplifiedPurchaseInvoice === true && validSimplifiedPurchaseCompanyContext(db, documentId);
    const reviewedIncomplete = payload.incompleteStandardPurchaseInvoice === true && validIncompleteStandardPurchaseVatEvidenceReview(db, documentId);
    if (document.document_type === "purchase_sale" && !invoiceStatesCompany && !contextIsValid && !reviewedIncomplete) {
      return ["standard purchase VAT requires invoice-stated recipient identity or a valid hash-bound simplified-invoice company context"];
    }
    return [];
  } catch {
    return ["document payload_json is not valid JSON"];
  }
}

/** Check the document's native currency with the existing one-øre rounding tolerance. */
export function uniformDanishPurchaseVatErrors(documentId: number, grossAmount: number, vatAmount: number): string[] {
  const documentNetAmount = subtractDkk(grossAmount, vatAmount);
  const expectedVatAmount = percentOfDkk(documentNetAmount, 25);
  if (compareDkk(absDkk(subtractDkk(vatAmount, expectedVatAmount)), 0.01) > 0) {
    return [`document ${documentId} vat_amount ${vatAmount} is inconsistent with the 25% rate (expected ~${expectedVatAmount} for net ${documentNetAmount})`];
  }
  return [];
}
