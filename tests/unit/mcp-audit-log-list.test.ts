// Tests for the new `audit_log_list` MCP read tool — the single highest-
// converged finding from the integration-review + virksomhedsejer-review:
// without an MCP-side audit-log query an agent cannot show its own work
// back to the human user it is nominally serving.
//
// Contract (mirrors the CLI's `gdpr audit-log` shape):
//   - input: company, limit?, offset?, fromDate?, toDate?, eventTypeLike?,
//            actorLike?
//   - output: envelope.data = { total, count, limit, offset, hasMore,
//             nextOffset?, rows: AuditLogRow[] }
//   - order: created_at DESC, id DESC (newest first)
//   - read-only — no confirm gate
//
// All assertions fail BEFORE the new tool ships.

import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { insertAuditLog } from "../../src/core/actor";
import { registerAuditTools } from "../../src/mcp/tools/audit";

type StructuredEnv = {
  ok: boolean;
  data?: Record<string, unknown> & {
    rows?: Array<Record<string, unknown>>;
    total?: number;
    count?: number;
    limit?: number;
    offset?: number;
    hasMore?: boolean;
    nextOffset?: number;
  };
  errors: string[];
  code?: string;
};

type RegisteredTool = {
  handler: (args: unknown, extra: unknown) => Promise<{ structuredContent: StructuredEnv }>;
};

let companyRoot: string;
let server: McpServer;
let registered: Record<string, RegisteredTool>;

beforeAll(() => {
  companyRoot = mkdtempSync(join(tmpdir(), "rentemester-audit-list-"));
  const paths = ensureCompanyDirs(companyRoot);
  const db = openDb(paths.db);
  try {
    migrate(db);
    seedAccounts(db);
    // Seed a deterministic set of audit-log entries we can filter against.
    insertAuditLog(db, {
      eventType: "INVOICE_ISSUED",
      entityType: "invoice",
      entityId: "1001",
      message: "Faktura 1001 udstedt",
      createdBy: "user:mikkel@56n.dk",
      createdByProgram: "rentemester-cockpit",
    });
    insertAuditLog(db, {
      eventType: "INVOICE_POSTED",
      entityType: "invoice",
      entityId: "1001",
      message: "Faktura 1001 bogført",
      createdBy: "user:mikkel@56n.dk",
      createdByProgram: "rentemester-cockpit",
    });
    insertAuditLog(db, {
      eventType: "BANK_IMPORTED",
      entityType: "bank_transaction",
      entityId: null,
      message: "12 banktransaktioner importeret",
      createdBy: "agent:rentemester-agent",
      createdByProgram: "rentemester-agent",
    });
    insertAuditLog(db, {
      eventType: "EXCEPTION_RESOLVED",
      entityType: "exception",
      entityId: "42",
      message: "Undtagelse #42 løst af ejer",
      createdBy: "user:mikkel@56n.dk",
      createdByProgram: "rentemester-cockpit",
    });
  } finally {
    db.close();
  }
  server = new McpServer({ name: "audit-list-test", version: "0.0.0" });
  registerAuditTools(server);
  registered = (server as any)._registeredTools as Record<string, RegisteredTool>;
});

afterAll(() => {
  rmSync(companyRoot, { recursive: true, force: true });
});

async function call(args: Record<string, unknown>): Promise<StructuredEnv> {
  const tool = registered["audit_log_list"];
  if (!tool) throw new Error("audit_log_list is not registered");
  const res = await tool.handler(args, { signal: new AbortController().signal });
  return res.structuredContent;
}

describe("#mcp audit_log_list — exists and is read-only", () => {
  test("the tool is registered on the audit-tools register", () => {
    expect(registered["audit_log_list"]).toBeDefined();
  });

  test("returns the 4 seeded entries, newest first, with pagination metadata", async () => {
    const env = await call({ company: companyRoot });
    expect(env.ok).toBe(true);
    expect(env.data?.total).toBe(4);
    expect(env.data?.count).toBe(4);
    expect(env.data?.hasMore).toBe(false);
    expect(env.data?.rows).toHaveLength(4);
    // Order: created_at DESC, id DESC → entry inserted last (EXCEPTION_RESOLVED)
    // comes first.
    expect(env.data?.rows?.[0]?.eventType).toBe("EXCEPTION_RESOLVED");
    expect(env.data?.rows?.[3]?.eventType).toBe("INVOICE_ISSUED");
  });
});

describe("#mcp audit_log_list — pagination", () => {
  test("limit=2 caps the page; hasMore=true; nextOffset=2", async () => {
    const env = await call({ company: companyRoot, limit: 2 });
    expect(env.ok).toBe(true);
    expect(env.data?.total).toBe(4);
    expect(env.data?.count).toBe(2);
    expect(env.data?.limit).toBe(2);
    expect(env.data?.offset).toBe(0);
    expect(env.data?.hasMore).toBe(true);
    expect(env.data?.nextOffset).toBe(2);
  });

  test("offset=2 returns the second page; hasMore=false", async () => {
    const env = await call({ company: companyRoot, limit: 2, offset: 2 });
    expect(env.ok).toBe(true);
    expect(env.data?.count).toBe(2);
    expect(env.data?.offset).toBe(2);
    expect(env.data?.hasMore).toBe(false);
    // The two rows on this page are the OLDEST two of the four.
    expect(env.data?.rows?.[1]?.eventType).toBe("INVOICE_ISSUED");
  });
});

describe("#mcp audit_log_list — filters", () => {
  test("eventTypeLike substring filters case-insensitively", async () => {
    const env = await call({ company: companyRoot, eventTypeLike: "invoice" });
    expect(env.ok).toBe(true);
    expect(env.data?.total).toBe(2);
    // INVOICE_ISSUED + INVOICE_POSTED — both match.
    const types = (env.data?.rows ?? []).map((r) => r.eventType);
    expect(types).toContain("INVOICE_ISSUED");
    expect(types).toContain("INVOICE_POSTED");
  });

  test("actorLike substring filters case-insensitively", async () => {
    const env = await call({ company: companyRoot, actorLike: "agent:" });
    expect(env.ok).toBe(true);
    // Only BANK_IMPORTED was inserted with an "agent:rentemester-agent" actor;
    // the other three entries used "user:mikkel@56n.dk".
    expect(env.data?.total).toBe(1);
    expect(env.data?.rows?.[0]?.eventType).toBe("BANK_IMPORTED");
  });

  test("the two filters AND together", async () => {
    const env = await call({
      company: companyRoot,
      eventTypeLike: "INVOICE",
      actorLike: "mikkel@56n.dk",
    });
    expect(env.ok).toBe(true);
    expect(env.data?.total).toBe(2);
  });
});
