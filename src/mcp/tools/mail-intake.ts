/**
 * MCP-tools for bilagsmail intake (#122).
 *
 *  - `mail_intake_ingest` (write-reversible — indlæser bilag fra en lokal
 *    `.eml`-fil eller maildrop-mappe og videresender vedhæftninger til
 *    document-ingest-pipelinen).
 *
 * IMAP / hosted-mailbox sync er bevidst ikke implementeret — dette er kun
 * den første deterministiske lokale transport-slice.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  ingestMailDrop,
  type IngestMailDropOptions,
  type MailIntakeMetadataInput,
} from "../../core/mail-intake";
import { successEnvelope, errorEnvelope } from "../envelope";
import { withCompanyDbConfirmed } from "../tool-runtime";

const metadataSchema = z
  .object({})
  .catchall(z.unknown())
  .describe("DocumentMetadata-payload (uden 'source') der anvendes på vedhæftningerne — se examples/bilagsmail.metadata.json");

export function registerMailIntakeTools(server: McpServer): void {
  server.registerTool(
    "mail_intake_ingest",
    {
      title: "Ingest bilagsmail",
      description:
        "Indlæser en lokal .eml-fil eller maildrop-mappe, parser vedhæftninger (PDF/JPG/PNG) " +
        "og videresender dem til document-ingest-pipelinen. Deduplikerer på message-id + " +
        "attachment-hash så reruns er idempotente. Beskeder uden brugbar vedhæftning eller " +
        "med tvetydig metadata routes til exception-køen. write-reversible — kræver confirm:true.",
      inputSchema: {
        company: z.string().min(1),
        source: z.string().min(1).describe("Sti til en .eml-fil eller en maildrop-mappe"),
        metadata: metadataSchema.optional(),
        metadataPerMessage: z
          .record(z.string(), metadataSchema)
          .optional()
          .describe("Metadata pr. message-id; overstyrer 'metadata' for den pågældende besked"),
        force: z.boolean().optional(),
        confirm: z.boolean().optional(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      source: string;
      metadata?: MailIntakeMetadataInput;
      metadataPerMessage?: Record<string, MailIntakeMetadataInput>;
      force?: boolean;
      confirm: boolean;
    }>(server, "mail_intake_ingest", ({ db, args }) => {
      const options: IngestMailDropOptions = {
        metadata: args.metadata,
        metadataPerMessage: args.metadataPerMessage,
        ingestOptions: { forceDuplicateLogicalIdentity: args.force === true },
      };
      const result = ingestMailDrop(db, args.company, args.source, options);
      if (!result.ok) return errorEnvelope(result.errors);
      return successEnvelope({
        messagesProcessed: result.messagesProcessed,
        attachmentsIngested: result.attachmentsIngested,
        attachmentsSkipped: result.attachmentsSkipped,
        exceptionsCreated: result.exceptionsCreated,
        documents: result.documents,
      });
    }),
  );
}
