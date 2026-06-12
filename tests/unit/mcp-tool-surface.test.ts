// Tests: src/mcp/registry.ts, docs/mcp-tool-surface.md (MCP tool surface doc)
import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { registerAllTools } from "../../src/mcp/registry";

const DOC_PATH = join(process.cwd(), "docs/mcp-tool-surface.md");

/** Count the tools `registerAllTools` actually registers on a fresh server. */
function actualToolCount(): number {
  const server = new McpServer({ name: "rentemester-test", version: "0.0.0" });
  registerAllTools(server);
  return Object.keys((server as unknown as { _registeredTools: Record<string, unknown> })._registeredTools).length;
}

const REQUIRED_SECTIONS = [
  "Designprincipper",
  "Klassifikation",
  "Read-tools",
  "Write-tools",
  "System-tools",
  "Eksempel-handshakes",
  "Actor-attribution",
  "Forudsætninger",
];

describe("docs/mcp-tool-surface.md", () => {
  test("file exists", () => {
    expect(existsSync(DOC_PATH)).toBe(true);
  });

  test("contains all required section headings", () => {
    const content = readFileSync(DOC_PATH, "utf8");
    for (const section of REQUIRED_SECTIONS) {
      expect(content, `missing section: ${section}`).toContain(section);
    }
  });

  test("lists at least 25 tools across all tables", () => {
    const content = readFileSync(DOC_PATH, "utf8");
    const toolRows = countToolRows(content);
    expect(toolRows).toBeGreaterThanOrEqual(25);
  });

  test("every tool name uses snake_case", () => {
    const content = readFileSync(DOC_PATH, "utf8");
    const toolNames = extractToolNames(content);
    expect(toolNames.length).toBeGreaterThanOrEqual(25);
    const snakeCase = /^[a-z][a-z0-9_]*$/;
    for (const name of toolNames) {
      expect(snakeCase.test(name), `tool name not snake_case: ${name}`).toBe(true);
    }
  });

  test("documents at least one example handshake for both read and write", () => {
    const content = readFileSync(DOC_PATH, "utf8");
    expect(content).toMatch(/Read-tool: `audit_verify`/);
    expect(content).toMatch(/Write-tool: `journal_post`/);
    // Both example JSON payloads present
    expect(content).toContain('"name": "audit_verify"');
    expect(content).toContain('"name": "journal_post"');
  });

  test("classification table mentions all four levels", () => {
    const content = readFileSync(DOC_PATH, "utf8");
    expect(content).toContain("`read`");
    expect(content).toContain("`write-reversible`");
    expect(content).toContain("`write-irreversible`");
    expect(content).toContain("`destructive`");
  });

  test("documents the real tool total — not a stale count", () => {
    // #217 — the doc previously said "Total: 50" while the server exposed
    // more tools. Guard against the count drifting out of sync again by
    // checking the documented total against the tools `registerAllTools`
    // actually registers, instead of a hard-coded number that goes stale
    // every time a new tool lands.
    const content = readFileSync(DOC_PATH, "utf8");
    const match = content.match(/\*\*?Total\*\*?\s*[:|]\s*\*\*?(\d+)\*\*?/);
    expect(match, "the surface doc must state a **Total**: <n>").not.toBeNull();
    const documentedTotal = Number(match![1]);
    expect(documentedTotal).toBe(actualToolCount());
    expect(content).not.toContain("Total**: 50");
  });

  test("does not describe period_list as a CLI command to be built", () => {
    // #217 — `period list` was never built as a CLI command; period_list
    // is an MCP-only tool. The doc must not claim a CLI command exists.
    const content = readFileSync(DOC_PATH, "utf8");
    expect(content).not.toMatch(/Kræver ny CLI-kommando/);
    // The CLI/MCP mapping gap must be documented explicitly instead.
    expect(content).toContain("CLI/MCP-mapping");
    expect(content).toMatch(/period_list[^\n]*ingen CLI-kommando/);
  });

  test("points the agent at the standalone-surface contract doc", () => {
    // #203 — the tool-surface doc must reference the agent contract.
    const content = readFileSync(DOC_PATH, "utf8");
    expect(content).toContain("mcp-agent-contract.md");
  });

  test("documents that every tool declares an outputSchema", () => {
    // #202 — the result contract must be discoverable from the surface doc.
    const content = readFileSync(DOC_PATH, "utf8");
    expect(content).toContain("outputSchema");
    // The shared envelope shape must be spelled out for the agent.
    expect(content).toMatch(/Resultat-shapes/);
  });

  test("#228 — the journal_post example handshake posts to real seeded accounts", () => {
    // The canonical example previously used accounts 3617/6902, which are
    // not in seedAccounts() — an agent copying it gets the posting rejected.
    const content = readFileSync(DOC_PATH, "utf8");
    // Locate the journal_post example payload block.
    const exampleStart = content.indexOf('"name": "journal_post"');
    expect(exampleStart).toBeGreaterThan(-1);
    const example = content.slice(exampleStart, exampleStart + 900);
    // The non-existent accounts must be gone from the example.
    expect(example).not.toContain('"3617"');
    expect(example).not.toContain('"6902"');
    // Every accountNo used in the example must be a seeded account.
    const seeded = new Set([
      "1000", "1010", "1100", "1200", "2000", "3000", "3010", "3020",
      "3050", "3070", "3080", "3120", "4000", "4500", "5000", "5800",
      "5810", "5820",
    ]);
    const used = [...example.matchAll(/"accountNo":\s*"(\d+)"/g)].map((m) => m[1]!);
    expect(used.length).toBeGreaterThanOrEqual(2);
    for (const acc of used) {
      expect(seeded.has(acc), `journal_post example uses unseeded account ${acc}`).toBe(true);
    }
  });

  test("does not promise an unbacked idempotencyKey on writes", () => {
    // #204 — the doc once promised retry-safe idempotency keys with a 24h
    // cache that was never implemented. The false promise must stay removed.
    const content = readFileSync(DOC_PATH, "utf8");
    // journal_post's input row must not advertise the dropped field.
    expect(content).not.toMatch(/journal_post[^\n]*idempotencyKey/);
    // No example handshake may pass an idempotencyKey argument.
    expect(content).not.toContain('"idempotencyKey"');
    // The doc must explicitly state there is no general idempotency mechanism.
    expect(content).toMatch(/[Ii]ngen generel idempotency-key/);
  });

  test("#245 — documents that the company argument also accepts a workspace slug", () => {
    // resolveCompanyArg accepts a path OR a slug; the doc must not claim
    // company is absolute-path-only.
    const content = readFileSync(DOC_PATH, "utf8");
    expect(content).toContain("slug");
    expect(content).toContain("RENTEMESTER_WORKSPACE");
  });

  test("#260 — invoice_status data shape matches the running tool", () => {
    // The doc once described documentId/invoiceNo/daysOverdue; the real tool
    // (getInvoiceStatus -> InvoiceStatusResult) returns invoiceDocumentId/
    // invoiceNumber/overdueDays. An agent reading the stale names gets undefined.
    const content = readFileSync(DOC_PATH, "utf8");
    expect(content).toContain("invoiceDocumentId");
    expect(content).toContain("invoiceNumber");
    expect(content).toContain("overdueDays");
    // The stale field names must be gone from the invoice_status example.
    const exampleStart = content.indexOf('"name": "invoice_status"');
    expect(exampleStart).toBeGreaterThan(-1);
    const example = content.slice(exampleStart, exampleStart + 700);
    expect(example).not.toContain('"documentId"');
    expect(example).not.toContain('"invoiceNo"');
    expect(example).not.toContain('"daysOverdue"');
  });

  test("#260 — Actor-attribution section matches src/mcp/actor.ts", () => {
    // deriveMcpActor: createdBy = agent:<name>/<version>; createdByProgram =
    // mcp:<RENTEMESTER_MCP_USER> | rentemester-mcp. The doc must not still
    // claim createdByProgram is the client name/version, nor invent a
    // userContext parameter on the tool call.
    const content = readFileSync(DOC_PATH, "utf8");
    const idx = content.indexOf("## Actor-attribution");
    expect(idx).toBeGreaterThan(-1);
    const section = content.slice(idx, idx + 1600);
    expect(section).toContain("RENTEMESTER_MCP_USER");
    expect(section).toContain("rentemester-mcp");
    expect(section).toContain("agent:<name>/<version>");
    // The stale claims must be gone.
    expect(section).not.toMatch(/createdByProgram.*MCP-client `name\/version`/);
    expect(section).not.toMatch(/userContext.*opgraderes/);
  });

  test("#260 — period reopen is listed as a CLI-only command", () => {
    // period reopen (the only way to reopen a closed period) is CLI-only —
    // it must appear in the CLI-only commands list, not be silently absent.
    const content = readFileSync(DOC_PATH, "utf8");
    expect(content).toContain("period reopen");
    const idx = content.indexOf("CLI-only-kommandoer");
    expect(idx).toBeGreaterThan(-1);
    expect(content.slice(idx, idx + 600)).toContain("period reopen");
  });

  test("#260 — customer validate-vat CLI/MCP divergence is not called consistent", () => {
    // CLI customer validate-vat is an actor-gated mutation; the MCP
    // customer_validate_vat tool is a read. The doc must not call them
    // "consistent" — they sit in different governance classes.
    const content = readFileSync(DOC_PATH, "utf8");
    expect(content).not.toMatch(/CLI og MCP\s+er altså konsistente/);
    // The divergence must be named explicitly.
    expect(content).toContain("MUTATING_COMMANDS");
    expect(content.toLowerCase()).toContain("divergens");
  });
});

