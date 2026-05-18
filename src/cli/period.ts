import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { closeAccountingPeriod } from "../core/periods";
import type { CommandDispatch } from "../cli-dispatch";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("period", "close", (ctx) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    if (!from || !to) {
      console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>");
      process.exit(2);
    }
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = closeAccountingPeriod(db, {
      periodStart: from,
      periodEnd: to,
      kind: (ctx.arg("--kind") as any) ?? undefined,
      status: (ctx.arg("--status") as any) ?? undefined,
      reference: ctx.arg("--reference") ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
