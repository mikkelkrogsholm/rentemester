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

describe("#276 — period_close warns that reported is irreversible", () => {
  test("period_close.status states reported cannot be reopened", async () => {
    const response = await client.send("tools/list");
    const tool = (response.result?.tools ?? []).find(
      (t: any) => t.name === "period_close",
    );
    expect(tool, "period_close not found").toBeDefined();
    const desc: string = (
      tool.inputSchema?.properties?.status?.description ?? ""
    ).toLowerCase();
    // The irreversibility of `reported` must be spelled out for the agent.
    expect(desc).toContain("irreversible");
    expect(desc).toContain("reopen");
  });
});
