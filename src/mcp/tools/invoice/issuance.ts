/**
 * Issuance-side invoice_* MCP tools: invoice_issue, invoice_render,
 * invoice_credit_note, invoice_post.
 *
 * Split out of `../invoice.ts` (Batch G). Registration order preserved.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { type InvoicePayload } from "../../../core/invoice";
import { issueInvoice } from "../../../core/issued-invoices";
import { preflightApprovedInvoice, snapshotApprovedInvoiceSource } from "../../../core/approved-invoice-preflight";
import { resolveDigisenseStatusChecker, digisenseProviderBinding } from "../../../core/efaktura/digisense-wiring";
import { resolveInvoiceMasterData } from "../../../core/master-data";
import { postIssuedInvoiceToLedger } from "../../../core/invoice-booking";
import { renderIssuedInvoicePdf } from "../../../core/invoice-pdf";
import { issueCreditNote, type IssueCreditNoteInput } from "../../../core/credit-notes";
import { withActor } from "../../actor";
import { envelopeShape, wrapCoreResult, errorEnvelope } from "../../envelope";
import {
  withCompanyDbConfirmed,
  resolveIssuedInvoiceDocumentId,
  confirmField,
} from "../../tool-runtime";
import {
  invoicePayloadSchema,
  docIdOrNumberSchema,
  notFoundEnvelope,
  resolveInvoiceSelectorInPayload,
} from "./_shared";

// --- credit note -------------------------------------------------------------
const creditNotePayloadSchema = z
  .object({
    originalInvoiceDocumentId: z
      .number()
      .int()
      .positive()
      .optional()
      .describe("Document ID of the invoice being credited. Provide this OR originalInvoiceNumber."),
    originalInvoiceNumber: z
      .string()
      .optional()
      .describe("Invoice number of the invoice being credited. Provide this OR originalInvoiceDocumentId."),
    issueDate: z.string().describe("Credit-note issue date in YYYY-MM-DD format."),
    reason: z.string().describe("Reason for the credit note."),
    grossAmount: z
      .number()
      .optional()
      .describe(
        "Amount to credit including VAT, in kroner (decimal DKK). Defaults to the full " +
          "remaining creditable amount; may not exceed it.",
      ),
    creditNoteNumber: z
      .string()
      .optional()
      .describe("Optional explicit credit-note number. If omitted, a sequential number is assigned."),
  })
  .describe("Credit-note payload. grossAmount is in kroner (decimal DKK, 2 decimals — NOT øre).");

// AGENT-8: shared payload-selector resolution — an unknown
// originalInvoiceNumber fails here with a clear "No issued invoice has
// invoice number '…'" envelope instead of the core's misleading
// "originalInvoiceDocumentId must be a positive integer".
function resolveOriginalInvoice(
  db: import("bun:sqlite").Database,
  payload: Record<string, unknown>,
) {
  return resolveInvoiceSelectorInPayload(db, payload, "originalInvoiceDocumentId", "originalInvoiceNumber");
}

export function registerInvoiceIssuanceTools(server: McpServer): void {
  server.registerTool(
    "invoice_issue",
    {
      title: "Issue invoice",
      description:
        "Udsteder kundefaktura + immutable snapshot. " +
        "Alle beløb i payload er i kroner (decimal DKK, 2 decimaler — ikke øre); vatRate er en brøk (0.25 = 25%). " +
        "write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        payload: invoicePayloadSchema,
        customerId: z.number().int().positive().optional(),
        approvedPdfPath: z.string().min(1).optional(),
        preflightId: z.string().uuid().optional(),
        pdfReviewed: z.boolean().optional(),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: InvoicePayload;
      customerId?: number;
      approvedPdfPath?: string;
      preflightId?: string;
      pdfReviewed?: boolean;
      confirm?: boolean;
    }>(server, "invoice_issue", ({ db, args, actor }) => {
      const resolved = resolveInvoiceMasterData(db, args.payload, { customerId: args.customerId });
      if (!resolved.ok) return wrapCoreResult(resolved);
      let approved: { preflightId: string; pdfBytes: Uint8Array; visualReviewConfirmed: boolean; providerBinding: string } | undefined;
      if (args.approvedPdfPath || args.preflightId || args.pdfReviewed !== undefined) {
        if (!args.approvedPdfPath || !args.preflightId || args.pdfReviewed !== true) return errorEnvelope(["Approved PDF issuance requires approvedPdfPath, preflightId and pdfReviewed:true"]);
        const provider = resolveDigisenseStatusChecker(db, args.company);
        if (!provider.ok) return errorEnvelope(provider.errors);
        try { approved = { preflightId: args.preflightId, pdfBytes: snapshotApprovedInvoiceSource(args.company, args.approvedPdfPath).bytes, visualReviewConfirmed: true, providerBinding: digisenseProviderBinding(provider.environment, provider.companyKey) }; }
        catch { return errorEnvelope(["Approved PDF could not be safely read"]); }
      }
      const result = issueInvoice(db, args.company, resolved.payload!, actor, approved);
      // EJER-3: surface master-data notes (fx "Betalingsfrist 30 dage fra
      // kundekortet — virksomhedens standard er 14 dage.") in the success
      // envelope, so a deviating kundefrist is never silently applied.
      const notes = (resolved as { notes?: string[] }).notes;
      return wrapCoreResult(result.ok && notes && notes.length > 0 ? { ...result, notes } : result);
    }),
  );

  server.registerTool(
    "invoice_preflight_approved_pdf",
    {
      title: "Preflight approved invoice PDF",
      description: "Kontrollerer PDF og DigiSense uden at udstede eller sende. Kontroller faktura og PDF visuelt før invoice_issue; angiv at modtageren har aftalt indlejrede bilag.",
      inputSchema: { company: z.string().min(1), payload: invoicePayloadSchema, customerId: z.number().int().positive().optional(), approvedPdfPath: z.string().min(1), attachmentAgreed: z.boolean(), descriptionDifferencesReviewed: z.boolean().optional().describe("Set only after visually reviewing any reported description differences against the PDF and invoice."), confirm: confirmField },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: true },
    },
    withCompanyDbConfirmed<{ company: string; payload: InvoicePayload; customerId?: number; approvedPdfPath: string; attachmentAgreed: boolean; descriptionDifferencesReviewed?: boolean; confirm?: boolean }>(server, "invoice_preflight_approved_pdf", async ({ db, args, actor }) => {
      if (!args.attachmentAgreed) return errorEnvelope(["Recipient agreement for an embedded attachment is required"]);
      let pdfBytes: Buffer;
      try { pdfBytes = snapshotApprovedInvoiceSource(args.company, args.approvedPdfPath).bytes; }
      catch { return errorEnvelope(["Approved PDF could not be safely read"]); }
      const resolved = resolveInvoiceMasterData(db, args.payload, { customerId: args.customerId });
      if (!resolved.ok) return wrapCoreResult(resolved);
      const provider = resolveDigisenseStatusChecker(db, args.company);
      if (!provider.ok) return errorEnvelope(provider.errors);
      return wrapCoreResult(await preflightApprovedInvoice(db, { payload: resolved.payload!, pdfBytes, bilateralAttachmentAgreement: true, descriptionDifferencesReviewed: args.descriptionDifferencesReviewed, providerBinding: digisenseProviderBinding(provider.environment, provider.companyKey), environment: provider.environment, ...actor }, provider.client));
    }),
  );

  server.registerTool(
    "invoice_render",
    {
      title: "Render invoice PDF",
      description:
        "Returnerer og hash-verificerer den immutabelt udstedte PDF for en faktura; manipuleret eller manglende evidens genskabes aldrig. " +
        "Forudsætning: fakturaen skal være udstedt med invoice_issue. write-irreversible.",
      inputSchema: { ...docIdOrNumberSchema, confirm: confirmField },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      confirm?: boolean;
    }>(server, "invoice_render", ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const result = renderIssuedInvoicePdf(db, args.company, { invoiceDocumentId: id });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_credit_note",
    {
      title: "Issue credit note",
      description:
        "Udsteder kreditnota mod en eksisterende faktura. " +
        "Forudsætning: den oprindelige faktura skal være udstedt med invoice_issue og bogført med invoice_post (kredit mod en ubogført faktura giver et åbent tilgodehavende uden modpostering). " +
        "payload.grossAmount er i kroner (decimal DKK, ikke øre). write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        payload: creditNotePayloadSchema,
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; payload: z.infer<typeof creditNotePayloadSchema>; confirm?: boolean }>(
      server,
      "invoice_credit_note",
      ({ db, actor, args }) => {
        const resolution = resolveOriginalInvoice(db, args.payload as Record<string, unknown>);
        if (!resolution.ok) return resolution.envelope;
        // Actor-invariant (#63/#76): attribute the credit-note's reversal entry
        // to the booking agent in the hash-chained ledger + audit_log.
        const result = issueCreditNote(
          db,
          args.company,
          withActor(resolution.payload as IssueCreditNoteInput, actor),
        );
        return wrapCoreResult(result);
      },
    ),
  );

  server.registerTool(
    "invoice_post",
    {
      title: "Post invoice to ledger",
      description:
        "Bogfører en udstedt faktura i finansen (debit 1100 Debitorer, credit 1000 Salg + udgående moms). " +
        "Forudsætning: fakturaen skal være udstedt med invoice_issue og må ikke allerede være bogført. " +
        "Når den er kørt kan downstream-tools som invoice_settle_bank/invoice_apply_payment/invoice_credit_note bruges. " +
        "write-irreversible.",
      inputSchema: { ...docIdOrNumberSchema, confirm: confirmField },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      confirm?: boolean;
    }>(server, "invoice_post", ({ db, actor, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      // Actor-invariant (#63/#76): attribute the ledger posting to the booking
      // agent in the hash chain + audit_log, not the OS user / "system".
      const result = postIssuedInvoiceToLedger(db, {
        invoiceDocumentId: id,
        createdBy: actor.createdBy,
        createdByProgram: actor.createdByProgram,
      });
      return wrapCoreResult(result);
    }),
  );
}
