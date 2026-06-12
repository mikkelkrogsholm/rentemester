/**
 * `system_backup_lock` MCP tool — configures the opt-in bookkeeping lock that
 * blocks new writes when the weekly backup is overdue.
 *
 * Split out of `../system.ts` as its own file (instead of living in
 * `./backup.ts`) so the composer in `../system.ts` can interleave the
 * backup-destination tools BEFORE the lock, matching the original inline
 * registration order in `system.ts`.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { configureBackupLock } from "../../../core/backup-governance";
import { envelopeShape, wrapCoreResult } from "../../envelope";
import { withCompanyDbConfirmed, confirmField } from "../../tool-runtime";

export function registerSystemBackupLockTools(server: McpServer): void {
  server.registerTool(
    "system_backup_lock",
    {
      title: "Configure the opt-in bookkeeping lock",
      description:
        "Konfigurerer den frivillige bogførings-lås. Slået til blokeres ny bogføring " +
        "hvis den ugentlige backup (BEK 205/2024 § 4) er forsømt ud over grace-perioden. " +
        "write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        enforced: z
          .boolean()
          .optional()
          .describe(
            "Whether the lock is enforced (true) or just monitored (false). When " +
              "enforced AND the weekly backup is overdue beyond graceDays, " +
              "subsequent bookkeeping writes return code:'BACKUP_LOCKED'. " +
              "Default: keep existing setting (or false on a fresh ledger).",
          ),
        graceDays: z
          .number()
          .optional()
          .describe(
            "Days of grace AFTER the weekly backup deadline before the lock " +
              "activates. Integer ≥ 0. Default: keep existing setting (initial: 3).",
          ),
        at: z
          .string()
          .optional()
          .describe(
            "Optional ISO-8601 timestamp recorded as the change time (default: now UTC).",
          ),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      enforced?: boolean;
      graceDays?: number;
      at?: string;
      confirm?: boolean;
    }>(server, "system_backup_lock", ({ db, args }) =>
      wrapCoreResult(
        configureBackupLock(db, args.company, {
          enforced: args.enforced,
          graceDays: args.graceDays,
          at: args.at,
        }),
      ),
    ),
  );
}
