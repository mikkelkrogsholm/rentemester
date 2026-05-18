import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { listExceptions, resolveException } from "../core/exceptions";
import type { CommandDispatch } from "../cli-dispatch";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("exceptions", "list", (ctx) => {
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = listExceptions(db, { status: (ctx.arg("--status") as any) ?? undefined });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else if (result.ok) {
      console.table(
        result.rows.map((row: any) => ({
          id: row.id,
          type: row.type,
          severity: row.severity,
          status: row.status,
          relatedBankTransactionId: row.relatedBankTransactionId,
          relatedDocumentId: row.relatedDocumentId,
          message: row.message,
          requiredAction: row.requiredAction,
          createdAt: row.createdAt,
          resolvedAt: row.resolvedAt,
        })),
      );
    } else {
      console.error(result.errors.join("\n"));
    }
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("exceptions", "resolve", (ctx) => {
    const id = Number(ctx.arg("--id"));
    if (!Number.isInteger(id) || id <= 0) {
      console.error("Missing required --id <n>");
      process.exit(2);
    }
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = resolveException(db, {
      id,
      note: ctx.arg("--note") ?? undefined,
      resolvedBy:
        ctx.cliActor ?? process.env.RENTEMESTER_ACTOR ?? ctx.inferredMutationActor() ?? null,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });
}
