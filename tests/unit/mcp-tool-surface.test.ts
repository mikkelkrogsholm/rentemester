import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const DOC_PATH = join(process.cwd(), "docs/mcp-tool-surface.md");

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
