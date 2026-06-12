/**
 * Shared building blocks for the invoice_* MCP tools.
 *
 * Split out of `../invoice.ts` (Batch G). Everything here is consumed by 2+
 * per-sub-domain register helpers (`issuance.ts`, `settlement.ts`,
 * `reminder.ts`, `interest.ts`, `compensation.ts`, `query.ts`). Anything that
 * only one file needs stays inline in that file.
 */

import { z } from "zod";
import { errorEnvelope } from "../../envelope";
import { invoiceNotFoundEnvelope } from "../../tool-runtime";

// --------------------------------------------------------------- payload schemas
// All monetary fields are in kroner — decimal DKK with 2 decimals (NOT øre).
// VAT rates are fractions (0.25 = 25%). FX rates are major-unit → DKK (e.g. 7.46).

const invoiceLineSchema = z.object({
  description: z
    .string()
    .describe("Description of the good or service. Required on every line."),
  quantity: z.number().optional().describe("Quantity of the line item."),
  unitPriceExVat: z
    .number()
    .optional()
    .describe("Unit price excluding VAT, in kroner (decimal DKK)."),
  lineTotalExVat: z
    .number()
    .optional()
    .describe(
      "Line total excluding VAT, in kroner (decimal DKK). Must equal quantity * unitPriceExVat.",
    ),
});

const invoiceTotalsSchema = z.object({
  netAmount: z
    .number()
    .optional()
    .describe(
      "Total excluding VAT, in kroner (decimal DKK). Required for full invoices; " +
        "must equal the sum of all lines' lineTotalExVat.",
    ),
  vatRate: z
    .number()
    .optional()
    .describe("VAT rate as a fraction (0.25 = 25%). Required for standard-VAT invoices; must be omitted for reverse-charge invoices."),
  vatAmount: z
    .number()
    .optional()
    .describe(
      "Total VAT, in kroner (decimal DKK). Required for standard-VAT invoices; " +
        "must be omitted for reverse-charge invoices.",
    ),
  grossAmount: z
    .number()
    .optional()
    .describe(
      "Total including VAT, in kroner (decimal DKK). Required. For standard VAT it must " +
        "equal netAmount + vatAmount; for reverse charge it must equal netAmount.",
    ),
  fxRateToDkk: z
    .number()
    .optional()
    .describe("For non-DKK invoices: FX rate from invoice currency to DKK (e.g. 7.46)."),
  netAmountDkk: z
    .number()
    .optional()
    .describe("For non-DKK invoices: netAmount converted to kroner (decimal DKK). Must equal netAmount * fxRateToDkk."),
  vatAmountDkk: z
    .number()
    .optional()
    .describe("For non-DKK standard-VAT invoices: vatAmount converted to kroner (decimal DKK). Must equal vatAmount * fxRateToDkk."),
  grossAmountDkk: z
    .number()
    .optional()
    .describe("For non-DKK invoices: grossAmount converted to kroner (decimal DKK). Must equal grossAmount * fxRateToDkk."),
  vatComputationBasis: z
    .string()
    .optional()
    .describe("Optional VAT computation basis, e.g. 'VAT_20_OF_GROSS' for simplified invoices."),
});

const invoicePartySchema = z.object({
  name: z.string().optional().describe("Party name."),
  address: z.string().optional().describe("Party postal address."),
  vatOrCvr: z.string().optional().describe("Party VAT or CVR number, e.g. 'DK12345678'."),
});

const invoiceBuyerSchema = z.object({
  name: z.string().optional().describe("Buyer name. Required for full invoices."),
  address: z.string().optional().describe("Buyer postal address. Required for full invoices."),
  vatOrCvr: z
    .string()
    .optional()
    .describe("Buyer VAT or CVR number. Required for foreign reverse-charge invoices."),
  eanNumber: z
    .string()
    .optional()
    .describe("13-digit EAN/GLN number — required when invoicing a Danish public-sector recipient."),
  publicRecipient: z
    .boolean()
    .optional()
    .describe("Set true when the buyer is a Danish public-sector body (forces full invoice + EAN)."),
});

export const invoicePayloadSchema = z
  .object({
    invoiceType: z
      .enum(["full", "simplified"])
      .describe("'full' for a full invoice; 'simplified' only for gross totals up to DKK 3,000."),
    vatTreatment: z
      .enum(["standard", "domestic_reverse_charge", "foreign_reverse_charge"])
      .optional()
      .describe("VAT treatment (default 'standard'). Reverse-charge variants also require reverseChargeBasis."),
    issueDate: z.string().optional().describe("Invoice issue date in YYYY-MM-DD format. Required."),
    invoiceNumber: z
      .string()
      .optional()
      .describe("Optional explicit invoice number. If omitted, a sequential number is assigned."),
    seller: invoicePartySchema
      .optional()
      .describe("Seller details. seller.name, seller.address and seller.vatOrCvr are all required."),
    buyer: invoiceBuyerSchema.optional().describe("Buyer details."),
    lines: z
      .array(invoiceLineSchema)
      .optional()
      .describe("Invoice lines — at least one line with a description is required."),
    totals: invoiceTotalsSchema
      .optional()
      .describe("Invoice totals. All amounts are in kroner (decimal DKK)."),
    reverseChargeBasis: z
      .enum([
        "DK_MOMSLOVEN_§46_STK_1_NR_3",
        "DK_MOMSLOVEN_§46_STK_1_NR_6",
        "DK_MOMSLOVEN_§46_STK_1_NR_7",
        "EU_MOMSDIREKTIV_ART_196",
        "EU_MOMSDIREKTIV_ART_199",
      ])
      .optional()
      .describe("Legal basis for reverse charge — required for reverse-charge invoices."),
    reverseChargeNote: z
      .string()
      .optional()
      .describe("Optional free-text note explaining the reverse-charge treatment."),
    currency: z
      .string()
      .optional()
      .describe("3-letter ISO currency code (default 'DKK'). Non-DKK invoices require the *Dkk total fields and fxRateToDkk."),
    dueDate: z.string().optional().describe("Payment due date in YYYY-MM-DD format; cannot be earlier than issueDate."),
    deliveryDate: z.string().optional().describe("Delivery date in YYYY-MM-DD format. Use this OR the deliveryPeriod fields, not both."),
    deliveryPeriodStart: z
      .string()
      .optional()
      .describe("Start of the delivery period in YYYY-MM-DD format. Must be provided together with deliveryPeriodEnd."),
    deliveryPeriodEnd: z
      .string()
      .optional()
      .describe("End of the delivery period in YYYY-MM-DD format. Must be provided together with deliveryPeriodStart."),
  })
  .describe(
    "Danish customer-invoice payload. All monetary amounts are in kroner " +
      "(decimal DKK, 2 decimals — NOT øre); vatRate is a fraction (0.25 = 25%).",
  );

