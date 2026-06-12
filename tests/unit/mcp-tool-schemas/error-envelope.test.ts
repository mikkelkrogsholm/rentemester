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

describe("#201 — an omitted confirm yields an error envelope, not a raw -32602", () => {
  // The set of write tools whose confirm-gating must survive an omitted flag.
  const cases: Array<{ name: string; args: Record<string, unknown> }> = [
    {
      name: "journal_post",
      args: {
        company: "__COMPANY__",
        payload: {
          transactionDate: "2026-05-18",
          text: "confirm omitted",
          lines: [
            { accountNo: "2000", debitAmount: 100 },
            { accountNo: "5000", creditAmount: 100 },
          ],
        },
        // confirm intentionally omitted
      },
    },
    {
      name: "invoice_issue",
      args: {
        company: "__COMPANY__",
        payload: { invoiceType: "full", invoiceNumber: "X" },
        // confirm intentionally omitted
      },
    },
    {
      name: "period_close",
      args: {
        company: "__COMPANY__",
        from: "2026-05-01",
        to: "2026-05-31",
        // confirm intentionally omitted
      },
    },
  ];

  for (const { name, args } of cases) {
    test(`${name}: omitted confirm returns { ok:false, errors:[...] } envelope`, async () => {
      const resolved = JSON.parse(
        JSON.stringify(args).replace(/__COMPANY__/g, companyRoot),
      );
      const response = await client.send("tools/call", { name, arguments: resolved });
      // The whole point: no raw JSON-RPC error, a structured envelope instead.
      expect(response.error).toBeUndefined();
      const structured = response.result?.structuredContent;
      expect(structured).toBeDefined();
      expect(structured.ok).toBe(false);
      expect(Array.isArray(structured.errors)).toBe(true);
      expect(
        structured.errors.some((m: string) => m.includes("confirm: true required")),
      ).toBe(true);
    });
  }

  test("an omitted confirm produces the same envelope as confirm:false", async () => {
    const base = {
      company: companyRoot,
      payload: { invoiceType: "full" as const, invoiceNumber: "X" },
    };
    const omitted = await client.send("tools/call", {
      name: "invoice_issue",
      arguments: base,
    });
    const explicitFalse = await client.send("tools/call", {
      name: "invoice_issue",
      arguments: { ...base, confirm: false },
    });
    expect(omitted.result?.structuredContent).toEqual(
      explicitFalse.result?.structuredContent,
    );
  });
});

describe("#202 — every tool declares the shared envelope outputSchema", () => {
  let tools: any[];

  beforeAll(async () => {
    const response = await client.send("tools/list");
    tools = response.result?.tools ?? [];
  });

  test("all 82 tools expose an outputSchema in tools/list", () => {
    expect(tools.length).toBeGreaterThanOrEqual(82);
    const missing = tools.filter((t) => !t.outputSchema).map((t) => t.name);
    expect(missing).toEqual([]);
  });

  test("the outputSchema is the shared { ok, data?, errors[], appliedRules? } envelope", () => {
    for (const tool of tools) {
      const schema = tool.outputSchema;
      expect(schema?.type, `tool ${tool.name} outputSchema`).toBe("object");
      const props = schema?.properties ?? {};
      // The machine-known envelope contract.
      expect(props.ok?.type, `${tool.name}.ok`).toBe("boolean");
      expect(props.errors?.type, `${tool.name}.errors`).toBe("array");
      expect(props.data?.type, `${tool.name}.data`).toBe("object");
      expect(props.appliedRules?.type, `${tool.name}.appliedRules`).toBe("array");
      // ok + errors are always present on the envelope.
      expect(schema?.required).toContain("ok");
      expect(schema?.required).toContain("errors");
    }
  });

  test("a success response's structuredContent validates against the outputSchema", async () => {
    // audit_verify is a read tool: a fresh company yields a clean ok envelope.
    const response = await client.send("tools/call", {
      name: "audit_verify",
      arguments: { company: companyRoot },
    });
    // With an outputSchema declared the SDK validates structuredContent on
    // success — a malformed envelope would come back as isError with no
    // structuredContent. Getting structuredContent back proves it validated.
    expect(response.error).toBeUndefined();
    expect(response.result?.isError).toBe(false);
    const structured = response.result?.structuredContent;
    expect(structured?.ok).toBe(true);
    expect(Array.isArray(structured?.errors)).toBe(true);
  });
});

describe("#200 — typed schemas reject structurally invalid payloads", () => {
  test("invoice_issue rejects a payload missing the required invoiceType", async () => {
    // With the typed schema the SDK rejects this before the handler. The point
    // of #200 is that the contract is real — an agent that omits a required
    // field gets told so, instead of the call silently being accepted.
    const response = await client.send("tools/call", {
      name: "invoice_issue",
      arguments: {
        company: companyRoot,
        payload: { invoiceNumber: "X" },
        confirm: true,
      },
    });
    // The typed schema makes the SDK reject this: either a JSON-RPC error, an
    // isError result, or an error envelope — never a success.
    const structured = response.result?.structuredContent;
    const failed =
      response.error !== undefined ||
      response.result?.isError === true ||
      structured?.ok === false;
    expect(failed).toBe(true);
  });
});
