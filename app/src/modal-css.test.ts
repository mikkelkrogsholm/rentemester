// Regression guard for the modal CSS contract (audit 2026-06-11, UI-1).
//
// Dialog chrome now belongs to the native StyleX Dialog. Keep the old audit
// regression: no view can recreate an inert-looking, interactive backdrop.
// Semantic modal content layout classes still have their CSS contract checked.
import { describe, expect, test } from "bun:test";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const SRC_DIR = join(__dirname);

/** Recursively collect all .tsx source files under src/ (tests excluded —
 *  they assert on markup, they don't produce it). */
function collectTsxFiles(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) {
      out.push(...collectTsxFiles(full));
    } else if (name.endsWith(".tsx") && !name.endsWith(".test.tsx")) {
      out.push(full);
    }
  }
  return out;
}

/** Extract every whitespace-separated class token starting with `modal`
 *  from string literals assigned to className (plain or template). */
function modalClassesIn(source: string): string[] {
  const classes: string[] = [];
  const literal = /className=\{?\s*["'`]([^"'`]+)["'`]/g;
  for (const match of source.matchAll(literal)) {
    for (const token of match[1].split(/\s+/)) {
      if (/^modal(-[a-z-]+)?$/.test(token)) classes.push(token);
    }
  }
  return classes;
}

describe("modal CSS contract (audit UI-1)", () => {
  const css = readFileSync(join(SRC_DIR, "styles.css"), "utf8");
  const tsxFiles = collectTsxFiles(SRC_DIR);

  test("the scan sees form layouts and no view recreates legacy modal chrome", () => {
    const all = tsxFiles.flatMap((f) => modalClassesIn(readFileSync(f, "utf8")));
    expect(all).toContain("modal-field");
    expect(all).toContain("modal-actions");
    expect(all).not.toContain("modal-overlay");
    expect(all).not.toContain("modal");
  });

  test("no component uses the undefined legacy class modal-backdrop", () => {
    const offenders = tsxFiles.filter((f) =>
      modalClassesIn(readFileSync(f, "utf8")).includes("modal-backdrop"),
    );
    expect(offenders).toEqual([]);
  });

  test("every modal-* class used in TSX is defined in styles.css", () => {
    const missing: string[] = [];
    for (const file of tsxFiles) {
      for (const cls of modalClassesIn(readFileSync(file, "utf8"))) {
        // A definition is the class used as a CSS selector: `.modal-foo`
        // followed by a non-word char (space, comma, brace, colon, dot).
        const selector = new RegExp(`\\.${cls}(?![\\w-])`);
        if (!selector.test(css) && !missing.includes(cls)) missing.push(cls);
      }
    }
    expect(missing).toEqual([]);
  });
});
