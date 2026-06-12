/**
 * Late-interest invoice_* MCP tools: invoice_claim_interest,
 * invoice_post_interest.
 *
 * Split out of `../invoice.ts` (Batch G). Registration order preserved.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  registerInvoiceLateInterest,
  postInvoiceLateInterestToLedger,
  postInterestCorrection,
} from "../../../core/invoice-interest";
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

function hasRegisteredInterestClaim(
  db: import("bun:sqlite").Database,
  documentId: number,
): boolean {
  const row = db
    .query(
      `SELECT id FROM invoice_interest_claims WHERE invoice_document_id = ? LIMIT 1`,
    )
    .get(documentId) as { id: number } | null;
  return row != null;
}

export function registerInvoiceInterestTools(server: McpServer): void {
  server.registerTool(
    "invoice_claim_interest",
    {
      title: "Register late-interest claim",
      description:
        "Registrerer morarentekrav (uden at bogføre — kald invoice_post_interest bagefter). " +
        "Forudsætning: fakturaen skal være bogført med invoice_post og være forfalden. " +
        "Et nyt krav opkræver kun renten for perioden siden sidste krav (inkrementel — " +
        "ingen dobbelt-opkrævning), så rente kan registreres ad flere omgange. " +
        "write-irreversible.",
      inputSchema: {
        ...docIdOrNumberSchema,
        asOf: z
          .string()
          .min(1)
          .describe("As-of date in YYYY-MM-DD format the late interest is computed up to."),
        referenceRate: z
          .number()
          .describe(
            "Nationalbanken's reference rate as a percentage (e.g. 2.65 for 2.65%); " +
              "the statutory late-interest surcharge is added on top.",
          ),
        note: z.string().optional().describe("Optional free-text note on the interest claim."),
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
      referenceRate: number;
      note?: string;
      confirm?: boolean;
    }>(server, "invoice_claim_interest", ({ db, actor, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const blocked = requireInvoicePostedEnvelope(db, id, "invoice_claim_interest");
      if (blocked) return blocked;
      // Actor-invariant (#63/#76): attribute the interest-claim registration to
      // the booking agent in the hash chain + audit_log, not the OS user.
      const result = registerInvoiceLateInterest(
        db,
        withActor(
          {
            invoiceDocumentId: id,
            asOfDate: args.asOf,
            referenceRatePercent: args.referenceRate,
            note: args.note,
          },
          actor,
        ),
      );
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_post_interest",
    {
      title: "Post late-interest claim to ledger",
      description:
        "Bogfører registreret morarentekrav (renten indtægtsføres). " +
        "Forudsætning: et morarentekrav skal være registreret med invoice_claim_interest først. " +
        "write-irreversible.",
      inputSchema: {
        ...docIdOrNumberSchema,
        claimId: z
          .number()
          .int()
          .positive()
          .optional()
          .describe(
            "Optional ID of the specific registered interest claim to post. When " +
              "omitted, the oldest not-yet-posted interest claim on the invoice is " +
              "posted (chronological booking order).",
          ),
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
      claimId?: number;
      date?: string;
      confirm?: boolean;
    }>(server, "invoice_post_interest", ({ db, actor, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      if (!hasRegisteredInterestClaim(db, id)) {
        return errorEnvelope(
          `${POSTED_REQUIRED_PREFIX} der er ingen registreret morarentekrav på faktura documentId=${id}. ` +
            `Kald invoice_claim_interest først for at registrere kravet før invoice_post_interest.`,
        );
      }
      // Actor-invariant (#63/#76): attribute the interest posting to the
      // booking agent in the hash chain + audit_log, not the OS user.
      const result = postInvoiceLateInterestToLedger(db, {
        invoiceDocumentId: id,
        claimId: args.claimId,
        transactionDate: args.date,
        createdBy: actor.createdBy,
        createdByProgram: actor.createdByProgram,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_post_interest_correction",
    {
      title: "Post late-interest correction",
      description:
        "Bogfører en korrektion af for meget opkrævet morarente (debiterer renteindtægt, " +
        "krediterer tilgodehavende) for det beløb invoice_interest_correction_calc foreslår. " +
        "Forudsætning: et bogført morarentekrav og en bagud-dateret saldonedsættelse der gør " +
        "den lovlige rente lavere end det bogførte. Afvises hvis der intet er at korrigere. " +
        "write-irreversible.",
      inputSchema: {
        ...docIdOrNumberSchema,
        date: z
          .string()
          .optional()
          .describe("Posting date in YYYY-MM-DD format. When omitted, the last posted claim's date is used."),
        reason: z.string().optional().describe("Optional free-text reason for the correction."),
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
      reason?: string;
      confirm?: boolean;
    }>(server, "invoice_post_interest_correction", ({ db, actor, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      // Actor-invariant (#63/#76): attribute the correction posting to the
      // booking agent in the hash chain + audit_log, not the OS user.
      const result = postInterestCorrection(db, {
        invoiceDocumentId: id,
        transactionDate: args.date,
        reason: args.reason,
        createdBy: actor.createdBy,
        createdByProgram: actor.createdByProgram,
      });
      return wrapCoreResult(result);
    }),
  );
}
