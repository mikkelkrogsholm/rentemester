import { afterEach, expect, test } from "bun:test";
import { mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { compileWebsite, websiteRoot } from "../scripts/stylex-build";
import { ogImageNames } from "../scripts/svg-templates";

const directories: string[] = [];
afterEach(async () => { await Promise.all(directories.splice(0).map(path => rm(path, { recursive: true, force: true }))); });

test("normal compilation regenerates SVGs on DESIGN and OG style changes without PNG tooling", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "rentemester-svg-drift-"));
  directories.push(temporary);
  const root = join(temporary, "www");
  await mkdir(join(root, "src"), { recursive: true });
  const [design, siteSource, ogSource] = await Promise.all([
    readFile(join(websiteRoot, "../DESIGN.md"), "utf8"),
    readFile(join(websiteRoot, "src/site.stylex.ts"), "utf8"),
    readFile(join(websiteRoot, "src/og.stylex.ts"), "utf8"),
  ]);
  await writeFile(join(temporary, "DESIGN.md"), design);
  await writeFile(join(root, "src/site.stylex.ts"), siteSource);
  await writeFile(join(root, "src/og.stylex.ts"), ogSource);
  const filenames = [...ogImageNames.map(name => name + ".svg"), "favicon.svg"];
  const readSvgs = () => Promise.all(filenames.map(name => readFile(join(root, "public", name), "utf8")));
  await compileWebsite(root);
  const first = await readSvgs();
  expect(first).toHaveLength(6);
  expect(first.every(svg => svg.includes('data-stylex="compiled"'))).toBe(true);
  expect(first.every(svg => !svg.includes("@layer") && !svg.includes("var("))).toBe(true);
  await compileWebsite(root);
  expect(await readSvgs()).toEqual(first);
  const newDesign = design.replace('primary: "#e9c176"', 'primary: "#ff0000"');
  expect(newDesign).not.toBe(design);
  await writeFile(join(temporary, "DESIGN.md"), newDesign);
  await compileWebsite(root);
  const paletteChanged = await readSvgs();
  expect(paletteChanged.every((svg, n) => svg !== first[n])).toBe(true);
  expect(paletteChanged.every(svg => /fill:(?:red|#f00|#ff0000)/.test(svg))).toBe(true);
  await writeFile(join(root, "src/og.stylex.ts"), ogSource.replace("fontSize: 20", "fontSize: 21"));
  await compileWebsite(root);
  const stylesChanged = await readSvgs();
  expect(stylesChanged.every((svg, n) => svg !== paletteChanged[n])).toBe(true);
  expect(stylesChanged[0]).toContain("font-size:21px");
  const module = await readFile(join(root, ".stylex/site-stylex.ts"), "utf8");
  await writeFile(join(root, "src/og.stylex.ts"), ogSource + "\nsyntax error");
  await expect(compileWebsite(root)).rejects.toThrow();
  expect(await readSvgs()).toEqual(stylesChanged);
  expect(await readFile(join(root, ".stylex/site-stylex.ts"), "utf8")).toBe(module);
});
