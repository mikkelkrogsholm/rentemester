// Tests: src/core/periods.ts — controlled, audit-logged period reopen (#247).
// The accounting_periods row is immutable (schema trigger forbids closed->open),
// so a reopen is an append-only `period_reopen` audit event. validateJournal-
// TransactionDate replays that lifecycle; a reopened period accepts postings
// again, and a re-close locks it once more.
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { cleanupDir } from "../helpers/cleanup";
import {
  closeAccountingPeriod,
  reopenAccountingPeriod,
  effectivePeriodState,
  validateJournalTransactionDate,
} from "../../src/core/periods";

function freshDb(prefix: string) {
  const root = mkdtempSync(join(tmpdir(), prefix));
  const db = openDb(ensureCompanyDirs(root).db);
  migrate(db);
  return { root, db };
}

describe("period reopen (#247)", () => {
  test("reopens a closed period via an append-only audit event without mutating the row", () => {
    const { root, db } = freshDb("rentemester-reopen-basic-");

    const closed = closeAccountingPeriod(db, {
      periodStart: "2026-04-01",
      periodEnd: "2026-06-30",
      kind: "vat_quarter",
      createdBy: "user:ejer",
    });
    expect(closed.ok).toBe(true);

    // While closed, a posting inside the period is blocked.
    expect(validateJournalTransactionDate(db, "2026-05-15")).toEqual([
      "transactionDate 2026-05-15 falls in closed period vat_quarter 2026-04-01..2026-06-30",
    ]);

    const reopened = reopenAccountingPeriod(db, {
      periodStart: "2026-04-01",
      periodEnd: "2026-06-30",
      kind: "vat_quarter",
      reason: "Restaurantbilag bogført for sent",
      createdBy: "user:ejer",
    });
    expect(reopened.ok).toBe(true);
    expect(reopened.effectiveStatus).toBe("open");
    expect(reopened.reopenedBy).toContain("user:ejer");
    expect(reopened.reason).toBe("Restaurantbilag bogført for sent");

    // The original row is untouched — still 'closed'.
    const row = db
      .query(`SELECT id, status FROM accounting_periods WHERE id = ?`)
      .get(closed.periodId!) as { id: number; status: string };
    expect(row.status).toBe("closed");

    // ...but the effective state is now open and postings are accepted.
    expect(effectivePeriodState(db, row.id, "closed")).toBe("open");
    expect(validateJournalTransactionDate(db, "2026-05-15")).toEqual([]);

    // The reopen is permanently in the append-only audit log, with reason.
    const audit = db
      .query(
        `SELECT event_type, message, actor FROM audit_log
          WHERE entity_type = 'accounting_period' AND event_type = 'period_reopen'`,
      )
      .get() as { event_type: string; message: string; actor: string };
    expect(audit.event_type).toBe("period_reopen");
    expect(audit.message).toContain("reason: Restaurantbilag bogført for sent");
    expect(audit.actor).toContain("user:ejer");

    db.close();
    cleanupDir(root);
  });

  test("a re-close locks the reopened period again", () => {
    const { root, db } = freshDb("rentemester-reopen-reclose-");

    closeAccountingPeriod(db, { periodStart: "2026-04-01", periodEnd: "2026-06-30", kind: "vat_quarter" });
    reopenAccountingPeriod(db, {
      periodStart: "2026-04-01",
      periodEnd: "2026-06-30",
      kind: "vat_quarter",
      reason: "Manglende postering",
      createdBy: "user:ejer",
    });
    expect(validateJournalTransactionDate(db, "2026-05-15")).toEqual([]);

    // Re-closing the SAME bounds is not an overlap conflict — it re-locks.
    const reclosed = closeAccountingPeriod(db, {
      periodStart: "2026-04-01",
      periodEnd: "2026-06-30",
      kind: "vat_quarter",
      createdBy: "user:ejer",
    });
    expect(reclosed.ok).toBe(true);
    expect(validateJournalTransactionDate(db, "2026-05-15")).toEqual([
      "transactionDate 2026-05-15 falls in closed period vat_quarter 2026-04-01..2026-06-30",
    ]);

    // Both the reopen and the re-close survive in the audit log.
    const events = db
      .query(
        `SELECT event_type FROM audit_log
          WHERE entity_type = 'accounting_period' ORDER BY id ASC`,
      )
      .all() as Array<{ event_type: string }>;
    expect(events.map((e) => e.event_type)).toEqual(["period_close", "period_reopen", "period_close"]);

    db.close();
    cleanupDir(root);
  });

  test("refuses to reopen a reported period (already submitted to the authority)", () => {
    const { root, db } = freshDb("rentemester-reopen-reported-");

    closeAccountingPeriod(db, {
      periodStart: "2026-01-01",
      periodEnd: "2026-03-31",
      kind: "vat_quarter",
      status: "reported",
      createdBy: "user:ejer",
    });
    const result = reopenAccountingPeriod(db, {
      periodStart: "2026-01-01",
      periodEnd: "2026-03-31",
      kind: "vat_quarter",
      reason: "Forsøg på at åbne en indberettet periode",
      createdBy: "user:ejer",
    });
    expect(result.ok).toBe(false);
    expect(result.errors[0]).toContain("is reported");
    expect(result.errors[0]).toContain("cannot be reopened");

    db.close();
    cleanupDir(root);
  });

  test("requires a reason and rejects unknown / already-open periods", () => {
    const { root, db } = freshDb("rentemester-reopen-guards-");

    // No reason.
    const noReason = reopenAccountingPeriod(db, {
      periodStart: "2026-04-01",
      periodEnd: "2026-06-30",
      kind: "vat_quarter",
      reason: "  ",
    });
    expect(noReason.ok).toBe(false);
    expect(noReason.errors[0]).toContain("reason is required");

    // Period does not exist.
    const missing = reopenAccountingPeriod(db, {
      periodStart: "2026-04-01",
      periodEnd: "2026-06-30",
      kind: "vat_quarter",
      reason: "Findes ikke",
    });
    expect(missing.ok).toBe(false);
    expect(missing.errors[0]).toContain("no vat_quarter period");

    // Period exists, closed, then reopened — reopening again is a no-op error.
    closeAccountingPeriod(db, { periodStart: "2026-04-01", periodEnd: "2026-06-30", kind: "vat_quarter" });
    reopenAccountingPeriod(db, {
      periodStart: "2026-04-01",
      periodEnd: "2026-06-30",
      kind: "vat_quarter",
      reason: "Første genåbning",
      createdBy: "user:ejer",
    });
    const again = reopenAccountingPeriod(db, {
      periodStart: "2026-04-01",
      periodEnd: "2026-06-30",
      kind: "vat_quarter",
      reason: "Anden genåbning",
      createdBy: "user:ejer",
    });
    expect(again.ok).toBe(false);
    expect(again.errors[0]).toContain("already open");

    db.close();
    cleanupDir(root);
  });
});
