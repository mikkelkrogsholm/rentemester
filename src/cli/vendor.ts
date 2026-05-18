import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { createVendor, listVendors } from "../core/master-data";
import type { CommandDispatch } from "../cli-dispatch";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("vendor", "create", (ctx) => {
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = createVendor(db, {
      name: ctx.arg("--name") ?? "",
      address: ctx.arg("--address") ?? undefined,
      vatOrCvr: ctx.arg("--cvr") ?? undefined,
      defaultExpenseAccount: ctx.arg("--expense-account") ?? undefined,
      defaultVatTreatment: ctx.arg("--default-vat") ?? undefined,
      notes: ctx.arg("--notes") ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("vendor", "list", (ctx) => {
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = listVendors(db, { archived: ctx.hasFlag("--archived") });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
