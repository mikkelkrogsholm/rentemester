// Tests: src/mcp/tool-runtime.ts, src/mcp/tools (typed payload schemas + confirm envelope)
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { startMcpFixture, stopMcpFixture, type StdioMcpClient } from "./_shared";

let companyRoot: string;
let client: StdioMcpClient;

beforeAll(async () => {
  ({ companyRoot, client } = await startMcpFixture());
});

afterAll(async () => {
  await stopMcpFixture(companyRoot, client);
});

describe("#295 — every write tool's description ends with a consistent write-class token", () => {
  let tools: any[];

  beforeAll(async () => {
    const response = await client.send("tools/list");
    tools = response.result?.tools ?? [];
  });

  test("no write tool's description ends with a bare 'write.'", () => {
    const offenders = tools
      .filter((t) => t.annotations?.readOnlyHint !== true)
      .filter((t) => /(^|[^-])\bwrite\.\s*$/.test((t.description ?? "").trim()))
      .map((t) => t.name);
    expect(offenders).toEqual([]);
  });

  test("every non-destructive write tool ENDS with write-reversible. or write-irreversible.", () => {
    // The class token must be the final sentence so it is reliably the last
    // thing an agent parses — not buried mid-description.
    const ENDS_WITH = /\b(write-reversible|write-irreversible)\.\s*$/;
    const missing: string[] = [];
    for (const tool of tools) {
      if (tool.annotations?.readOnlyHint === true) continue; // read tools
      if (tool.annotations?.destructiveHint === true) continue; // destructive class
      const desc: string = (tool.description ?? "").trim();
      if (!ENDS_WITH.test(desc)) missing.push(tool.name);
    }
    expect(missing).toEqual([]);
  });

  test("the class token appears exactly once per write tool description", () => {
    // No description may carry two conflicting class tokens.
    const offenders: string[] = [];
    for (const tool of tools) {
      if (tool.annotations?.readOnlyHint === true) continue;
      if (tool.annotations?.destructiveHint === true) continue;
      const matches = (tool.description ?? "").match(
        /\bwrite-(?:reversible|irreversible)\b/gi,
      );
      if ((matches?.length ?? 0) !== 1) offenders.push(tool.name);
    }
    expect(offenders).toEqual([]);
  });

  test("the previously-untokened backup + workspace tools carry an explicit class token", () => {
    const TOKEN = /\b(write-reversible|write-irreversible)\b/;
    for (const name of [
      "system_backup_archive",
      "system_backup_destination_add",
      "system_backup_destination_remove",
      "system_backup_place",
      "system_backup_confirm_placement",
      "system_backup_lock",
      "company_add",
    ]) {
      const tool = tools.find((t) => t.name === name);
      expect(tool, `tool ${name} not found`).toBeDefined();
      expect(TOKEN.test(tool.description ?? ""), `${name} has a write-class token`).toBe(
        true,
      );
    }
  });

  test("bank_import stays write-reversible and journal_post stays write-irreversible", () => {
    const bank = tools.find((t) => t.name === "bank_import");
    const journal = tools.find((t) => t.name === "journal_post");
    expect((bank?.description ?? "")).toContain("write-reversible");
    expect((journal?.description ?? "")).toContain("write-irreversible");
    // The two classes must be machine-distinguishable from the description.
    expect((journal?.description ?? "")).not.toContain("write-reversible");
  });
});
