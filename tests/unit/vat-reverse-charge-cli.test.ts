// Tests: src/cli/vat.ts, src/cli.ts (VAT reverse-charge CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { cleanupDir } from "../helpers/cleanup";
describe("EU service reverse-charge CLI", () => {
  test("posts reverse-charge purchase from input json", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-rc-cli-"));
    const company = join(root, "company");
    const server = Bun.serve({
      port: 0,
      fetch() {
        return Response.json({ valid: true, name: "EU Supplier GmbH", address: "Berlin" });
      }
    });
    const env = { ...process.env, RENTEMESTER_VIES_ENDPOINT: `http://127.0.0.1:${server.port}/check` };

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();
    await Bun.$`bun run src/cli.ts documents ingest --company ${company} --file examples/eu-service-invoice.txt --metadata examples/eu-service-invoice.metadata.json`.quiet();
    const validateProc = Bun.spawn(["bun", "run", "src/cli.ts", "customer", "validate-vat", "--company", company, "--cvr", "DE123456789"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
      env,
    });
    const validateExitCode = await validateProc.exited;
    expect(validateExitCode).toBe(0);

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "vat", "post-eu-service-purchase", "--company", company, "--input", "examples/eu-service-purchase.json"], {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
      env,
    });
    const stdout = await new Response(proc.stdout).text();
    const stderr = await new Response(proc.stderr).text();
    const exitCode = await proc.exited;

    server.stop(true);
    cleanupDir(root);
    expect({ exitCode, stderr }).toEqual({ exitCode: 0, stderr: "" });
    const parsed = JSON.parse(stdout);
    expect(parsed.ok).toBe(true);
    expect(parsed.appliedRules).toContain("DK-VAT-REVERSE-CHARGE-001");
  });
});
