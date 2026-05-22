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
  packBackupArchive,
} from "../../core/system-backups";
import { restoreSystemBackup } from "../../core/system-restore";
import {
  addBackupDestination,
  confirmBackupPlacement,
  configureBackupLock,
  getBackupGovernanceStatus,
  listBackupDestinations,
  placeBackupArchive,
  removeBackupDestination,
} from "../../core/backup-governance";
import { exportAuthorityPackage } from "../../core/authority-export";
import { companyPaths } from "../../core/paths";
import { envelopeShape, errorEnvelope, successEnvelope, wrapCoreResult } from "../envelope";
import {
  withCompanyDb,
  withCompanyDbConfirmed,
  withDestructiveConfirm,
  confirmField,
} from "../tool-runtime";

export function registerSystemTools(server: McpServer): void {
  server.registerTool(
    "system_healthcheck",
    {
      title: "Company directory healthcheck",
      description: "Tjekker at virksomhedsmappen og kernefilerne findes. Read-only.",
      inputSchema: { company: z.string().min(1) },
      outputSchema: envelopeShape,
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
        company: z.string().min(1),
        at: z.string().optional(),
        archive: z.boolean().optional(),
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
        company: z.string().min(1),
        backupId: z.string().optional(),
        out: z.string().optional(),
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

  server.registerTool(
    "system_backup_governance",
    {
      title: "Backup governance status",
      description:
        "Samlet backup-status: forfald, bogførings-lås, destinationer og om seneste " +
        "backup er placeret sikkert i EU/EØS. Read-only.",
      inputSchema: { company: z.string().min(1), asOf: z.string().optional() },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string; asOf?: string }>(server, ({ db, args }) =>
      wrapCoreResult(getBackupGovernanceStatus(db, args.company, args.asOf)),
    ),
  );

  server.registerTool(
    "system_backup_destination_list",
    {
      title: "List backup destinations",
      description: "Lister konfigurerede backup-destinationer med deres attestering. Read-only.",
      inputSchema: { company: z.string().min(1) },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string }>(server, ({ args }) => {
      const destinations = listBackupDestinations(args.company);
      return successEnvelope({ ok: true, destinationCount: destinations.length, destinations });
    }),
  );

  server.registerTool(
    "system_backup_destination_add",
    {
      title: "Add a backup destination",
      description:
        "Tilføjer en backup-destination. inEeaOrEu attesterer at destinationen ligger " +
        "på en server i EU/EØS, jf. BEK 205/2024 § 4, stk. 2. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        label: z.string().min(1).describe("Human-readable name for this destination, e.g. 'Revisor Dropbox'."),
        kind: z
          .enum(["local-folder", "dropbox", "google-drive", "ssh", "other"])
          .describe(
            "Destination kind: 'local-folder' = a folder on this machine; " +
              "'dropbox' / 'google-drive' = a synced desktop folder of that cloud service; " +
              "'ssh' = a remote host reached over SSH; 'other' = anything else.",
          ),
        location: z
          .string()
          .min(1)
          .describe("Path or URI of the destination, e.g. a local folder path or an ssh:// URI."),
        inEeaOrEu: z
          .boolean()
          .describe(
            "ATTESTATION (BEK 205/2024 § 4, stk. 2): true ⇒ you attest, as a human, " +
              "that this destination's server is located inside the EU/EEA. Rentemester " +
              "cannot determine this itself — you are legally attesting it.",
          ),
        attestedBy: z
          .string()
          .min(1)
          .describe(
            "Identity of the human making these attestations (e.g. an email). " +
              "A human — not Rentemester — must attest where this backup is stored.",
          ),
        regionCountry: z
          .string()
          .optional()
          .describe("Optional ISO country code where the destination's server is located, e.g. 'DK'."),
        regionNote: z
          .string()
          .optional()
          .describe("Optional free-text note backing the EU/EEA region attestation."),
        nonRelatedParty: z
          .boolean()
          .optional()
          .describe(
            "ATTESTATION: true (default) ⇒ you attest the destination is held by a " +
              "non-related third party (not the company itself or a related party). " +
              "Set false if the destination is a related party.",
          ),
        itSecurityMeetsStandards: z
          .boolean()
          .optional()
          .describe(
            "ATTESTATION: true ⇒ you attest, as a human, that the destination meets " +
              "recognised IT-security standards. Rentemester cannot verify this itself.",
          ),
        itSecurityNote: z
          .string()
          .optional()
          .describe("Optional free-text note backing the IT-security attestation."),
        at: z
          .string()
          .optional()
          .describe("Optional ISO-8601 timestamp for when the destination was added (default: now)."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      label: string;
      kind: "local-folder" | "dropbox" | "google-drive" | "ssh" | "other";
      location: string;
      inEeaOrEu: boolean;
      attestedBy: string;
      regionCountry?: string;
      regionNote?: string;
      nonRelatedParty?: boolean;
      itSecurityMeetsStandards?: boolean;
      itSecurityNote?: string;
      at?: string;
      confirm?: boolean;
    }>(server, "system_backup_destination_add", ({ db, actor, args }) =>
      wrapCoreResult(
        addBackupDestination(db, args.company, {
          label: args.label,
          kind: args.kind,
          location: args.location,
          inEeaOrEu: args.inEeaOrEu,
          attestedBy: args.attestedBy,
          actor: actor.createdBy,
          regionCountry: args.regionCountry,
          regionNote: args.regionNote,
          nonRelatedParty: args.nonRelatedParty,
          itSecurityMeetsStandards: args.itSecurityMeetsStandards,
          itSecurityNote: args.itSecurityNote,
          at: args.at,
        }),
      ),
    ),
  );

  server.registerTool(
    "system_backup_destination_remove",
    {
      title: "Remove a backup destination",
      description: "Fjerner en konfigureret backup-destination. write-irreversible.",
      inputSchema: { company: z.string().min(1), id: z.string().min(1), confirm: confirmField },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{ company: string; id: string; confirm?: boolean }>(
      server,
      "system_backup_destination_remove",
      ({ db, args }) => wrapCoreResult(removeBackupDestination(db, args.company, args.id)),
    ),
  );

  server.registerTool(
    "system_backup_place",
    {
      title: "Place a backup archive at a destination",
      description:
        "Kopierer et backup-arkiv til en destination med en lokal/synkroniseret mappe " +
        "(fx en Dropbox-desktopmappe) og verificerer kopien med sha256. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        archivePath: z.string().min(1),
        destinationId: z.string().min(1),
        actorKind: z.enum(["human", "agent"]).optional(),
        at: z.string().optional(),
        note: z.string().optional(),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      archivePath: string;
      destinationId: string;
      actorKind?: "human" | "agent";
      at?: string;
      note?: string;
      confirm?: boolean;
    }>(server, "system_backup_place", ({ db, actor, args }) =>
      wrapCoreResult(
        placeBackupArchive(db, args.company, {
          archivePath: args.archivePath,
          destinationId: args.destinationId,
          actorKind: args.actorKind ?? "agent",
          actor: actor.createdBy,
          at: args.at,
          note: args.note,
        }),
      ),
    ),
  );

  server.registerTool(
    "system_backup_confirm_placement",
    {
      title: "Record an externally-performed backup placement",
      description:
        "Registrerer en backup-placering foretaget uden for Rentemester — fx en agent " +
        "der har pushet arkivet til Dropbox/Drive/SSH med egne værktøjer. Verificeres " +
        "med sha256 hvis destinationen er læsbar. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        destinationId: z.string().min(1),
        backupId: z.string().min(1),
        archiveSha256: z.string().min(1),
        archiveSizeBytes: z.number().optional(),
        actorKind: z.enum(["human", "agent"]).optional(),
        at: z.string().optional(),
        note: z.string().optional(),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      destinationId: string;
      backupId: string;
      archiveSha256: string;
      archiveSizeBytes?: number;
      actorKind?: "human" | "agent";
      at?: string;
      note?: string;
      confirm?: boolean;
    }>(server, "system_backup_confirm_placement", ({ db, actor, args }) =>
      wrapCoreResult(
        confirmBackupPlacement(db, args.company, {
          destinationId: args.destinationId,
          backupId: args.backupId,
          archiveSha256: args.archiveSha256,
          archiveSizeBytes: args.archiveSizeBytes,
          actorKind: args.actorKind ?? "agent",
          actor: actor.createdBy,
          at: args.at,
          note: args.note,
        }),
      ),
    ),
  );

  server.registerTool(
    "system_backup_lock",
    {
      title: "Configure the opt-in bookkeeping lock",
      description:
        "Konfigurerer den frivillige bogførings-lås. Slået til blokeres ny bogføring " +
        "hvis den ugentlige backup (BEK 205/2024 § 4) er forsømt ud over grace-perioden. " +
        "write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        enforced: z.boolean().optional(),
        graceDays: z.number().optional(),
        at: z.string().optional(),
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

  server.registerTool(
    "system_export_authority",
    {
      title: "Export authority package",
      description: "Eksporterer materiale til myndighedsudlevering. write-irreversible.",
      inputSchema: {
        company: z
          .string()
          .min(1)
          .describe("Absolute path to the company directory, or a workspace slug."),
        from: z
          .string()
          .min(1)
          .describe("Start of the export period (inclusive), in YYYY-MM-DD format."),
        to: z
          .string()
          .min(1)
          .describe("End of the export period (inclusive), in YYYY-MM-DD format. Must not be before `from`."),
        out: z
          .string()
          .min(1)
          .describe(
            "Output directory path ON THE MCP SERVER'S FILESYSTEM where the authority " +
              "export package is written. Created if it does not exist.",
          ),
        requestedAt: z
          .string()
          .optional()
          .describe("Optional ISO-8601 timestamp for when the authority requested the material (default: now)."),
        requester: z
          .string()
          .optional()
          .describe("Optional name of the requesting authority/person, recorded in the export manifest."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      from: string;
      to: string;
      out: string;
      requestedAt?: string;
      requester?: string;
      confirm?: boolean;
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
        backupDir: z
          .string()
          .min(1)
          .describe(
            "Path to the backup directory (or .tar archive) to restore from. " +
              "ON THE MCP SERVER'S FILESYSTEM. Never modified by the restore.",
          ),
        targetCompany: z
          .string()
          .min(1)
          .describe(
            "Path to the company directory the backup is restored INTO. " +
              "Existing files here may be overwritten.",
          ),
        verifyKey: z
          .string()
          .optional()
          .describe(
            "Optional path to an ed25519 public key used to verify the backup " +
              "signature. Typically required when backupDir is a .tar archive.",
          ),
        confirm: confirmField,
        confirmText: z
          .string()
          .min(1)
          .optional()
          .describe(
            "Destructive-confirmation text. Must be EXACTLY 'RESTORE <targetCompany>' " +
              "— i.e. the literal word RESTORE, a space, then the targetCompany value " +
              "passed above, verbatim. Any mismatch rejects the call.",
          ),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false, openWorldHint: false },
    },
    withDestructiveConfirm<{
      backupDir: string;
      targetCompany: string;
      verifyKey?: string;
      confirm?: boolean;
      confirmText?: string;
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
