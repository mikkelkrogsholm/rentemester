/**
 * MCP-tools: `audit_verify` (read) + `audit_log_list` (read).
 *
 * Read-side of the append-only audit chain. Both tools are non-mutating.
 *
 * Klassifikation: `read` — ingen state-bivirkninger, må kaldes frit.
 * Kræver derfor ikke `confirm: true`.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { verifyAuditChain } from "../../core/ledger";
import { listAuditLog } from "../../core/audit-log";
import { envelopeShape, successEnvelope, wrapCoreResult } from "../envelope";
import { withCompanyDb } from "../tool-runtime";
import {
  applyPagination,
  paginationFields,
  paginationDescriptionSuffix,
} from "../pagination";

const inputSchema = {
  company: z
    .string()
    .min(1, "company path is required")
    .describe("Absolute path to the company directory, or a workspace slug."),
};

export function registerAuditTools(server: McpServer): void {
  server.registerTool(
    "audit_verify",
    {
      title: "Verify audit chain",
      description:
        "Verificerer hash-chain og bogføringsintegritet for virksomhedsmappen. " +
        "Returnerer { ok, entries, errors[] }. Read-only — ingen state-bivirkninger.",
      inputSchema,
      outputSchema: envelopeShape,
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withCompanyDb<{ company: string }>(server, ({ db }) => {
      // `withCompanyDb` already resolves + existsSync-guards `company` and
      // returns a *path-redacted* error envelope on a bad/missing directory,
      // so the absolute host path is never disclosed to the caller (#228).
      return wrapCoreResult(verifyAuditChain(db));
    }),
  );

  server.registerTool(
    "audit_log_list",
    {
      title: "List audit-log entries (revisionsspor)",
      description:
        "Returnerer rækker fra audit_log-tabellen for virksomheden — det " +
        "menneskelæsbare revisionsspor over hvad agenten/cockpittet/CLI'en " +
        "har gjort. Append-only på server-siden, så svaret er bit-identisk " +
        "på re-kald givet samme filtre. Rækkefølge: created_at DESC, " +
        "id DESC (nyeste først). Read-only.\n\n" +
        "Hver række har { id, eventType, entityType, entityId, message, " +
        "actor, createdAt }. Filtre AND'es. Brug det her tool når en agent " +
        "skal vise sin egen aktivitet tilbage til ejeren, eller når man " +
        "skal forklare hvorfor en konkret postering fyrede." +
        paginationDescriptionSuffix,
      inputSchema: {
        company: z
          .string()
          .min(1)
          .describe("Absolute path to the company directory, or a workspace slug."),
        fromDate: z
          .string()
          .optional()
          .describe(
            "Inclusive lower bound (YYYY-MM-DD or full ISO timestamp). " +
              "Filters rows where created_at >= this value.",
          ),
        toDate: z
          .string()
          .optional()
          .describe(
            "Inclusive upper bound (YYYY-MM-DD or full ISO timestamp). " +
              "A bare date is promoted to end-of-day (23:59:59.999Z) so the " +
              "whole day is included.",
          ),
        eventTypeLike: z
          .string()
          .optional()
          .describe(
            "Case-insensitive substring filter on event_type " +
              "(e.g. 'INVOICE', 'BANK_IMPORTED', 'EXCEPTION_RESOLVED').",
          ),
        actorLike: z
          .string()
          .optional()
          .describe(
            "Case-insensitive substring filter on the actor string " +
              "(matches human e-mail addresses, agent IDs, MCP-client tags).",
          ),
        ...paginationFields,
      },
      outputSchema: envelopeShape,
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    withCompanyDb<{
      company: string;
      fromDate?: string;
      toDate?: string;
      eventTypeLike?: string;
      actorLike?: string;
      limit?: number;
      offset?: number;
    }>(server, ({ db, args }) => {
      const result = listAuditLog(db, {
        fromDate: args.fromDate,
        toDate: args.toDate,
        eventTypeLike: args.eventTypeLike,
        actorLike: args.actorLike,
        // Pull ALL matching rows from the core so we can slice via the shared
        // paginationFields contract (consistent with bank_list / journal_list).
      });
      const { pageRows, meta } = applyPagination(result.rows, {
        limit: args.limit,
        offset: args.offset,
      });
      return successEnvelope({
        rows: pageRows,
        total: result.total,
        ...meta,
      });
    }),
  );
}