// The issued-invoice selector. Every tool below identifies an invoice by
// EITHER documentId OR invoiceNumber — the rule is spelled out in BOTH field
// descriptions so an agent reading either one sees it.
const SELECTOR_DOC_ID =
  "Document ID of the issued invoice. Provide exactly one of documentId or " +
  "invoiceNumber. Find IDs with invoice_list / invoice_find.";
const SELECTOR_INVOICE_NUMBER =
  "Invoice number of the issued invoice, e.g. '2026-001'. Provide exactly one " +
  "of documentId or invoiceNumber. Find numbers with invoice_list / invoice_find.";

export const docIdOrNumberSchema = {
  company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
  documentId: z.number().int().positive().optional().describe(SELECTOR_DOC_ID),
  invoiceNumber: z.string().optional().describe(SELECTOR_INVOICE_NUMBER),
};

// Re-exported under the local name `notFoundEnvelope` to keep all the
// `if (!id) return notFoundEnvelope(args)` call-sites in this file readable.
// The actual builder lives in tool-runtime.ts so peppol.ts / email.ts use
// the same shared string (Batch D-6).
export const notFoundEnvelope = invoiceNotFoundEnvelope;

// ---------------------------------------------------------- lifecycle gates (#374)
// The invoice_* family is a sequence: invoice_issue → invoice_post →
// (invoice_settle_bank | invoice_apply_payment | invoice_refund_bank |
//  invoice_write_off_bad_debt | invoice_remind → invoice_post_reminder |
//  invoice_claim_interest → invoice_post_interest |
//  invoice_claim_compensation → invoice_post_compensation |
//  invoice_credit_note | invoice_settle_claim_bank).
//
// Core functions only check the document type ('issued_invoice'); they do not
// know that a downstream tool requires the receivable to be booked first. If we
// let the call through, a settle/payment proceeds against a phantom balance and
// surfaces as a confusing "amount exceeds open balance" or balance-sheet skew.
//
// These helpers reject the call early with an envelope error that NAMES the
// prior tool the agent must run — the gap #374 calls out.

export const POSTED_REQUIRED_PREFIX = "Forudsætning ikke opfyldt:";

function lookupIssuedInvoice(
  db: import("bun:sqlite").Database,
  documentId: number,
): { id: number; invoice_no: string; document_type: string } | null {
  return (
    db
      .query(
        `SELECT id, invoice_no, document_type FROM documents WHERE id = ? LIMIT 1`,
      )
      .get(documentId) as
      | { id: number; invoice_no: string; document_type: string }
      | null
  );
}

function isInvoicePostedToLedger(
  db: import("bun:sqlite").Database,
  documentId: number,
): boolean {
  const row = db
    .query(
      `SELECT id FROM journal_entries WHERE document_id = ? AND reversal_of_entry_id IS NULL LIMIT 1`,
    )
    .get(documentId) as { id: number } | null;
  return row != null;
}

/**
 * Returns `null` when the invoice exists, is an issued invoice, and has a
 * non-reversed journal entry (i.e. `invoice_post` ran). Otherwise returns an
 * envelope error whose `errors[]` names the prior tool the agent must call —
 * either `invoice_issue` (no such document / wrong type) or `invoice_post`
 * (issued but not yet booked).
 */
export function requireInvoicePostedEnvelope(
  db: import("bun:sqlite").Database,
  documentId: number,
  /** Name of the tool the agent is currently calling — surfaced in the error. */
  currentTool: string,
) {
  const doc = lookupIssuedInvoice(db, documentId);
  if (!doc) {
    return errorEnvelope(
      `${POSTED_REQUIRED_PREFIX} ingen faktura med documentId=${documentId}. ` +
        `Udsted fakturaen først med invoice_issue, eller find dens documentId/invoiceNumber via invoice_list / invoice_find inden ${currentTool}.`,
    );
  }
  if (doc.document_type !== "issued_invoice") {
    return errorEnvelope(
      `${POSTED_REQUIRED_PREFIX} document ${documentId} er ikke en udstedt faktura (document_type='${doc.document_type}'). ` +
        `${currentTool} virker kun på en faktura udstedt med invoice_issue.`,
    );
  }
  if (!isInvoicePostedToLedger(db, documentId)) {
    return errorEnvelope(
      `${POSTED_REQUIRED_PREFIX} faktura ${doc.invoice_no} (documentId=${documentId}) er udstedt men ikke bogført. ` +
        `Kald invoice_post på fakturaen før ${currentTool}.`,
    );
  }
  return null;
}
