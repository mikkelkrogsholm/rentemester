import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const ALLOWLIST = new Set([
  "src/core/money.ts",                  // det er modulet selv
  "src/core/system-backups.ts",         // days-beregning, ikke currency (markeret med kommentar)
]);

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walk(path));
    else if (entry.isFile() && path.endsWith(".ts")) out.push(path);
  }
  return out;
}

describe("money adoption", () => {
  test("no toFixed(2) for currency math in src/core except allowlist", () => {
    const violations: string[] = [];
    for (const file of walk("src/core")) {
      const rel = file;
      if (ALLOWLIST.has(rel)) continue;
      const content = readFileSync(file, "utf8");
      const lines = content.split("\n");
      lines.forEach((line, idx) => {
        if (/toFixed\(2\)/.test(line)) {
          // Tillad kommentar-baseret undtagelse: linjen eller den forrige linje skal indeholde "money-allowed:"
          const prev = lines[idx - 1] ?? "";
          if (!/money-allowed:/.test(line) && !/money-allowed:/.test(prev)) {
            violations.push(`${rel}:${idx + 1}: ${line.trim()}`);
          }
        }
      });
    }
    expect(violations).toEqual([]);
  });
});
