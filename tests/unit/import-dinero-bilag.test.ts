// Tests: src/core/import/dinero-bilag.ts — the Dinero export's bilag
// (receipts) ingest (#196, the final piece of epic #173).
//
// A Dinero export ships the actual supporting documents:
//   - `<year>/Bilag/<year>-Bilag-<n>.{pdf,jpg,png}` — booked receipts; `<n>` is
//     the voucher number matching the `Bilag` column in `Posteringer.csv`.
//   - `Ikke-bogførte-bilag/` — receipts that were never booked.
//
// #196 ingests each cut-over-year bilag through the documents pipeline
// (SHA-256, retention, originals storage), links it to its voucher's journal
// entry through the append-only `import_document_links` table, and flags every
// unbooked receipt in the exception queue.
//
// Tests run against the synthetic fixture in examples/import-dinero/ — the real
// Dinero export is private and is never committed.
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts, verifyAuditChain } from "../../src/core/ledger";
import { resolveSource } from "../../src/core/import/source";
import { dineroParser } from "../../src/core/import/dinero";
import { runImportFromSource } from "../../src/core/import/framework";
import { ingestDineroBilag, UNBOOKED_RECEIPT_EXCEPTION } from "../../src/core/import/dinero-bilag";
import { listExceptions } from "../../src/core/exceptions";

import { cleanupDir } from "../helpers/cleanup";
const FIXTURE = join(import.meta.dir, "../../examples/import-dinero");

