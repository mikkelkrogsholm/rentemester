import type { CommandDispatch } from "../cli-dispatch";
import { openLedgerReadOnly } from "../core/ledger-inspection";
import { companyPaths } from "../core/paths";
import { getPurchaseCase, listPurchaseCases } from "../core/purchase-cases";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("purchase-case", "list", (ctx) => {
    const db = openLedgerReadOnly(companyPaths(ctx.companyRoot()).db);
    try { ctx.emitResult({ ok: true, purchaseCases: listPurchaseCases(db) }); }
    finally { db.close(); }
  });
  dispatch.on("purchase-case", "show", (ctx) => {
    const id = ctx.arg("--case-id") ?? ctx.fatal("--case-id is required");
    const db = openLedgerReadOnly(companyPaths(ctx.companyRoot()).db);
    try { ctx.emitResult({ ok: true, purchaseCase: getPurchaseCase(db, id) }); }
    finally { db.close(); }
  });
}
