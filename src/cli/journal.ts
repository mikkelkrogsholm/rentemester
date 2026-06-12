import type { Database } from "bun:sqlite";
import { migrate } from "../core/db";
import { dryRunJournalEntry, postJournalEntry, reverseJournalEntry } from "../core/ledger";
import { asJournalEntryId, type JournalEntryId } from "../core/ids";
import { openCommandDb, readJsonCliInput } from "../cli-dispatch";
import { formatKroner } from "../cli-format";
import { journalStatusDa } from "../core/messages";
import type { CommandDispatch } from "../cli-dispatch";

function resolveJournalEntryId(
  db: Database,
  ctx: { arg(name: string): string | undefined; fatal(message: string): never; trimToNull(v: string | null | undefined): string | null },
): JournalEntryId | null {
  const entryId = Number(ctx.arg("--entry-id"));
  if (Number.isInteger(entryId) && entryId > 0) return asJournalEntryId(entryId);
  const entryNo = ctx.arg("--entry-no")?.trim();
  if (entryNo) {
    const row = db
      .query(`SELECT id FROM journal_entries WHERE entry_no = ? LIMIT 1`)
      .get(entryNo) as { id: number } | null;
    return row?.id == null ? null : asJournalEntryId(row.id);
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
  return rows[0]?.id == null ? null : asJournalEntryId(rows[0].id);
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("journal", "post", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = readJsonCliInput(ctx, input, "--input");
    const result = postJournalEntry(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("journal", "dry-run", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = readJsonCliInput(ctx, input, "--input");
    // Non-binding preview: dryRunJournalEntry rolls its transaction back, so
    // this never writes to the append-only ledger.
    const result = dryRunJournalEntry(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("journal", "reverse", (ctx) => {
    const date = ctx.arg("--date");
    const reason = ctx.arg("--reason");
    const db = openCommandDb(ctx);
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
    const db = openCommandDb(ctx);
    migrate(db);
    const rows = db
      .query(
        "SELECT id, entry_no, transaction_date, text, currency, amount_foreign, amount_dkk, fx_rate_to_dkk, document_id, source_bank_transaction_id, status, reversal_of_entry_id FROM journal_entries ORDER BY id DESC",
      )
      .all() as Array<Record<string, unknown>>;
    // The header `amount_dkk` is null for plain DKK entries — the booked
    // amount lives on the posting lines. Compute each entry's debit total
    // (which equals the credit total for a balanced entry) so the human
    // renderer can show what was actually booked instead of "—".
    const debitTotalByEntry = new Map<number, number>();
    for (const row of db
      .query(
        "SELECT journal_entry_id, SUM(debit_amount) AS debit_total FROM journal_lines GROUP BY journal_entry_id",
      )
      .all() as Array<{ journal_entry_id: number; debit_total: number }>) {
      debitTotalByEntry.set(row.journal_entry_id, Number(row.debit_total));
    }
    if (ctx.outputFormat === "json") {
      console.log(JSON.stringify(rows, null, 2));
      db.close();
      return;
    }
    console.log(`Finansposteringer (${rows.length})`);
    if (rows.length === 0) {
      console.log("Ingen finansposteringer registreret.");
    }
    for (const row of rows) {
      const status = row.status != null ? journalStatusDa(String(row.status)) : "—";
      const currency = String(row.currency ?? "DKK").toUpperCase();
      console.log("");
      console.log(`#${row.entry_no ?? row.id} — ${row.transaction_date ?? "—"}`);
      console.log(`  Tekst: ${row.text ?? "—"}`);
      const debitTotal = debitTotalByEntry.get(Number(row.id));
      const amountValue = row.amount_dkk ?? debitTotal ?? null;
      let amountLine = `  Beløb: ${formatKroner(amountValue)}`;
      if (currency !== "DKK") {
        amountLine += ` (${row.amount_foreign} ${currency}, kurs ${row.fx_rate_to_dkk})`;
      }
      console.log(amountLine);
      console.log(`  Status: ${status}`);
      if (row.document_id != null) console.log(`  Bilag: ${row.document_id}`);
      if (row.source_bank_transaction_id != null) {
        console.log(`  Kilde-banktransaktion: ${row.source_bank_transaction_id}`);
      }
      if (row.reversal_of_entry_id != null) {
        console.log(`  Tilbagefører postering: ${row.reversal_of_entry_id}`);
      }
    }
    db.close();
  });
}
