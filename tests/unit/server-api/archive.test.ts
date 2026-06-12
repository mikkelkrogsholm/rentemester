import { describe, expect, test } from "bun:test";
import { config, get, makeWorkspace, rmSync, seedArchiveYear } from "./_shared";

describe("cockpit API — archive (GET .../archive/:year)", () => {
  test("returns the archived year's SaldoBalance and posting summary", async () => {
    const ws = makeWorkspace("arc-live", ["Acme ApS"]);
    try {
      seedArchiveYear(
        ws,
        "acme-aps",
        2024,
        [
          ["1000", "Omsætning", -5000],
          ["3000", "Vareforbrug", 1200],
        ],
        [
          ["1000", -5000],
          ["3000", 1200],
        ],
      );
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/archive/2024",
      );
      expect(res.status).toBe(200);
      const a = res.body.archive;
      expect(a.slug).toBe("acme-aps");
      expect(a.year).toBe("2024");
      expect(a.saldoBalance).toHaveLength(2);
      expect(a.saldoBalance[0]).toEqual({
        accountNo: "1000",
        name: "Omsætning",
        amount: -5000,
      });
      expect(a.postings.count).toBe(2);
      expect(a.postings.grossTotal).toBe(6200);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unarchived year is a safe 404", async () => {
    const ws = makeWorkspace("arc-noyear", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/archive/2099",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a malformed year in the path is a safe 400", async () => {
    const ws = makeWorkspace("arc-badyear", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/archive/20xx",
      );
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("archive for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("arc-404", ["Acme ApS"]);
    try {
      const res = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/ghost/archive/2024",
      );
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
