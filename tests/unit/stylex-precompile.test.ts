import { describe, expect, test } from "bun:test";
import babel from "@babel/core";
import plugin from "@stylexjs/babel-plugin";
import * as stylex from "@stylexjs/stylex";
import { compileStylex } from "../../scripts/stylex-precompile";

const dependencies = {
  transformSync: babel.transformSync as Parameters<typeof compileStylex>[2]["transformSync"],
  stylexBabelPlugin: plugin,
  processStylexRules: plugin.processStylexRules as Parameters<typeof compileStylex>[2]["processStylexRules"],
  stylexRuntime: stylex,
};

describe("portable static StyleX precompiler", () => {
  test("composes conflicting styles before serializing and emits deterministic media CSS", () => {
    const source = `import * as stylex from "@stylexjs/stylex";
      const s = stylex.create({ base: { color: "red", padding: {default: 8, "@media print": 0} }, selected: {color: "blue"} });
      export const attributes = { selected: stylex.attrs(s.base,s.selected) };`;
    const compiled = compileStylex(source, "src/design/synthetic.stylex.ts", dependencies);
    expect(compiled).toEqual(compileStylex(source, "src/design/synthetic.stylex.ts", dependencies));
    const colorClass = compiled.css.match(/\.([a-z0-9]+)[^{}]*\{color:blue\}/)![1]!;
    const discardedClass = compiled.css.match(/\.([a-z0-9]+)[^{}]*\{color:red\}/)![1]!;
    expect(compiled.attributes.selected!.class!.split(" ")).toContain(colorClass);
    expect(compiled.attributes.selected!.class!.split(" ")).not.toContain(discardedClass);
    expect(compiled.css).toContain("@media print");
    expect(Object.keys(compiled.attributes.selected!)).toEqual(["class"]);
  });

  test("preserves authored backgrounds and borders in Bun instead of silently dropping shorthands", () => {
    const source = `import * as stylex from "@stylexjs/stylex";
      const s = stylex.create({ card: { background: "linear-gradient(red, blue)", border: "1px solid black", borderBottom: "2px solid green" } });
      export const attributes = { card: stylex.attrs(s.card) };`;
    const compiled = compileStylex(source, "src/design/synthetic.stylex.ts", dependencies);
    expect(compiled.css).toContain("background:linear-gradient(red,blue)");
    expect(compiled.css).toContain("border:1px solid black");
    expect(compiled.css).toContain("border-bottom:2px solid green");
  });

  test("requires portable filenames and self-contained authoring inputs", () => {
    expect(() => compileStylex("", "/absolute/path.ts", dependencies)).toThrow("repository-relative");
    expect(() => compileStylex('import data from "./runtime"; export const attributes = data;', "synthetic.ts", dependencies)).toThrow("self-contained");
  });
});
