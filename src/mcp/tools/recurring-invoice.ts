/**
 * MCP-tools for recurring invoice templates (#118).
 *
 * 1:1-mapping af CLI-kommandoerne `recurring-invoice create|generate|list`.
 * Skabelonen fanger den gentagende faktura; `generate` materialiserer
 * deterministisk den faktura der er forfalden for en given `asOfDate` og er
 * idempotent pr. template/periode.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  createRecurringInvoiceTemplate,
  generateRecurringInvoice,
  listRecurringInvoiceTemplates,
  type RecurringInvoiceTemplateInput,
} from "../../core/recurring-invoices";
import { wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed } from "../tool-runtime";

export function registerRecurringInvoiceTools(server: McpServer): void {
  server.registerTool(
    "recurring_invoice_create",
    {
      title: "Create recurring invoice template",
      description:
        "Opretter en gentagende fakturaskabelon (interval, kunde, linjer, moms, leveringsperiode). write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        name: z.string().min(1),
        interval: z.enum(["monthly", "quarterly", "yearly"]),
        firstIssueDate: z.string().min(1),
        invoice: z.record(z.string(), z.unknown()),
        paymentTermsDays: z.number().int().min(0).max(365).optional(),
        deliveryPeriodMode: z.enum(["issue_month", "interval_window", "none"]).optional(),
        notes: z.string().optional(),
        confirm: z.boolean().optional(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      name: string;
      interval: "monthly" | "quarterly" | "yearly";
      firstIssueDate: string;
      invoice: Record<string, unknown>;
      paymentTermsDays?: number;
      deliveryPeriodMode?: "issue_month" | "interval_window" | "none";
      notes?: string;
      confirm: boolean;
    }>(server, "recurring_invoice_create", ({ db, args }) => {
      const result = createRecurringInvoiceTemplate(db, {
        name: args.name,
        interval: args.interval,
        firstIssueDate: args.firstIssueDate,
        invoice: args.invoice as RecurringInvoiceTemplateInput["invoice"],
        paymentTermsDays: args.paymentTermsDays,
        deliveryPeriodMode: args.deliveryPeriodMode,
        notes: args.notes,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "recurring_invoice_generate",
    {
      title: "Generate invoice from recurring template",
      description:
        "Materialiserer deterministisk den faktura der er forfalden for skabelonen pr. asOfDate. Idempotent pr. template/periode. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        templateId: z.number().int().positive(),
        asOfDate: z.string().min(1),
        confirm: z.boolean().optional(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      templateId: number;
      asOfDate: string;
      confirm: boolean;
    }>(server, "recurring_invoice_generate", ({ db, args }) => {
      const result = generateRecurringInvoice(db, args.company, {
        templateId: args.templateId,
        asOfDate: args.asOfDate,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "recurring_invoice_list",
    {
      title: "List recurring invoice templates",
      description: "Lister gentagende fakturaskabeloner. read-only.",
      inputSchema: {
        company: z.string().min(1),
        includeInactive: z.boolean().optional(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; includeInactive?: boolean }>(server, ({ db, args }) => {
      const result = listRecurringInvoiceTemplates(db, {
        includeInactive: args.includeInactive === true,
      });
      return wrapCoreResult(result);
    }),
  );
}
