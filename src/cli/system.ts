import { writeFileSync } from "node:fs";
import { migrate } from "../core/db";
import {
  createSystemBackup,
  exportBackupPublicKey,
  getBackupComplianceStatus,
  packBackupArchive,
  rotateBackupKeypair,
} from "../core/system-backups";
import { restoreSystemBackup, verifyBackupSignature } from "../core/system-restore";
import {
  addBackupDestination,
  confirmBackupPlacement,
  configureBackupLock,
  getBackupGovernanceStatus,
  listBackupDestinations,
  placeBackupArchive,
  removeBackupDestination,
} from "../core/backup-governance";
import { renderBackupGuide } from "../core/backup-guide";
import { getCompanySettings } from "../core/company";
import { exportAuthorityPackage } from "../core/authority-export";
import { exportSaftPackage } from "../core/saft-export";
import { openCommandDb } from "../cli-dispatch";
import type { CommandContext, CommandDispatch } from "../cli-dispatch";
import { emitHumanReport } from "../cli-format";

function requireBool(ctx: CommandContext, flag: string): boolean {
  const value = ctx.arg(flag);
  if (value === "true") return true;
  if (value === "false") return false;
  ctx.fatal(`${flag} is required and must be true or false`);
}

function optionalBool(ctx: CommandContext, flag: string): boolean | undefined {
  const value = ctx.arg(flag);
  if (value === undefined) return undefined;
  if (value === "true") return true;
  if (value === "false") return false;
  ctx.fatal(`${flag} must be true or false`);
}

function resolveActorId(ctx: CommandContext): string | undefined {
  return (
    ctx.cliActor ??
    ctx.trimToNull(process.env.RENTEMESTER_ACTOR) ??
    ctx.inferredMutationActor() ??
    undefined
  );
}

// A placement record names who placed the backup (a human pressing the
// button, or the agent pushing via its own tooling) — surfaced both ways.
function placementActor(ctx: CommandContext): {
  actor: string | undefined;
  actorKind: "human" | "agent";
} {
  const explicit = ctx.arg("--actor-kind");
  if (explicit !== undefined && explicit !== "human" && explicit !== "agent") {
    ctx.fatal("--actor-kind must be human or agent");
  }
  const actor = resolveActorId(ctx);
  const actorKind: "human" | "agent" =
    explicit === "agent" || explicit === "human"
      ? explicit
      : actor?.startsWith("agent:")
        ? "agent"
        : "human";
  return { actor: actor ?? undefined, actorKind };
}

