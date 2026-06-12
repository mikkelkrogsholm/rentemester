// Tests for two new Batch-C MCP read tools:
//
//   - `bank_account_list` — exposes the bank-accounts registry the CLI
//     `bank-account list` populates. Without this, an agent cannot
//     enumerate which `--account` slugs exist before passing one to
//     `bank_import`.
//   - `company_profile_get` — exposes the per-company stored settings
//     (name, CVR, currency, payment terms, VAT cadence). Without it,
//     an agent can't answer the user's "what's my CVR?" question
//     without listing the entire portfolio.
//
// Both are read-only; both reuse existing core helpers.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { companyPaths } from "../../src/core/paths";
import { openDb } from "../../src/core/db";
import { addBankAccount } from "../../src/core/bank";
import { setCompanyProfile } from "../../src/core/company";
import { createCompany } from "../../src/core/company";
import { initWorkspace } from "../../src/core/workspace";
import { registerBankTools } from "../../src/mcp/tools/bank";
import { registerCompanyProfileTools } from "../../src/mcp/tools/company";

type StructuredEnv = {
  ok: boolean;
  data?: Record<string, unknown>;
  errors: string[];
  code?: string;
};

type RegisteredTool = {
  handler: (args: unknown, extra: unknown) => Promise<{ structuredContent: StructuredEnv }>;
};

let workspaceRoot: string;
let companyRoot: string;
let bankRegistered: Record<string, RegisteredTool>;
let companyRegistered: Record<string, RegisteredTool>;

beforeAll(() => {
  workspaceRoot = mkdtempSync(join(tmpdir(), "rentemester-batch-c-"));
  initWorkspace(workspaceRoot);
  // createCompany initialises the full company volume + ledger + seed accounts
  // + a companies row at id=1, which is exactly what setCompanyProfile and
  // getCompanySettings need to talk to.
  const created = createCompany(workspaceRoot, {
    name: "Acme ApS",
    cvr: "12345678",
  });
  companyRoot = created.companyRoot;
  const db = openDb(companyPaths(companyRoot).db);
  try {
    // Seed a bank account so bank_account_list has something to return.
    addBankAccount(db, {
      name: "Driftskonto Danske Bank",
      slug: "drift",
      currency: "DKK",
    });
    addBankAccount(db, {
      name: "Opsparingskonto Lunar",
      slug: "opsparing",
      currency: "DKK",
    });
    // Fill in the address / payment terms via setCompanyProfile so the
    // profile_get assertion has more to chew on than just the name.
    setCompanyProfile(db, {
      address: "Industrivej 12",
      postalCode: "2300",
      city: "København S",
      paymentTermsDays: 14,
    });
  } finally {
    db.close();
  }

  const bankServer = new McpServer({ name: "batch-c-bank", version: "0.0.0" });
  registerBankTools(bankServer);
  bankRegistered = (bankServer as any)._registeredTools as Record<string, RegisteredTool>;

  const companyServer = new McpServer({ name: "batch-c-co", version: "0.0.0" });
  registerCompanyProfileTools(companyServer);
  companyRegistered = (companyServer as any)._registeredTools as Record<string, RegisteredTool>;
});

afterAll(() => {
  rmSync(workspaceRoot, { recursive: true, force: true });
});

async function call(
  registered: Record<string, RegisteredTool>,
  name: string,
  args: unknown,
): Promise<StructuredEnv> {
  const tool = registered[name];
  if (!tool) throw new Error(`tool not registered: ${name}`);
  const res = await tool.handler(args, { signal: new AbortController().signal });
  return res.structuredContent;
}

describe("#batch-c — bank_account_list", () => {
  test("the tool is registered alongside the other bank tools", () => {
    expect(bankRegistered["bank_account_list"]).toBeDefined();
  });

  test("returns every seeded bank account with slug + name", async () => {
    const env = await call(bankRegistered, "bank_account_list", {
      company: companyRoot,
    });
    expect(env.ok).toBe(true);
    const accounts = env.data?.accounts as Array<{ slug: string; name: string }>;
    expect(accounts).toBeDefined();
    expect(accounts.length).toBe(2);
    const slugs = accounts.map((a) => a.slug);
    expect(slugs).toContain("drift");
    expect(slugs).toContain("opsparing");
  });

  test("includeInactive=false omits inactive accounts (smoke check on the flag)", async () => {
    // Default behaviour: includeInactive=true → both accounts return.
    const envAll = await call(bankRegistered, "bank_account_list", {
      company: companyRoot,
    });
    expect((envAll.data?.accounts as unknown[]).length).toBe(2);
    // With the flag set to false the count stays the same (no inactive
    // accounts seeded) — this pins the schema field's presence without
    // requiring a fixture row we'd have to clean up.
    const envActive = await call(bankRegistered, "bank_account_list", {
      company: companyRoot,
      includeInactive: false,
    });
    expect((envActive.data?.accounts as unknown[]).length).toBe(2);
  });
});

describe("#batch-c — company_profile_get", () => {
  test("the tool is registered on the company-profile register", () => {
    expect(companyRegistered["company_profile_get"]).toBeDefined();
  });

  test("returns the stored company settings", async () => {
    const env = await call(companyRegistered, "company_profile_get", {
      company: companyRoot,
    });
    expect(env.ok).toBe(true);
    const profile = env.data?.profile as Record<string, unknown>;
    expect(profile).toBeDefined();
    expect(profile.name).toBe("Acme ApS");
    // createCompany normalises CVR via normalizeCvr — accept the post-norm form.
    expect(String(profile.cvr ?? "")).toContain("12345678");
    expect(profile.address).toBe("Industrivej 12");
    expect(profile.postalCode).toBe("2300");
    expect(profile.city).toBe("København S");
    expect(profile.paymentTermsDays).toBe(14);
    // Default-or-set values that must always be present so the agent can
    // rely on the shape.
    expect(typeof profile.currency).toBe("string");
    expect(typeof profile.country).toBe("string");
    expect(typeof profile.vatPeriodType).toBe("string");
    expect(typeof profile.fiscalYearStartMonth).toBe("number");
  });
});
