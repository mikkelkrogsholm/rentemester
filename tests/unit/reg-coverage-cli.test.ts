// Tests: src/cli/reg.ts, src/cli.ts (regulatory coverage CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
function runCli(args: string[]) {
  return Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
  });
}

describe("reg coverage CLI", () => {
  test("reports regulatory coverage as JSON without a company", async () => {
    const proc = runCli(["reg", "coverage", "--format", "json"]);
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.closureErrors).toBe(0);
    expect(parsed.driftErrors).toBe(0);
    expect(parsed.scopeErrors).toBe(0);
    expect(typeof parsed.operativeProvisions).toBe("number");
    expect(parsed.operativeProvisions).toBeGreaterThan(0);
    expect(parsed.citedProvisions).toBeLessThanOrEqual(parsed.operativeProvisions);
    // The headline metric narrows the denominator to in-scope provisions.
    expect(parsed.inScopeOperativeProvisions).toBeGreaterThan(0);
    expect(parsed.inScopeOperativeProvisions).toBeLessThan(parsed.operativeProvisions);
    expect(parsed.inScopeCitedProvisions).toBeLessThanOrEqual(
      parsed.inScopeOperativeProvisions,
    );
    expect(Array.isArray(parsed.perSource)).toBe(true);
  });

  test("renders a deterministic human summary", async () => {
    const proc = runCli(["reg", "coverage", "--format", "human"]);
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;

    expect(exitCode).toBe(0);
    expect(stdout).toContain("reg coverage");
    expect(stdout).toContain("In-scope provisions cited:");
    expect(stdout).toContain("Corpus-wide (incl. out of scope):");
    expect(stdout).toContain("Closure errors: 0");
    expect(stdout).toContain("Scope errors: 0");
    expect(stdout).toContain("Per source");
  });

  test("--out writes the deterministic Markdown report", async () => {
    const dir = mkdtempSync(join(tmpdir(), "rentemester-reg-cli-"));
    const out = join(dir, "report.md");

    const proc = runCli(["reg", "coverage", "--out", out, "--format", "json"]);
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    expect(JSON.parse(stdout).reportPath).toBe(out);

    const report = readFileSync(out, "utf8");
    expect(report).toContain("# Regulatory coverage");
    expect(report).toContain("## Coverage per source");

    // Re-running must produce a byte-identical report.
    const second = runCli(["reg", "coverage", "--out", join(dir, "again.md")]);
    await second.exited;
    const again = readFileSync(join(dir, "again.md"), "utf8");
    cleanupDir(dir);
    expect(again).toBe(report);
  });
});
