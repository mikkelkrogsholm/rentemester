/**
 * MCP-tools for moms.
 *
 *  - `vat_report` (read)
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
import { wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed } from "../helpers";

const payloadSchema = z.object({}).catchall(z.unknown());

export function registerVatTools(server: McpServer): void {
  server.registerTool(
    "vat_report",
    {
      title: "VAT period report",
      description: "Bygger momsrapport for perioden. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        from: z.string().min(1),
        to: z.string().min(1),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; from: string; to: string }>(server, ({ db, args }) => {
      const result = buildVatReport(db, args.from, args.to);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "vat_post_eu_service_purchase",
    {
      title: "Post EU service reverse-charge purchase",
      description:
        "Bogfører EU-servicekøb med reverse charge. write-irreversible. " +
        "Hvis payload.invoiceNo er sat, slås documentId op automatisk.",
      inputSchema: {
        company: z.string().min(1),
        payload: payloadSchema,
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: ReverseChargePurchaseInput & { invoiceNo?: string };
      confirm: boolean;
    }>(server, "vat_post_eu_service_purchase", ({ db, args }) => {
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
      const result = postEuServiceReverseChargePurchase(db, payload as ReverseChargePurchaseInput);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "vat_post_representation_purchase",
    {
      title: "Post representation purchase (partial VAT deduction)",
      description: "Bogfører repræsentationsudgift med delvis momsfradrag. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        payload: payloadSchema,
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      payload: RepresentationPurchaseInput;
      confirm: boolean;
    }>(server, "vat_post_representation_purchase", ({ db, args }) => {
      const result = postRepresentationPurchase(db, args.payload);
      return wrapCoreResult(result);
    }),
  );
}
