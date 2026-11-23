import { migrate } from "../core/db";
import { createAccount } from "../core/chart-of-accounts";
import { openCommandDb } from "../cli-dispatch";
import type { CommandDispatch } from "../cli-dispatch";
import { accountRoleStatus, resolveAccountRole, ACCOUNT_ROLES, confirmAccountRole } from "../core/account-roles";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("accounts", "list", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const rows = db
      .query("SELECT account_no, name, type, default_vat_code FROM accounts ORDER BY account_no")
      .all() as Array<Record<string, unknown>>;
    // `docs/cli-contract.md` promises every result is available as JSON. The
    // accounts list used to always print a console.table, ignoring --json /
    // --format json — so an agent could not parse the chart of accounts. (#231)
    if (ctx.outputFormat === "json") {
      console.log(JSON.stringify({ ok: true, count: rows.length, rows }, null, 2));
      db.close();
      return;
    }
    // `console.table` prepends a raw 0,1,2… array-index column — developer
    // noise on an owner-facing list. Render the chart of accounts as an
    // aligned text table keyed on the account number instead. (#246)
    console.log(renderAccountsTable(rows));
    db.close();
  });
  dispatch.on("accounts", "roles-status", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const status = accountRoleStatus(db);
    const roles = ACCOUNT_ROLES.map((role) => resolveAccountRole(db, role));
    ctx.emitResult({ ok: true, ...status, roles });
    db.close();
  });
  dispatch.on("accounts", "role-confirm", (ctx) => {
    const role = ctx.arg("--role");
    const accountNo = ctx.arg("--account");
    if (!role || !ACCOUNT_ROLES.includes(role as typeof ACCOUNT_ROLES[number]) || !accountNo) {
      ctx.fatal("brug: accounts role-confirm --company <path> --role <role> --account <kontonr>; kør accounts roles-status først for forslag og blokeringer");
    }
    const requiredAccountNo = accountNo ?? ctx.fatal("--account is required");
    const db = openCommandDb(ctx);
    migrate(db);
    const actor = ctx.cliActor ?? process.env.RENTEMESTER_ACTOR ?? ctx.inferredMutationActor();
    const requiredActor = actor ?? ctx.fatal("actor required for mutations");
    const dineroExport = ctx.arg("--dinero-export");
    const accountRole = role as typeof ACCOUNT_ROLES[number];
    const result = confirmAccountRole(db, accountRole, requiredAccountNo, requiredActor, "rentemester-cli", "explicit", dineroExport);
    const status = accountRoleStatus(db);
    ctx.emitResult(result.ok
      ? { ok: true, resolution: result.resolution, ...status, roles: ACCOUNT_ROLES.map((item) => resolveAccountRole(db, item)) }
      : { ok: false, errors: [result.error], ...status, roles: ACCOUNT_ROLES.map((item) => resolveAccountRole(db, item)) });
    db.close();
    if (!result.ok) process.exit(1);
  });
  dispatch.on("accounts", "add", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);

    const rawDirectPosting = ctx.arg("--allow-direct-posting");
    let allowDirectPosting: boolean | undefined;
    if (rawDirectPosting != null) {
      const lower = rawDirectPosting.toLowerCase();
      if (lower === "true") allowDirectPosting = true;
      else if (lower === "false") allowDirectPosting = false;
      else {
        ctx.emitResult({
          ok: false,
          errors: [
            `--allow-direct-posting must be 'true' or 'false' (got '${rawDirectPosting}')`,
          ],
        });
        db.close();
        process.exit(1);
      }
    }

    const actor = ctx.cliActor ?? process.env.RENTEMESTER_ACTOR ?? ctx.inferredMutationActor();
    if (!actor) ctx.fatal("actor required for mutations");
    const result = createAccount(
      db,
      {
        accountNo: ctx.arg("--account-no") ?? "",
        name: ctx.arg("--name") ?? "",
        type: ctx.arg("--type") ?? "",
        normalBalance: ctx.arg("--normal-balance"),
        defaultVatCode: ctx.arg("--default-vat-code"),
        allowDirectPosting,
      },
      { createdBy: actor, createdByProgram: "rentemester-cli" },
    );
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });
}

/** Render the chart of accounts as a Danish aligned text table, no index column. (#246) */
function renderAccountsTable(rows: Array<Record<string, unknown>>): string {
  if (rows.length === 0) return "Ingen konti i kontoplanen.";
  const cell = (value: unknown): string =>
    value == null || value === "" ? "—" : String(value);
  const headers = ["Kontonr.", "Navn", "Type", "Moms-kode"];
  const body = rows.map((row) => [
    cell(row.account_no),
    cell(row.name),
    cell(row.type),
    cell(row.default_vat_code),
  ]);
  const widths = headers.map((header, col) =>
    Math.max(header.length, ...body.map((line) => line[col]!.length)),
  );
  const formatLine = (cells: string[]): string =>
    cells.map((value, col) => value.padEnd(widths[col]!)).join("  ").trimEnd();
  const lines = [
    `Kontoplan (${rows.length} ${rows.length === 1 ? "konto" : "konti"})`,
    "",
    formatLine(headers),
    widths.map((width) => "-".repeat(width)).join("  "),
    ...body.map(formatLine),
  ];
  return lines.join("\n");
}
