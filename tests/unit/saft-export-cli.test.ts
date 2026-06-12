// Tests: src/cli/system.ts, src/cli.ts (SAF-T export CLI)
import { describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

describe("SAF-T export CLI", () => {
  test("exports a deterministic first-slice SAF-T package", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-saft-export-cli-"));
    const company = join(root, "company");
    const outDir = join(root, "exports");

    await Bun.$`bun run src/cli.ts init --company ${company} --cvr DK12345678`.quiet();
    await Bun.$`bun run src/cli.ts invoice issue --company ${company} --input examples/full-invoice.dk.json`.quiet();
    await Bun.$`bun run src/cli.ts invoice post --company ${company} --invoice-number 2026-0001`.quiet();

    const proc = Bun.spawn([
      "bun", "run", "src/cli.ts", "system", "export-saft",
      "--company", company,
      "--from", "2026-05-01",
      "--to", "2026-05-31",
      "--out", outDir,
      "--generated-at", "2026-05-17T02:24:00.000Z"
    ], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    });

    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(existsSync(parsed.manifestPath)).toBe(true);
    expect(existsSync(parsed.saftXmlPath)).toBe(true);

    const manifest = JSON.parse(readFileSync(parsed.manifestPath, "utf8"));
    expect(manifest.packageType).toBe("saft_export");
    // ===== Second slice profile/version regression (#127) =====
    expect(manifest.profileId).toBe("rentemester-dk-saft-v3-ledger-sales-purchases-masterfiles");
    expect(manifest.counts.salesInvoices).toBe(1);
    expect(manifest.counts.purchaseInvoices).toBe(0);
    expect(manifest.counts).toHaveProperty("vatSummaryCodes");

    rmSync(root, { recursive: true, force: true });
  });
});
