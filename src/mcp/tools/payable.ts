/**
 * MCP-tools for kreditorstyring (accounts payable).
 *
 *  - `payable_register` (write-irreversible) — registers a booked supplier
 *     bill as an open creditor item and posts it to 7000 Leverandørgæld.
 *  - `payable_pay` (write-irreversible) — matches an outgoing bank payment
 *     against an open payable and posts the settlement entry.
 *  - `payable_list` (read) — builds the kreditorliste with aging buckets.
 *
 * 1:1-mapping af CLI-kommandoerne `payable register`, `payable pay` og
 * `payable list`.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  registerPayable,
  payPayableFromBank,
  buildPayablesList,
} from "../../core/payables";
import { withActor } from "../actor";
import { envelopeShape, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, confirmField } from "../tool-runtime";

export function registerPayableTools(server: McpServer): void {
  server.registerTool(
    "payable_register",
    {
      title: "Register a supplier bill as an open payable",
      description:
        "Registrerer et bogført leverandørbilag som en åben kreditorpost. " +
        "Posteringen debiterer udgift + købsmoms og krediterer 7000 Leverandørgæld. " +
        "write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        documentId: z
          .number()
          .int()
          .positive()
          .describe(
            "ID of the ingested purchase document (bilag) the payable is registered from. " +
              "Find it with documents_list. A document can only be registered once.",
          ),
        billDate: z
          .string()
          .describe("Bill date in YYYY-MM-DD format — the posting date of the payable recognition entry."),
        dueDate: z
          .string()
          .describe("Due date in YYYY-MM-DD format — the date the creditor item is aged against."),
        expenseAccount: z
          .string()
          .min(1)
          .describe(
            "Account number from the chart of accounts the expense is posted to, e.g. '3000'. See accounts_list.",
          ),
        vatTreatment: z
          .enum(["standard", "exempt"])
          .optional()
          .describe(
            "'standard' books 25% Danish input VAT off the bill; 'exempt' books no VAT. " +
              "When omitted it is inferred from the document's vat_amount (>0 => standard, =0 => exempt).",
          ),
        vendorId: z
          .number()
          .int()
          .positive()
          .optional()
          .describe("Optional vendor id from the vendor register to associate with the payable."),
        note: z.string().optional().describe("Optional free-text note stored on the payable."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId: number;
      billDate: string;
      dueDate: string;
      expenseAccount: string;
      vatTreatment?: "standard" | "exempt";
      vendorId?: number;
      note?: string;
      confirm?: boolean;
    }>(server, "payable_register", ({ db, actor, args }) => {
      // Actor-invariant (#63/#76): thread the MCP-client identity into the
      // hash-chained ledger so the creditor recognition entry is attributed to
      // the booking agent, not the OS user. withActor keeps explicit values.
      const result = registerPayable(
        db,
        withActor(
          {
            documentId: args.documentId,
            billDate: args.billDate,
            dueDate: args.dueDate,
            expenseAccountNo: args.expenseAccount,
            vatTreatment: args.vatTreatment,
            vendorId: args.vendorId,
            note: args.note,
          },
          actor,
        ),
      );
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "payable_pay",
    {
      title: "Pay a payable from a bank transaction",
      description:
        "Matcher en udgående bankbetaling mod en åben kreditorpost. " +
        "Posteringen debiterer 7000 Leverandørgæld og krediterer bankkontoen. " +
        "write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        payableId: z
          .number()
          .int()
          .positive()
          .describe("ID of the open payable. Find it with payable_list."),
        bankTransactionId: z
          .number()
          .int()
          .positive()
          .describe(
            "ID of the outgoing (negative) DKK bank transaction that paid the bill. Find it with bank_list.",
          ),
        amount: z
          .number()
          .positive()
          .optional()
          .describe(
            "Optional payment amount in kroner. When omitted the bank transaction's amount is used. " +
              "When given it must match the bank amount and not exceed the open balance.",
          ),
        date: z
          .string()
          .optional()
          .describe("Posting date in YYYY-MM-DD format. When omitted the bank transaction's date is used."),
        paymentAccount: z
          .string()
          .optional()
          .describe("Account number the payment is credited to. Defaults to account 2000 (Bank)."),
        note: z.string().optional().describe("Optional free-text note stored on the payment."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payableId: number;
      bankTransactionId: number;
      amount?: number;
      date?: string;
      paymentAccount?: string;
      note?: string;
      confirm?: boolean;
    }>(server, "payable_pay", ({ db, actor, args }) => {
      // Actor-invariant (#63/#76): attribute the settlement entry to the
      // booking agent in the hash-chained ledger + audit_log, not the OS user.
      const result = payPayableFromBank(
        db,
        withActor(
          {
            payableId: args.payableId,
            bankTransactionId: args.bankTransactionId,
            amount: args.amount,
            paymentDate: args.date,
            paymentAccountNo: args.paymentAccount,
            note: args.note,
          },
          actor,
        ),
      );
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "payable_list",
    {
      title: "List payables (kreditorliste) with aging",
      description:
        "Bygger kreditorlisten: åbne leverandørposter med åben saldo og " +
        "forfaldsintervaller (forfaldne / ikke-forfaldne). Read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        status: z
          .enum(["open", "paid", "overdue", "all"])
          .optional()
          .describe("Filter by payable status. Default 'all'."),
        asOf: z
          .string()
          .optional()
          .describe(
            "Date in YYYY-MM-DD format the due dates are aged against. " +
              "Defaults to today (UTC) when omitted, so status='overdue' returns " +
              "currently overdue items. This is the canonical name used by every " +
              "other MCP tool; the legacy `asOfDate` alias is still accepted.",
          ),
        asOfDate: z
          .string()
          .optional()
          .describe(
            "DEPRECATED — use `asOf`. Still accepted for back-compat. " +
              "If both are present, `asOf` wins.",
          ),
        supplier: z.string().optional().describe("Case-insensitive substring filter on the supplier name."),
        vendorId: z.number().int().positive().optional().describe("Filter to payables linked to this vendor id."),
        from: z.string().optional().describe("Earliest bill date (YYYY-MM-DD) to include."),
        to: z.string().optional().describe("Latest bill date (YYYY-MM-DD) to include."),
        minDays: z
          .number()
          .int()
          .nonnegative()
          .optional()
          .describe("With status 'overdue', only include payables overdue by at least this many days."),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{
      company: string;
      status?: "open" | "paid" | "overdue" | "all";
      asOf?: string;
      asOfDate?: string;
      supplier?: string;
      vendorId?: number;
      from?: string;
      to?: string;
      minDays?: number;
    }>(server, ({ db, args }) => {
      // #(integration-review) — accept both `asOf` (canonical, used by every
      // other tool) and `asOfDate` (legacy alias). The new field wins when
      // both are supplied.
      const asOfDate = args.asOf ?? args.asOfDate;
      const result = buildPayablesList(db, {
        status: args.status,
        asOfDate,
        supplier: args.supplier,
        vendorId: args.vendorId,
        from: args.from,
        to: args.to,
        minDays: args.minDays,
      });
      return wrapCoreResult(result);
    }),
  );
}
