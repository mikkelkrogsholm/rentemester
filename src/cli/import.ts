import { existsSync, readFileSync } from "node:fs";
import { migrate } from "../core/db";
import { runImportFromSource } from "../core/import/framework";
import { queryArchive } from "../core/import/dinero-archive";
import { importDineroContacts } from "../core/import/dinero-contacts";
import { importBillyContacts } from "../core/import/billy-contacts";
import { ingestBillyBilag } from "../core/import/billy-bilag";
import { PARSERS } from "../core/import/synthetic-csv";
import { openCommandDb } from "../cli-dispatch";
import type { CommandDispatch } from "../cli-dispatch";

// `import run` — migrates a company from another accounting system into
// Rentemester. The framework (#185) parses the raw export with the per-system
// parser selected by `--system`, then lands the normalised result on the live
// ledger: the chart of accounts + company master data are reconciled, and (for
// a primobalance import) the #179 primobalance is posted. See src/core/import/.
//
// `--file` accepts a single text file (synthetic-csv) OR an export directory
// (Dinero #193): the multi-file parser declares the files it needs.
export function register(dispatch: CommandDispatch): void {
  dispatch.on("import", "run", (ctx) => {
    const file = ctx.arg("--file");
    if (!file) {
      console.error("Missing required --file <export-file-or-directory>");
      process.exit(2);
    }
    if (!existsSync(file)) {
      console.error(`Export path does not exist: ${file}`);
      process.exit(2);
    }
    const system = ctx.trimToNull(ctx.arg("--system")) ?? "synthetic-csv";
    const parser = PARSERS[system];
    if (!parser) {
      console.error(
        `Unknown --system '${system}'. Available: ${Object.keys(PARSERS).join(", ")}`,
      );
      process.exit(2);
    }

    const db = openCommandDb(ctx);
    migrate(db);
    const result = runImportFromSource(db, parser, file, {
      createdBy: ctx.cliActor ?? ctx.inferredMutationActor() ?? undefined,
      createdByProgram: "rentemester-import-cli",
    });
    ctx.emitResult(result as unknown as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  // `import contacts` — migrates a Dinero "Kontakter" CSV export into the
  // customer/vendor master data. Separate from `import run` (which lands a
  // ledger primobalance): this only touches master data, posts nothing.
  dispatch.on("import", "contacts", async (ctx) => {
    const file = ctx.arg("--file");
    if (!file) {
      console.error("Missing required --file <Kontakter.csv>");
      process.exit(2);
    }
    if (!existsSync(file)) {
      console.error(`Contacts CSV does not exist: ${file}`);
      process.exit(2);
    }
    const defaultRole = ctx.trimToNull(ctx.arg("--default-role"));
    if (defaultRole !== null && defaultRole !== "customer" && defaultRole !== "vendor") {
      console.error("--default-role must be 'customer' or 'vendor'");
      process.exit(2);
    }

    const db = openCommandDb(ctx);
    migrate(db);
    const result = await importDineroContacts(db, readFileSync(file, "utf8"), {
      enrichCvr: ctx.hasFlag("--enrich-cvr"),
      defaultRole: defaultRole ?? undefined,
    });
    ctx.emitResult(result as unknown as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  // `import billy-contacts` — migrates a Billy contacts.json export into the
  // customer/vendor master data. Similar to `import contacts` (Dinero) but
  // reads a JSON array from the Billy API export.
  dispatch.on("import", "billy-contacts", (ctx) => {
    const file = ctx.arg("--file");
    if (!file) {
      console.error("Missing required --file <contacts.json>");
      process.exit(2);
    }
    if (!existsSync(file)) {
      console.error(`Contacts JSON does not exist: ${file}`);
      process.exit(2);
    }
    const defaultRole = ctx.trimToNull(ctx.arg("--default-role"));
    if (defaultRole !== null && defaultRole !== "customer" && defaultRole !== "vendor") {
      console.error("--default-role must be 'customer' or 'vendor'");
      process.exit(2);
    }

    const db = openCommandDb(ctx);
    migrate(db);
    const result = importBillyContacts(db, readFileSync(file, "utf8"), {
      defaultRole: defaultRole ?? undefined,
    });
    ctx.emitResult(result as unknown as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  // `import billy-bilag` — ingests downloaded Billy attachment files and links
  // them to their journal entries. Requires a prior `import run --system billy`
  // that produced historicalEntriesPosted, and a Billy export directory with
  // bilag/ files and bill-transaction-map.json.
  dispatch.on("import", "billy-bilag", (ctx) => {
    const file = ctx.arg("--file");
    if (!file) {
      console.error("Missing required --file <billy-export-directory>");
      process.exit(2);
    }
    if (!existsSync(file)) {
      console.error(`Billy export directory does not exist: ${file}`);
      process.exit(2);
    }
    const resultFile = ctx.arg("--import-result");
    if (!resultFile || !existsSync(resultFile)) {
      console.error("Missing required --import-result <import-result.json> (output of `import run`)");
      process.exit(2);
    }
    let importResult;
    try {
      importResult = JSON.parse(readFileSync(resultFile, "utf8"));
    } catch {
      console.error("--import-result: invalid JSON");
      process.exit(2);
    }

    const db = openCommandDb(ctx);
    migrate(db);
    const companyRoot = ctx.arg("--company")!;
    const result = ingestBillyBilag(db, companyRoot, file, importResult);
    ctx.emitResult(result as unknown as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("import", "systems", (ctx) => {
    const rows = Object.values(PARSERS).map((p) => ({ system: p.system, label: p.label }));
    if (ctx.outputFormat === "json") {
      ctx.emitResult({ ok: true, systems: rows });
    } else {
      console.table(rows);
    }
  });

  // `import archive` — the read path for the pre-cut-over fiscal-year archive
  // (#197). A Dinero export spans several fiscal years; only the cut-over year
  // is posted to the live ledger, the earlier years are kept as a read-only
  // archive. With no `--year` it lists the archived years; with `--year` it
  // dumps that year's archived Posteringer / SaldoBalance detail rows.
  dispatch.on("import", "archive", (ctx) => {
    const system = ctx.trimToNull(ctx.arg("--system")) ?? "dinero";
    const yearArg = ctx.trimToNull(ctx.arg("--year"));
    let fiscalYear: number | undefined;
    if (yearArg != null) {
      const parsedYear = Number(yearArg);
      if (!Number.isInteger(parsedYear)) {
        console.error(`Invalid --year '${yearArg}': expected a four-digit fiscal year`);
        process.exit(2);
      }
      fiscalYear = parsedYear;
    }

    const db = openCommandDb(ctx);
    migrate(db);
    const result = queryArchive(db, { sourceSystem: system, fiscalYear });
    db.close();

    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as unknown as Record<string, unknown>);
    } else if (!result.ok) {
      console.error(result.errors.join("; "));
    } else if (fiscalYear != null) {
      const year = result.years[0]!;
      console.log(
        `Archived ${year.sourceSystem} fiscal year ${year.fiscalYear} ` +
          `(imported ${year.importedAt}) — read-only, not in the live ledger`,
      );
      console.table((year.postings ?? []).map((p) => ({
        konto: p.accountNo,
        dato: p.transactionDate,
        bilag: p.voucher,
        tekst: p.text,
        beloeb: p.amount,
      })));
      console.table((year.balances ?? []).map((b) => ({
        konto: b.accountNo,
        kontonavn: b.accountName,
        beloeb: b.amount,
      })));
    } else {
      console.table(result.years.map((y) => ({
        fiscalYear: y.fiscalYear,
        sourceSystem: y.sourceSystem,
        postings: y.postingCount,
        balances: y.balanceCount,
        importedAt: y.importedAt,
      })));
    }
    if (!result.ok) process.exit(1);
  });
}
