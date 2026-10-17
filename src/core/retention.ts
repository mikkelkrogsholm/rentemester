import type { Database } from "bun:sqlite";
import { getCompanySettings } from "./company";
import { currentUtcIsoDate } from "./sequences";
import { isValidIsoDate } from "./dates";

export type RetentionStatusRow = {
  table: "documents" | "journal_entries" | "bank_transactions";
  total: number;
  expired: number;
  nextExpiry: string | null;
  oldestExpired: string | null;
};

export type RetentionStatusReport = {
  ok: boolean;
  asOf: string;
  appliedRules: string[];
  rows: RetentionStatusRow[];
  errors: string[];
};

const RULE_ID = "DK-BOOKKEEPING-RETENTION-001";

export function retainUntilForDate(db: Database, dateText: string) {
  if (!isValidIsoDate(dateText)) throw new Error(`invalid ISO date: ${dateText}`);
  const basis = dateText.trim();
  const company = getCompanySettings(db);
  const startMonth = company.fiscalYearStartMonth;
  const year = Number(basis.slice(0, 4));
  const month = Number(basis.slice(5, 7));
  const targetYear = year + (startMonth !== 1 && month >= startMonth ? 1 : 0) + 5;
  if (targetYear > 9999) throw new Error("retention deadline exceeds YYYY-MM-DD range");
  const endMonth = startMonth === 1 ? 12 : startMonth - 1;
  // Compute the last day in the TARGET year: adding five to a leap-day string
  // can otherwise create an impossible deadline. setUTCFullYear avoids the
  // Date.UTC remapping of years 0–99 to 1900–1999.
  const end = new Date(0);
  end.setUTCFullYear(targetYear, endMonth, 0);
  return end.toISOString().slice(0, 10);
}

export function effectiveRetainUntil(db: Database, retainUntil: string | null | undefined, basisDate: string | null | undefined) {
  if (retainUntil && isValidIsoDate(retainUntil)) return retainUntil;
  if (basisDate && isValidIsoDate(basisDate)) return retainUntilForDate(db, basisDate);
  return null;
}

function backfillTableRetention(db: Database, table: "documents" | "journal_entries" | "bank_transactions") {
  if (table === "documents") {
    const rows = db.query(`SELECT CAST(id AS TEXT) AS id, COALESCE(invoice_date, substr(upload_datetime, 1, 10)) AS basis_date FROM documents WHERE retain_until IS NULL`).all() as Array<{ id: string; basis_date: string | null }>;
    const update = db.prepare("UPDATE documents SET retain_until = ? WHERE id = ?");
    for (const row of rows) {
      if (!row.basis_date || !isValidIsoDate(row.basis_date)) continue;
      update.run(retainUntilForDate(db, row.basis_date), BigInt(row.id));
    }
  }
  if (table === "journal_entries") {
    return;
  }
  if (table === "bank_transactions") {
    const rows = db.query(`SELECT CAST(id AS TEXT) AS id, COALESCE(booking_date, transaction_date) AS basis_date FROM bank_transactions WHERE retain_until IS NULL`).all() as Array<{ id: string; basis_date: string | null }>;
    const update = db.prepare("UPDATE bank_transactions SET retain_until = ? WHERE id = ?");
    for (const row of rows) {
      if (!row.basis_date || !isValidIsoDate(row.basis_date)) continue;
      update.run(retainUntilForDate(db, row.basis_date), BigInt(row.id));
    }
  }
}

export function backfillRetentionDeadlines(db: Database) {
  backfillTableRetention(db, "documents");
  backfillTableRetention(db, "journal_entries");
  backfillTableRetention(db, "bank_transactions");
}

export function buildRetentionStatusReport(db: Database, asOf = currentUtcIsoDate(db)): RetentionStatusReport {
  if (!isValidIsoDate(asOf)) return { ok: false, asOf, appliedRules: [RULE_ID], rows: [], errors: ["asOf must be YYYY-MM-DD"] };

  const summarize = (table: RetentionStatusRow["table"]) => {
    const rows = table === "documents"
      ? db.query(`SELECT retain_until, COALESCE(invoice_date, substr(upload_datetime, 1, 10)) AS basis_date FROM documents`).all() as Array<{ retain_until: string | null; basis_date: string | null }>
      : table === "journal_entries"
        ? db.query(`SELECT retain_until, transaction_date AS basis_date FROM journal_entries`).all() as Array<{ retain_until: string | null; basis_date: string | null }>
        : db.query(`SELECT retain_until, COALESCE(booking_date, transaction_date) AS basis_date FROM bank_transactions`).all() as Array<{ retain_until: string | null; basis_date: string | null }>;

    const effective = rows
      .map((row) => effectiveRetainUntil(db, row.retain_until, row.basis_date))
      .filter((value): value is string => Boolean(value));
    const future = effective.filter((value) => value >= asOf).sort();
    const expired = effective.filter((value) => value < asOf).sort();
    return {
      table,
      total: rows.length,
      expired: expired.length,
      nextExpiry: future[0] ?? null,
      oldestExpired: expired[0] ?? null,
    } satisfies RetentionStatusRow;
  };

  return {
    ok: true,
    asOf,
    appliedRules: [RULE_ID],
    rows: [summarize("documents"), summarize("journal_entries"), summarize("bank_transactions")],
    errors: [],
  };
}
