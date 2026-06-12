/**
 * MCP tools for backup-governance: destinations and placements.
 *
 * Tools registered (in original order):
 *   - system_backup_governance            (read)
 *   - system_backup_destination_list      (read)
 *   - system_backup_destination_add       (write-irreversible)
 *   - system_backup_destination_remove    (write-irreversible)
 *   - system_backup_place                 (write-irreversible)
 *   - system_backup_confirm_placement     (write-irreversible)
 *
 * Split out of `../system.ts` (mechanical split, no behavior change).
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  addBackupDestination,
  confirmBackupPlacement,
  getBackupGovernanceStatus,
  listBackupDestinations,
  placeBackupArchive,
  removeBackupDestination,
} from "../../../core/backup-governance";
import { envelopeShape, successEnvelope, wrapCoreResult } from "../../envelope";
import {
  withCompanyDb,
  withCompanyDbConfirmed,
  confirmField,
} from "../../tool-runtime";

export function registerSystemBackupDestinationTools(server: McpServer): void {
  server.registerTool(
    "system_backup_governance",
    {
      title: "Backup governance status",
      description:
        "Samlet backup-status: forfald, bogførings-lås, destinationer og om seneste " +
        "backup er placeret sikkert i EU/EØS. Read-only.",
      inputSchema: {
        company: z
          .string()
          .min(1)
          .describe("Absolute path to the company directory, or a workspace slug."),
        asOf: z
          .string()
          .optional()
          .describe(
            "Optional YYYY-MM-DD date to evaluate governance status against " +
              "(default: today UTC).",
          ),
      },
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
      inputSchema: {
        company: z
          .string()
          .min(1)
          .describe("Absolute path to the company directory, or a workspace slug."),
      },
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
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
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
      inputSchema: {
        company: z
          .string()
          .min(1)
          .describe("Absolute path to the company directory, or a workspace slug."),
        id: z
          .string()
          .min(1)
          .describe(
            "Destination id to remove. Get the id from `system_backup_destination_list`.",
          ),
        confirm: confirmField,
      },
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
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        archivePath: z
          .string()
          .min(1)
          .describe(
            "Absolute path to the .tar archive produced by `system_backup` with " +
              "archive:true (or `system_backup_archive`). The .sha256 sidecar is " +
              "located automatically next to it.",
          ),
        destinationId: z
          .string()
          .min(1)
          .describe(
            "Destination id (from `system_backup_destination_list`) to copy the " +
              "archive to. The destination's `location` must be a local or synced " +
              "folder reachable by this process.",
          ),
        actorKind: z
          .enum(["human", "agent"])
          .optional()
          .describe(
            "Who is performing the placement: 'human' for a person clicking a " +
              "button, 'agent' for an AI/automation. Default: 'agent'.",
          ),
        at: z
          .string()
          .optional()
          .describe(
            "Optional ISO-8601 timestamp recorded in the placement log " +
              "(default: current UTC time).",
          ),
        note: z
          .string()
          .optional()
          .describe(
            "Optional free-text note attached to the placement record.",
          ),
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
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        destinationId: z
          .string()
          .min(1)
          .describe(
            "Destination id (from `system_backup_destination_list`) the agent " +
              "placed the archive at.",
          ),
        backupId: z
          .string()
          .min(1)
          .describe(
            "Backup id (returned by `system_backup`) the archive was produced from.",
          ),
        archiveSha256: z
          .string()
          .min(1)
          .describe(
            "Lowercase hex sha256 digest (64 chars) of the placed .tar archive. " +
              "If the destination is reachable from this process, the digest is " +
              "re-verified server-side before the placement is recorded.",
          ),
        archiveSizeBytes: z
          .number()
          .optional()
          .describe(
            "Optional file size in bytes of the placed .tar archive. Recorded " +
              "alongside the sha256 for audit.",
          ),
        actorKind: z
          .enum(["human", "agent"])
          .optional()
          .describe(
            "Who performed the external placement: 'human' or 'agent'. Default: 'agent'.",
          ),
        at: z
          .string()
          .optional()
          .describe(
            "Optional ISO-8601 timestamp recorded in the placement log " +
              "(default: current UTC time).",
          ),
        note: z
          .string()
          .optional()
          .describe("Optional free-text note attached to the placement record."),
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
}
