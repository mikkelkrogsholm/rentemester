// Corporate tax return preparation: the `report tax` CLI command.
//
// `report tax --company <c> --from <fy-start> --to <fy-end>` derives the
// corporate taxable income (skattepligtig indkomst) for a locked fiscal year
// and computes the 22% selskabsskat for the micro-ApS case, emitting an
// oplysningsskema preparation as JSON (or Danish human text).
//
// Read-only on the ledger: Rentemester prepares the figures; the owner or
// advisor reviews them and files via TastSelv Erhverv.

import { migrate } from "../core/db";
import { buildTaxReturn } from "../core/tax-return";
import { openCommandDb } from "../cli-dispatch";
import type { CommandDispatch } from "../cli-dispatch";
import { emitHumanReport } from "../cli-format";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("report", "tax", (ctx) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    if (!from || !to) {
      console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const result = buildTaxReturn(db, from, to);
    emitHumanReport("report-tax", result as unknown as Record<string, unknown>, ctx.outputFormat);
    db.close();
  });
}
