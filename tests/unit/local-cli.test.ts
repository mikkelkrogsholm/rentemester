// Process-level contract tests for `rentemester local start`.
// `--no-open` is intentionally used in every spawned command: tests must never
// invoke a platform browser as a side effect of verifying the launcher.

import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { createCompany } from "../../src/core/company";
import { initWorkspace, loadWorkspaceManifest } from "../../src/core/workspace";

function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

function testPort() {
  // Ask the OS for an available port instead of colliding with desktop services.
  const reservation = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: () => new Response(null, { status: 503 }) });
  const port = reservation.port;
  reservation.stop(true);
  return port;
}

async function waitForServer(url: string, deadlineMs = 5000): Promise<boolean> {
  const start = Date.now();
  while (Date.now() - start < deadlineMs) {
    try {
      if ((await fetch(url)).ok) return true;
    } catch {
      // The child has not bound its loopback socket yet.
    }
    await Bun.sleep(50);
  }
  return false;
}

function localEnv(): Record<string, string> {
  // Deliberately hostile inherited hosted settings: local start must clear them
  // itself rather than silently starting Better Auth on a desktop workspace.
  return {
    ...process.env,
    RENTEMESTER_DEPLOYMENT_PROFILE: "hosted",
    RENTEMESTER_AUTH_SECRET: "not-a-real-secret",
    RENTEMESTER_AUTH_BASE_URL: "https://example.invalid",
    RENTEMESTER_AUTH_TRUSTED_ORIGINS: "https://example.invalid",
    RENTEMESTER_DOCUMENT_SCANNER_POLICY: "required",
    RENTEMESTER_DOCUMENT_SCANNER_PROVIDER: "http-json-v1",
    RENTEMESTER_DOCUMENT_SCANNER_URL: "https://scanner.example.invalid/v1/scan",
    RENTEMESTER_DOCUMENT_SCANNER_BEARER_TOKEN: "do-not-use",
    RENTEMESTER_DOCUMENT_SCANNER_TIMEOUT_MS: "1000",
    RENTEMESTER_APP_AUTH: "required",
    RENTEMESTER_APP_TOKEN: "do-not-use",
    RENTEMESTER_WORKSPACE: "",
    RENTEMESTER_COMPANY: "",
  } as Record<string, string>;
}

describe("local start CLI", () => {
  test("opens an existing one-company workspace on loopback without hosted auth", async () => {
    const workspace = tmpRoot("local-existing");
    initWorkspace(workspace);
    createCompany(workspace, { name: "Existing Example ApS", cvr: "DK90000100" });
    const port = testPort();
    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "local", "start", "--workspace", workspace, "--port", String(port), "--no-open"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe", env: localEnv() },
    );
    try {
      const base = `http://127.0.0.1:${port}`;
      expect(await waitForServer(`${base}/api/health`)).toBe(true);
      const health = await (await fetch(`${base}/api/health`)).json() as Record<string, unknown>;
      expect(health.ok).toBe(true);
      const ready = await fetch(`${base}/api/ready`);
      expect(ready.status).toBe(200);
      expect(await ready.json()).toMatchObject({ ok: true, ready: true });
      const companies = await (await fetch(`${base}/api/companies`)).json() as { companies: Array<{ slug: string }> };
      expect(companies.companies.map((company) => company.slug)).toEqual(["existing-example-aps"]);
    } finally {
      proc.kill();
      await proc.exited;
      rmSync(workspace, { recursive: true, force: true });
    }
  }, 15000);

  test("creates a new generic company only after explicit actor and confirmation", async () => {
    const parent = tmpRoot("local-new");
    const workspace = join(parent, "workspace");
    const port = testPort();
    const proc = Bun.spawn(
      [
        "bun", "run", "src/cli.ts", "local", "start",
        "--workspace", workspace,
        "--company-name", "New Example ApS",
        "--actor", "agent:local-launch-test",
        "--confirm", "yes",
        "--port", String(port),
        "--no-open",
      ],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe", env: localEnv() },
    );
    try {
      expect(await waitForServer(`http://127.0.0.1:${port}/api/health`)).toBe(true);
      const manifest = loadWorkspaceManifest(workspace);
      expect(manifest.companies).toHaveLength(1);
      expect(manifest.companies[0]).toMatchObject({ slug: "new-example-aps", name: "New Example ApS", archived: false });
    } finally {
      proc.kill();
      await proc.exited;
      rmSync(parent, { recursive: true, force: true });
    }
  }, 15000);

  test("refuses a new workspace without confirmation before writing any files", async () => {
    const parent = tmpRoot("local-no-confirm");
    const workspace = join(parent, "workspace");
    const proc = Bun.spawn(
      [
        "bun", "run", "src/cli.ts", "local", "start",
        "--workspace", workspace,
        "--company-name", "No Confirm ApS",
        "--actor", "agent:local-launch-test",
        "--no-open",
        "--format", "json",
      ],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe", env: localEnv() },
    );
    try {
      const stdout = await new Response(proc.stdout).text();
      expect(await proc.exited).toBe(1);
      expect(JSON.parse(stdout)).toMatchObject({ ok: false });
      expect(existsSync(workspace)).toBe(false);
    } finally {
      rmSync(parent, { recursive: true, force: true });
    }
  }, 15000);

  test("refuses a new workspace without an explicit actor before writing any files", async () => {
    const parent = tmpRoot("local-no-actor");
    const workspace = join(parent, "workspace");
    const proc = Bun.spawn(
      [
        "bun", "run", "src/cli.ts", "local", "start",
        "--workspace", workspace,
        "--company-name", "No Actor ApS",
        "--confirm", "yes",
        "--no-open",
      ],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe", env: localEnv() },
    );
    try {
      const stderr = await new Response(proc.stderr).text();
      expect(await proc.exited).toBe(2);
      expect(stderr).toContain("explicit --actor");
      expect(existsSync(workspace)).toBe(false);
    } finally {
      rmSync(parent, { recursive: true, force: true });
    }
  }, 15000);

  test("refuses multi-company workspaces and directs operators to serve", async () => {
    const workspace = tmpRoot("local-multi");
    initWorkspace(workspace);
    createCompany(workspace, { name: "Alpha Example ApS" });
    createCompany(workspace, { name: "Beta Example ApS" });
    const proc = Bun.spawn(
      ["bun", "run", "src/cli.ts", "local", "start", "--workspace", workspace, "--no-open", "--format", "json"],
      { cwd: process.cwd(), stdout: "pipe", stderr: "pipe", env: localEnv() },
    );
    try {
      const stdout = await new Response(proc.stdout).text();
      expect(await proc.exited).toBe(1);
      const result = JSON.parse(stdout) as { errors: string[] };
      expect(result.errors.join(" ")).toContain("rentemester serve --workspace");
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  }, 15000);
});
