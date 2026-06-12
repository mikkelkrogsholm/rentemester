/**
 * Billy incremental-sync bilag (receipt) handling.
 *
 * Supports `scripts/billy-sync.ts`: attachments downloaded from the Billy API
 * are ingested as documents and either carried as `document_id` on the journal
 * entry (when the bilag exists at post time) or linked afterwards via
 * `import_document_links` (when the bilag arrives in Billy later).
 *
 * Entries posted before this feature carry no transactionId mapping, so
 * `matchEntriesToBillyTxns` reconstructs it from date + total debit + text.
 * Ambiguous matches are reported, never guessed.
 */

import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import { ingestDocument } from "../documents";
import { toOre } from "../money";

const SYSTEM = "billy";
const SYNC_PROGRAM = "billy-sync";
export const BILLY_SYNC_BILAG_SOURCE = "billy-sync-bilag";

const ENTRY_TEXT_PREFIX = "Billy sync: ";

export type SyncBilagIngestResult = {
  ok: boolean;
  documentId?: number;
  documentNo?: string;
  deduped: boolean;
  errors: string[];
};

export type UnlinkedSyncEntry = {
  entryId: number;
  entryNo: string;
  transactionDate: string;
  text: string;
  totalDebitOre: bigint;
};

export type BillySyncTxn = {
  transactionId: string;
  entryDate: string;
  text: string;
  totalDebitOre: bigint;
};

export type SyncEntryMatchResult = {
  matches: Array<{ entryId: number; entryNo: string; transactionId: string }>;
  ambiguous: string[];
  unmatched: string[];
};

/**
 * Ingests one downloaded attachment file as a document, deduplicating on
 * content: if a document with the same SHA-256 already exists, it is reused.
 */
export function ingestSyncBilagFile(
  db: Database,
  companyRoot: string,
  filePath: string,
): SyncBilagIngestResult {
  let bytes: Uint8Array;
  try {
    bytes = readFileSync(filePath);
  } catch (error) {
    return {
      ok: false,
      deduped: false,
      errors: [error instanceof Error ? error.message : String(error)],
    };
  }
  if (bytes.length === 0) {
    return { ok: false, deduped: false, errors: [`empty file: ${filePath}`] };
  }

  const sha256 = createHash("sha256").update(bytes).digest("hex");
  const existing = db
    .query("SELECT id, document_no FROM documents WHERE sha256_hash = ?")
    .get(sha256) as { id: number; document_no: string } | null;
  if (existing) {
    return {
      ok: true,
      documentId: existing.id,
      documentNo: existing.document_no,
      deduped: true,
      errors: [],
    };
  }

  const ingest = ingestDocument(db, companyRoot, filePath, {
    source: BILLY_SYNC_BILAG_SOURCE,
    documentType: "cash_register_receipt",
  });
  if (!ingest.ok || ingest.documentId == null || !ingest.documentNo) {
    return { ok: false, deduped: false, errors: ingest.errors ?? ["ingest failed"] };
  }
  return {
    ok: true,
    documentId: ingest.documentId as unknown as number,
    documentNo: ingest.documentNo,
    deduped: false,
    errors: [],
  };
}

/**
 * Links a document to a journal entry via `import_document_links`.
 * Returns true when a new link was created, false when it already existed.
 */
export function linkBilagDocument(
  db: Database,
  transactionId: string,
  documentId: number,
  journalEntryId: number,
): boolean {
  const existing = db
    .query("SELECT id FROM import_document_links WHERE document_id = ? AND journal_entry_id = ?")
    .get(documentId, journalEntryId) as { id: number } | null;
  if (existing) return false;
  db.query(
    `INSERT INTO import_document_links (source_system, voucher_ref, document_id, journal_entry_id)
     VALUES (?, ?, ?, ?)`,
  ).run(SYSTEM, transactionId, documentId, journalEntryId);
  return true;
}

/**
 * Returns billy-sync journal entries that carry neither a `document_id` nor an
 * `import_document_links` row — the backfill candidates.
 */
export function findUnlinkedSyncEntries(db: Database): UnlinkedSyncEntry[] {
  const rows = db
    .query(
      `SELECT e.id, e.entry_no, e.transaction_date, e.text
       FROM journal_entries e
       WHERE e.created_by_program = ?
         AND e.document_id IS NULL
         AND NOT EXISTS (
           SELECT 1 FROM import_document_links l WHERE l.journal_entry_id = e.id
         )
       ORDER BY e.id`,
    )
    .all(SYNC_PROGRAM) as Array<{
    id: number;
    entry_no: string;
    transaction_date: string;
    text: string;
  }>;

  const lineQuery = db.query(
    "SELECT debit_amount FROM journal_lines WHERE journal_entry_id = ? AND debit_amount > 0",
  );

  return rows.map((row) => {
    const lines = lineQuery.all(row.id) as Array<{ debit_amount: number }>;
    let totalDebitOre = 0n;
    for (const line of lines) totalDebitOre += toOre(Number(line.debit_amount));
    return {
      entryId: row.id,
      entryNo: row.entry_no,
      transactionDate: row.transaction_date,
      text: row.text,
      totalDebitOre,
    };
  });
}

function entryKey(date: string, totalDebitOre: bigint, text: string): string {
  return `${date}|${totalDebitOre}|${text.trim()}`;
}

/**
 * Matches unlinked billy-sync entries to Billy transactions on transaction
 * date + total debit + text. The entry text is the sync's `Billy sync: {txt}`;
 * the prefix is stripped before comparison. A key shared by several
 * transactions, or a transaction already claimed by another entry, is
 * reported as ambiguous — backfill never links on a guess.
 */
export function matchEntriesToBillyTxns(
  entries: UnlinkedSyncEntry[],
  txns: BillySyncTxn[],
): SyncEntryMatchResult {
  const txnsByKey = new Map<string, string[]>();
  for (const t of txns) {
    const key = entryKey(t.entryDate, t.totalDebitOre, t.text);
    if (!txnsByKey.has(key)) txnsByKey.set(key, []);
    txnsByKey.get(key)!.push(t.transactionId);
  }

  const matches: SyncEntryMatchResult["matches"] = [];
  const ambiguous: string[] = [];
  const unmatched: string[] = [];
  const claimed = new Set<string>();

  for (const entry of entries) {
    const text = entry.text.startsWith(ENTRY_TEXT_PREFIX)
      ? entry.text.slice(ENTRY_TEXT_PREFIX.length)
      : entry.text;
    const candidates = txnsByKey.get(entryKey(entry.transactionDate, entry.totalDebitOre, text));
    if (!candidates || candidates.length === 0) {
      unmatched.push(entry.entryNo);
      continue;
    }
    const free = candidates.filter((id) => !claimed.has(id));
    if (candidates.length > 1 || free.length === 0) {
      ambiguous.push(entry.entryNo);
      continue;
    }
    claimed.add(free[0]!);
    matches.push({ entryId: entry.entryId, entryNo: entry.entryNo, transactionId: free[0]! });
  }

  return { matches, ambiguous, unmatched };
}
