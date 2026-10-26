import { describe, expect, test } from "bun:test";
import { inspectStyling } from "../../scripts/check-stylex";

describe("StyleX source ownership", () => {
  test("blocks parallel styling entry points", () => {
    for (const source of [
      'import "./old.css";',
      '<div className="legacy" />',
      '<div className={`legacy`} />',
      '<div className={`legacy ${computed}`} />',
      '`<div class="legacy ${compiled.class}"></div>`',
      '`body { color:red }`',
      '`<style>${css}</style>`',
      '<div className={active ? "legacy-active" : "legacy"} />',
      '<div style={{ color: "red" }} />',
      'element.style.color = "red";',
      'element.setAttribute("style", "color:red")',
      'sheet.insertRule(".old { color:red; }")',
      '`<style>.old { color:red; }</style>`',
      '`<svg><path fill="red" /></svg>`',
      '`<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Example">`',
    ]) expect(inspectStyling("owned.tsx", source).length).toBeGreaterThan(0);
    expect(inspectStyling("old.scss", "").length).toBe(1);
    expect(inspectStyling("page.astro", '<button class={"legacy"}>Title</button>').length).toBeGreaterThan(0);
  });
  test("accepts compiled attributes and SVG data geometry", () => {
    expect(inspectStyling("chart.tsx", '<svg {...stylex.props(styles.chart)} viewBox="0 0 100 100"><path {...stylex.props(styles.line)} d={path} /></svg>')).toEqual([]);
    expect(inspectStyling("src/core/backup-guide.ts", '`<style>${documentCss}</style><main class="${documentStyles.attributes.main.class}">${escapeHtml(title)}</main>`')).toEqual([]);
    expect(inspectStyling("page.astro", '<main {...attributes.main}><h1 {...attributes.heading}>Title</h1></main>')).toEqual([]);
    expect(inspectStyling("page.astro", '<link rel="stylesheet" href={cssHref} />')).toEqual([]);
  });
});
