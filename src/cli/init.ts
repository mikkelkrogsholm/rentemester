import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { ensureCompanyDirs, companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { seedAccounts } from "../core/ledger";
import {
  normalizeCvr,
  normalizeFiscalYearLabelStrategy,
  normalizeFiscalYearStartMonth,
} from "../core/company";
import type { CommandDispatch } from "../cli-dispatch";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("init", null, (ctx) => {
    const root = ctx.companyRoot();
    const p = ensureCompanyDirs(root);
    const db = openDb(p.db);
    migrate(db);
    seedAccounts(db);
    const cvr = normalizeCvr(ctx.arg("--cvr"));
    const fiscalYearStartMonth =
      normalizeFiscalYearStartMonth(ctx.arg("--fiscal-year-start-month")) ?? 1;
    const fiscalYearLabelStrategy =
      normalizeFiscalYearLabelStrategy(ctx.arg("--fiscal-year-label-strategy")) ?? "end-year";
    if (
      ctx.arg("--fiscal-year-start-month") &&
      !normalizeFiscalYearStartMonth(ctx.arg("--fiscal-year-start-month"))
    ) {
      console.error("--fiscal-year-start-month must be an integer between 1 and 12");
      process.exit(2);
    }
    if (
      ctx.arg("--fiscal-year-label-strategy") &&
      !normalizeFiscalYearLabelStrategy(ctx.arg("--fiscal-year-label-strategy"))
    ) {
      console.error("--fiscal-year-label-strategy must be one of end-year, start-year, span");
      process.exit(2);
    }
    db.query(
      `INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy)
       VALUES (1, 'Rentemester company', ?, ?, ?)
       ON CONFLICT(id) DO UPDATE SET
         cvr = excluded.cvr,
         fiscal_year_start_month = excluded.fiscal_year_start_month,
         fiscal_year_label_strategy = excluded.fiscal_year_label_strategy`,
    ).run(cvr, fiscalYearStartMonth, fiscalYearLabelStrategy);
    const policy = join(p.config, "policy.yaml");
    if (!existsSync(policy)) {
      writeFileSync(
        policy,
        `company_policy:\n  country: DK\n  currency: DKK\n  allow_direct_sql_write: false\n  block_if_uncertain: true\n`,
      );
    }
    db.run(
      "INSERT INTO audit_log (event_type, entity_type, message) VALUES ('init','company','Company volume initialized')",
    );
    console.log(`Initialized Rentemester company at ${root}`);
    console.log(`Ledger: ${p.db}`);
    db.close();
  });

  dispatch.on("system", "healthcheck", (ctx) => {
    const p = companyPaths(ctx.companyRoot());
    const checks: Array<[string, boolean]> = [
      ["company_root", existsSync(p.root)],
      ["data_dir", existsSync(p.data)],
      ["ledger", existsSync(p.db)],
      ["documents", existsSync(p.documentsInbox)],
      ["config", existsSync(p.config)],
    ];
    let ok = true;
    for (const [name, pass] of checks) {
      console.log(`${pass ? "OK" : "FAIL"} ${name}`);
      if (!pass) ok = false;
    }
    if (!ok) process.exit(1);
  });
}
