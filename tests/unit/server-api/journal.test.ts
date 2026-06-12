import { describe, expect, test } from "bun:test";
import { config, get, makeWorkspace, postPnlEntry, rmSync } from "./_shared";

describe("cockpit API — journal (GET .../journal)", () => {
  test("returns posted entries for the year, each with its lines", async () => {
    const ws = makeWorkspace("jrn-live", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/journal?year=2026",
      );
      expect(res.status).toBe(200);
      const j = res.body.journal;
      expect(j.slug).toBe("acme-aps");
      expect(j.archived).toBe(false);
      expect(j.entries.length).toBe(2);
      const entry = j.entries[0];
      expect(entry).toHaveProperty("entryNo");
      expect(entry).toHaveProperty("total");
      expect(entry.lines.length).toBeGreaterThan(0);
      expect(entry.lines[0]).toHaveProperty("accountNo");
      expect(entry.lines[0]).toHaveProperty("accountName");
      expect(entry.lines[0]).toHaveProperty("debit");
      expect(entry.lines[0]).toHaveProperty("credit");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an ?account= filter limits the journal to that account's entries", async () => {
    const ws = makeWorkspace("jrn-acct", ["Acme ApS"]);
    try {
      // A sale (touches 1000/1200/2000) and a purchase (3000/4000/2000).
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const cfg = config({ workspaceRoot: ws });

      // Account 1000 only appears on the sale — exactly one entry.
      const sale = await get(
        cfg,
        "/api/companies/acme-aps/journal?year=2026&account=1000",
      );
      expect(sale.status).toBe(200);
      expect(sale.body.journal.accountFilter.accountNo).toBe("1000");
      expect(sale.body.journal.entries.length).toBe(1);
      expect(sale.body.journal.entries[0].text).toBe("Overblik salg");

      // Account 2000 (bank) is on both — two entries.
      const bank = await get(
        cfg,
        "/api/companies/acme-aps/journal?year=2026&account=2000",
      );
      expect(bank.body.journal.entries.length).toBe(2);

      // Without the filter, accountFilter is null and all entries are shown.
      const all = await get(cfg, "/api/companies/acme-aps/journal?year=2026");
      expect(all.body.journal.accountFilter).toBeNull();
      expect(all.body.journal.entries.length).toBe(2);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("#379 — each entry carries its linked documentId and documentNo so the cockpit can link from posting to bilag", async () => {
    const ws = makeWorkspace("jrn-doc", ["Acme ApS"]);
    try {
      // postPnlEntry ingester et bilag og bogfører posten med `documentId`
      // sat, så journal-endpointet skal returnere `documentId` !== null og
      // `documentNo` matching `OV-<dato>`.
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/journal?year=2026",
      );
      expect(res.status).toBe(200);
      const entries = res.body.journal.entries as Array<{
        documentId: number | null;
        documentNo: string | null;
        text: string;
      }>;
      expect(entries.length).toBe(2);
      for (const entry of entries) {
        expect(entry).toHaveProperty("documentId");
        expect(entry).toHaveProperty("documentNo");
        // postPnlEntry sætter documentId på begge posts.
        expect(entry.documentId).not.toBeNull();
      }
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("journal for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("jrn-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/journal",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
