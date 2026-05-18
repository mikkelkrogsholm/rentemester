/**
 * MCP-tool: `expense_book` (write-irreversible).
 *
 * 1:1-mapping af CLI-kommandoen `expense book`. Bogfører en leverandørudgift
 * direkte fra et bilag + en bankpost.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { bookExpenseFromBank } from "../../core/expense-booking";
import { wrapCoreResult } from "../envelope";
import { withCompanyDbConfirmed } from "../helpers";

const vatTreatmentEnum = z
  .enum(["standard", "reverse_charge", "representation", "exempt"])
  .optional();

export function registerExpenseTools(server: McpServer): void {
  server.registerTool(
    "expense_book",
    {
      title: "Book expense from bank + document",
      description:
        "Bogfører leverandørudgift fra bilag + bankpost i ét tag. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        documentId: z.number().int().positive(),
        bankTransactionId: z.number().int().positive(),
        expenseAccount: z.string().min(1),
        vatTreatment: vatTreatmentEnum,
        paymentAccount: z.string().optional(),
        date: z.string().optional(),
        text: z.string().optional(),
        confirm: z.boolean(),
      },
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
      confirm: boolean;
    }>(server, "expense_book", ({ db, args }) => {
      const result = bookExpenseFromBank(db, {
        documentId: args.documentId,
        bankTransactionId: args.bankTransactionId,
        expenseAccountNo: args.expenseAccount,
        vatTreatment: args.vatTreatment,
        paymentAccountNo: args.paymentAccount,
        transactionDate: args.date,
        text: args.text,
      });
      return wrapCoreResult(result);
    }),
  );
}
