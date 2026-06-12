#!/usr/bin/env bun
import { Database } from "bun:sqlite";
import { writeFileSync } from "node:fs";

const company = process.argv[2] || "C:/Users/mads_/Documents/rentemester-mahope";
const db = new Database(`${company}/data/ledger.sqlite`);
const rows = db.query(
  `SELECT id, entry_no, transaction_date, text FROM journal_entries WHERE text LIKE 'Import: bilag %' ORDER BY id`,
).all() as Array<{ id: number; entry_no: string; transaction_date: string; text: string }>;

const posted = rows.map((r) => {
  const match = r.text.match(/^Import: bilag (\S+)/);
  return {
    voucherRef: match ? match[1] : "",
    entryId: r.id,
    entryNo: r.entry_no,
    transactionDate: r.transaction_date,
  };
});

const result = {
  ok: true,
  sourceSystem: "billy",
  historicalEntriesPosted: posted,
  openingBalanceLineCount: 13,
  historicalEntriesSkipped: 0,
  auditTrail: [],
  appliedRules: [],
  errors: [],
};

const outPath = `${company}/import-result.json`;
writeFileSync(outPath, JSON.stringify(result, null, 2));
console.log(`Wrote ${posted.length} entries to ${outPath}`);
db.close();
