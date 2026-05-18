import { describe, expect, test } from "bun:test";
import { mkdtempSync, existsSync, statSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

async function runCli(args: string[]) {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
  });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const exitCode = await proc.exited;
  return { stdout, stderr, exitCode };
}

describe("dashboard CLI", () => {
  test("renders a dashboard HTML file from an initialized company", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-dashboard-cli-"));
    const company = join(root, "company");
    const outPath = join(root, "dashboard.html");

    const init = await runCli(["init", "--company", company, "--cvr", "12345678"]);
    expect({ exitCode: init.exitCode, stderr: init.stderr }).toEqual({ exitCode: 0, stderr: "" });

    const dash = await runCli([
      "dashboard",
      "--company", company,
      "--out", outPath,
      "--as-of", "2026-05-17",
    ]);
    expect({ exitCode: dash.exitCode, stderr: dash.stderr }).toEqual({ exitCode: 0, stderr: "" });

    expect(existsSync(outPath)).toBe(true);
    const size = statSync(outPath).size;
    expect(size).toBeGreaterThan(1024);

    const html = readFileSync(outPath, "utf8");
    expect(html.startsWith("<!DOCTYPE html>")).toBe(true);
    expect(html).toContain('<html lang="da">');
    expect(html).toContain("Rentemester company");
    expect(html).toContain("CVR DK12345678");
    expect(html).toContain('<header class="header">');
    expect(html).toContain('<section class="metrics">');
    expect(html).toContain("Næste deadline");
    expect(html).toContain("Åbne fakturaer");
    expect(html).toContain("Seneste aktivitet");
    expect(html).toContain("Backup-status");
    expect(html).toContain("Audit-chain");
    expect(html.trimEnd().endsWith("</html>")).toBe(true);

    rmSync(root, { recursive: true, force: true });
  });

  test("errors when --out is missing", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-dashboard-cli-noout-"));
    const company = join(root, "company");
    const init = await runCli(["init", "--company", company]);
    expect(init.exitCode).toBe(0);

    const dash = await runCli([
      "dashboard",
      "--company", company,
      "--as-of", "2026-05-17",
    ]);
    expect(dash.exitCode).toBe(2);
    expect(dash.stderr).toContain("--out");

    rmSync(root, { recursive: true, force: true });
  });

  test("errors on invalid --as-of format", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-dashboard-cli-asof-"));
    const company = join(root, "company");
    const out = join(root, "dashboard.html");
    const init = await runCli(["init", "--company", company]);
    expect(init.exitCode).toBe(0);

    const dash = await runCli([
      "dashboard",
      "--company", company,
      "--out", out,
      "--as-of", "17-05-2026",
    ]);
    expect(dash.exitCode).toBe(2);
    expect(dash.stderr).toContain("YYYY-MM-DD");

    rmSync(root, { recursive: true, force: true });
  });

  test("render wall-clock under 100ms for fresh company", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-dashboard-cli-timing-"));
    const company = join(root, "company");
    const out = join(root, "dashboard.html");
    const init = await runCli(["init", "--company", company]);
    expect(init.exitCode).toBe(0);

    const dash = await runCli([
      "dashboard",
      "--company", company,
      "--out", out,
      "--as-of", "2026-05-17",
      "--format", "json",
    ]);
    expect(dash.exitCode).toBe(0);

    // CLI emits a JSON result via emitResult; extract renderMs from the JSON.
    // JSON output is wrapped in human-readable header; we grep for the value.
    const match = /"renderMs"\s*:\s*([0-9.]+)/.exec(dash.stdout);
    expect(match, `renderMs missing in CLI output:\n${dash.stdout}`).not.toBeNull();
    const renderMs = Number(match![1]);
    expect(renderMs).toBeLessThan(100);

    rmSync(root, { recursive: true, force: true });
  });
});
