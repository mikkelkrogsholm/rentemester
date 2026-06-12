/**
 * Reminder-side invoice_* MCP tools: invoice_remind, invoice_post_reminder.
 *
 * Split out of `../invoice.ts` (Batch G). Registration order preserved.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  registerInvoiceReminder,
  postInvoiceReminderToLedger,
} from "../../../core/invoice-reminders";
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

function hasRegisteredReminder(
  db: import("bun:sqlite").Database,
  documentId: number,
): boolean {
  const row = db
    .query(
      `SELECT id FROM invoice_reminders WHERE invoice_document_id = ? LIMIT 1`,
    )
    .get(documentId) as { id: number } | null;
  return row != null;
}

export function registerInvoiceReminderTools(server: McpServer): void {
  server.registerTool(
    "invoice_remind",
    {
      title: "Register invoice reminder",
      description:
        "Registrerer rykker på forfalden faktura (uden at bogføre rykkergebyret — kald invoice_post_reminder bagefter). " +
        "Forudsætning: fakturaen skal være bogført med invoice_post og være forfalden. " +
        "write-irreversible.",
      inputSchema: {
        ...docIdOrNumberSchema,
        date: z
          .string()
          .min(1)
          .describe("Reminder date in YYYY-MM-DD format."),
        fee: z
          .number()
          .optional()
          .describe(
            "Optional reminder fee in kroner (decimal DKK). When omitted, no fee " +
              "is added to the reminder.",
          ),
        note: z.string().optional().describe("Optional free-text note on the reminder."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      date: string;
      fee?: number;
      note?: string;
      confirm?: boolean;
    }>(server, "invoice_remind", ({ db, actor, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      const blocked = requireInvoicePostedEnvelope(db, id, "invoice_remind");
      if (blocked) return blocked;
      // Actor-invariant (#63/#76): attribute the reminder-registration entry to
      // the booking agent in the hash chain + audit_log, not the OS user.
      const result = registerInvoiceReminder(
        db,
        withActor(
          {
            invoiceDocumentId: id,
            reminderDate: args.date,
            feeAmount: args.fee,
            note: args.note,
          },
          actor,
        ),
      );
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "invoice_post_reminder",
    {
      title: "Post invoice reminder to ledger",
      description:
        "Bogfører en registreret rykker (rykkergebyret indtægtsføres). " +
        "Forudsætning: en rykker skal være registreret med invoice_remind først. " +
        "write-irreversible.",
      inputSchema: {
        ...docIdOrNumberSchema,
        reminderId: z
          .number()
          .int()
          .positive()
          .optional()
          .describe(
            "Optional ID of the specific registered reminder to post. When " +
              "omitted, the latest unposted reminder on the invoice is posted.",
          ),
        date: z
          .string()
          .optional()
          .describe(
            "Posting date in YYYY-MM-DD format. When omitted, the reminder's own " +
              "date is used.",
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
      reminderId?: number;
      date?: string;
      confirm?: boolean;
    }>(server, "invoice_post_reminder", ({ db, actor, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return notFoundEnvelope(args);
      if (!hasRegisteredReminder(db, id)) {
        return errorEnvelope(
          `${POSTED_REQUIRED_PREFIX} der er ingen registreret rykker på faktura documentId=${id}. ` +
            `Kald invoice_remind først for at registrere rykkeren før invoice_post_reminder.`,
        );
      }
      // Actor-invariant (#63/#76): attribute the reminder-fee posting to the
      // booking agent in the hash chain + audit_log, not the OS user.
      const result = postInvoiceReminderToLedger(db, {
        invoiceDocumentId: id,
        reminderId: args.reminderId,
        transactionDate: args.date,
        createdBy: actor.createdBy,
        createdByProgram: actor.createdByProgram,
      });
      return wrapCoreResult(result);
    }),
  );
}
