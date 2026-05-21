/**
 * MCP-tool for direkte PEPPOL-submission (#128).
 *
 * Write-irreversible:
 *   - peppol_submit_public_invoice
 *
 * Bygger en deterministisk, idempotent PEPPOL-submission-envelope oven på
 * det allerede shippede OIOUBL-handoff-artifact. Trust-boundary: access-point
 * CREDENTIALS indgår aldrig i kerne-bogføringstilstanden — kun den ikke-
 * hemmelige access-point-konfiguration (id, endpoint-URL, sender-endpoint-id)
 * sendes ind for at udlede envelope'en. Der foretages intet rigtigt netkald;
 * tool'et producerer submission-request-artifaktet og registrerer forsøget.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  submitPublicEInvoicePeppol,
  type PeppolAccessPointConfig,
  type PeppolTransportAcknowledgement,
} from "../../core/public-einvoice";
import { wrapCoreResult, errorEnvelope } from "../envelope";
import { withCompanyDbConfirmed, resolveIssuedInvoiceDocumentId } from "../tool-runtime";

const accessPointSchema = z
  .object({
    accessPointId: z.string().min(1),
    endpointUrl: z.string().min(1),
    senderEndpointId: z.string().min(1),
  })
  .describe("Ikke-hemmelig PEPPOL access-point-konfiguration — credentials hører ikke til her");

const acknowledgementSchema = z
  .object({
    transmissionId: z.string().min(1),
    acknowledgedAt: z.string().min(1),
  })
  .optional()
  .describe("Valgfri transport-kvittering fra access-point'et");

export function registerPeppolTools(server: McpServer): void {
  server.registerTool(
    "peppol_submit_public_invoice",
    {
      title: "Submit public invoice via PEPPOL",
      description:
        "Bygger en deterministisk, idempotent PEPPOL-submission-envelope oven på " +
        "OIOUBL-handoff-artifaktet og registrerer submission-forsøget. " +
        "Access-point-credentials gemmes aldrig i bogføringstilstanden. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        documentId: z.number().int().positive().optional(),
        invoiceNumber: z.string().optional(),
        accessPoint: accessPointSchema,
        acknowledgement: acknowledgementSchema,
        confirm: z.boolean().optional(),
      },
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      accessPoint: PeppolAccessPointConfig;
      acknowledgement?: PeppolTransportAcknowledgement;
      confirm: boolean;
    }>(server, "peppol_submit_public_invoice", ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) {
        return errorEnvelope(
          `Could not resolve invoice: provide documentId or invoiceNumber (got documentId=${args.documentId ?? "-"}, invoiceNumber='${args.invoiceNumber ?? ""}')`,
        );
      }
      const result = submitPublicEInvoicePeppol(db, {
        invoiceDocumentId: id,
        accessPoint: args.accessPoint,
        acknowledgement: args.acknowledgement,
      });
      return wrapCoreResult(result);
    }),
  );
}
