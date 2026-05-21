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
import { withCompanyDb, withCompanyDbConfirmed } from "../tool-runtime";

export function registerExceptionTools(server: McpServer): void {
  server.registerTool(
    "exceptions_list",
    {
      title: "List exceptions",
      description:
        "Lister exceptions-køen. Filtrér på status: open|resolved|all. Exceptions i arkiverede/lukkede perioder udelades som standard — sæt includeArchived:true for at vise dem. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        status: z.enum(["open", "resolved", "all"]).optional(),
        includeArchived: z.boolean().optional(),
      },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; status?: ExceptionStatus; includeArchived?: boolean }>(
      server,
      ({ db, args }) => {
        const result = listExceptions(db, {
          status: args.status,
          includeArchived: args.includeArchived,
        });
        return wrapCoreResult(result);
      },
    ),
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
        confirm: z.boolean().optional(),
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
