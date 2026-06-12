// Tests: src/cli/serve.ts — the `rentemester serve` command boots Bun.serve
// on the config-driven bind address and serves the workspace API.
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createCompany } from "../../src/core/company";
import { initWorkspace } from "../../src/core/workspace";

import { cleanupDir } from "../helpers/cleanup";
function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

/** Polls `url` until it answers (or the deadline lapses). */
async function waitForServer(url: string, deadlineMs = 5000): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < deadlineMs) {
    try {
      const res = await fetch(url);
      if (res.ok) return true;
    } catch {
      // not up yet
    }
    await Bun.sleep(50);
  }
  return false;
}

describe("serve CLI", () => {
  test("serve boots the API on a config-driven port and serves the workspace", async () => {
    const ws = tmpRoot("serve-cli");
    initWorkspace(ws);
    createCompany(ws, { name: "Acme ApS" });
    // Port 0 lets the OS pick a free port deterministically per run.
    const port = 4400 + Math.floor(Math.random() * 200);
    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "serve", "--port", String(port)],
      {
        cwd: process.cwd(),
        stdout: "pipe",
        stderr: "pipe",
        env: { ...process.env, RENTEMESTER_WORKSPACE: ws, RENTEMESTER_COMPANY: "" },
      },
    );
    try {
      const base = `http://127.0.0.1:${port}`;
      const up = await waitForServer(`${base}/api/health`);
      expect(up).toBe(true);

      const health = await (await fetch(`${base}/api/health`)).json();
      expect(health.ok).toBe(true);
      expect(health.service).toBe("rentemester-cockpit");

      const companies = await (await fetch(`${base}/api/companies`)).json();
      expect(companies.companies.map((c: any) => c.slug)).toContain("acme-aps");
    } finally {
      proc.kill();
      await proc.exited;
      cleanupDir(ws);
    }
  }, 15000);

  test("serve fails clearly when no workspace is configured", async () => {
    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "serve"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
      env: { ...process.env, RENTEMESTER_WORKSPACE: "", RENTEMESTER_COMPANY: "" },
    });
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;
    expect(exitCode).toBe(2);
    expect(stderr.toLowerCase()).toContain("workspace");
  }, 15000);
});
