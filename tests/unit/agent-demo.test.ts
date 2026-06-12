// Tests: src/core/ledger.ts, src/core/exceptions.ts (end-to-end agent workflow demo)
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { openDb, migrate } from "../../src/core/db";
import { companyPaths } from "../../src/core/paths";
import { verifyAuditChain } from "../../src/core/ledger";
import { listExceptions } from "../../src/core/exceptions";
import { cleanupDir } from "../helpers/cleanup";

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

const RUNNER_PATH = resolve(fileURLToPath(new URL("../../examples/agent-demo/run.ts", import.meta.url)));
const DEMO_DIR = resolve(fileURLToPath(new URL("../../examples/agent-demo", import.meta.url)));

const TMP_ROOT = mkdtempSync(join(tmpdir(), "rentemester-agent-demo-test-"));
const COMPANY = join(TMP_ROOT, "company");

afterAll(() => {
  cleanupDir(TMP_ROOT);
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

  test("exception queue contains the restaurant-bon with an actionable Danish message", () => {
    const db = openDb(companyPaths(COMPANY).db);
    try {
      migrate(db);
      const result = listExceptions(db, { status: "open" });
      expect(result.ok).toBe(true);
      const rows = result.rows ?? [];
      // Restaurant + Stripe payout begge unmatched.
      expect(rows.length).toBeGreaterThanOrEqual(1);
      // The restaurant voucher surfaces with a clear, fully-Danish exception
      // (#224): it names the transaction concretely (date, amount) and gives a
      // plain-language required action — no half-English generic technical
      // code, no English command hint.
      const restaurant = rows.find(
        (row) =>
          String(row.type ?? "") === "UNMATCHED_BANK_TRANSACTION" &&
          String(row.message ?? "").includes("Restaurant Madklubben"),
      );
      expect(restaurant).toBeTruthy();
      const message = String(restaurant!.message ?? "");
      const action = String(restaurant!.requiredAction ?? "");
      expect(message).toContain("er endnu ikke bogført");
      // No English leakage in the human-facing text.
      expect(message).not.toContain("Bank transaction");
      expect(action).not.toContain("suggest-matches");
      expect(action.length).toBeGreaterThan(0);
    } finally {
      db.close();
    }
  });
});
