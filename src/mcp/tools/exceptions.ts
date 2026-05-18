/**
 * MCP-tools for exceptions-køen.
 *
 *  - `exceptions_list` (read)
 *  - `exception_resolve` (write-reversible — markerer som løst)
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  listExceptions,
  resolveException,
  type ExceptionStatus,
} from "../../core/exceptions";
import { wrapCoreResult } from "../envelope";
import { withCompanyDb, withCompanyDbConfirmed } from "../helpers";

export function registerExceptionTools(server: McpServer): void {
  server.registerTool(
    "exceptions_list",
    {
      title: "List exceptions",
      description: "Lister exceptions-køen. Filtrér på status: open|resolved|all. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        status: z.enum(["open", "resolved", "all"]).optional(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; status?: ExceptionStatus }>(server, ({ db, args }) => {
      const result = listExceptions(db, { status: args.status });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "exception_resolve",
    {
      title: "Resolve exception",
      description:
        "Markerer en exception som løst. write-reversible — kræver confirm:true. " +
        "Kan ikke gen-åbnes manuelt.",
      inputSchema: {
        company: z.string().min(1),
        id: z.number().int().positive(),
        note: z.string().optional(),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; id: number; note?: string; confirm: boolean }>(
      server,
      "exception_resolve",
      ({ db, args, actor }) => {
        const result = resolveException(db, {
          id: args.id,
          note: args.note ?? null,
          resolvedBy: actor.createdBy,
        });
        return wrapCoreResult(result);
      },
    ),
  );
}