function freshCompany(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

describe("Dinero bilag ingest (#196)", () => {
  test("ingests every cut-over-year bilag with a SHA-256 hash", () => {
    const { root, db } = freshCompany("rentemester-bilag-hash-");
    try {
      const result = runImportFromSource(db, dineroParser, FIXTURE, {
        createdBy: "user:tester",
      });
      expect(result.ok).toBe(true);
      // Five booked bilag (1..5) in the 2025 fixture.
      expect(result.bilag!.linkedCount).toBe(5);
      const docs = db
        .query("SELECT sha256_hash, stored_path FROM documents WHERE source = 'dinero-import-bilag'")
        .all() as Array<{ sha256_hash: string; stored_path: string }>;
      // Five booked + one unbooked = six receipts ingested.
      expect(docs.length).toBe(6);
      for (const doc of docs) {
        expect(doc.sha256_hash).toMatch(/^[0-9a-f]{64}$/);
        expect(doc.stored_path.length).toBeGreaterThan(0);
      }
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("links each receipt to the journal entry its voucher was posted as", () => {
    const { root, db } = freshCompany("rentemester-bilag-link-");
    try {
      const result = runImportFromSource(db, dineroParser, FIXTURE, {
        createdBy: "user:tester",
      });
      expect(result.ok).toBe(true);

      // Every link row points a document at the journal entry of its voucher.
      const links = db
        .query(
          `SELECT l.voucher_ref, l.document_id, l.journal_entry_id, je.entry_no
             FROM import_document_links l
             JOIN journal_entries je ON je.id = l.journal_entry_id
            ORDER BY l.voucher_ref`,
        )
        .all() as Array<{ voucher_ref: string; document_id: number; journal_entry_id: number; entry_no: string }>;
      expect(links.map((l) => l.voucher_ref)).toEqual(["1", "2", "3", "4", "5"]);

      // Cross-check one link against #195's voucher -> entry mapping.
      const voucher3 = result.historicalEntriesPosted!.find((v) => v.voucherRef === "3")!;
      const link3 = links.find((l) => l.voucher_ref === "3")!;
      expect(link3.journal_entry_id).toBe(voucher3.entryId);

      // The journal entry itself was NOT mutated — append-only, document_id null.
      const entryDoc = db
        .query("SELECT document_id FROM journal_entries WHERE id = ?")
        .get(voucher3.entryId) as { document_id: number | null };
      expect(entryDoc.document_id).toBeNull();
      expect(verifyAuditChain(db).ok).toBe(true);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("re-ingesting the same export is idempotent — no duplicate docs or links", () => {
    const { root, db } = freshCompany("rentemester-bilag-idem-");
    try {
      // The first import posts the ledger AND ingests the bilag.
      const result = runImportFromSource(db, dineroParser, FIXTURE, {
        createdBy: "user:tester",
      });
      expect(result.ok).toBe(true);
      const docsAfterFirst = (
        db.query("SELECT COUNT(*) AS n FROM documents").get() as { n: number }
      ).n;
      const linksAfterFirst = (
        db.query("SELECT COUNT(*) AS n FROM import_document_links").get() as { n: number }
      ).n;
      const exAfterFirst = (
        db.query("SELECT COUNT(*) AS n FROM exceptions").get() as { n: number }
      ).n;

      // Re-run the bilag ingest directly with the SAME import result: every
      // receipt is already stored by content hash, so it is a pure no-op.
      const second = ingestDineroBilag(db, root, resolveSource(FIXTURE), result);
      expect(second.duplicates.length).toBe(6);
      expect(second.linked.length).toBe(5);

      expect(
        (db.query("SELECT COUNT(*) AS n FROM documents").get() as { n: number }).n,
      ).toBe(docsAfterFirst);
      expect(
        (db.query("SELECT COUNT(*) AS n FROM import_document_links").get() as { n: number }).n,
      ).toBe(linksAfterFirst);
      expect(
        (db.query("SELECT COUNT(*) AS n FROM exceptions").get() as { n: number }).n,
      ).toBe(exAfterFirst);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("flags each unbooked receipt with an exception", () => {
    const { root, db } = freshCompany("rentemester-bilag-unbooked-");
    try {
      const result = runImportFromSource(db, dineroParser, FIXTURE, {
        createdBy: "user:tester",
      });
      expect(result.bilag!.unbookedCount).toBe(1);

      const exceptions = listExceptions(db, { status: "open" });
      const unbooked = exceptions.rows.filter(
        (r) => r.type === UNBOOKED_RECEIPT_EXCEPTION,
      );
      expect(unbooked.length).toBe(1);
      expect(unbooked[0]!.severity).toBe("medium");
      expect(unbooked[0]!.relatedDocumentId).not.toBeNull();
      expect(unbooked[0]!.message).toContain("never booked");

      // The unbooked receipt is a real ingested document with a hash.
      const doc = db
        .query("SELECT sha256_hash FROM documents WHERE id = ?")
        .get(unbooked[0]!.relatedDocumentId) as { sha256_hash: string };
      expect(doc.sha256_hash).toMatch(/^[0-9a-f]{64}$/);
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("import_document_links is append-only", () => {
    const { root, db } = freshCompany("rentemester-bilag-append-");
    try {
      runImportFromSource(db, dineroParser, FIXTURE, { createdBy: "user:tester" });
      const link = db.query("SELECT id FROM import_document_links LIMIT 1").get() as {
        id: number;
      };
      expect(() =>
        db.run("UPDATE import_document_links SET voucher_ref = 'x' WHERE id = ?", link.id),
      ).toThrow();
      expect(() =>
        db.run("DELETE FROM import_document_links WHERE id = ?", link.id),
      ).toThrow();
    } finally {
      db.close();
      cleanupDir(root);
    }
  });

  test("a bilag whose voucher matched no entry is ingested but left unlinked", () => {
    const { root, db } = freshCompany("rentemester-bilag-unmatched-");
    try {
      const result = runImportFromSource(db, dineroParser, FIXTURE, {
        createdBy: "user:tester",
      });
      // Drop voucher 5's posted entry from the result so its bilag has no match.
      const trimmed = {
        ...result,
        historicalEntriesPosted: result.historicalEntriesPosted!.filter(
          (v) => v.voucherRef !== "5",
        ),
      };
      const bilag = ingestDineroBilag(db, root, resolveSource(FIXTURE), trimmed);
      expect(bilag.unmatched.map((u) => u.voucherRef)).toEqual(["5"]);
      // The unmatched receipt is still a stored document (re-ingest is a no-op
      // here, so it surfaces as a duplicate of the first run's ingest).
      expect(bilag.duplicates).toContain("2025/Bilag/2025-Bilag-5.pdf");
    } finally {
      db.close();
      cleanupDir(root);
    }
  });
});