function runExportPackage(
  ctx: CommandContext,
  packageProfile: "authority" | "accountant_handoff",
): void {
  const from = ctx.arg("--from");
  const to = ctx.arg("--to");
  const outputDir = ctx.arg("--out");
  if (!from || !to || !outputDir) {
    console.error("Missing required --from <YYYY-MM-DD>, --to <YYYY-MM-DD>, or --out <dir>");
    process.exit(2);
  }
  const db = openCommandDb(ctx);
  migrate(db);
  const result = exportAuthorityPackage(db, ctx.companyRoot(), {
    periodStart: from,
    periodEnd: to,
    outputDir,
    requestedAt: ctx.arg("--requested-at"),
    requester: ctx.arg("--requester"),
    packageProfile,
  });
  ctx.emitResult(result as Record<string, unknown>);
  db.close();
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("system", "backup", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const companyRoot = ctx.companyRoot();
    const result = createSystemBackup(db, companyRoot, {
      createdAt: ctx.arg("--at"),
      signWithEd25519: ctx.hasFlag("--sign-with-ed25519"),
    });
    const payload: Record<string, unknown> = { ...(result as Record<string, unknown>) };
    if (result.ok && ctx.hasFlag("--archive")) {
      const archived = packBackupArchive(db, companyRoot, { backupId: result.backupId });
      payload.archive = archived;
      if (!archived.ok) {
        payload.ok = false;
        payload.errors = [...result.errors, ...archived.errors];
      }
    }
    ctx.emitResult(payload);
    db.close();
  });

  dispatch.on("system", "backup-archive", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const result = packBackupArchive(db, ctx.companyRoot(), {
      backupId: ctx.arg("--backup-id"),
      outPath: ctx.arg("--out"),
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("system", "backup-governance", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const result = getBackupGovernanceStatus(db, ctx.companyRoot(), ctx.arg("--as-of"));
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("system", "backup-destinations", (ctx) => {
    const destinations = listBackupDestinations(ctx.companyRoot());
    ctx.emitResult({ ok: true, destinationCount: destinations.length, destinations });
  });

  dispatch.on("system", "backup-add-destination", (ctx) => {
    const inEeaOrEu = requireBool(ctx, "--region-eu");
    const nonRelatedParty = optionalBool(ctx, "--non-related");
    const itSecurity = optionalBool(ctx, "--it-security");
    const db = openCommandDb(ctx);
    migrate(db);
    const result = addBackupDestination(db, ctx.companyRoot(), {
      label: ctx.arg("--label") ?? "",
      kind: ctx.arg("--kind") ?? "",
      location: ctx.arg("--location") ?? "",
      inEeaOrEu,
      attestedBy: ctx.arg("--attested-by") ?? "",
      actor: resolveActorId(ctx),
      regionCountry: ctx.arg("--region-country"),
      regionNote: ctx.arg("--region-note"),
      nonRelatedParty,
      itSecurityMeetsStandards: itSecurity,
      itSecurityNote: ctx.arg("--it-security-note"),
      at: ctx.arg("--at"),
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("system", "backup-remove-destination", (ctx) => {
    const id = ctx.arg("--id");
    if (!id) {
      console.error("Missing required --id <dest-id>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const result = removeBackupDestination(db, ctx.companyRoot(), id);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("system", "backup-place", (ctx) => {
    const archiveFile = ctx.arg("--archive-file");
    const destination = ctx.arg("--destination");
    if (!archiveFile || !destination) {
      console.error("Missing required --archive-file <file.tar> or --destination <dest-id>");
      process.exit(2);
    }
    const { actor, actorKind } = placementActor(ctx);
    const db = openCommandDb(ctx);
    migrate(db);
    const result = placeBackupArchive(db, ctx.companyRoot(), {
      archivePath: archiveFile,
      destinationId: destination,
      actor,
      actorKind,
      at: ctx.arg("--at"),
      note: ctx.arg("--note"),
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("system", "backup-confirm-placement", (ctx) => {
    const destination = ctx.arg("--destination");
    const backupId = ctx.arg("--backup-id");
    const sha256 = ctx.arg("--archive-sha256");
    if (!destination || !backupId || !sha256) {
      console.error("Missing required --destination <dest-id>, --backup-id <id> or --archive-sha256 <hex>");
      process.exit(2);
    }
    const size = ctx.parseOptionalNumber("--archive-size");
    if (!size.ok) {
      console.error(size.error);
      process.exit(2);
    }
    const { actor, actorKind } = placementActor(ctx);
    const db = openCommandDb(ctx);
    migrate(db);
    const result = confirmBackupPlacement(db, ctx.companyRoot(), {
      destinationId: destination,
      backupId,
      archiveSha256: sha256,
      archiveSizeBytes: size.value,
      actor,
      actorKind,
      at: ctx.arg("--at"),
      note: ctx.arg("--note"),
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("system", "backup-lock", (ctx) => {
    const enforced = optionalBool(ctx, "--enforce");
    const graceDays = ctx.parseOptionalNumber("--grace-days");
    if (!graceDays.ok) {
      console.error(graceDays.error);
      process.exit(2);
    }
    if (enforced === undefined && graceDays.value === undefined) {
      console.error("Missing change: specify --enforce true|false and/or --grace-days <n>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const result = configureBackupLock(db, ctx.companyRoot(), {
      enforced,
      graceDays: graceDays.value,
      at: ctx.arg("--at"),
      actor: resolveActorId(ctx),
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("system", "backup-guide", (ctx) => {
    const out = ctx.arg("--out");
    if (!out) {
      console.error("Missing required --out <file.html>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const asOf = ctx.arg("--as-of");
    const governance = getBackupGovernanceStatus(db, ctx.companyRoot(), asOf);
    const settings = getCompanySettings(db);
    const html = renderBackupGuide({
      generatedAt: asOf ?? new Date().toISOString(),
      companyName: settings.name,
      governance,
    });
    writeFileSync(out, html);
    ctx.emitResult({ ok: true, out, checkedAt: governance.checkedAt });
    db.close();
  });

  dispatch.on("system", "export-public-key", (ctx) => {
    const out = ctx.arg("--out");
    if (!out) {
      console.error("Missing required --out <file>");
      process.exit(2);
    }
    const result = exportBackupPublicKey(ctx.companyRoot(), out);
    ctx.emitResult(result as Record<string, unknown>);
  });

  dispatch.on("system", "verify-backup-signature", (ctx) => {
    const backupDir = ctx.arg("--backup-dir");
    if (!backupDir) {
      console.error("Missing required --backup-dir <dir>");
      process.exit(2);
    }
    const result = verifyBackupSignature({
      backupDir,
      publicKeyPath: ctx.arg("--public-key") ?? undefined,
      verificationKeyPath: ctx.arg("--verify-key") ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
  });

  // `system rotate-backup-keypair` — generate a fresh Ed25519 signing keypair,
  // archive the old one with its fingerprint, and audit-log the rotation.
  dispatch.on("system", "rotate-backup-keypair", (ctx) => {
    const reason = ctx.arg("--reason");
    if (!reason || reason.trim().length === 0) {
      console.error('Missing required --reason "<text>"');
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const result = rotateBackupKeypair(db, ctx.companyRoot(), {
      reason,
      rotatedAt: ctx.arg("--at") ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("system", "backup-status", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const result = getBackupComplianceStatus(db, ctx.companyRoot(), ctx.arg("--as-of"));
    emitHumanReport("backup-status", result as Record<string, unknown>, ctx.outputFormat);
    db.close();
  });

  dispatch.on("system", "restore-backup", (ctx) => {
    const backupDir = ctx.arg("--backup-dir");
    const targetCompanyRoot = ctx.arg("--target-company");
    if (!backupDir || !targetCompanyRoot) {
      console.error("Missing required --backup-dir <dir> or --target-company <path>");
      process.exit(2);
    }
    // restore-backup is destructive: it can overwrite files in
    // --target-company. The MCP equivalent gates this as a destructive tool
    // (confirm:true + confirmText). The CLI matches the `asset write-off`
    // convention — a valued `--confirm yes` flag (bare booleans cannot be
    // added to the append-only cli-args BOOLEAN_FLAGS set). Without it the
    // command is refused before the filesystem is touched.
    const confirmValue = (ctx.arg("--confirm") ?? "").trim().toLowerCase();
    if (confirmValue !== "yes") {
      ctx.emitResult({
        ok: false,
        errors: [
          "system restore-backup is destructive: it can overwrite files in --target-company. " +
            "Re-run with --confirm yes to proceed.",
        ],
      });
      process.exit(1);
    }
    const result = restoreSystemBackup({
      backupDir,
      targetCompanyRoot,
      verificationKeyPath: ctx.arg("--verify-key") ?? undefined,
      publicKeyPath: ctx.arg("--public-key") ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    if (!result.ok) process.exit(1);
  });

  dispatch.on("system", "export-authority", (ctx) => {
    runExportPackage(ctx, "authority");
  });

  dispatch.on("system", "export-accountant", (ctx) => {
    runExportPackage(ctx, "accountant_handoff");
  });

  dispatch.on("system", "export-saft", (ctx) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    const outputDir = ctx.arg("--out");
    if (!from || !to || !outputDir) {
      console.error("Missing required --from <YYYY-MM-DD>, --to <YYYY-MM-DD>, or --out <dir>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const result = exportSaftPackage(db, ctx.companyRoot(), {
      periodStart: from,
      periodEnd: to,
      outputDir,
      generatedAt: ctx.arg("--generated-at") ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
