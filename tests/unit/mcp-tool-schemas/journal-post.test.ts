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

describe("#204 — journal_post no longer advertises an unbacked idempotencyKey", () => {
  test("journal_post inputSchema does not contain idempotencyKey", async () => {
    const response = await client.send("tools/list");
    const tool = (response.result?.tools ?? []).find(
      (t: any) => t.name === "journal_post",
    );
    expect(tool, "journal_post not found").toBeDefined();
    const props = tool.inputSchema?.properties ?? {};
    // The field was documented as retry-safe but had no backing cache — #204
    // removed the false promise. It must not reappear in the schema.
    expect(props.idempotencyKey).toBeUndefined();
    expect(Object.keys(props).sort()).toEqual(["company", "confirm", "payload"]);
  });
});

describe("#238 — journal_post requires at least two lines", () => {
  let tools: any[];

  beforeAll(async () => {
    const response = await client.send("tools/list");
    tools = response.result?.tools ?? [];
  });

  test("journal_post lines schema declares minItems 2, matching the core", () => {
    // The core (src/core/ledger.ts) rejects any entry with fewer than two
    // lines. The MCP schema must advertise the same minimum so an agent
    // building from tools/list does not get its first posting rejected.
    const tool = tools.find((t) => t.name === "journal_post");
    expect(tool, "journal_post not found").toBeDefined();
    const lines = tool.inputSchema?.properties?.payload?.properties?.lines;
    expect(lines?.type).toBe("array");
    expect(lines?.minItems).toBe(2);
  });

  test("journal_post lines description states the debit-must-balance-credit rule", () => {
    const tool = tools.find((t) => t.name === "journal_post");
    const desc: string = (
      tool.inputSchema?.properties?.payload?.properties?.lines?.description ?? ""
    ).toLowerCase();
    expect(desc).toContain("debit");
    expect(desc).toContain("credit");
    // It must spell out the two-line minimum too.
    expect(desc).toContain("two");
  });

  test("a single-line journal_post payload is rejected before the handler", async () => {
    const response = await client.send("tools/call", {
      name: "journal_post",
      arguments: {
        company: companyRoot,
        payload: {
          transactionDate: "2026-05-18",
          text: "one line only",
          lines: [{ accountNo: "2000", debitAmount: 100 }],
        },
        confirm: true,
      },
    });
    // The min(2) schema makes the SDK reject this before the handler runs.
    const structured = response.result?.structuredContent;
    const failed =
      response.error !== undefined ||
      response.result?.isError === true ||
      structured?.ok === false;
    expect(failed).toBe(true);
  });
});
