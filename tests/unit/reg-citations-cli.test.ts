// Tests: src/cli/reg.ts, src/cli.ts (regulatory citation review CLI)
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

describe("reg citations CLI", () => {
  test("prints a Markdown review with rule names, explanations and statutory text", async () => {
    const proc = runCli(["reg", "citations", "--format", "human"]);
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    expect(stdout).toContain("# Regulatory citation review");
    // A known cited rule, its name, explanation and a verbatim provision text.
    expect(stdout).toContain("## DK-DOCUMENT-STORAGE-001");
    expect(stdout).toContain("- Name: ");
    expect(stdout).toContain("- Explanation: ");
    expect(stdout).toContain("### `§ 1, stk. 1`");
    // Folded block-scalar explanations must be joined, not left as `>-`.
    expect(stdout).not.toContain("- Explanation: >-");
  });

  test("--out writes the deterministic review and re-runs byte-identically", async () => {
    const dir = mkdtempSync(join(tmpdir(), "rentemester-reg-cit-"));
    const out = join(dir, "review.md");

    const proc = runCli(["reg", "citations", "--out", out, "--format", "json"]);
    const stdout = await new Response(proc.stdout).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(0);
    expect(JSON.parse(stdout).reportPath).toBe(out);

    const review = readFileSync(out, "utf8");
    expect(review).toContain("# Regulatory citation review");

    const second = runCli(["reg", "citations", "--out", join(dir, "again.md")]);
    await second.exited;
    const again = readFileSync(join(dir, "again.md"), "utf8");
    cleanupDir(dir);
    expect(again).toBe(review);
  });
});
