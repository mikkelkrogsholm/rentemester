/**
 * MCP-tool for bilagsmail IMAP intake (#181).
 *
 *  - `imap_intake_poll` (write-reversible — poller en hosted IMAP-postkasse
 *    og videresender nye beskeder til den eksisterende #122-pipeline:
 *    attachment-extraction, message-id + attachment-hash dedup og
 *    exception-routing).
 *
 * IMAP-credentials kommer fra værktøjets argumenter (host/port/username/
 * mailbox) plus RENTEMESTER_IMAP_PASSWORD i miljøet — aldrig fra ledger'en.
 * Provider-OAuth og SMTP er uden for scope.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  createImapClient,
  pollImapMailbox,
  resolveImapConfig,
  type ImapConfig,
  type PollImapOptions,
} from "../../core/imap-intake";
import type { MailIntakeMetadataInput } from "../../core/mail-intake";
import { documentMetadataFields } from "./documents";
import { envelopeShape, errorEnvelope, successEnvelope } from "../envelope";
import { withCompanyDbConfirmed, confirmField } from "../tool-runtime";

/**
 * The bilagsmail intake metadata schema (#274).
 *
 * It is the SAME named `DocumentMetadata` fields as `documents_ingest.metadata`
 * but WITHOUT `source` — the intake pipeline sets `source` itself. Built from
 * the shared `documentMetadataFields` so the two schemas cannot drift apart.
 * `.passthrough()` keeps any forward-compatible extra keys accepted.
 */
const metadataSchema = z
  .object(documentMetadataFields)
  .passthrough()
  .describe("DocumentMetadata-payload (uden 'source') der anvendes på vedhæftningerne — se examples/bilagsmail.metadata.json");

export function registerImapIntakeTools(server: McpServer): void {
  server.registerTool(
    "imap_intake_poll",
    {
      title: "Poll bilagsmail via IMAP",
      description:
        "Poller en hosted IMAP-postkasse, henter nye beskeder og videresender vedhæftninger " +
        "(PDF/JPG/PNG) til den eksisterende bilagsmail-pipeline (#122). Dedup på message-id + " +
        "attachment-hash er rerun-stabil — gentaget poll skaber ingen dubletter. Beskeder uden " +
        "brugbar vedhæftning routes til exception-køen. IMAP-credentials kommer fra argumenter " +
        "og RENTEMESTER_IMAP_PASSWORD i miljøet, aldrig fra ledger'en. Kræver confirm:true. " +
        "write-reversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        imapHost: z.string().min(1).optional().describe("IMAP-host; standard RENTEMESTER_IMAP_HOST"),
        imapPort: z.number().int().positive().optional().describe("IMAP-port; standard 993 (TLS)"),
        imapUsername: z.string().min(1).optional().describe("IMAP-brugernavn; standard RENTEMESTER_IMAP_USERNAME"),
        imapMailbox: z.string().min(1).optional().describe("Mailbox; standard INBOX"),
        sinceUid: z.number().int().nonnegative().optional().describe("Valgfri UID-nedre grænse"),
        metadata: metadataSchema.optional(),
        metadataPerMessage: z
          .record(z.string(), metadataSchema)
          .optional()
          .describe("Metadata pr. message-id; overstyrer 'metadata' for den pågældende besked"),
        force: z.boolean().optional(),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: true },
    },
    withCompanyDbConfirmed<{
      company: string;
      imapHost?: string;
      imapPort?: number;
      imapUsername?: string;
      imapMailbox?: string;
      sinceUid?: number;
      metadata?: MailIntakeMetadataInput;
      metadataPerMessage?: Record<string, MailIntakeMetadataInput>;
      force?: boolean;
      confirm?: boolean;
    }>(server, "imap_intake_poll", async ({ db, args }) => {
      const partial: Partial<ImapConfig> = {};
      if (args.imapHost) partial.host = args.imapHost;
      if (args.imapPort) partial.port = args.imapPort;
      if (args.imapUsername) partial.username = args.imapUsername;
      if (args.imapMailbox) partial.mailbox = args.imapMailbox;

      const config = resolveImapConfig(partial);
      if (!config.ok) return errorEnvelope(config.errors);

      const options: PollImapOptions = {
        metadata: args.metadata,
        metadataPerMessage: args.metadataPerMessage,
        ingestOptions: { forceDuplicateLogicalIdentity: args.force === true },
      };
      if (args.sinceUid !== undefined) options.sinceUid = args.sinceUid;

      const client = createImapClient(config.config);
      const result = await pollImapMailbox(db, args.company, client, options);
      if (!result.ok) return errorEnvelope(result.errors);
      return successEnvelope({
        messagesFetched: result.messagesFetched,
        messagesProcessed: result.messagesProcessed,
        attachmentsIngested: result.attachmentsIngested,
        attachmentsSkipped: result.attachmentsSkipped,
        exceptionsCreated: result.exceptionsCreated,
        documents: result.documents,
      });
    }),
  );
}
