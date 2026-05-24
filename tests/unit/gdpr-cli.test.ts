// Tests: src/cli/gdpr.ts, src/cli.ts (GDPR export/erase CLI — #184)
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

async function runCli(args: string[]) {
  const proc = Bun.spawn(["bun", "run", "src/cli.ts", ...args], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, RENTEMESTER_ACTOR: undefined, USER: "gdpr-test" },
  });
  const stdout = await new Response(proc.stdout).text();
  const stderr = await new Response(proc.stderr).text();
  const exitCode = await proc.exited;
  return { stdout, stderr, exitCode };
}

// #248: init seeds the OS-derived actor into the allowlist. Use the same
// USER for init as runCli does for follow-on mutating commands so the
// derived actor on both calls is the same identity.
async function initCompany(company: string): Promise<void> {
  await Bun.$`bun run src/cli.ts init --company ${company}`
    .env({ ...process.env, USER: "gdpr-test" })
    .quiet();
}

describe("GDPR CLI", () => {
  test("exports a customer's personal data and then erases it once unretained", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-gdpr-cli-"));
    const company = join(root, "company");

    await initCompany(company);
    const created = await runCli([
      "customer", "create", "--company", company,
      "--name", "GDPR Kunde", "--cvr", "DK90909090",
      "--email", "gdpr-kunde@example.com", "--address", "Indsigtsvej 1, 1000 København K",
    ]);
    expect(created.exitCode).toBe(0);

    const exported = await runCli([
      "gdpr", "export", "--company", company, "--cvr", "DK90909090",
    ]);
    expect(exported.exitCode).toBe(0);
    const exportJson = JSON.parse(exported.stdout);
    expect(exportJson.ok).toBe(true);
    expect(exportJson.records.length).toBe(1);
    expect(exportJson.records[0].personalData.email).toBe("gdpr-kunde@example.com");

    // No linked bookkeeping records, so a far-future date allows erasure.
    const erased = await runCli([
      "gdpr", "erase", "--company", company, "--cvr", "DK90909090", "--as-of", "2099-01-01",
    ]);
    expect(erased.exitCode).toBe(0);
    const eraseJson = JSON.parse(erased.stdout);
    expect(eraseJson.ok).toBe(true);
    expect(eraseJson.erasedCount).toBeGreaterThan(0);
    expect(eraseJson.refusedCount).toBe(0);

    const reExported = await runCli([
      "gdpr", "export", "--company", company, "--cvr", "DK90909090", "--as-of", "2099-01-01",
    ]);
    rmSync(root, { recursive: true, force: true });
    expect(reExported.exitCode).toBe(0);
    const reExportJson = JSON.parse(reExported.stdout);
    expect(reExportJson.records[0].erased).toBe(true);
    expect(reExportJson.records[0].personalData.email).toBeNull();
  });

  test("export requires a subject identifier", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-gdpr-cli-nosubj-"));
    const company = join(root, "company");
    await initCompany(company);

    const exported = await runCli(["gdpr", "export", "--company", company]);
    rmSync(root, { recursive: true, force: true });
    expect(exported.exitCode).toBe(1);
    expect(JSON.parse(exported.stdout)).toMatchObject({ ok: false });
  });

  test("export --out writes the insight report as one JSON file", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-gdpr-cli-out-"));
    const company = join(root, "company");
    const outDir = join(root, "export");

    await initCompany(company);
    await runCli([
      "customer", "create", "--company", company,
      "--name", "Out Kunde", "--cvr", "DK11111111",
      "--email", "out@example.com",
    ]);

    const exported = await runCli([
      "gdpr", "export", "--company", company,
      "--subject", "DK11111111", "--out", outDir,
    ]);
    expect(exported.exitCode).toBe(0);
    const payload = JSON.parse(exported.stdout);
    expect(payload.outPath).toBeDefined();
    expect(existsSync(payload.outPath)).toBe(true);
    const onDisk = JSON.parse(readFileSync(payload.outPath, "utf8"));
    expect(onDisk.ok).toBe(true);
    expect(onDisk.subject.cvr).toBe("DK11111111");
    expect(onDisk.records[0].personalData.email).toBe("out@example.com");
    rmSync(root, { recursive: true, force: true });
  });

  test("forget without --after-retention-expiry is refused", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-gdpr-cli-forget-no-"));
    const company = join(root, "company");
    await initCompany(company);
    await runCli([
      "customer", "create", "--company", company,
      "--name", "Forget Kunde", "--cvr", "DK22222222",
    ]);

    const refused = await runCli([
      "gdpr", "forget", "--company", company, "--subject", "DK22222222",
    ]);
    rmSync(root, { recursive: true, force: true });
    expect(refused.exitCode).toBe(2);
    expect(refused.stderr).toContain("--after-retention-expiry");
  });

  test("forget --after-retention-expiry runs and redacts unretained personal data", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-gdpr-cli-forget-ok-"));
    const company = join(root, "company");
    await initCompany(company);
    await runCli([
      "customer", "create", "--company", company,
      "--name", "Forget Kunde", "--cvr", "DK33333333",
      "--email", "forget@example.com",
    ]);

    const forgotten = await runCli([
      "gdpr", "forget", "--company", company, "--subject", "DK33333333",
      "--as-of", "2099-01-01", "--after-retention-expiry",
    ]);
    expect(forgotten.exitCode).toBe(0);
    const payload = JSON.parse(forgotten.stdout);
    expect(payload.ok).toBe(true);
    expect(payload.erasedCount).toBeGreaterThan(0);
    expect(payload.refusedCount).toBe(0);
    rmSync(root, { recursive: true, force: true });
  });
});
