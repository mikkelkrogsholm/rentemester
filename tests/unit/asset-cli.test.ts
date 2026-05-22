// Tests: src/cli/asset.ts, src/cli.ts (asset register / depreciate / write-off CLI, #124 + #125)
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { cleanupDir } from "../helpers/cleanup";

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

async function bootstrapCompanyWithDocument(label: string) {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  const inbox = mkdtempSync(join(tmpdir(), `rentemester-${label}-inbox-`));
  const company = join(root, "company");
  const sourceFile = join(inbox, "asset.txt");
  const metadataFile = join(root, "asset.metadata.json");
  writeFileSync(sourceFile, `Asset invoice ${label}\n`);
  writeFileSync(metadataFile, JSON.stringify({
    source: "email",
    issueDate: "2026-01-10",
    invoiceNo: `CLI-ASSET-${label}`,
    deliveryDescription: "Laptop",
    amountIncVat: 40000,
    currency: "DKK",
    sender: { name: "Hardware ApS", address: "Vej 1", vatOrCvr: "DK11223344" },
    recipient: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    vatAmount: 0,
    paymentDetails: "Bank transfer",
  }, null, 2));
  await runCli(["init", "--company", company]);
  await runCli(["documents", "ingest", "--company", company, "--file", sourceFile, "--metadata", metadataFile]);
  return {
    company,
    cleanup: () => {
      cleanupDir(root, inbox);
    },
  };
}

describe("asset CLI", () => {
  test("registers an asset, posts depreciation and shows the register report", async () => {
    const { company, cleanup } = await bootstrapCompanyWithDocument("asset-cli-depr");

    const registered = await runCli([
      "asset", "register", "--company", company,
      "--name", "MacBook", "--category", "hardware",
      "--acquisition-date", "2026-01-10", "--cost", "40000",
      "--useful-life-months", "40", "--document-id", "1",
    ]);
    expect(registered.exitCode).toBe(0);
    const regParsed = JSON.parse(registered.stdout);
    expect(regParsed.ok).toBe(true);
    expect(regParsed.totalPeriods).toBe(40);

    const depreciated = await runCli([
      "asset", "depreciate", "--company", company,
      "--asset-id", String(regParsed.assetId), "--period", "1", "--date", "2026-02-01",
    ]);
    expect(depreciated.exitCode).toBe(0);
    const deprParsed = JSON.parse(depreciated.stdout);
    expect(deprParsed.ok).toBe(true);
    expect(deprParsed.periodAmount).toBe(1000);

    const report = await runCli(["asset", "register-report", "--company", company]);
    expect(report.exitCode).toBe(0);
    const reportParsed = JSON.parse(report.stdout);
    expect(reportParsed.ok).toBe(true);
    expect(reportParsed.totals.accumulatedDepreciation).toBe(1000);
    expect(reportParsed.totals.netBookValue).toBe(39000);

    cleanup();
  });

  test("immediate write-off CLI requires the --confirm flag", async () => {
    const { company, cleanup } = await bootstrapCompanyWithDocument("asset-cli-wo");

    const withoutConfirm = await runCli([
      "asset", "write-off", "--company", company,
      "--name", "Drill", "--category", "tools",
      "--acquisition-date", "2026-01-10", "--cost", "5000",
      "--document-id", "1", "--expense-account", "3120",
      "--date", "2026-01-12", "--threshold-source", "SKAT afskrivningsloven",
    ]);
    expect(withoutConfirm.exitCode).toBe(1);
    expect(JSON.parse(withoutConfirm.stdout).ok).toBe(false);

    const confirmed = await runCli([
      "asset", "write-off", "--company", company,
      "--name", "Drill", "--category", "tools",
      "--acquisition-date", "2026-01-10", "--cost", "5000",
      "--document-id", "1", "--expense-account", "3120",
      "--date", "2026-01-12", "--threshold-source", "SKAT afskrivningsloven", "--confirm", "yes",
    ]);
    expect(confirmed.exitCode).toBe(0);
    const parsed = JSON.parse(confirmed.stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.writeOffId).toBeGreaterThan(0);

    cleanup();
  });
});
