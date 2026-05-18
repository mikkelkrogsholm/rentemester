import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { createSystemBackup, getBackupComplianceStatus } from "../core/system-backups";
import { restoreSystemBackup } from "../core/system-restore";
import { exportAuthorityPackage } from "../core/authority-export";
import type { CommandDispatch } from "../cli-dispatch";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("system", "backup", (ctx) => {
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = createSystemBackup(db, ctx.companyRoot(), { createdAt: ctx.arg("--at") });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("system", "backup-status", (ctx) => {
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = getBackupComplianceStatus(db, ctx.companyRoot(), ctx.arg("--as-of"));
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("system", "restore-backup", (ctx) => {
    const backupDir = ctx.arg("--backup-dir");
    const targetCompanyRoot = ctx.arg("--target-company");
    if (!backupDir || !targetCompanyRoot) {
      console.error("Missing required --backup-dir <dir> or --target-company <path>");
      process.exit(2);
    }
    const result = restoreSystemBackup({
      backupDir,
      targetCompanyRoot,
      verificationKeyPath: ctx.arg("--verify-key") ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
  });

  dispatch.on("system", "export-authority", (ctx) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    const outputDir = ctx.arg("--out");
    if (!from || !to || !outputDir) {
      console.error("Missing required --from <YYYY-MM-DD>, --to <YYYY-MM-DD>, or --out <dir>");
      process.exit(2);
    }
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = exportAuthorityPackage(db, ctx.companyRoot(), {
      periodStart: from,
      periodEnd: to,
      outputDir,
      requestedAt: ctx.arg("--requested-at"),
      requester: ctx.arg("--requester"),
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
