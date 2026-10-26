// Tests: src/core/audit-log.ts verifyAuditLogIntegrity — KODE-14.
//
// audit_log is append-only (UPDATE/DELETE blocked by triggers), but unlike the
// journal it carries no hash chain or sequence guard — so a row removed via a
// privileged path (a dropped trigger, a raw sqlite3 session, a restored-from-
// tampered-file db) leaves no evidence. This adds a minimal, deterministic
// defence-in-depth check that detects removed rows:
//   1. id-gap detection — a hole in the contiguous id sequence means a middle
//      row was deleted.
//   2. journal cross-check — every posted/reversed journal entry must have its
//      matching journal_post / journal_reverse audit event; a missing one means
//      an audit row was removed (catches tail deletion that leaves no id gap).
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { postJournalEntry, seedAccounts } from "../../src/core/ledger";
import { listChangesSince, verifyAuditLogIntegrity } from "../../src/core/audit-log";

function freshDb(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  seedAccounts(db);
  return { root, db };
}

function post(db: any, date: string, text: string) {
  return postJournalEntry(db, {
    transactionDate: date,
    text,
    lines: [
      { accountNo: "2000", debitAmount: 1000 },
      { accountNo: "5000", creditAmount: 1000 },
    ],
  });
}

// Drop the append-only triggers so the test can model a privileged tamper.
function unlockAuditLog(db: any) {
  db.run("DROP TRIGGER IF EXISTS audit_log_no_update");
  db.run("DROP TRIGGER IF EXISTS audit_log_no_delete");
}

describe("audit_log tamper-evidence (KODE-14)", () => {
  test("changes-since returns only new data events with actor and stable cursor", () => {
    const { root, db } = freshDb("rentemester-changes-");
    const posted = post(db, "2026-05-15", "new data");
    expect(posted.ok).toBe(true);
    const first = listChangesSince(db);
    expect(first.events).toHaveLength(1);
    expect(first.events[0]).toMatchObject({ eventType: "journal_post" });
    expect(first.events[0]!.actor.length).toBeGreaterThan(0);
    expect(first.cursor).toBe(first.events[0]!.id);
    expect(listChangesSince(db, first.cursor)).toEqual({ events: [], cursor: first.cursor });
    db.close(); rmSync(root, { recursive: true, force: true });
  });
  test("a clean ledger passes integrity verification", () => {
    const { root, db } = freshDb("rentemester-auditint-ok-");
    post(db, "2026-05-15", "one");
    post(db, "2026-05-16", "two");

    const result = verifyAuditLogIntegrity(db);
    expect(result.ok).toBe(true);
    expect(result.errors).toEqual([]);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("detects a deleted middle row via an id gap", () => {
    const { root, db } = freshDb("rentemester-auditint-gap-");
    post(db, "2026-05-15", "one");
    post(db, "2026-05-16", "two");
    post(db, "2026-05-17", "three");

    // Remove a middle audit row, leaving a hole in the id sequence.
    const mid = db
      .query("SELECT id FROM audit_log ORDER BY id ASC LIMIT 1 OFFSET 1")
      .get() as { id: number };
    unlockAuditLog(db);
    db.run("DELETE FROM audit_log WHERE id = ?", mid.id);

    const result = verifyAuditLogIntegrity(db);
    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toMatch(/gap|missing/i);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  test("detects a removed journal_post audit row via the journal cross-check", () => {
    const { root, db } = freshDb("rentemester-auditint-tail-");
    const first = post(db, "2026-05-15", "one");
    expect(first.ok).toBe(true);

    // Remove the LAST audit row (the journal_post for the only entry). No id gap
    // remains, but a journal entry now has no matching audit event.
    const last = db.query("SELECT MAX(id) AS id FROM audit_log").get() as { id: number };
    unlockAuditLog(db);
    db.run("DELETE FROM audit_log WHERE id = ?", last.id);

    const result = verifyAuditLogIntegrity(db);
    expect(result.ok).toBe(false);
    expect(result.errors.join(" ")).toMatch(/journal|audit event/i);

    db.close();
    rmSync(root, { recursive: true, force: true });
  });
});