describe("docs/mcp-agent-contract.md", () => {
  const CONTRACT_PATH = join(process.cwd(), "docs/mcp-agent-contract.md");

  test("file exists", () => {
    expect(existsSync(CONTRACT_PATH)).toBe(true);
  });

  test("covers the standalone tool surface conventions", () => {
    // #203 — a contract for the loose MCP tool surface (not the agent run
    // loop): ordering, confirm/destructive conventions, preconditions.
    const content = readFileSync(CONTRACT_PATH, "utf8");
    for (const topic of [
      "confirm",
      "confirmText",
      "destructive",
      "read before you write",
      "Preconditions",
      "idempotencyKey",
      "append-only",
    ]) {
      expect(content, `missing topic: ${topic}`).toContain(topic);
    }
    // It must distinguish itself from the agent run loop contract.
    expect(content).toContain("runtime-agent-contract.md");
  });

  test("#229 — documents the raw -32602 form for a missing confirm + schema error", () => {
    // When confirm is omitted AND the payload has a schema error, the SDK
    // rejects the call with a raw -32602 (no envelope). The contract must
    // tell an agent how to detect and branch on that shape.
    const content = readFileSync(CONTRACT_PATH, "utf8");
    expect(content).toContain("-32602");
    // It must spell out that this reply has no structuredContent.
    expect(content).toMatch(/structuredContent/);
    expect(content).toMatch(/Input validation error/);
    // And give the agent a concrete branch condition.
    expect(content).toMatch(/isError/);
  });

  test("does not promise retry-safe idempotency keys (#204)", () => {
    // #204 — the contract previously told agents that re-sending a write
    // with the same idempotencyKey would not double-post. That mechanism
    // does not exist; the contract must warn the opposite.
    const content = readFileSync(CONTRACT_PATH, "utf8");
    expect(content).toMatch(/no general `idempotencyKey` mechanism/i);
    expect(content).toMatch(/double-post/);
    // It must not still claim a key-keyed retry returns the original result.
    expect(content).not.toMatch(/same key does not double-post/);
  });

  test("#245 — Identification section states company accepts a path OR a workspace slug", () => {
    // resolveCompanyArg accepts both forms; the contract must not claim the
    // company argument is absolute-path-only.
    const content = readFileSync(CONTRACT_PATH, "utf8");
    const idxStart = content.indexOf("## Identification");
    expect(idxStart).toBeGreaterThan(-1);
    const section = content.slice(idxStart, idxStart + 1400);
    expect(section).toContain("slug");
    expect(section).toContain("RENTEMESTER_WORKSPACE");
    expect(section).toContain("resolveCompanyArg");
  });

  test("#260 — names period reopen as the CLI-only closed-period recovery path", () => {
    // The closed-period precondition row must name the actual recovery
    // command and state there is no MCP tool for it.
    const content = readFileSync(CONTRACT_PATH, "utf8");
    expect(content).toContain("period reopen");
  });
});

/**
 * Count rows in the tool tables by scanning for lines that look like
 * `| \`tool_name\` | ... |`. Header rows and separator rows are excluded
 * because they don't start with a backticked identifier.
 */
function countToolRows(content: string): number {
  const lines = content.split(/\r?\n/);
  let count = 0;
  for (const line of lines) {
    if (/^\s*\|\s*`[a-z][a-z0-9_]*`\s*\|/.test(line)) count += 1;
  }
  return count;
}

function extractToolNames(content: string): string[] {
  const names = new Set<string>();
  const re = /^\s*\|\s*`([a-z][a-z0-9_]*)`\s*\|/gm;
  let match: RegExpExecArray | null;
  while ((match = re.exec(content)) !== null) {
    names.add(match[1]!);
  }
  return [...names];
}
