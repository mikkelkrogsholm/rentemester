/**
 * MCP-tools for bilag.
 *
 *  - `documents_list` (read)
 *  - `documents_ingest` (write-reversible — indlæser et bilag fra disk)
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { ingestDocument, type DocumentMetadata } from "../../core/documents";
import { resolveDocumentMasterData } from "../../core/master-data";
import { recordException } from "../../core/exceptions";
import { envelopeShape, errorEnvelope, successEnvelope, wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed, confirmField } from "../tool-runtime";
import { applyPagination, paginationFields, paginationDescriptionSuffix } from "../pagination";

const documentPartySchema = z.object({
  name: z.string().optional().describe("Party name."),
  address: z.string().optional().describe("Party postal address."),
  vatOrCvr: z.string().optional().describe("Party VAT or CVR number, e.g. 'DK12345678'."),
});

/**
 * The named `DocumentMetadata` fields shared by `documents_ingest` and the
 * bilagsmail intake tools (`imap_intake_poll`, `mail_intake_ingest`).
 *
 * Exported as a bare shape (not a `z.object`) so the intake tools — which do
 * NOT take `source` (the pipeline sets it) — can build their own object from
 * the SAME field definitions, guaranteeing the two schemas cannot drift
 * apart (#274).
 */
export const documentMetadataFields = {
  documentType: z
      .enum(["purchase_sale", "cash_register_receipt"])
      .optional()
      .describe("Document type (default 'purchase_sale')."),
    issueDate: z.string().optional().describe("Document/invoice date in YYYY-MM-DD format."),
    invoiceNo: z.string().optional().describe("Invoice or receipt number printed on the document."),
    deliveryDescription: z
      .string()
      .optional()
      .describe("Free-text description of the goods or services."),
    amountIncVat: z
      .number()
      .optional()
      .describe("Total amount including VAT, in kroner (decimal DKK, 2 decimals — NOT øre)."),
    currency: z
      .string()
      .optional()
      .describe("3-letter ISO currency code (default 'DKK')."),
    sender: documentPartySchema.optional().describe("Sender/supplier details."),
    recipient: documentPartySchema.optional().describe("Recipient/buyer details."),
    vatAmount: z
      .number()
      .optional()
      .describe("VAT amount, in kroner (decimal DKK, 2 decimals — NOT øre)."),
    paymentDetails: z
      .string()
      .optional()
      .describe("Free-text payment details, e.g. 'Bankoverførsel 2026-05-17'."),
    exemptionCode: z
      .literal("FOREIGN_PHYSICAL_ONLY")
      .nullable()
      .optional()
      .describe("Set to 'FOREIGN_PHYSICAL_ONLY' for a foreign physical-only receipt; otherwise omit."),
} as const;

/**
 * The `documents_ingest` metadata schema: the shared `DocumentMetadata`
 * fields PLUS the required `source` field (how the document arrived).
 */
const documentMetadataSchema = z
  .object({
    source: z
      .string()
      .describe("How the document arrived, e.g. 'email', 'photo-upload', 'mobile-scan'. Required."),
    ...documentMetadataFields,
  })
  .describe(
    "Document (bilag) metadata. amountIncVat and vatAmount are in kroner " +
      "(decimal DKK, 2 decimals — NOT øre).",
  );

export function registerDocumentTools(server: McpServer): void {
  server.registerTool(
    "documents_list",
    {
      title: "List documents",
      description: "Lister gemte bilag i virksomhedsmappen. Read-only." + paginationDescriptionSuffix,
      inputSchema: {
        company: z.string().min(1),
        ...paginationFields,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; limit?: number; offset?: number }>(server, ({ db, args }) => {
      const rows = db
        .query(
          `SELECT id, document_no, source, original_filename, invoice_date, amount_inc_vat,
                  currency, status, stored_path
           FROM documents
           ORDER BY id DESC`,
        )
        .all() as Array<{
          id: number;
          document_no: string | null;
          source: string;
          original_filename: string;
          invoice_date: string | null;
          amount_inc_vat: number | null;
          currency: string | null;
          status: string;
          stored_path: string | null;
        }>;
      const mapped = rows.map((row) => ({
        id: row.id,
        documentNo: row.document_no,
        source: row.source,
        originalFilename: row.original_filename,
        invoiceDate: row.invoice_date,
        amountIncVat: row.amount_inc_vat,
        currency: row.currency,
        status: row.status,
        storedPath: row.stored_path,
      }));
      const { pageRows, meta } = applyPagination(mapped, { limit: args.limit, offset: args.offset });
      return successEnvelope({ documents: pageRows, ...meta });
    }),
  );

  server.registerTool(
    "documents_ingest",
    {
      title: "Ingest document",
      description:
        "Indlæser og hash-lagrer et bilag med metadata. Kræver confirm:true. " +
        "Skriver en exception hvis ingest blokeres (fx duplicate). " +
        "VIGTIGT: filePath er en sti på MCP-serverens eget filsystem — bilaget skal allerede " +
        "ligge på serveren. Klienten/agenten kan IKKE uploade en fil her, og der findes (i " +
        "modsætning til bank_import's csvContent) ingen inline-content-variant: filen kan kun " +
        "angives via sti. Alle beløb i metadata er i kroner (decimal DKK, ikke øre). " +
        "write-reversible.",
      inputSchema: {
        company: z.string().min(1),
        filePath: z
          .string()
          .min(1)
          .describe(
            "Absolute path to the document file ON THE MCP SERVER'S FILESYSTEM. The file " +
              "must already exist on the server — this tool does not accept uploaded or " +
              "inline file content (no csvContent-style alternative exists, unlike bank_import).",
          ),
        metadata: documentMetadataSchema,
        vendorId: z
          .number()
          .int()
          .positive()
          .optional()
          .describe("Optional ID of an existing vendor to associate with the document. See vendor_list."),
        force: z
          .boolean()
          .optional()
          .describe(
            "Set true to bypass duplicate detection and force ingest even when a " +
              "document with the same logical identity already exists. When omitted " +
              "(or false), a duplicate is blocked and an exception is recorded.",
          ),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      filePath: string;
      metadata: DocumentMetadata;
      vendorId?: number;
      force?: boolean;
      confirm?: boolean;
    }>(server, "documents_ingest", ({ db, args }) => {
      const resolved = resolveDocumentMasterData(db, args.metadata, { vendorId: args.vendorId });
      if (!resolved.ok) return errorEnvelope(resolved.errors ?? ["resolveDocumentMasterData failed"]);
      const result = ingestDocument(db, args.company, args.filePath, resolved.metadata, {
        forceDuplicateLogicalIdentity: args.force === true,
      });
      if (!result.ok) {
        recordException(db, {
          type: "DOCUMENT_INGEST_BLOCKED",
          severity: "medium",
          message: `Document ingest blocked for ${args.filePath}`,
          requiredAction: "Fix document metadata or duplicate handling, then retry ingest.",
          sourceEvidence: { file: args.filePath, errors: result.errors ?? [] },
          postingPreview: { retryCommand: "documents_ingest" },
        });
      }
      return wrapCoreResult(result);
    }),
  );
}
