/**
 * MCP tools for creating system backups (the backup *artifact* itself —
 * destinations and placements live in `./backup-destinations.ts`; the
 * opt-in bookkeeping lock lives in `./backup-lock.ts`).
 *
 * Tools registered (in original order):
 *   - system_backup_status         (read)
 *   - system_backup                (write-irreversible)
 *   - system_backup_archive        (write-irreversible)
 *
 * Split out of `../system.ts` (mechanical split, no behavior change).
 * `system_backup_lock` was extracted into its own file so the composer in
 * `../system.ts` can interleave the destination tools BEFORE the lock,
 * matching the original inline registration order.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  createSystemBackup,
  getBackupComplianceStatus,
  packBackupArchive,
} from "../../../core/system-backups";
import { envelopeShape, wrapCoreResult } from "../../envelope";
import {
  withCompanyDb,
  withCompanyDbConfirmed,
  confirmField,
} from "../../tool-runtime";

export function registerSystemBackupTools(server: McpServer): void {
  server.registerTool(
    "system_backup_status",
    {
      title: "Backup compliance status",
      description: "Tjekker om backup-pligten er opfyldt. Read-only.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        asOf: z
          .string()
          .optional()
          .describe(
            "Optional YYYY-MM-DD date to evaluate the backup compliance against " +
              "(default: today UTC). Use this to ask 'were we compliant on 2026-03-31?'",
          ),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; asOf?: string }>(server, ({ db, args }) => {
      const result = getBackupComplianceStatus(db, args.company, args.asOf);
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "system_backup",
    {
      title: "Create system backup",
      description:
        "Opretter revisionsklar backup. Med archive:true " +
        "pakkes backuppen straks til ét .tar-arkiv klar til off-site placering. " +
        "write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        at: z
          .string()
          .optional()
          .describe(
            "Optional ISO-8601 timestamp the backup is created at (overrides the " +
              "wall clock for deterministic testing). Default: current UTC time.",
          ),
        archive: z
          .boolean()
          .optional()
          .describe(
            "When true, ALSO pack the backup into a single deterministic .tar " +
              "(plus a .sha256 sidecar) ready for off-site placement. Default: false " +
              "— the on-disk backup directory is created without an archive.",
          ),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; at?: string; archive?: boolean; confirm?: boolean }>(
      server,
      "system_backup",
      ({ db, args }) => {
        const result = createSystemBackup(db, args.company, { createdAt: args.at });
        if (args.archive && result.ok) {
          const archived = packBackupArchive(db, args.company, { backupId: result.backupId });
          return wrapCoreResult({
            ...result,
            ok: result.ok && archived.ok,
            errors: [...result.errors, ...archived.errors],
            archive: archived,
          });
        }
        return wrapCoreResult(result);
      },
    ),
  );

  server.registerTool(
    "system_backup_archive",
    {
      title: "Pack a backup into a single-file archive",
      description:
        "Pakker en eksisterende backup til ét deterministisk .tar-arkiv (+ .sha256) " +
        "klar til at agenten kan flytte det off-site. Uden backupId pakkes den nyeste. " +
        "write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        backupId: z
          .string()
          .optional()
          .describe(
            "Backup id to pack (returned by `system_backup` or visible in " +
              "`system_backup_status`). When omitted, the most recent backup is " +
              "selected.",
          ),
        out: z
          .string()
          .optional()
          .describe(
            "Optional output path for the .tar archive (default: alongside the " +
              "backup directory). A .sha256 sidecar is always written next to it.",
          ),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; backupId?: string; out?: string; confirm?: boolean }>(
      server,
      "system_backup_archive",
      ({ db, args }) =>
        wrapCoreResult(packBackupArchive(db, args.company, { backupId: args.backupId, outPath: args.out })),
    ),
  );
}
