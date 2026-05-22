import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { importBankCsv, resolveBankAccount } from "../core/bank";
import { suggestBankMatches } from "../core/bank-suggest-matches";
import { buildBankReconciliationReport, listBankTransactions } from "../core/reconciliation";
import { syncUnmatchedBankTransactionExceptions } from "../core/exceptions";
import { openCommandDb } from "../cli-dispatch";
import { renderHumanReport, formatKroner } from "../cli-format";
import { ledgerStatusDa } from "../core/messages";
import type { Database } from "bun:sqlite";
import type { CommandContext, CommandDispatch } from "../cli-dispatch";

// ===== BANK CLUSTER (#187) =====
// Resolves an optional `--account <id|slug>` filter to a numeric bank-account
// id. A given-but-unknown account is a fatal CLI error.
function resolveAccountFilter(ctx: CommandContext, db: Database): number | undefined {
  const raw = ctx.trimToNull(ctx.arg("--account"));
  if (!raw) return undefined;
  const account = resolveBankAccount(db, raw);
  if (!account) {
    console.error(`--account '${raw}' does not match any registered bank account`);
    process.exit(2);
  }
  return account.id;
}
// ===== END BANK CLUSTER (#187) =====

function renderBankTransactionsHuman(rows: any[]): void {
  console.log(`Banktransaktioner (${rows.length})`);
  if (rows.length === 0) {
    console.log("Ingen banktransaktioner for det valgte filter.");
    return;
  }
  for (const row of rows) {
    const status = row.ledgerStatus != null ? ledgerStatusDa(String(row.ledgerStatus)) : "—";
    console.log("");
    console.log(`#${row.id} — ${row.transactionDate} | ${formatKroner(row.amount)}`);
    console.log(`  Tekst: ${row.text ?? "—"}`);
    const ref = row.reference ? ` | Reference: ${row.reference}` : "";
    console.log(`  Status: ${status}${ref}`);
    if (row.journalEntryNo) {
      console.log(`  Bogført som postering ${row.journalEntryNo}`);
    }
  }
}

function renderBankSuggestionsHuman(rows: any[]): void {
  if (rows.length === 0) {
    console.log("Ingen uafstemte banktransaktioner for det valgte filter.");
    return;
  }
  for (const row of rows) {
    console.log(
      `Banktransaktion ${row.bankTransactionId} | ${row.date} | ${row.amount} ${row.currency} | ${row.text}`,
    );
    if (row.suggestions.length === 0) {
      console.log("  Ingen sikre forslag.");
      continue;
    }
    console.table(
      row.suggestions.map((suggestion: any) => ({
        type: suggestion.kind,
        bilagsId: suggestion.documentId,
        fakturanr: suggestion.invoiceNo,
        leverandør: suggestion.supplierName ?? null,
        kunde: suggestion.customerName ?? null,
        sikkerhed: suggestion.confidence,
        begrundelser: suggestion.reasons.join("; "),
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
    const result = importBankCsv(db, root, file, {
      account: ctx.trimToNull(ctx.arg("--account")) ?? undefined,
      profile: ctx.trimToNull(ctx.arg("--profile")) ?? undefined,
    });
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
    const db = openCommandDb(ctx);
    migrate(db);
    const bankAccountId = resolveAccountFilter(ctx, db);
    const result = listBankTransactions(db, {
      status: ctx.arg("--status") as any,
      from: ctx.arg("--from") ?? undefined,
      to: ctx.arg("--to") ?? undefined,
      textMatch: ctx.arg("--text-match") ?? undefined,
      amount,
      bankAccountId,
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else if (result.ok) {
      renderBankTransactionsHuman(result.rows);
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
    const db = openCommandDb(ctx);
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
    const db = openCommandDb(ctx);
    migrate(db);
    const bankAccountId = resolveAccountFilter(ctx, db);
    const result = buildBankReconciliationReport(db, from, to, {
      status: ctx.arg("--status") as any,
      textMatch: ctx.arg("--text-match") ?? undefined,
      amount,
      bankAccountId,
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else if (result.ok) {
      const human = renderHumanReport("reconcile-bank", result as Record<string, unknown>);
      console.log(human ?? "");
    } else {
      console.error(result.errors.join("\n"));
    }
    db.close();
    if (!result.ok) process.exit(1);
  });
}
