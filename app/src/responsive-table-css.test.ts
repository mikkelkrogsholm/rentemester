// Regression guard for #650: responsive tables become cards at phone widths.
// The earlier <=520px scroll treatment intentionally gives tables a 640px
// minimum, but the later <=640px card layout must clear that desktop minimum.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const css = readFileSync(join(__dirname, "styles.css"), "utf8");

function mediaBody(source: string, query: string): string {
  const start = source.indexOf(query);
  if (start < 0) return "";
  const opening = source.indexOf("{", start);
  let depth = 1;
  for (let index = opening + 1; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}" && --depth === 0) return source.slice(opening + 1, index);
  }
  return "";
}

const mobileCards = mediaBody(css, "@media (max-width: 640px)");

describe("responsive table mobile CSS contract (#650)", () => {
  test("clears the scroll-table minimum width when rows become cards", () => {
    expect(css).toContain(".responsive-table { min-width: 640px; }");
    expect(mobileCards).toMatch(/\.responsive-table[^\{]*\{[^}]*min-width:\s*0/);
  });

  test("lets card values shrink and wrap long evidence", () => {
    expect(mobileCards).toContain("grid-template-columns: minmax(7rem, 42%) minmax(0, 1fr)");
    expect(mobileCards).toMatch(/\.responsive-table td[^\{]*\{[^}]*overflow-wrap:\s*anywhere/);
  });

  test("lets every shared action group wrap before it widens a phone page", () => {
    expect(mobileCards).toMatch(/\.row-actions\s*\{[^}]*flex-wrap:\s*wrap/);
  });
});
