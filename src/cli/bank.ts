import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { importBankCsv } from "../core/bank";
import { suggestBankMatches } from "../core/bank-suggest-matches";
import { buildBankReconciliationReport, listBankTransactions } from "../core/reconciliation";
import { syncUnmatchedBankTransactionExceptions } from "../core/exceptions";
import type { CommandDispatch } from "../cli-dispatch";

function renderBankSuggestionsHuman(rows: any[]): void {
  if (rows.length === 0) {
    console.log("No unmatched bank transactions for current filter.");
    return;
  }
  for (const row of rows) {
    console.log(
      `Bank transaction ${row.bankTransactionId} | ${row.date} | ${row.amount} ${row.currency} | ${row.text}`,
    );
    if (row.suggestions.length === 0) {
      console.log("  No deterministic suggestions.");
      continue;
    }
    console.table(
      row.suggestions.map((suggestion: any) => ({
        kind: suggestion.kind,
        documentId: suggestion.documentId,
        invoiceNo: suggestion.invoiceNo,
        supplierName: suggestion.supplierName ?? null,
        customerName: suggestion.customerName ?? null,
        confidence: suggestion.confidence,
        reasons: suggestion.reasons.join("; "),
      })),
    );
  }
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("bank", "import", (ctx) => {
    const file = ctx.arg("--file");
    if (!file) {
      console.error("Missing required --file <transactions.csv>");
      process.exit(2);
    }
    const root = ctx.companyRoot();
    const db = openDb(companyPaths(root).db);
    migrate(db);
    const result = importBankCsv(db, root, file);
    const sync = result.ok
      ? syncUnmatchedBankTransactionExceptions(db)
      : { ok: true, created: 0, errors: [] };
    ctx.emitResult({
      ...(result as Record<string, unknown>),
      exceptionsCreated: sync.created,
    });
    db.close();
  });

  dispatch.on("bank", "list", (ctx) => {
    const amountArg = ctx.arg("--amount");
    const amount = amountArg === undefined ? undefined : Number(amountArg);
    if (amountArg !== undefined && Number.isNaN(amount)) {
      console.error("--amount must be numeric when present");
      process.exit(2);
    }
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = listBankTransactions(db, {
      status: ctx.arg("--status") as any,
      from: ctx.arg("--from") ?? undefined,
      to: ctx.arg("--to") ?? undefined,
      textMatch: ctx.arg("--text-match") ?? undefined,
      amount,
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else if (result.ok) {
      console.table(result.rows);
    } else {
      console.error(result.errors.join("\n"));
    }
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("bank", "suggest-matches", (ctx) => {
    const bankTransactionId = ctx.parseOptionalNumber("--bank-transaction-id");
    const max = ctx.parseOptionalNumber("--max");
    if (!bankTransactionId.ok) ctx.fatal(bankTransactionId.error);
    if (!max.ok) ctx.fatal(max.error);
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = suggestBankMatches(db, {
      bankTransactionId:
        bankTransactionId.value === undefined ? undefined : Number(bankTransactionId.value),
      max: max.value === undefined ? undefined : Number(max.value),
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else if (result.ok) {
      renderBankSuggestionsHuman(result.rows);
    } else {
      console.error(result.errors.join("\n"));
    }
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("reconcile", "bank", (ctx) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    const amountArg = ctx.arg("--amount");
    const amount = amountArg === undefined ? undefined : Number(amountArg);
    if (!from || !to) {
      console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>");
      process.exit(2);
    }
    if (amountArg !== undefined && Number.isNaN(amount)) {
      console.error("--amount must be numeric when present");
      process.exit(2);
    }
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = buildBankReconciliationReport(db, from, to, {
      status: ctx.arg("--status") as any,
      textMatch: ctx.arg("--text-match") ?? undefined,
      amount,
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else if (result.ok) {
      console.log(
        `Matched: ${result.matchedCount} | Unmatched: ${result.unmatchedCount} | Period: ${result.periodStart}..${result.periodEnd}`,
      );
      if (result.matched.length > 0) {
        console.log("\nMatched");
        console.table(result.matched);
      }
      if (result.unmatched.length > 0) {
        console.log("\nUnmatched");
        console.table(result.unmatched);
      }
      if (result.matched.length === 0 && result.unmatched.length === 0) {
        console.log("No rows for current filter.");
      }
    } else {
      console.error(result.errors.join("\n"));
    }
    db.close();
    if (!result.ok) process.exit(1);
  });
}
