import { describe, expect, test } from "bun:test";
import {
  config,
  get,
  issueTestInvoice,
  makeWorkspace,
  postPnlEntry,
  rmSync,
} from "./_shared";

describe("cockpit API — invoices (GET .../invoices)", () => {
  test("returns issued invoices with their status for the year", async () => {
    const ws = makeWorkspace("inv-live", ["Acme ApS"]);
    try {
      issueTestInvoice(ws, "acme-aps", "2026-03-15", 1000);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/invoices?year=2026",
      );
      expect(res.status).toBe(200);
      const inv = res.body.invoices;
      expect(inv.slug).toBe("acme-aps");
      expect(inv.selectedYear).toBe("2026");
      expect(inv.archived).toBe(false);
      expect(inv.invoices.length).toBe(1);
      expect(inv.invoices[0]).toHaveProperty("invoiceNo");
      expect(inv.invoices[0]).toHaveProperty("status");
      expect(inv.invoices[0].grossAmount).toBe(1250);
      expect(inv.totalGross).toBe(1250);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a company with no issued invoices returns an empty list", async () => {
    const ws = makeWorkspace("inv-empty", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-03-15", 1000, 400);
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/invoices?year=2026",
      );
      expect(res.status).toBe(200);
      expect(res.body.invoices.invoices).toEqual([]);
      expect(res.body.invoices.totalGross).toBe(0);
      expect(res.body.invoices.overdueCount).toBe(0);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("invoices for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("inv-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/invoices",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
