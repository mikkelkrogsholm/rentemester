/**
 * Late-compensation invoice_* MCP tools: invoice_claim_compensation,
 * invoice_post_compensation.
 *
 * Split out of `../invoice.ts` (Batch G). Registration order preserved.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  registerInvoiceLateCompensation,
  postInvoiceLateCompensationToLedger,
} from "../../../core/invoice-compensation";
import { withActor } from "../../actor";
import { envelopeShape, errorEnvelope, wrapCoreResult } from "../../envelope";
import {
  withCompanyDbConfirmed,
  resolveIssuedInvoiceDocumentId,
  confirmField,
} from "../../tool-runtime";
import {
  docIdOrNumberSchema,
  notFoundEnvelope,
  requireInvoicePostedEnvelope,
  POSTED_REQUIRED_PREFIX,
} from "./_shared";

function hasRegisteredCompensationClaim(
  db: import("bun:sqlite").Database,
  documentId: number,
): boolean {
  const row = db
    .query(
      `SELECT id FROM invoice_compensation_claims WHERE invoice_document_id = ? LIMIT 1`,
    )
    .get(documentId) as { id: number } | null;
  return row != null;
}

export function registerInvoiceCompensationTools(server: McpServer): void {
  server.registerTool(
    "invoice_claim_compensation",
    {
      title: "Register late-compensation claim",
      description:
        "Registrerer kompensationskrav (uden at bogføre — kald invoice_post_compensation bagefter). " +
        "Forudsætning: fakturaen skal være bogført med invoice_post og være forfalden. " +
        "write-irreversible.",
      inputSchema: {
        ...docIdOrNumberSchema,
        asOf: z
          .string()
          .min(1)
          .describe("As-of date in YYYY-MM-DD format the compensation is computed up to."),
        amountDkk: z
          .number()
          .optional()
          .describe(
            "Optional fixed compensation amount in kroner (decimal DKK). When " +
              "omitted, the statutory DKK 310 late-payment compensation is used.",
          ),
        note: z.string().optional().describe("Optional free-text note on the compensation claim."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      asOf: string;
      amountDkk?: number;
      note?: string;
      confirm?: boolean;
    }>(server, "invoice_claim_compensation", ({ db, actor, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const blocked = requireInvoicePostedEnvelope(db, id, "invoice_claim_compensation");
      if (blocked) return blocked;
      // Actor-invariant (#63/#76): attribute the compensation-claim registration
      // to the booking agent in the hash chain + audit_log, not the OS user.
      const result = registerInvoiceLateCompensation(
        db,
        withActor(
          {
            invoiceDocumentId: id,
            asOfDate: args.asOf,
            compensationAmountDkk: args.amountDkk,
            note: args.note,
          },
          actor,
        ),
      );
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_post_compensation",
    {
      title: "Post compensation claim to ledger",
      description:
        "Bogfører registreret kompensationskrav (kompensationen indtægtsføres). " +
        "Forudsætning: et kompensationskrav skal være registreret med invoice_claim_compensation først. " +
        "write-irreversible.",
      inputSchema: {
        ...docIdOrNumberSchema,
        date: z
          .string()
          .optional()
          .describe(
            "Posting date in YYYY-MM-DD format. When omitted, the claim's own date is used.",
          ),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      date?: string;
      confirm?: boolean;
    }>(server, "invoice_post_compensation", ({ db, actor, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      if (!hasRegisteredCompensationClaim(db, id)) {
        return errorEnvelope(
          `${POSTED_REQUIRED_PREFIX} der er ingen registreret kompensationskrav på faktura documentId=${id}. ` +
            `Kald invoice_claim_compensation først for at registrere kravet før invoice_post_compensation.`,
        );
      }
      // Actor-invariant (#63/#76): attribute the compensation posting to the
      // booking agent in the hash chain + audit_log, not the OS user.
      const result = postInvoiceLateCompensationToLedger(db, {
        invoiceDocumentId: id,
        transactionDate: args.date,
        createdBy: actor.createdBy,
        createdByProgram: actor.createdByProgram,
      });
      return wrapCoreResult(result);
    }),
  );
}
