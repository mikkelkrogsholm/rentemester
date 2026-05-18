import { readFileSync } from "node:fs";
import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { postJournalEntry, reverseJournalEntry } from "../core/ledger";
import type { CommandDispatch } from "../cli-dispatch";

function resolveJournalEntryId(
  db: ReturnType<typeof openDb>,
  ctx: { arg(name: string): string | undefined; fatal(message: string): never; trimToNull(v: string | null | undefined): string | null },
): number | null {
  const entryId = Number(ctx.arg("--entry-id"));
  if (Number.isInteger(entryId) && entryId > 0) return entryId;
  const entryNo = ctx.arg("--entry-no")?.trim();
  if (entryNo) {
    const row = db
      .query(`SELECT id FROM journal_entries WHERE entry_no = ? LIMIT 1`)
      .get(entryNo) as { id: number } | null;
    return row?.id ?? null;
  }
  const matchText = ctx.trimToNull(ctx.arg("--match-text"));
  const matchDate = ctx.trimToNull(ctx.arg("--match-date"));
  const matchDocumentIdRaw = ctx.arg("--match-document-id");
  const matchDocumentId = Number(matchDocumentIdRaw);
  const hasMatchDocumentId = matchDocumentIdRaw !== undefined;
  if (!matchText) return null;
  if (hasMatchDocumentId && (!Number.isInteger(matchDocumentId) || matchDocumentId <= 0)) {
    ctx.fatal("--match-document-id must be a positive integer when present");
  }
  const rows = db
    .query(
      `
    SELECT id
    FROM journal_entries
    WHERE text = ?
      AND (? IS NULL OR transaction_date = ?)
      AND (? IS NULL OR document_id = ?)
    ORDER BY id DESC
    LIMIT 2
  `,
    )
    .all(
      matchText,
      matchDate,
      matchDate,
      hasMatchDocumentId ? matchDocumentId : null,
      hasMatchDocumentId ? matchDocumentId : null,
    ) as Array<{ id: number }>;
  if (rows.length > 1) {
    ctx.fatal(
      "Multiple journal entries matched --match-text; narrow with --match-date or --match-document-id",
    );
  }
  return rows[0]?.id ?? null;
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("journal", "post", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const payload = JSON.parse(readFileSync(input, "utf8"));
    const result = postJournalEntry(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("journal", "reverse", (ctx) => {
    const date = ctx.arg("--date");
    const reason = ctx.arg("--reason");
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const entryId = resolveJournalEntryId(db, ctx);
    if (!entryId || !date || !reason) {
      console.error(
        "Missing required --entry-id <n>, --entry-no <no>, or --match-text <text> [--match-date <YYYY-MM-DD>] [--match-document-id <n>], plus --date <YYYY-MM-DD> and --reason <text>",
      );
      process.exit(2);
    }
    const result = reverseJournalEntry(db, { entryId, transactionDate: date, reason });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("journal", "list", (ctx) => {
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const rows = db
      .query(
        "SELECT id, entry_no, transaction_date, text, currency, amount_foreign, amount_dkk, fx_rate_to_dkk, document_id, source_bank_transaction_id, status, reversal_of_entry_id FROM journal_entries ORDER BY id DESC",
      )
      .all();
    console.table(rows);
    db.close();
  });
}
