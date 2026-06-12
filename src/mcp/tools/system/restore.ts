/**
 * `system_restore_backup` MCP tool — DESTRUCTIVE restore of a backup into a
 * (possibly new) company directory.
 *
 * Split out of `../system.ts` (mechanical split, no behavior change).
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { restoreSystemBackup } from "../../../core/system-restore";
import { envelopeShape, wrapCoreResult } from "../../envelope";
import { withDestructiveConfirm, confirmField } from "../../tool-runtime";

export function registerSystemRestoreTools(server: McpServer): void {
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
            "Optional path to the SYMMETRIC HMAC verification key (the " +
              "'.backup-manifest.key' file) used to verify the backup manifest's " +
              "HMAC tag. This is NOT the ed25519 public key — see publicKey for " +
              "that. Typically required when backupDir is a .tar archive; for a " +
              "backup directory still inside its company 'backups/' folder the " +
              "key is otherwise inferred. Mirrors the CLI's --verify-key.",
          ),
        publicKey: z
          .string()
          .optional()
          .describe(
            "Optional path to the ASYMMETRIC ed25519 public key used to verify " +
              "the backup manifest's ed25519 signature (the signature added by " +
              "'system backup --sign-with-ed25519'). Supplying it out-of-band lets " +
              "an independent third party verify authenticity without the HMAC " +
              "key. Distinct from verifyKey, which is the symmetric HMAC key. " +
              "Mirrors the CLI's --public-key.",
          ),
        confirm: confirmField,
        confirmText: z
          .string()
          .optional()
          .describe(
            "Destructive-confirmation text. Must be EXACTLY 'RESTORE <targetCompany>' " +
              "— the literal word RESTORE, a space, then your targetCompany argument " +
              "verbatim. Concrete example: for targetCompany='/data/acme', use " +
              "confirmText='RESTORE /data/acme'. Deliberately schema-OPTIONAL (#307): " +
              "a missing/empty value reaches the handler and yields the normal " +
              "{ ok:false, errors:[...] } envelope (with code:'CONFIRMTEXT_MISMATCH'), " +
              "exactly like a mismatch — never a raw -32602. Any mismatch (or omission) " +
              "rejects the call.",
          ),
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false, openWorldHint: false },
    },
    withDestructiveConfirm<{
      backupDir: string;
      targetCompany: string;
      verifyKey?: string;
      publicKey?: string;
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
          publicKeyPath: args.publicKey,
        });
        return wrapCoreResult(result);
      },
    ),
  );
}
