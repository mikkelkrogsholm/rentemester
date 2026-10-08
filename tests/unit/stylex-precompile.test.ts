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

  test("requires portable filenames and self-contained authoring inputs", () => {
    expect(() => compileStylex("", "/absolute/path.ts", dependencies)).toThrow("repository-relative");
    expect(() => compileStylex('import data from "./runtime"; export const attributes = data;', "synthetic.ts", dependencies)).toThrow("self-contained");
  });
});
