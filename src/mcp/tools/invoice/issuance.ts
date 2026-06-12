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
import { resolveInvoiceMasterData } from "../../../core/master-data";
import { postIssuedInvoiceToLedger } from "../../../core/invoice-booking";
import { renderIssuedInvoicePdf } from "../../../core/invoice-pdf";
import { issueCreditNote, type IssueCreditNoteInput } from "../../../core/credit-notes";
import { withActor } from "../../actor";
import { envelopeShape, wrapCoreResult } from "../../envelope";
import {
  withCompanyDbConfirmed,
  resolveIssuedInvoiceDocumentId,
  confirmField,
} from "../../tool-runtime";
import {
  invoicePayloadSchema,
  docIdOrNumberSchema,
  notFoundEnvelope,
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

function resolveOriginalInvoice(
  db: import("bun:sqlite").Database,
  payload: Record<string, unknown>,
): Record<string, unknown> {
  if (
    payload.originalInvoiceDocumentId === undefined ||
    payload.originalInvoiceDocumentId === null ||
    payload.originalInvoiceDocumentId === 0
  ) {
    const value =
      typeof payload.originalInvoiceNumber === "string" ? payload.originalInvoiceNumber.trim() : "";
    if (value) {
      const row = db
        .query(`SELECT id FROM documents WHERE document_type = 'issued_invoice' AND invoice_no = ? LIMIT 1`)
        .get(value) as { id: number } | null;
      if (row) return { ...payload, originalInvoiceDocumentId: row.id };
    }
  }
  return payload;
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
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: InvoicePayload;
      customerId?: number;
      confirm?: boolean;
    }>(server, "invoice_issue", ({ db, args }) => {
      const resolved = resolveInvoiceMasterData(db, args.payload, { customerId: args.customerId });
      if (!resolved.ok) return wrapCoreResult(resolved);
      const result = issueInvoice(db, args.company, resolved.payload);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_render",
    {
      title: "Render invoice PDF",
      description:
        "Renderer (eller genskaber) deterministisk PDF for udstedt faktura. " +
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
    withCompanyDbConfirmed<{ company: string; payload: IssueCreditNoteInput; confirm?: boolean }>(
      server,
      "invoice_credit_note",
      ({ db, actor, args }) => {
        const resolved = resolveOriginalInvoice(db, args.payload as Record<string, unknown>);
        // Actor-invariant (#63/#76): attribute the credit-note's reversal entry
        // to the booking agent in the hash-chained ledger + audit_log.
        const result = issueCreditNote(
          db,
          args.company,
          withActor(resolved as IssueCreditNoteInput, actor),
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
