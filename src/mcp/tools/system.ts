/**
 * MCP-tools for system-niveau-operationer.
 *
 *  - `system_healthcheck` (read)
 *  - `system_backup_status` (read)
 *  - `system_backup` (write-irreversible)
 *  - `system_export_authority` (write-irreversible)
 *  - `system_restore_backup` (DESTRUCTIVE — kræver confirmText)
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { existsSync } from "node:fs";
import {
  createSystemBackup,
  getBackupComplianceStatus,
} from "../../core/system-backups";
import { restoreSystemBackup } from "../../core/system-restore";
import { exportAuthorityPackage } from "../../core/authority-export";
import { companyPaths } from "../../core/paths";
import { wrapCoreResult, successEnvelope, errorEnvelope } from "../envelope";
import {
  withCompanyDb,
  withCompanyDbConfirmed,
  withDestructiveConfirm,
} from "../helpers";

export function registerSystemTools(server: McpServer): void {
  server.registerTool(
    "system_healthcheck",
    {
      title: "Company directory healthcheck",
      description: "Tjekker at virksomhedsmappen og kernefilerne findes. Read-only.",
      inputSchema: { company: z.string().min(1) },
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    async ({ company }: { company: string }) => {
      if (typeof company !== "string" || company.length === 0) {
        const env = errorEnvelope("company path is required");
        return {
          content: [{ type: "text" as const, text: JSON.stringify(env) }],
          isError: true,
          structuredContent: env,
        };
      }
      const p = companyPaths(company);
      const checks: Array<{ name: string; ok: boolean }> = [
        { name: "company_root", ok: existsSync(p.root) },
        { name: "data_dir", ok: existsSync(p.data) },
        { name: "ledger", ok: existsSync(p.db) },
        { name: "documents", ok: existsSync(p.documentsInbox) },
        { name: "config", ok: existsSync(p.config) },
      ];
      const missing = checks.filter((c) => !c.ok).map((c) => c.name);
      const env =
        missing.length === 0
          ? successEnvelope({ ok: true, missing: [], checks })
          : { ...errorEnvelope(missing.map((m) => `missing: ${m}`)), data: { ok: false, missing, checks } };
      return {
        content: [{ type: "text" as const, text: JSON.stringify(env) }],
        isError: !env.ok,
        structuredContent: env,
      };
    },
  );

  server.registerTool(
    "system_backup_status",
    {
      title: "Backup compliance status",
      description: "Tjekker om backup-pligten er opfyldt. Read-only.",
      inputSchema: {
        company: z.string().min(1),
        asOf: z.string().optional(),
      },
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
      description: "Opretter revisionsklar backup. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        at: z.string().optional(),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; at?: string; confirm: boolean }>(
      server,
      "system_backup",
      ({ db, args }) => {
        const result = createSystemBackup(db, args.company, { createdAt: args.at });
        return wrapCoreResult(result);
      },
    ),
  );

  server.registerTool(
    "system_export_authority",
    {
      title: "Export authority package",
      description: "Eksporterer materiale til myndighedsudlevering. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        from: z.string().min(1),
        to: z.string().min(1),
        out: z.string().min(1),
        requestedAt: z.string().optional(),
        requester: z.string().optional(),
        confirm: z.boolean(),
      },
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      from: string;
      to: string;
      out: string;
      requestedAt?: string;
      requester?: string;
      confirm: boolean;
    }>(server, "system_export_authority", ({ db, args }) => {
      const result = exportAuthorityPackage(db, args.company, {
        periodStart: args.from,
        periodEnd: args.to,
        outputDir: args.out,
        requestedAt: args.requestedAt,
        requester: args.requester,
      });
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "system_restore_backup",
    {
      title: "Restore system backup (DESTRUCTIVE)",
      description:
        "Gendanner backup til ny virksomhedssti. DESTRUCTIVE — kræver confirm:true OG " +
        "confirmText='RESTORE <targetCompany>'. Sletter ikke nogen filer på source, " +
        "men kan overskrive filer i targetCompany.",
      inputSchema: {
        backupDir: z.string().min(1),
        targetCompany: z.string().min(1),
        verifyKey: z.string().optional(),
        confirm: z.boolean(),
        confirmText: z.string().min(1),
      },
      annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false, openWorldHint: false },
    },
    withDestructiveConfirm<{
      backupDir: string;
      targetCompany: string;
      verifyKey?: string;
      confirm: boolean;
      confirmText: string;
    }>(
      "system_restore_backup",
      (args) => `RESTORE ${args.targetCompany}`,
      (args) => {
        const result = restoreSystemBackup({
          backupDir: args.backupDir,
          targetCompanyRoot: args.targetCompany,
          verificationKeyPath: args.verifyKey,
        });
        return wrapCoreResult(result);
      },
    ),
  );
}
