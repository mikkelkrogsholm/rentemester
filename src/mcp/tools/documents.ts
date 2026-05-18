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
import { wrapCoreResult, successEnvelope, errorEnvelope } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed } from "../helpers";

const documentMetadataSchema = z
  .object({})
  .catchall(z.unknown())
  .describe("DocumentMetadata payload — se examples/vendor-invoice.metadata.json");

export function registerDocumentTools(server: McpServer): void {
  server.registerTool(
    "documents_list",
    {
      title: "List documents",
      description: "Lister gemte bilag i virksomhedsmappen. Read-only.",
      inputSchema: { company: z.string().min(1) },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string }>(server, ({ db }) => {
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
      return successEnvelope({
        documents: rows.map((row) => ({
          id: row.id,
          documentNo: row.document_no,
          source: row.source,
          originalFilename: row.original_filename,
          invoiceDate: row.invoice_date,
          amountIncVat: row.amount_inc_vat,
          currency: row.currency,
          status: row.status,
          storedPath: row.stored_path,
        })),
        count: rows.length,
      });
    }),
  );

  server.registerTool(
    "documents_ingest",
    {
      title: "Ingest document",
      description:
        "Indlæser og hash-lagrer et bilag med metadata. write-reversible — kræver confirm:true. " +
        "Skriver en exception hvis ingest blokeres (fx duplicate).",
      inputSchema: {
        company: z.string().min(1),
        filePath: z.string().min(1),
        metadata: documentMetadataSchema,
        vendorId: z.number().int().positive().optional(),
        force: z.boolean().optional(),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      filePath: string;
      metadata: DocumentMetadata;
      vendorId?: number;
      force?: boolean;
      confirm: boolean;
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
