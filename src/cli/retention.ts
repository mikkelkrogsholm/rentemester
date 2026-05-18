import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { buildRetentionStatusReport } from "../core/retention";
import type { CommandDispatch } from "../cli-dispatch";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("retention", "status", (ctx) => {
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = buildRetentionStatusReport(db, ctx.arg("--as-of"));
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
