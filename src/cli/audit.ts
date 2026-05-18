import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { verifyAuditChain } from "../core/ledger";
import type { CommandDispatch } from "../cli-dispatch";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("audit", "verify", (ctx) => {
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = verifyAuditChain(db);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
