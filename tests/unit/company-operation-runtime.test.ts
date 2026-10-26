import { describe, expect, test } from "bun:test";
import type { Database } from "bun:sqlite";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { runCompanyWriteSession } from "../../src/core/company-operation";
import { successEnvelope } from "../../src/mcp/envelope";
import { withCompanyDbConfirmed } from "../../src/mcp/tool-runtime";

describe("company write operation runtime", () => {
  test("marks confirmed MCP writes for the registry to avoid a preflight session", () => {
    const server = new McpServer({ name: "company-operation-runtime", version: "0.0.0" });
    const callback = withCompanyDbConfirmed(
      server,
      "journal_post",
      () => successEnvelope({}),
    );

    expect((callback as unknown as { companyDbOpening?: string }).companyDbOpening).toBe("write");
  });

  test("opens, migrates, lock-checks and closes one database session for a write", async () => {
    let opens = 0;
    let migrations = 0;
    let lockChecks = 0;
    let closes = 0;
    const db = { close: () => { closes += 1; } } as unknown as Database;

    const result = await runCompanyWriteSession(
      { companyRoot: "/synthetic/company", checkBackupLock: true },
      (session) => {
        expect(session).toBe(db);
        return "written";
      },
      {
        openDb: () => { opens += 1; return db; },
        migrate: () => { migrations += 1; },
        evaluateBackupLock: () => { lockChecks += 1; return { locked: false } as never; },
      },
    );

    expect(result).toEqual({ kind: "completed", value: "written" });
    expect({ opens, migrations, lockChecks, closes }).toEqual({ opens: 1, migrations: 1, lockChecks: 1, closes: 1 });
  });

  test("fails closed on the backup lock without calling the mutation", async () => {
    let mutationCalls = 0;
    let closes = 0;
    const db = { close: () => { closes += 1; } } as unknown as Database;

    const result = await runCompanyWriteSession(
      { companyRoot: "/synthetic/company", checkBackupLock: true },
      () => { mutationCalls += 1; return "must not run"; },
      {
        openDb: () => db,
        migrate: () => undefined,
        evaluateBackupLock: () => ({ locked: true, reason: "overdue backup" } as never),
      },
    );

    expect(result).toEqual({ kind: "backup_locked", reason: "overdue backup" });
    expect(mutationCalls).toBe(0);
    expect(closes).toBe(1);
  });
});
