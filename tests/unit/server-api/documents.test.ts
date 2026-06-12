import { describe, expect, test } from "bun:test";
import { config, get, makeWorkspace, postPnlEntry, rmSync } from "./_shared";

describe("cockpit API — documents (GET .../documents)", () => {
  test("returns ingested documents with their link state", async () => {
    const ws = makeWorkspace("doc-live", ["Acme ApS"]);
    try {
      // postPnlEntry ingests one minimal document for the P&L entries.
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/documents",
      );
      expect(res.status).toBe(200);
      const d = res.body.documents;
      expect(d.slug).toBe("acme-aps");
      expect(d.documents.length).toBeGreaterThan(0);
      expect(d.documents[0]).toHaveProperty("documentNo");
      expect(d.documents[0]).toHaveProperty("journalEntryNo");
      expect(d).toHaveProperty("linkedCount");
      expect(d).toHaveProperty("unlinkedCount");
      // Each row carries the linked journal entry's text + total fields, so
      // the Bilag view can show what the receipt is for (null when unlinked).
      expect(d.documents[0]).toHaveProperty("journalEntryText");
      expect(d.documents[0]).toHaveProperty("journalEntryTotal");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("documents for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("doc-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/documents",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
