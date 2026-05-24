// Tests: src/cli/company.ts, src/cli.ts (company add/list CLI + slug resolution)
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { companyPaths } from "../../src/core/paths";

function tmpRoot(label: string) {
  return mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
}

async function run(args: string[], env?: Record<string, string>) {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, RENTEMESTER_COMPANY: "", RENTEMESTER_WORKSPACE: "", ...env },
  });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const exitCode = await proc.exited;
  return { stdout, stderr, exitCode };
}

describe("company CLI", () => {
  test("company add creates a company volume inside the workspace", async () => {
    const ws = tmpRoot("company-cli-add");
    try {
      const res = await run([
        "company", "add", "--name", "Acme ApS", "--cvr", "DK12345678",
        // #241: bank details keep stderr clean — this test asserts company add
        // creates the volume, not the missing-bank-details advisory.
        "--bank-name", "Testbank", "--bank-reg", "1234", "--bank-account", "5678901234",
      ], {
        RENTEMESTER_WORKSPACE: ws,
      });
      expect({ exitCode: res.exitCode, stderr: res.stderr }).toEqual({ exitCode: 0, stderr: "" });
      expect(existsSync(companyPaths(join(ws, "acme-aps")).db)).toBe(true);
      expect(existsSync(join(ws, "workspace.json"))).toBe(true);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("company list reports companies in the workspace", async () => {
    const ws = tmpRoot("company-cli-list");
    try {
      await run(["company", "add", "--name", "Acme ApS"], { RENTEMESTER_WORKSPACE: ws });
      await run(["company", "add", "--name", "Beta IVS"], { RENTEMESTER_WORKSPACE: ws });
      const res = await run(["company", "list"], { RENTEMESTER_WORKSPACE: ws });
      expect(res.exitCode).toBe(0);
      expect(res.stdout).toContain("acme-aps");
      expect(res.stdout).toContain("beta-ivs");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("--company accepts a workspace slug and resolves it to the company dir", async () => {
    const ws = tmpRoot("company-cli-slug");
    try {
      await run(["company", "add", "--name", "Acme ApS"], { RENTEMESTER_WORKSPACE: ws });
      const res = await run(["system", "healthcheck", "--company", "acme-aps"], {
        RENTEMESTER_WORKSPACE: ws,
      });
      expect(res.exitCode).toBe(0);
      expect(res.stdout).toContain("OK ledger");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a raw --company path still works exactly as before (back-compat)", async () => {
    const root = tmpRoot("company-cli-rawpath");
    try {
      const company = join(root, "company");
      const initRes = await run([
        "init", "--company", company,
        // #241: bank details keep stderr clean — this test asserts the raw
        // --company path still resolves, not the missing-bank-details advisory.
        "--bank-name", "Testbank", "--bank-reg", "1234", "--bank-account", "5678901234",
      ]);
      expect({ exitCode: initRes.exitCode, stderr: initRes.stderr }).toEqual({
        exitCode: 0,
        stderr: "",
      });
      expect(existsSync(companyPaths(company).db)).toBe(true);

      const hcRes = await run(["system", "healthcheck", "--company", company]);
      expect(hcRes.exitCode).toBe(0);
      expect(hcRes.stdout).toContain("OK ledger");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("an unknown slug with no matching path fails with a clear error", async () => {
    const ws = tmpRoot("company-cli-badslug");
    try {
      await run(["company", "add", "--name", "Acme ApS"], { RENTEMESTER_WORKSPACE: ws });
      const res = await run(["system", "healthcheck", "--company", "ghost"], {
        RENTEMESTER_WORKSPACE: ws,
      });
      expect(res.exitCode).toBe(2);
      expect(res.stderr.toLowerCase()).toContain("ghost");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  // #262: company add's idempotency behaviour must be documented in --help —
  // a repeated name/slug is rejected, never overwritten.
  test("company add help documents the non-idempotent / no-overwrite behaviour", async () => {
    const res = await run(["company", "add", "--help"]);
    expect(res.exitCode).toBe(0);
    expect(res.stdout).toContain("Inputnoter:");
    expect(res.stdout).toMatch(/IKKE idempotent/);
    expect(res.stdout).toContain("a company already exists at");
  });

  test("company add rejects a repeated name without overwriting", async () => {
    const ws = tmpRoot("company-cli-dup");
    try {
      const first = await run(["company", "add", "--name", "Acme ApS"], {
        RENTEMESTER_WORKSPACE: ws,
      });
      expect(first.exitCode).toBe(0);
      const repeat = await run(["company", "add", "--name", "Acme ApS"], {
        RENTEMESTER_WORKSPACE: ws,
      });
      // The repeat is rejected; the original company is untouched.
      expect(repeat.exitCode).not.toBe(0);
      expect(`${repeat.stdout}${repeat.stderr}`).toContain("a company already exists at");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  // #267: `init` (and `init --help`) point owners at `company set-profile` to
  // set the bank/payment details they skipped. The command's handler exists,
  // but with no COMMAND_SPEC `--help` fell back to the global command index,
  // flag typos were silently accepted, and `--example` failed. The spec is the
  // command's documented contract — these tests pin it down.
  test("company set-profile --help shows per-command help, not the global index", async () => {
    const res = await run(["company", "set-profile", "--help"]);
    expect(res.exitCode).toBe(0);
    // The per-command help block — never the global index header.
    expect(res.stdout).not.toContain("Læsekommandoer (read-only");
    expect(res.stdout).toContain("Brug:");
    expect(res.stdout).toContain("company set-profile");
    expect(res.stdout).toContain("Inputnoter:");
    // Every profile flag the handler reads must be documented.
    for (const flag of [
      "--bank-name",
      "--bank-reg",
      "--bank-account",
      "--iban",
      "--name",
      "--cvr",
      "--address",
      "--postal-code",
      "--city",
      "--payment-terms",
    ]) {
      expect(res.stdout).toContain(flag);
    }
  });

  test("company set-profile rejects an unknown flag with a clear error", async () => {
    const root = tmpRoot("company-cli-setprofile-badflag");
    try {
      const company = join(root, "company");
      await run(["init", "--company", company]);
      const res = await run([
        "company",
        "set-profile",
        "--company",
        company,
        "--bank-naem",
        "Danske Bank",
      ]);
      expect(res.exitCode).toBe(2);
      expect(`${res.stdout}${res.stderr}`).toContain("Unknown flag --bank-naem");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  test("company set-profile --example prints a runnable example invocation", async () => {
    const res = await run(["company", "set-profile", "--example"]);
    expect(res.exitCode).toBe(0);
    // The example must be usable as-is: it names the command and the bank flags
    // an owner who skipped the bank account at `init` actually needs.
    expect(res.stdout).toContain("company set-profile");
    expect(res.stdout).toContain("--bank-name");
    expect(res.stdout).toContain("--bank-reg");
    expect(res.stdout).toContain("--bank-account");
  });

  test("company set-profile is discoverable in the global command index", async () => {
    const res = await run(["--help"]);
    expect(res.exitCode).toBe(0);
    expect(res.stdout).toContain("company set-profile");
  });
});
