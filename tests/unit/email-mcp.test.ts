// Tests: src/mcp/tools/email.ts, src/mcp/registry.ts (#180 email delivery MCP tool)
//
// Drives the registered `invoice_send_email` tool callback through an
// McpServer — the same surface the JSON-RPC tools/call path invokes — so the
// registration and the SMTP-config trust boundary are both covered.

import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { registerEmailTools } from "../../src/mcp/tools/email";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { issueInvoice } from "../../src/core/issued-invoices";

import { cleanupDir } from "../helpers/cleanup";
function harness() {
  const server = new McpServer({ name: "email-test", version: "0.0.0" });
  registerEmailTools(server);
  const tools = (server as any)._registeredTools as Record<
    string,
    { handler: (args: unknown, extra: unknown) => Promise<{ structuredContent: unknown }> }
  >;
  return {
    toolNames: () => Object.keys(tools),
    async call(name: string, args: unknown) {
      const tool = tools[name];
      if (!tool) throw new Error(`tool not registered: ${name}`);
      const result = await tool.handler(args, { signal: new AbortController().signal });
      return result.structuredContent as { ok: boolean; data?: any; errors: string[] };
    },
  };
}

function seedCompany(company: string) {
  ensureCompanyDirs(company);
  const db = openDb(join(company, "data", "ledger.sqlite"));
  migrate(db);
  const issued = issueInvoice(db, company, {
    invoiceType: "full",
    vatTreatment: "standard",
    issueDate: "2026-05-16",
    dueDate: "2026-06-15",
    invoiceNumber: "2026-0001",
    seller: { name: "Rentemester ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
    buyer: { name: "Kunde A/S", address: "Købervej 9", vatOrCvr: "DK87654321" },
    lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
    totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
    currency: "DKK",
  });
  db.close();
  expect(issued.ok).toBe(true);
}

describe("invoice_send_email MCP tool (#180)", () => {
  test("invoice_send_email is registered", () => {
    expect(harness().toolNames()).toContain("invoice_send_email");
  });

  test("requires confirm: true", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-email-mcp-confirm-"));
    const company = join(root, "company");
    seedCompany(company);
    const env = await harness().call("invoice_send_email", {
      company,
      invoiceNumber: "2026-0001",
      to: "kunde@example.test",
    });
    cleanupDir(root);
    expect(env.ok).toBe(false);
    expect(env.errors.join(" ")).toContain("confirm");
  });

  test("fails clearly when config/smtp.json is missing", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-email-mcp-noconfig-"));
    const company = join(root, "company");
    seedCompany(company);
    const env = await harness().call("invoice_send_email", {
      company,
      invoiceNumber: "2026-0001",
      to: "kunde@example.test",
      confirm: true,
    });
    cleanupDir(root);
    expect(env.ok).toBe(false);
    expect(env.errors.join(" ")).toContain("smtp");
  });

  test("sends an invoice and records the append-only send log", async () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-email-mcp-send-"));
    const company = join(root, "company");
    seedCompany(company);
    writeFileSync(
      join(company, "config", "smtp.json"),
      JSON.stringify({
        host: "smtp.example.test",
        port: 587,
        fromAddress: "faktura@rentemester.test",
        fromName: "Rentemester ApS",
        dryRun: true,
      }),
    );

    const env = await harness().call("invoice_send_email", {
      company,
      invoiceNumber: "2026-0001",
      to: "kunde@example.test",
      confirm: true,
    });
    expect(env.ok).toBe(true);
    expect(env.data?.recipient).toBe("kunde@example.test");
    expect(env.data?.kind).toBe("invoice");
    expect(env.data?.messageId).toBeDefined();

    const db = openDb(join(company, "data", "ledger.sqlite"));
    const rows = db.query("SELECT recipient, kind FROM email_send_log").all() as Array<{
      recipient: string;
      kind: string;
    }>;
    db.close();
    cleanupDir(root);
    expect(rows).toEqual([{ recipient: "kunde@example.test", kind: "invoice" }]);
  });
});
