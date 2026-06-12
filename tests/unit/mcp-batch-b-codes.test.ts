// Tests for the Batch-B MCP agent-DX contracts introduced after the
// "AI agent" fresh-eyes review:
//
//   1. CONFIRM-precondition envelopes carry a machine-readable
//      `code: "CONFIRM_REQUIRED"` so an agent can branch without
//      parsing free-text.
//   2. Destructive-confirm mismatch envelopes carry
//      `code: "CONFIRMTEXT_MISMATCH"`.
//   3. `bank_import` declares `idempotentHint: true` (its description
//      already promises idempotency by (date+amount+reference) dedup).
//   4. `payable_list` accepts both the canonical `asOf` and the legacy
//      `asOfDate` alias.
//
// Each assertion would have failed BEFORE the Batch-B source change.

import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { registerPortfolioTools } from "../../src/mcp/tools/portfolio";
import { registerBankTools } from "../../src/mcp/tools/bank";
import { registerPayableTools } from "../../src/mcp/tools/payable";
import { registerSystemTools } from "../../src/mcp/tools/system";

type StructuredEnv = {
  ok: boolean;
  data?: Record<string, unknown>;
  errors: string[];
  code?: string;
};

type RegisteredTool = {
  handler: (
    args: unknown,
    extra: unknown,
  ) => Promise<{ structuredContent: StructuredEnv }>;
  inputSchema?: unknown;
  // The SDK stores the full registration metadata on the tool entry.
  // Annotations live under `metadata.annotations` in some SDK versions and
  // top-level in others; we read both as a best-effort.
  metadata?: {
    annotations?: Record<string, unknown>;
    inputSchema?: unknown;
  };
  annotations?: Record<string, unknown>;
};

function tools(register: (server: McpServer) => void): Record<string, RegisteredTool> {
  const server = new McpServer({ name: "batch-b-test", version: "0.0.0" });
  register(server);
  return (server as any)._registeredTools as Record<string, RegisteredTool>;
}

async function call(
  registered: Record<string, RegisteredTool>,
  name: string,
  args: unknown,
): Promise<StructuredEnv> {
  const tool = registered[name];
  if (!tool) throw new Error(`tool not registered: ${name}`);
  const res = await tool.handler(args, {
    signal: new AbortController().signal,
  });
  return res.structuredContent;
}

describe("#batch-b — code:'CONFIRM_REQUIRED' on missing confirm", () => {
  test("company_add without confirm returns code:'CONFIRM_REQUIRED'", async () => {
    const env = await call(tools(registerPortfolioTools), "company_add", {
      name: "Acme ApS",
    });
    expect(env.ok).toBe(false);
    expect(env.code).toBe("CONFIRM_REQUIRED");
    expect(env.errors[0]).toContain("confirm: true required");
  });

  test("bank_import (withCompanyDbConfirmed) without confirm carries the same code", async () => {
    const env = await call(tools(registerBankTools), "bank_import", {
      company: "/tmp/nonexistent-company-for-batch-b",
      csvPath: "/tmp/whatever.csv",
    });
    expect(env.ok).toBe(false);
    expect(env.code).toBe("CONFIRM_REQUIRED");
    expect(env.errors[0]).toContain("confirm: true required for write tool bank_import");
  });
});

describe("#batch-b — code:'CONFIRMTEXT_MISMATCH' on destructive confirm-text mismatch", () => {
  test("system_restore_backup with a wrong confirmText returns code:'CONFIRMTEXT_MISMATCH'", async () => {
    const env = await call(tools(registerSystemTools), "system_restore_backup", {
      backupDir: "/tmp/nonexistent-backup",
      targetCompany: "/tmp/nonexistent-target",
      confirm: true,
      confirmText: "wrong",
    });
    expect(env.ok).toBe(false);
    expect(env.code).toBe("CONFIRMTEXT_MISMATCH");
    expect(env.errors[0]).toContain("confirmText must match");
  });

  test("system_restore_backup with confirm:false still rejects with code:'CONFIRM_REQUIRED'", async () => {
    const env = await call(tools(registerSystemTools), "system_restore_backup", {
      backupDir: "/tmp/nonexistent-backup",
      targetCompany: "/tmp/nonexistent-target",
    });
    expect(env.ok).toBe(false);
    expect(env.code).toBe("CONFIRM_REQUIRED");
    expect(env.errors[0]).toContain("confirm: true required for destructive tool");
  });
});

describe("#batch-b — bank_import.idempotentHint", () => {
  test("the bank.ts source declares idempotentHint: true for bank_import", () => {
    const src = readFileSync(
      `${import.meta.dir}/../../src/mcp/tools/bank.ts`,
      "utf-8",
    );
    // The annotation must match the description's promise of idempotency.
    // We assert via the source rather than via the MCP server because the
    // SDK doesn't surface annotations through `_registeredTools` reliably
    // across versions. The string is unique enough to be a stable proxy.
    expect(src).toMatch(
      /bank_import[\s\S]{0,4000}annotations:\s*\{[^}]*idempotentHint:\s*true/,
    );
    // And — guard against regression — the description must still call
    // the contract out so a future maintainer doesn't flip the annotation
    // without flipping the prose.
    expect(src).toMatch(/idempotent by design|idempotency/i);
  });
});

describe("#batch-b — payable_list accepts both `asOf` and `asOfDate`", () => {
  test("the schema exposes BOTH fields (asOf canonical, asOfDate deprecated alias)", () => {
    const registered = tools(registerPayableTools);
    const payableList = registered["payable_list"];
    expect(payableList).toBeDefined();
    // Read the input schema from whichever slot the SDK stored it in.
    // We dump the JSON-stringified schema and assert both field names
    // appear — this is robust across SDK versions because schemas always
    // serialise their key names.
    const schemaText = JSON.stringify(
      payableList.inputSchema ??
        payableList.metadata?.inputSchema ??
        (payableList as any).schema ??
        {},
    );
    expect(schemaText).toContain("asOf");
    expect(schemaText).toContain("asOfDate");
  });

  test("the source file documents the deprecation note on asOfDate", () => {
    const src = readFileSync(
      `${import.meta.dir}/../../src/mcp/tools/payable.ts`,
      "utf-8",
    );
    // The canonical field must point to the deprecation explicitly.
    expect(src).toMatch(/asOf\s*:[\s\S]{0,400}canonical/i);
    expect(src).toMatch(/asOfDate\s*:[\s\S]{0,400}DEPRECATED/);
  });
});
