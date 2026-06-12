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
import { withActor } from "../actor";
import { envelopeShape, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, confirmField } from "../tool-runtime";

// --------------------------------------------------------------- invoice template schema
// The `invoice` field of a recurring template is a reusable InvoicePayload
// (src/core/invoice.ts) — the *same* shape `invoice_issue` takes — EXCEPT the
// date/number fields are supplied per generation: `recurring_invoice_generate`
// derives invoiceNumber, issueDate, dueDate and the delivery dates, so they
// must NOT be set on the template. All monetary amounts are in kroner (decimal
// DKK, 2 decimals — NOT øre); vatRate is a fraction (0.25 = 25%).

const templateLineSchema = z.object({
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
    .describe("Line total excluding VAT, in kroner (decimal DKK). Must equal quantity * unitPriceExVat."),
});

const templateTotalsSchema = z.object({
  netAmount: z
    .number()
    .optional()
    .describe("Total excluding VAT, in kroner (decimal DKK). Must equal the sum of all lines' lineTotalExVat."),
  vatRate: z
    .number()
    .optional()
    .describe("VAT rate as a fraction (0.25 = 25%). Required for standard-VAT invoices; omit for reverse-charge."),
  vatAmount: z
    .number()
    .optional()
    .describe("Total VAT, in kroner (decimal DKK). Required for standard-VAT invoices; omit for reverse-charge."),
  grossAmount: z
    .number()
    .optional()
    .describe(
      "Total including VAT, in kroner (decimal DKK). Required. For standard VAT it must equal " +
        "netAmount + vatAmount; for reverse charge it must equal netAmount.",
    ),
});

const templatePartySchema = z.object({
  name: z.string().optional().describe("Party name."),
  address: z.string().optional().describe("Party postal address."),
  vatOrCvr: z.string().optional().describe("Party VAT or CVR number, e.g. 'DK12345678'."),
});

const recurringInvoicePayloadSchema = z
  .object({
    invoiceType: z
      .enum(["full", "simplified"])
      .describe("'full' for a full invoice; 'simplified' only for gross totals up to DKK 3,000."),
    vatTreatment: z
      .enum(["standard", "domestic_reverse_charge", "foreign_reverse_charge"])
      .optional()
      .describe("VAT treatment (default 'standard'). Reverse-charge variants also require reverseChargeBasis."),
    seller: templatePartySchema
      .optional()
      .describe("Seller details. seller.name, seller.address and seller.vatOrCvr are all required for full invoices."),
    buyer: templatePartySchema
      .optional()
      .describe("Buyer (customer) details. Required for full invoices."),
    lines: z
      .array(templateLineSchema)
      .optional()
      .describe("Invoice lines reused every generation — at least one line with a description is required."),
    totals: templateTotalsSchema
      .optional()
      .describe("Invoice totals. All amounts are in kroner (decimal DKK)."),
    reverseChargeBasis: z
      .string()
      .optional()
      .describe("Legal basis for reverse charge — required for reverse-charge invoices (e.g. 'EU_MOMSDIREKTIV_ART_196')."),
    reverseChargeNote: z
      .string()
      .optional()
      .describe("Optional free-text note explaining the reverse-charge treatment."),
    currency: z
      .string()
      .optional()
      .describe("3-letter ISO currency code (default 'DKK')."),
  })
  .describe(
    "The reusable invoice payload (an InvoicePayload) materialised for every generation. " +
      "Do NOT set invoiceNumber, issueDate, dueDate or any delivery date here — " +
      "recurring_invoice_generate derives those per period. All monetary amounts are in kroner " +
      "(decimal DKK, 2 decimals — NOT øre); vatRate is a fraction (0.25 = 25%).",
  );

export function registerRecurringInvoiceTools(server: McpServer): void {
  server.registerTool(
    "recurring_invoice_create",
    {
      title: "Create recurring invoice template",
      description:
        "Opretter en gentagende fakturaskabelon (interval, kunde, linjer, moms, leveringsperiode). write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        name: z
          .string()
          .min(1)
          .describe("Human-readable name of the template, e.g. 'Monthly retainer — Acme ApS'."),
        interval: z
          .enum(["monthly", "quarterly", "yearly"])
          .describe("How often the template generates an invoice."),
        firstIssueDate: z
          .string()
          .min(1)
          .describe("Issue date of the first invoice this template should produce, in YYYY-MM-DD format."),
        invoice: recurringInvoicePayloadSchema,
        paymentTermsDays: z
          .number()
          .int()
          .min(0)
          .max(365)
          .optional()
          .describe(
            "Payment terms in days (default 30). The generated invoice's dueDate = its issueDate + this many days. " +
              "Must be an integer between 0 and 365.",
          ),
        deliveryPeriodMode: z
          .enum(["issue_month", "interval_window", "none"])
          .optional()
          .describe(
            "How the generated invoice's delivery period is derived (default 'issue_month'): " +
              "'issue_month' = the calendar month of the issue date; " +
              "'interval_window' = the full interval window leading up to the issue date; " +
              "'none' = no delivery period is set.",
          ),
        notes: z.string().optional().describe("Optional free-text notes stored on the template."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      name: string;
      interval: "monthly" | "quarterly" | "yearly";
      firstIssueDate: string;
      invoice: z.infer<typeof recurringInvoicePayloadSchema>;
      paymentTermsDays?: number;
      deliveryPeriodMode?: "issue_month" | "interval_window" | "none";
      notes?: string;
      confirm?: boolean;
    }>(server, "recurring_invoice_create", ({ db, actor, args }) => {
      // Actor-invariant (#63/#76): attribute the template-creation audit entry
      // to the booking agent in the append-only audit_log, not the OS user.
      const result = createRecurringInvoiceTemplate(
        db,
        withActor(
          {
            name: args.name,
            interval: args.interval,
            firstIssueDate: args.firstIssueDate,
            invoice: args.invoice as RecurringInvoiceTemplateInput["invoice"],
            paymentTermsDays: args.paymentTermsDays,
            deliveryPeriodMode: args.deliveryPeriodMode,
            notes: args.notes,
          },
          actor,
        ),
      );
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
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        templateId: z
          .number()
          .int()
          .positive()
          .describe("ID of the recurring-invoice template to generate from. See recurring_invoice_list."),
        asOfDate: z
          .string()
          .min(1)
          .describe(
            "The reference date in YYYY-MM-DD format. The invoice due on or before this date for the " +
              "template's schedule is materialised. Idempotent: re-running for the same template/period " +
              "produces no duplicate.",
          ),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      templateId: number;
      asOfDate: string;
      confirm?: boolean;
    }>(server, "recurring_invoice_generate", ({ db, actor, args }) => {
      // Actor-invariant (#63/#76): attribute the materialised invoice's ledger
      // posting to the booking agent in the hash chain + audit_log, not the OS
      // user. The input payload is the 3rd arg (companyRoot is the 2nd).
      const result = generateRecurringInvoice(
        db,
        args.company,
        withActor(
          {
            templateId: args.templateId,
            asOfDate: args.asOfDate,
          },
          actor,
        ),
      );
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "recurring_invoice_list",
    {
      title: "List recurring invoice templates",
      description: "Lister gentagende fakturaskabeloner. read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        includeInactive: z
          .boolean()
          .optional()
          .describe("When true, also list deactivated templates (default false — active templates only)."),
      },
      outputSchema: envelopeShape,
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
