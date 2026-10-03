// Tests: docs/mcp-*.md, docs/cockpit-api.md, src/mcp/registry.ts JSDoc —
// agent-facing tool counts must match the live MCP server's registered
// tool count (#367).
//
// Why this guard exists: the MCP agent contract is handed verbatim to
// every external client in `initialize.instructions`. An agent that
// trusts a stale tool count (e.g. "81 loose tools") will either ignore
// 14+ extra tools it discovers via `tools/list`, or treat them as
// experimental. We can't let docs drift from the registry.
import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { registerAllTools } from "../../src/mcp/registry";

const REPO_ROOT = new URL("../..", import.meta.url).pathname.replace(/\/$/, "");

function countRegisteredTools(): number {
  // Recurses through src/mcp/tools/ so the count stays correct when a large
  // tool family (e.g. invoice.ts → invoice/{issuance,settlement,...}.ts) is
  // split into a subdirectory of per-sub-domain register helpers.
  let count = 0;
  function walk(dir: string): void {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
        continue;
      }
      if (!entry.isFile() || !entry.name.endsWith(".ts")) continue;
      const text = readFileSync(full, "utf8");
      const matches = text.match(/\bregisterTool\s*\(/g);
      if (matches) count += matches.length;
    }
  }
  walk(join(REPO_ROOT, "src/mcp/tools"));
  return count;
}

const AGENT_FACING_FILES = [
  "docs/mcp-agent-contract.md",
  "docs/mcp-tool-surface.md",
  "docs/mcp-install.md",
  "docs/cockpit-api.md",
  "src/mcp/registry.ts",
];

describe("MCP tool count in agent-facing docs (#367)", () => {
  test("read, ordinary write and destructive subtotals match live annotations", () => {
    const counts = { "Read-tools": 0, "Ordinary write-tools": 0, Destructive: 0 };
    registerAllTools({
      registerTool(_name: string, config: { annotations?: { readOnlyHint?: boolean; destructiveHint?: boolean } }) {
        if (config.annotations?.destructiveHint) counts.Destructive += 1;
        else if (config.annotations?.readOnlyHint) counts["Read-tools"] += 1;
        else counts["Ordinary write-tools"] += 1;
      },
    } as never);
    const text = readFileSync(join(REPO_ROOT, "docs/mcp-tool-surface.md"), "utf8");
    for (const [label, actual] of Object.entries(counts)) {
      const match = text.match(new RegExp(`- \\*\\*${label}\\*\\*: (\\d+)`));
      expect(match, `missing ${label} subtotal`).not.toBeNull();
      expect(Number(match![1]), `${label} must match tools/list annotations`).toBe(actual);
    }
  });

  test("Total in mcp-tool-surface.md matches src/mcp/tools/*.ts registerTool count", () => {
    const actual = countRegisteredTools();
    const text = readFileSync(
      join(REPO_ROOT, "docs/mcp-tool-surface.md"),
      "utf8",
    );
    // Match: "- **Total**: **95**"
    const match = text.match(/-\s*\*\*Total\*\*:\s*\*\*(\d+)\*\*/);
    expect(match, "Total line in 'Tool-count summary' not found").not.toBeNull();
    const documented = Number(match![1]);
    expect(documented).toBe(actual);
  });

  test("no agent-facing file uses a stale tool count phrasing", () => {
    const actual = countRegisteredTools();
    // Phrases that bind a number to "tool"/"tools" in a way an agent will
    // read as the catalogue size. We only complain when the number does
    // not equal the live registered count.
    const phrasePatterns: RegExp[] = [
      /\*\*(\d+)\s+(?:loose|MCP)?\s*tools?\*\*/gi, // "**81 tools**", "**81 loose tools**"
      /\b(\d+)\s+(?:loose|MCP)\s+tools?\b/gi, // "81 loose tools", "81 MCP tools"
      /\bAlle\s+(\d+)\s+tools?\b/gi, // "Alle 81 tools"
      /\b(\d+)\s+tools?\s+(?:fordelt|deklarerer)\b/gi, // "81 tools fordelt", "81 tools deklarerer"
    ];
    const offenders: string[] = [];
    for (const rel of AGENT_FACING_FILES) {
      const text = readFileSync(join(REPO_ROOT, rel), "utf8");
      const lines = text.split("\n");
      for (let i = 0; i < lines.length; i++) {
        for (const pat of phrasePatterns) {
          pat.lastIndex = 0;
          let m = pat.exec(lines[i]);
          while (m !== null) {
            const n = Number(m[1]);
            if (n !== actual) {
              offenders.push(`${rel}:${i + 1}: "${m[0]}" (actual=${actual})`);
            }
            m = pat.exec(lines[i]);
          }
        }
      }
    }
    expect(offenders, offenders.join("\n")).toEqual([]);
  });
});
