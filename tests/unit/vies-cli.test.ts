// Tests: src/cli/customer.ts, src/cli.ts (VIES validation CLI)
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { cleanupDir } from "../helpers/cleanup";
import {
  validateVatAgainstVies,
  lookupCachedViesValidation,
  requireCachedViesValidation,
} from "../../src/core/vies";

describe("#293 — the missing-VIES error is surface-neutral", () => {
  test("does not hardcode a CLI-only `rentemester ...` command", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-vies-msg-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const result = requireCachedViesValidation(db, "DE123456789", "supplier VAT");
    expect(result.ok).toBe(false);
    const message = result.errors[0] ?? "";

    // The agent calling this over MCP cannot run a CLI binary — the error
    // must not send it to one.
    expect(message).not.toContain("rentemester customer validate-vat");
    expect(message).not.toContain("--company");
    expect(message).not.toContain("--cvr");
    // It must still name the required action and the VAT number.
    expect(message).toContain("DE123456789");
    expect(message.toLowerCase()).toContain("vies");
    // Both surfaces must be discoverable: the CLI subcommand and the MCP tool.
    expect(message).toContain("customer validate-vat");
    expect(message).toContain("customer_validate_vat");

    db.close();
    cleanupDir(root);
  });
});

describe("VIES malformed-response handling", () => {
  test("does not cache an ambiguous VIES response as an authoritative invalid result (#144)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-vies-ambiguous-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    // HTTP 200 with a body lacking any recognised validity field — VIES could
    // not actually answer. This must NOT be cached as valid=0.
    const fetchImpl = (async () =>
      new Response(JSON.stringify({ status: "degraded", message: "service unavailable" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      })) as unknown as typeof fetch;

    const result = await validateVatAgainstVies(db, "DE123456789", { fetchImpl });
    expect(result.ok).toBe(false);
    expect(result.errors.length).toBeGreaterThan(0);

    // The cache must remain empty so a later genuine lookup is not blocked.
    expect(lookupCachedViesValidation(db, "DE123456789")).toBeNull();

    db.close();
    cleanupDir(root);
  });

  test("caches an unambiguous boolean VIES response (#144)", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-vies-valid-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const fetchImpl = (async () =>
      new Response(JSON.stringify({ valid: false, name: "Unknown", address: "" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      })) as unknown as typeof fetch;

    const result = await validateVatAgainstVies(db, "DE999999999", { fetchImpl });
    expect(result.ok).toBe(true);
    expect(result.validation?.valid).toBe(false);
    // An explicit boolean answer IS authoritative and is cached.
    expect(lookupCachedViesValidation(db, "DE999999999")?.valid).toBe(false);

    db.close();
    cleanupDir(root);
  });
});

describe("customer validate-vat CLI", () => {
  test("validates an EU VAT number and caches the result", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-vies-cli-"));
    const company = join(root, "company");
    const server = Bun.serve({
      port: 0,
      fetch() {
        return Response.json({ valid: true, name: "EU Kunde GmbH", address: "Berlin" });
      }
    });
    const env = { ...process.env, RENTEMESTER_VIES_ENDPOINT: `http://127.0.0.1:${server.port}/check` };

    await Bun.$`bun run src/cli.ts init --company ${company}`.quiet();

    const proc = Bun.spawn(["bun", "run", "src/cli.ts", "customer", "validate-vat", "--company", company, "--cvr", "DE123456789"], {
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
    expect(parsed.validation.normalized).toBe("DE123456789");
    expect(parsed.validation.valid).toBe(true);
  });
});
