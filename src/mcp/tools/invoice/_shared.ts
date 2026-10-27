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
import { resolveInvoiceReceivableAccount } from "../../../core/invoice-fx-receivable";

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
  unitCode: z.string().optional().describe("OIOUBL unit code, such as EA."),
  taxClassification: z.enum(["taxable", "exempt", "reverse_charge"]).optional().describe("Explicit VAT classification for this invoice line."),
  vatRate: z.number().optional().describe("Line VAT rate as a fraction, for example 0.25."),
  reverseChargeBasis: z.string().optional().describe("Legal basis for a reverse-charge line."),
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
  buyerReference: z.string().optional().describe("Explicit buyer reference for the electronic invoice."),
  orderReference: z.string().optional().describe("Explicit buyer order reference for the electronic invoice."),
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

// ------------------------------------------------- payload selector resolution (AGENT-8)
// The settlement family (invoice_settle_bank, invoice_settle_claim_bank,
// invoice_write_off_bad_debt, invoice_apply_payment, invoice_refund_bank) and
// invoice_credit_note carry the invoice selector INSIDE `payload` as
// "<idKey> OR <numberKey>". The old per-file resolvers silently passed an
// UNRESOLVED invoiceNumber through to the core, which then rejected with
// "invoiceDocumentId must be a positive integer" — naming a field the agent
// never sent. Resolution must fail FIRST, with the same clear wording the CLI
// uses for `--invoice-number` (src/cli/invoice/_shared.ts), adapted to the
// MCP discovery tools.

export type PayloadInvoiceResolution =
  | { ok: true; payload: Record<string, unknown> }
  | { ok: false; envelope: ReturnType<typeof errorEnvelope> };

/**
 * Resolve the issued-invoice selector inside a tool payload.
 *
 * - `idKey` present and a positive integer → passthrough (the number, if any,
 *   is ignored; the id wins, matching the field descriptions).
 * - otherwise a non-blank `numberKey` → look the invoice up by `invoice_no`;
 *   a miss is an error envelope naming the number and the discovery tools —
 *   NEVER a fallthrough to the core's "<idKey> must be a positive integer".
 * - neither present → passthrough so the core's own required-field validation
 *   produces its usual message.
 */
export function resolveInvoiceSelectorInPayload(
  db: import("bun:sqlite").Database,
  payload: Record<string, unknown>,
  idKey: string,
  numberKey: string,
): PayloadInvoiceResolution {
  const id = payload[idKey];
  if (typeof id === "number" && Number.isInteger(id) && id > 0) {
    return { ok: true, payload };
  }
  const value = typeof payload[numberKey] === "string" ? (payload[numberKey] as string).trim() : "";
  if (!value) return { ok: true, payload };
  const row = db
    .query(`SELECT id FROM documents WHERE document_type = 'issued_invoice' AND invoice_no = ? LIMIT 1`)
    .get(value) as { id: number } | null;
  if (!row) {
    return {
      ok: false,
      envelope: errorEnvelope(
        `No issued invoice has invoice number '${value}' — check the value with 'invoice_list' or 'invoice_find'`,
      ),
    };
  }
  return { ok: true, payload: { ...payload, [idKey]: row.id } };
}

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

function invoicePostingState(
  db: import("bun:sqlite").Database,
  documentId: number,
):
  | { kind: "valid" }
  | { kind: "missing" }
  | { kind: "legacy"; journalEntryId: number }
  | { kind: "invalid"; error: string } {
  const link = db
    .query(
      `SELECT journal_entry_id
         FROM issued_invoice_postings
        WHERE invoice_document_id = ?`,
    )
    .get(documentId) as { journal_entry_id: number } | null;
  if (!link) {
    const active = db.query(
      `SELECT j.id
         FROM journal_entries j
        WHERE j.document_id = ?
          AND j.status = 'posted'
          AND j.reversal_of_entry_id IS NULL
          AND NOT EXISTS (
            SELECT 1 FROM journal_entries reversal
             WHERE reversal.reversal_of_entry_id = j.id
          )
        ORDER BY j.id ASC`,
    ).all(documentId) as Array<{ id: number }>;
    if (active.length === 0) return { kind: "missing" };
    if (active.length === 1) return { kind: "legacy", journalEntryId: active[0]!.id };
    return {
      kind: "invalid",
      error: `fakturaen har ${active.length} aktive journaler uden et entydigt issued_invoice_postings-link`,
    };
  }
  const evidence = resolveInvoiceReceivableAccount(db, { invoiceDocumentId: documentId });
  return evidence.ok
    ? { kind: "valid" }
    : { kind: "invalid", error: evidence.error };
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
    // No such invoice at all — a NOT_FOUND, not a lifecycle precondition.
    return errorEnvelope(
      `${POSTED_REQUIRED_PREFIX} ingen faktura med documentId=${documentId}. ` +
        `Udsted fakturaen først med invoice_issue, eller find dens documentId/invoiceNumber via invoice_list / invoice_find inden ${currentTool}.`,
      { code: "NOT_FOUND" },
    );
  }
  if (doc.document_type !== "issued_invoice") {
    return errorEnvelope(
      `${POSTED_REQUIRED_PREFIX} document ${documentId} er ikke en udstedt faktura (document_type='${doc.document_type}'). ` +
        `${currentTool} virker kun på en faktura udstedt med invoice_issue.`,
      { code: "PRECONDITION_MISSING" },
    );
  }
  const posting = invoicePostingState(db, documentId);
  if (posting.kind === "missing") {
    return errorEnvelope(
      `${POSTED_REQUIRED_PREFIX} faktura ${doc.invoice_no} (documentId=${documentId}) er udstedt men ikke bogført. ` +
        `Kald invoice_post på fakturaen før ${currentTool}.`,
      { code: "PRECONDITION_MISSING" },
    );
  }
  if (posting.kind === "legacy") {
    return errorEnvelope(
      `${POSTED_REQUIRED_PREFIX} faktura ${doc.invoice_no} (documentId=${documentId}) har aktiv uklassificeret journal ${posting.journalEntryId}, men mangler kanonisk bogføringsevidens. ` +
        `Stop automatiseringen; efter menneskelig kontrol kan en administrator bruge CLI-kommandoen invoice repair-posting med det eksplicitte journal-id før ${currentTool}.`,
      { code: "PRECONDITION_MISSING" },
    );
  }
  if (posting.kind === "invalid") {
    return errorEnvelope(
      `${POSTED_REQUIRED_PREFIX} faktura ${doc.invoice_no} (documentId=${documentId}) har ugyldig eller beskadiget bogføringsevidens: ${posting.error}. ` +
        `Stop automatiseringen og få bogføringen auditeret/repareret manuelt før ${currentTool}; gentag ikke invoice_post.`,
      { code: "PRECONDITION_MISSING" },
    );
  }
  return null;
}
