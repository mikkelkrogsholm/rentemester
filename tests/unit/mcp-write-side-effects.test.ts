// Tests: src/mcp/tools/documents.ts (documents_ingest) and
//        src/mcp/tools/bank.ts (bank_import) side-effect contracts (#383).
//
// Issue: both tools historically had write-side-effects on the error/retry
// path that were not visible in the tool annotations or descriptions:
//
//   1. `documents_ingest` records a `DOCUMENT_INGEST_BLOCKED` exception when
//      ingest fails. Repeated retries of the SAME failing input must NOT
//      create duplicate exception rows; the contract must be idempotent on
//      `(type, filePath, reason)`.
//
//   2. `bank_import` writes the inline `csvContent` variant to a tmpdir
//      under `os.tmpdir()`. That directory must always be cleaned up — on
//      success AND on failure paths — and the description must announce
//      the side-effect.
import { describe, expect, test } from "bun:test";
import { existsSync, readdirSync, mkdtempSync, rmSync, writeFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { registerDocumentTools } from "../../src/mcp/tools/documents";
import { registerBankTools } from "../../src/mcp/tools/bank";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";

function tmpCompany(label: string): string {
  const root = mkdtempSync(join(tmpdir(), `rentemester-${label}-`));
  const paths = ensureCompanyDirs(root);
  const db = openDb(paths.db);
  migrate(db);
  db.close();
  return root;
}

function harness(register: (server: McpServer) => void) {
  const server = new McpServer({ name: "side-effects-test", version: "0.0.0" });
  register(server);
  const tools = (server as any)._registeredTools as Record<
    string,
    {
      handler: (args: unknown, extra: unknown) => Promise<{ structuredContent: unknown }>;
      description?: string;
      annotations?: Record<string, unknown>;
    }
  >;
  return {
    tools,
    async call(name: string, args: unknown) {
      const tool = tools[name];
      if (!tool) throw new Error(`tool not registered: ${name}`);
      const result = await tool.handler(args, { signal: new AbortController().signal });
      return result.structuredContent as { ok: boolean; data?: any; errors: string[] };
    },
  };
}

function countMcpBankTmpDirs(): number {
  // Surveys `os.tmpdir()` for any leftover `rentemester-mcp-bank-*` dirs.
  // Other tests in this run may create+clean them too, so we use the count
  // delta around a single call rather than an absolute count.
  return readdirSync(tmpdir()).filter((n) => n.startsWith("rentemester-mcp-bank-")).length;
}

describe("documents_ingest exception idempotence (#383)", () => {
  test("repeated failing ingest of the same filePath does NOT create duplicate exceptions", async () => {
    const company = tmpCompany("doc-idem");
    try {
      const h = harness(registerDocumentTools);
      // Missing file path -> ingest fails -> exception recorded.
      const args = {
        company,
        filePath: "/nonexistent/path/to/bilag.pdf",
        metadata: { source: "email" },
        confirm: true,
      };

      const first = await h.call("documents_ingest", args);
      expect(first.ok).toBe(false);
      const second = await h.call("documents_ingest", args);
      expect(second.ok).toBe(false);
      const third = await h.call("documents_ingest", args);
      expect(third.ok).toBe(false);

      // Inspect the exceptions table: exactly ONE open
      // DOCUMENT_INGEST_BLOCKED row for this filePath.
      const paths = ensureCompanyDirs(company);
      const db = openDb(paths.db);
      try {
        const rows = db
          .query(
            `SELECT id, message FROM exceptions
             WHERE type = 'DOCUMENT_INGEST_BLOCKED' AND status = 'open'`,
          )
          .all() as Array<{ id: number; message: string }>;
        expect(rows.length).toBe(1);
        expect(rows[0]!.message).toContain("/nonexistent/path/to/bilag.pdf");
      } finally {
        db.close();
      }
    } finally {
      rmSync(company, { recursive: true, force: true });
    }
  });

  test("description documents the exception write-side-effect on failure", () => {
    const h = harness(registerDocumentTools);
    const desc = h.tools["documents_ingest"]!.description ?? "";
    // Acceptance criterion: failure side-effects must be listed in description.
    expect(desc).toMatch(/exception/i);
    // The description must point the reader at exceptions_list so the agent
    // can discover the side-effect via the tool surface.
    expect(desc).toContain("exceptions_list");
    // The description must state retries are idempotent (no duplicate rows).
    expect(desc.toLowerCase()).toMatch(/idempot|ingen duplikat|no duplicate/);
  });
});

describe("bank_import tmpdir hygiene (#383)", () => {
  test("csvContent variant cleans up its tmpdir on success", async () => {
    const company = tmpCompany("bank-clean-ok");
    try {
      const before = countMcpBankTmpDirs();
      const h = harness(registerBankTools);
      const csv = [
        "transaction_date,booking_date,text,amount,currency,reference",
        "2026-05-16,2026-05-17,Card payment,-1250,DKK,REF-OK",
      ].join("\n");
      const env = await h.call("bank_import", {
        company,
        csvContent: csv,
        confirm: true,
      });
      expect(env.ok).toBe(true);
      const after = countMcpBankTmpDirs();
      expect(after).toBe(before);
    } finally {
      rmSync(company, { recursive: true, force: true });
    }
  });

  test("csvContent variant cleans up its tmpdir on import failure", async () => {
    const company = tmpCompany("bank-clean-fail");
    try {
      const before = countMcpBankTmpDirs();
      const h = harness(registerBankTools);
      // CSV header missing required columns — importBankCsv returns { ok: false }
      // AFTER the tmpdir is created, so this exercises the failure-cleanup path.
      const env = await h.call("bank_import", {
        company,
        csvContent: "foo,bar\n1,2\n",
        confirm: true,
      });
      expect(env.ok).toBe(false);
      const after = countMcpBankTmpDirs();
      expect(after).toBe(before);
    } finally {
      rmSync(company, { recursive: true, force: true });
    }
  });

  test("description documents the tmpdir side-effect and cleanup", () => {
    const h = harness(registerBankTools);
    const desc = h.tools["bank_import"]!.description ?? "";
    // csvContent variant writes to a tmpdir — that has to be announced.
    expect(desc.toLowerCase()).toMatch(/tmp|midlertidig/);
    // And the cleanup contract must be documented.
    expect(desc.toLowerCase()).toMatch(/slet|cleanup|removed|fjern/);
  });
});
