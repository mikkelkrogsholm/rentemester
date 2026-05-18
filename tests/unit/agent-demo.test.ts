import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { openDb, migrate } from "../../src/core/db";
import { companyPaths } from "../../src/core/paths";
import { verifyAuditChain } from "../../src/core/ledger";
import { listExceptions } from "../../src/core/exceptions";

/**
 * Integration-test for `examples/agent-demo/run.ts`.
 *
 * Vi kører hele demoen via Bun.spawn på en frisk midlertidig
 * virksomhedsmappe, lader scriptet exit'e, og sætter derefter
 * direkte ind i SQLite-filen og verificerer det vi forventer:
 *
 *   1. exit code 0
 *   2. audit-chain hash-verificerer
 *   3. mindst én exception er åben (restaurant + Stripe payout)
 *   4. mindst én journal-entry blev oprettet (auto-bogføring virkede)
 */

const RUNNER_PATH = new URL("../../examples/agent-demo/run.ts", import.meta.url).pathname;
const DEMO_DIR = new URL("../../examples/agent-demo", import.meta.url).pathname;

const TMP_ROOT = mkdtempSync(join(tmpdir(), "rentemester-agent-demo-test-"));
const COMPANY = join(TMP_ROOT, "company");

afterAll(() => {
  rmSync(TMP_ROOT, { recursive: true, force: true });
});

describe("examples/agent-demo/run.ts (rule-based)", () => {
  let stdoutText = "";
  let stderrText = "";
  let exitCode: number | null = null;

  beforeAll(async () => {
    const proc = Bun.spawn(
      [
        "bun",
        RUNNER_PATH,
        "--company",
        COMPANY,
        "--mode",
        "rule-based",
        "--demo-dir",
        DEMO_DIR,
      ],
      {
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    [stdoutText, stderrText] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
    ]);
    await proc.exited;
    exitCode = proc.exitCode;
    if (exitCode !== 0) {
      console.error("agent-demo stdout:\n" + stdoutText);
      console.error("agent-demo stderr:\n" + stderrText);
    }
  });

  test("exits with code 0", () => {
    expect(exitCode).toBe(0);
    expect(existsSync(COMPANY)).toBe(true);
  });

  test("prints final summary banner", () => {
    expect(stdoutText).toContain("Rentemester agent-demo, kørsel afsluttet");
    expect(stdoutText).toContain("bilag ingested");
    expect(stdoutText).toContain("udgifter bogført automatisk");
    expect(stdoutText).toContain("exception queue");
    expect(stdoutText).toContain("Audit-chain: OK");
  });

  test("audit chain verifies and journal entries exist", () => {
    const db = openDb(companyPaths(COMPANY).db);
    try {
      migrate(db);
      const audit = verifyAuditChain(db);
      expect(audit.ok).toBe(true);
      const count = audit.entries ?? 0;
      // 5 auto-bogførte expense-entries.
      expect(count).toBeGreaterThanOrEqual(5);
    } finally {
      db.close();
    }
  });

  test("exception queue contains the restaurant-bon", () => {
    const db = openDb(companyPaths(COMPANY).db);
    try {
      migrate(db);
      const result = listExceptions(db, { status: "open" });
      expect(result.ok).toBe(true);
      const rows = result.rows ?? [];
      // Restaurant + Stripe payout begge unmatched.
      expect(rows.length).toBeGreaterThanOrEqual(1);
      const restaurant = rows.find((row) =>
        String(row.message ?? "").includes("Bank transaction") &&
        String(row.type ?? "") === "UNMATCHED_BANK_TRANSACTION",
      );
      expect(restaurant).toBeTruthy();
    } finally {
      db.close();
    }
  });
});
