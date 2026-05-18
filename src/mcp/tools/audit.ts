/**
 * MCP-tool: `audit_verify` (read).
 *
 * 1:1-mapping af CLI-kommandoen `audit verify`. Verificerer hash-chain
 * og bogføringsintegritet for en virksomhedsmappe.
 *
 * Klassifikation: `read` — ingen state-bivirkninger, må kaldes frit.
 * Kræver derfor ikke `confirm: true`.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { existsSync } from "node:fs";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import { verifyAuditChain } from "../../core/ledger";
import { envelopeToCallResult, errorEnvelope, wrapCoreResult } from "../envelope";

const inputSchema = {
  company: z.string().min(1, "company path is required"),
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
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: false,
      },
    },
    async ({ company }) => {
      if (!existsSync(company)) {
        return envelopeToCallResult(
          errorEnvelope(`company path does not exist: ${company}`),
        );
      }
      const db = openDb(companyPaths(company).db);
      try {
        migrate(db);
        const result = verifyAuditChain(db);
        return envelopeToCallResult(wrapCoreResult(result));
      } finally {
        db.close();
      }
    },
  );
}
