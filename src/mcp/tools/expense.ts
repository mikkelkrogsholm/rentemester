/**
 * MCP-tool: `expense_book` (write-irreversible).
 *
 * 1:1-mapping af CLI-kommandoen `expense book`. Bogfører en leverandørudgift
 * direkte fra et bilag + en bankpost.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { bookExpenseFromBank } from "../../core/expense-booking";
import { withActor } from "../actor";
import { envelopeShape, wrapCoreResult } from "../envelope";
import { withCompanyDbConfirmed, confirmField } from "../tool-runtime";

const vatTreatmentEnum = z
  .enum(["standard", "reverse_charge", "representation", "exempt"])
  .optional()
  .describe(
    "How VAT on the expense is treated. " +
      "'standard' = ordinary Danish purchase VAT, deducted as input VAT (default). " +
      "'reverse_charge' = EU/foreign purchase where the buyer self-accounts for VAT " +
      "(omvendt betalingspligt). " +
      "'representation' = entertainment/representation costs with the statutory " +
      "limited VAT deduction. " +
      "'exempt' = the expense carries no deductible VAT. " +
      "When omitted, 'standard' is used.",
  );

export function registerExpenseTools(server: McpServer): void {
  server.registerTool(
    "expense_book",
    {
      title: "Book expense from bank + document",
      description:
        "Bogfører leverandørudgift fra bilag + bankpost i ét tag. write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        documentId: z
          .number()
          .int()
          .positive()
          .describe(
            "ID of the ingested document (bilag) the expense is booked from. " +
              "Find it with documents_list.",
          ),
        bankTransactionId: z
          .number()
          .int()
          .positive()
          .describe(
            "ID of the imported bank transaction that paid the expense. " +
              "Find it with bank_list.",
          ),
        expenseAccount: z
          .string()
          .min(1)
          .describe(
            "Account number from the chart of accounts the expense is posted to, " +
              "e.g. '3000' (Software og SaaS). See accounts_list.",
          ),
        vatTreatment: vatTreatmentEnum,
        paymentAccount: z
          .string()
          .optional()
          .describe(
            "Account number the payment is credited to. Defaults to account 2000 " +
              "(Bank); set it only when the payment came from a different account.",
          ),
        date: z
          .string()
          .optional()
          .describe(
            "Posting date in YYYY-MM-DD format. When omitted, the bank " +
              "transaction's own date is used.",
          ),
        text: z
          .string()
          .optional()
          .describe("Optional free-text description of the expense posting."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId: number;
      bankTransactionId: number;
      expenseAccount: string;
      vatTreatment?: "standard" | "reverse_charge" | "representation" | "exempt";
      paymentAccount?: string;
      date?: string;
      text?: string;
      confirm?: boolean;
    }>(server, "expense_book", ({ db, actor, args }) => {
      // Actor-invariant (#63/#76): thread the MCP-client identity into the
      // hash-chained ledger so created_by/created_by_program + audit_log.actor
      // are attributed to the booking agent, not the OS user (resolveActor's
      // process.env.USER fallback). withActor never overwrites explicit values.
      const result = bookExpenseFromBank(
        db,
        withActor(
          {
            documentId: args.documentId,
            bankTransactionId: args.bankTransactionId,
            expenseAccountNo: args.expenseAccount,
            vatTreatment: args.vatTreatment,
            paymentAccountNo: args.paymentAccount,
            transactionDate: args.date,
            text: args.text,
          },
          actor,
        ),
      );
      return wrapCoreResult(result);
    }),
  );
}
