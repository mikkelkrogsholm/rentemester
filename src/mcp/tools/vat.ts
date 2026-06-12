/**
 * MCP-tools for moms.
 *
 *  - `vat_report` (read)
 *  - `vat_eu_sales_list` (read)
 *  - `vat_oss_report` (read)
 *  - `vat_post_eu_service_purchase` (write-irreversible)
 *  - `vat_post_representation_purchase` (write-irreversible)
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  buildVatReport,
  postEuServiceReverseChargePurchase,
  postRepresentationPurchase,
  type ReverseChargePurchaseInput,
  type RepresentationPurchaseInput,
} from "../../core/vat";
import { buildViesRecapitulativeStatement } from "../../core/vat-vies-list";
import { buildOssReport } from "../../core/vat-oss";
import { withActor } from "../actor";
import { envelopeShape, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, confirmField } from "../tool-runtime";

// All monetary fields below are in kroner — decimal DKK with 2 decimals (NOT øre).

const euServicePurchasePayloadSchema = z
  .object({
    transactionDate: z.string().describe("Posting date in YYYY-MM-DD format."),
    text: z.string().describe("Human-readable description of the purchase."),
    documentId: z
      .number()
      .int()
      .positive()
      .optional()
      .describe(
        "Document ID (bilag) of the underlying EU-service invoice. Required, but may be " +
          "supplied indirectly via invoiceNo, which is resolved to a documentId automatically.",
      ),
    invoiceNo: z
      .string()
      .optional()
      .describe("Invoice number of the underlying document. If set, documentId is looked up automatically."),
    netAmount: z
      .number()
      .describe("Purchase amount excluding VAT, in kroner (decimal DKK, 2 decimals — NOT øre)."),
    expenseAccountNo: z
      .string()
      .describe("Expense account number from the chart of accounts to debit, e.g. '3010'."),
    paymentAccountNo: z
      .string()
      .optional()
      .describe("Optional bank/payment account number from the chart of accounts to credit."),
    sourceBankTransactionId: z
      .number()
      .int()
      .positive()
      .optional()
      .describe("Optional ID of the bank transaction this purchase settles. See bank_list."),
    currency: z
      .string()
      .optional()
      .describe("3-letter ISO currency code (default 'DKK'). Non-DKK purchases also require amountForeign, amountDkk and fxRateToDkk."),
    amountForeign: z
      .number()
      .optional()
      .describe("For non-DKK purchases: net amount in the foreign currency, in major units (e.g. euros — NOT cents)."),
    amountDkk: z
      .number()
      .optional()
      .describe("For non-DKK purchases: net amount converted to kroner (decimal DKK, 2 decimals)."),
    fxRateToDkk: z
      .number()
      .optional()
      .describe("For non-DKK purchases: FX rate from the foreign currency to DKK (e.g. 7.46)."),
  })
  .describe(
    "EU service reverse-charge purchase. netAmount/amountDkk are in kroner " +
      "(decimal DKK, 2 decimals — NOT øre).",
  );

const representationPurchasePayloadSchema = z
  .object({
    transactionDate: z.string().describe("Posting date in YYYY-MM-DD format."),
    text: z.string().describe("Human-readable description of the representation expense."),
    documentId: z
      .number()
      .int()
      .positive()
      .describe("Document ID (bilag) of the underlying representation receipt. Required. See documents_list."),
    netAmount: z
      .number()
      .describe("Expense amount excluding VAT, in kroner (decimal DKK, 2 decimals — NOT øre)."),
    expenseAccountNo: z
      .string()
      .optional()
      .describe("Optional representation expense account number from the chart of accounts to debit."),
    paymentAccountNo: z
      .string()
      .optional()
      .describe("Optional bank/payment account number from the chart of accounts to credit."),
    sourceBankTransactionId: z
      .number()
      .int()
      .positive()
      .optional()
      .describe("Optional ID of the bank transaction this purchase settles. See bank_list."),
    currency: z
      .string()
      .optional()
      .describe("3-letter ISO currency code (default 'DKK'). Non-DKK purchases also require amountForeign, amountDkk and fxRateToDkk."),
    amountForeign: z
      .number()
      .optional()
      .describe("For non-DKK purchases: net amount in the foreign currency, in major units (e.g. euros — NOT cents)."),
    amountDkk: z
      .number()
      .optional()
      .describe("For non-DKK purchases: net amount converted to kroner (decimal DKK, 2 decimals)."),
    fxRateToDkk: z
      .number()
      .optional()
      .describe("For non-DKK purchases: FX rate from the foreign currency to DKK (e.g. 7.46)."),
  })
  .describe(
    "Representation purchase with partial VAT deduction. netAmount/amountDkk are in kroner " +
      "(decimal DKK, 2 decimals — NOT øre).",
  );

export function registerVatTools(server: McpServer): void {
  server.registerTool(
    "vat_report",
    {
      title: "VAT period report",
      description: "Bygger momsrapport for perioden. Read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        from: z
          .string()
          .min(1)
          .describe("Period start, YYYY-MM-DD (inclusive). The day itself is included in the report."),
        to: z
          .string()
          .min(1)
          .describe(
            "Period end, YYYY-MM-DD (inclusive — the last day of the period is included). " +
              "Pass e.g. '2026-03-31' for Q1 2026, not '2026-04-01'.",
          ),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; from: string; to: string }>(server, ({ db, args }) => {
      const result = buildVatReport(db, args.from, args.to);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "vat_eu_sales_list",
    {
      title: "EU sales list (EU-salg uden moms)",
      description:
        "Bygger EU-salg uden moms-listen (VIES recapitulative statement) for perioden: " +
        "en liste pr. kunde med EU-momsnummer og samlet værdi af grænseoverskridende " +
        "B2B-salg uden dansk moms. En selvstændig indberetning ved siden af momsangivelsen. " +
        "Read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        from: z.string().min(1).describe("Period start, YYYY-MM-DD (inclusive)."),
        to: z
          .string()
          .min(1)
          .describe("Period end, YYYY-MM-DD (inclusive — the last day of the period is included)."),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; from: string; to: string }>(server, ({ db, args }) => {
      const result = buildViesRecapitulativeStatement(db, args.from, args.to);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "vat_oss_report",
    {
      title: "OSS report (One Stop Shop, first slice)",
      description:
        "Bygger en deterministisk OSS-rapport (One Stop Shop, første slice): det samlede " +
        "grundlag for digitale ydelser solgt til EU-forbrugere bogført med momskoden " +
        "OSS_EU_CONSUMER. Bevidst smal — ikke en OSS-indberetning til SKAT. Read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        from: z.string().min(1).describe("Period start, YYYY-MM-DD (inclusive)."),
        to: z
          .string()
          .min(1)
          .describe("Period end, YYYY-MM-DD (inclusive — the last day of the period is included)."),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; from: string; to: string }>(server, ({ db, args }) => {
      const result = buildOssReport(db, args.from, args.to);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "vat_post_eu_service_purchase",
    {
      title: "Post EU service reverse-charge purchase",
      description:
        "Bogfører EU-servicekøb med reverse charge. " +
        "Hvis payload.invoiceNo er sat, slås documentId op automatisk. " +
        "payload.netAmount er i kroner (decimal DKK, ikke øre). write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        payload: euServicePurchasePayloadSchema,
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: ReverseChargePurchaseInput & { invoiceNo?: string };
      confirm?: boolean;
    }>(server, "vat_post_eu_service_purchase", ({ db, actor, args }) => {
      const payload = { ...(args.payload as Record<string, unknown>) };
      const invoiceNo =
        typeof payload.invoiceNo === "string" ? (payload.invoiceNo as string).trim() : "";
      if (invoiceNo) {
        const row = db
          .query(`SELECT id FROM documents WHERE invoice_no = ? ORDER BY id DESC LIMIT 1`)
          .get(invoiceNo) as { id: number } | null;
        if (!row) {
          return wrapCoreResult({
            ok: false,
            errors: [`Could not resolve document for invoiceNo ${invoiceNo}`],
            appliedRules: [],
          });
        }
        payload.documentId = row.id;
      }
      // Actor-invariant (#63/#76): attribute the reverse-charge posting to the
      // booking agent in the hash chain + audit_log, not the OS user.
      const result = postEuServiceReverseChargePurchase(
        db,
        withActor(payload as ReverseChargePurchaseInput, actor),
      );
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "vat_post_representation_purchase",
    {
      title: "Post representation purchase (partial VAT deduction)",
      description:
        "Bogfører repræsentationsudgift med delvis momsfradrag. " +
        "payload.netAmount er i kroner (decimal DKK, ikke øre). write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        payload: representationPurchasePayloadSchema,
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: RepresentationPurchaseInput;
      confirm?: boolean;
    }>(server, "vat_post_representation_purchase", ({ db, actor, args }) => {
      // Actor-invariant (#63/#76): attribute the representation posting to the
      // booking agent in the hash chain + audit_log, not the OS user.
      const result = postRepresentationPurchase(db, withActor(args.payload, actor));
      return wrapCoreResult(result);
    }),
  );
}
