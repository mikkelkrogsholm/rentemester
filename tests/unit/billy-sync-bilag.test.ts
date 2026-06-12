import { expect, test, describe } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { Database } from "bun:sqlite";
import { migrate } from "../../src/core/db";
import { postJournalEntry, seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { initialiseCompanyVolume } from "../../src/core/company";
import {
  findUnlinkedSyncEntries,
  ingestSyncBilagFile,
  linkBilagDocument,
  matchEntriesToBillyTxns,
  type BillySyncTxn,
} from "../../src/core/import/billy-sync-bilag";

function freshCompany(): { root: string; db: Database } {
  const root = mkdtempSync(join(tmpdir(), "rentemester-billy-sync-bilag-"));
  initialiseCompanyVolume(root, {});
  const db = new Database(join(root, "data", "ledger.sqlite"));
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

function writePdf(dir: string, name: string, marker: string): string {
  const path = join(dir, name);
  writeFileSync(path, new TextEncoder().encode(`%PDF-1.4\n1 0 obj\n<< >>\nendobj\n${marker}`));
  return path;
}

function postSyncExpense(db: Database, text: string, amount: number, documentId?: number) {
  const result = postJournalEntry(db, {
    transactionDate: "2026-01-15",
    text,
    createdByProgram: "billy-sync",
    importedHistorical: true,
    documentId,
    lines: [
      { accountNo: "3000", debitAmount: amount, text },
      { accountNo: "2000", creditAmount: amount, text },
    ],
  });
  expect(result.ok).toBe(true);
  return { entryId: result.entryId as unknown as number, entryNo: result.entryNo! };
}

describe("ingestSyncBilagFile", () => {
  test("ingests a new bilag file as a document", () => {
    const { root, db } = freshCompany();
    try {
      const path = writePdf(root, "owner-1__att-1.pdf", "FIRST");
      const result = ingestSyncBilagFile(db, root, path);
      expect(result.ok).toBe(true);
      expect(result.documentId).toBeGreaterThan(0);
      expect(result.documentNo).toBeTruthy();
      expect(result.deduped).toBe(false);
      db.close();
    } finally {
      try { rmSync(root, { recursive: true, force: true }); } catch {}
    }
  });

  test("dedupes on content — second ingest returns the existing document", () => {
    const { root, db } = freshCompany();
    try {
      const path1 = writePdf(root, "a.pdf", "SAME");
      const path2 = writePdf(root, "b.pdf", "SAME");
      const first = ingestSyncBilagFile(db, root, path1);
      const second = ingestSyncBilagFile(db, root, path2);
      expect(second.ok).toBe(true);
      expect(second.deduped).toBe(true);
      expect(second.documentId).toBe(first.documentId);
      db.close();
    } finally {
      try { rmSync(root, { recursive: true, force: true }); } catch {}
    }
  });

  test("rejects an empty file", () => {
    const { root, db } = freshCompany();
    try {
      const path = join(root, "empty.pdf");
      writeFileSync(path, new Uint8Array());
      const result = ingestSyncBilagFile(db, root, path);
      expect(result.ok).toBe(false);
      expect(result.errors.length).toBeGreaterThan(0);
      db.close();
    } finally {
      try { rmSync(root, { recursive: true, force: true }); } catch {}
    }
  });
});

describe("linkBilagDocument", () => {
  test("links a document to a journal entry, idempotently", () => {
    const { root, db } = freshCompany();
    try {
      const { entryId } = postSyncExpense(db, "Billy sync: Software", 100);
      const path = writePdf(root, "receipt.pdf", "LINK");
      const ingest = ingestSyncBilagFile(db, root, path);
      expect(ingest.ok).toBe(true);

      const first = linkBilagDocument(db, "txn-1", ingest.documentId!, entryId);
      const second = linkBilagDocument(db, "txn-1", ingest.documentId!, entryId);
      expect(first).toBe(true);
      expect(second).toBe(false);

      const rows = db
        .query("SELECT COUNT(*) AS n FROM import_document_links WHERE journal_entry_id = ?")
        .get(entryId) as { n: number };
      expect(rows.n).toBe(1);
      db.close();
    } finally {
      try { rmSync(root, { recursive: true, force: true }); } catch {}
    }
  });
});

describe("findUnlinkedSyncEntries", () => {
  test("returns billy-sync entries without document or link", () => {
    const { root, db } = freshCompany();
    try {
      const { entryNo } = postSyncExpense(db, "Billy sync: Uden bilag", 250);
      const entries = findUnlinkedSyncEntries(db);
      expect(entries.length).toBe(1);
      expect(entries[0]!.entryNo).toBe(entryNo);
      expect(entries[0]!.transactionDate).toBe("2026-01-15");
      expect(entries[0]!.text).toBe("Billy sync: Uden bilag");
      expect(entries[0]!.totalDebitOre).toBe(25000n);
      db.close();
    } finally {
      try { rmSync(root, { recursive: true, force: true }); } catch {}
    }
  });

  test("skips entries that carry a document or a link, and other programs", () => {
    const { root, db } = freshCompany();
    try {
      // Entry with document_id
      const docPath = writePdf(root, "withdoc.pdf", "WITHDOC");
      const ingest = ingestSyncBilagFile(db, root, docPath);
      postSyncExpense(db, "Billy sync: Med dokument", 100, ingest.documentId);

      // Entry with an import_document_links row
      const { entryId } = postSyncExpense(db, "Billy sync: Med link", 200);
      const linkPath = writePdf(root, "linked.pdf", "LINKED");
      const linkIngest = ingestSyncBilagFile(db, root, linkPath);
      linkBilagDocument(db, "txn-x", linkIngest.documentId!, entryId);

      // Entry from another program
      const other = postJournalEntry(db, {
        transactionDate: "2026-01-15",
        text: "Manuel postering",
        createdByProgram: "rentemester-import-postings",
        importedHistorical: true,
        lines: [
          { accountNo: "3000", debitAmount: 50, text: "x" },
          { accountNo: "2000", creditAmount: 50, text: "x" },
        ],
      });
      expect(other.ok).toBe(true);

      expect(findUnlinkedSyncEntries(db).length).toBe(0);
      db.close();
    } finally {
      try { rmSync(root, { recursive: true, force: true }); } catch {}
    }
  });
});

describe("matchEntriesToBillyTxns", () => {
  const entry = (entryId: number, entryNo: string, text: string, totalDebitOre: bigint) => ({
    entryId,
    entryNo,
    transactionDate: "2026-01-15",
    text,
    totalDebitOre,
  });
  const txn = (transactionId: string, text: string, totalDebitOre: bigint): BillySyncTxn => ({
    transactionId,
    entryDate: "2026-01-15",
    text,
    totalDebitOre,
  });

  test("matches an entry to its Billy transaction on date, amount, and text", () => {
    const result = matchEntriesToBillyTxns(
      [entry(1, "2026-00001", "Billy sync: Sentry", 1147n)],
      [txn("txn-a", "Sentry", 1147n), txn("txn-b", "Cloudflare", 3221n)],
    );
    expect(result.matches).toEqual([
      { entryId: 1, entryNo: "2026-00001", transactionId: "txn-a" },
    ]);
    expect(result.ambiguous).toEqual([]);
    expect(result.unmatched).toEqual([]);
  });

  test("reports ambiguity when two transactions share the same key", () => {
    const result = matchEntriesToBillyTxns(
      [entry(1, "2026-00001", "Billy sync: Software", 1000n)],
      [txn("txn-a", "Software", 1000n), txn("txn-b", "Software", 1000n)],
    );
    expect(result.matches).toEqual([]);
    expect(result.ambiguous).toEqual(["2026-00001"]);
  });

  test("does not match the same transaction to two entries", () => {
    const result = matchEntriesToBillyTxns(
      [
        entry(1, "2026-00001", "Billy sync: Software", 1000n),
        entry(2, "2026-00002", "Billy sync: Software", 1000n),
      ],
      [txn("txn-a", "Software", 1000n)],
    );
    expect(result.matches.length).toBe(1);
    expect(result.ambiguous).toEqual(["2026-00002"]);
  });

  test("reports unmatched when no transaction fits", () => {
    const result = matchEntriesToBillyTxns(
      [entry(1, "2026-00001", "Billy sync: Ukendt", 999n)],
      [txn("txn-a", "Sentry", 1147n)],
    );
    expect(result.matches).toEqual([]);
    expect(result.unmatched).toEqual(["2026-00001"]);
  });
});

describe("audit waiver for billy-sync", () => {
  test("a billy-sync expense entry without a document passes the audit chain", () => {
    const { root, db } = freshCompany();
    try {
      postSyncExpense(db, "Billy sync: Uden bilag endnu", 100);
      const audit = verifyAuditChain(db);
      const evidenceErrors = audit.errors.filter((e: string) =>
        e.includes("missing document evidence"),
      );
      expect(evidenceErrors).toEqual([]);
      db.close();
    } finally {
      try { rmSync(root, { recursive: true, force: true }); } catch {}
    }
  });
});
