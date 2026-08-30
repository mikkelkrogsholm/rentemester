// Surface contract for #594. Core accounting semantics live in the dedicated
// correction tests; these checks pin the CLI's non-bypassable safety boundary.
import { describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

async function run(args: string[]) {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(), stdout: "pipe", stderr: "pipe",
  });
  return {
    exitCode: await proc.exited,
    stdout: await new Response(proc.stdout).text(),
    stderr: await new Response(proc.stderr).text(),
  };
}

describe("bank direct-payable CLI surface (#594)", () => {
  test("apply requires explicit confirmation, a stable principal, and an idempotency key before opening the ledger", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-direct-payable-cli-"));
    const company = join(root, "company");
    try {
      expect((await run(["init", "--company", company])).exitCode).toBe(0);
      const base = [
        "bank", "direct-payable-apply", "--company", company,
        "--document-id", "1", "--bank-transaction-id", "1",
        "--bill-date", "2026-01-10", "--due-date", "2026-01-10",
        "--expense-account", "3000", "--plan-hash", "0".repeat(64),
        "--reason", "synthetic review", "--actor", "user:mikkelfreltoftkrogsholm",
      ];

      const deniedActor = await run([...base.slice(0,-1), "agent:not-allowed"]);
      expect(deniedActor.exitCode).toBe(2);
      expect(deniedActor.stderr).toContain("actor_allowlist");

      const noConfirm = await run(base);
      expect(noConfirm.exitCode).toBe(2);
      expect(noConfirm.stderr).toContain("requires the exact confirmation --confirm yes");

      const noPrincipal = await run([...base, "--confirm", "yes"]);
      expect(noPrincipal.exitCode).toBe(2);
      expect(noPrincipal.stderr).toContain("requires --principal user:<id>|service-account:<id>");

      const noKey = await run([...base, "--confirm", "yes", "--principal", "user:synthetic-reviewer"]);
      expect(noKey.exitCode).toBe(2);
      expect(noKey.stderr).toContain("requires --idempotency-key <key>");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("plan is a read surface: it does not require actor, confirmation, or idempotency", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-direct-payable-plan-cli-"));
    const company = join(root, "company");
    try {
      expect((await run(["init", "--company", company])).exitCode).toBe(0);
      const result = await run([
        "bank", "direct-payable-plan", "--company", company,
        "--document-id", "1", "--bank-transaction-id", "1",
        "--bill-date", "2026-01-10", "--due-date", "2026-01-10",
        "--expense-account", "3000",
      ]);
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toBe("");
      expect(JSON.parse(result.stdout)).toMatchObject({ ok: false });
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
