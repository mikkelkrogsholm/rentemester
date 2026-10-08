import { afterEach, expect, test } from "bun:test";
import { mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { compileWebsite } from "../scripts/stylex-build";

const svgSource = (source: string) => source.replace("box:stylex.attrs(s.box)", ["background", "glowLayer", "cyberLayer", "glowStart", "glowEnd", "cyberStart", "cyberEnd", "eyebrow", "title", "titleAccent", "line", "subtitle", "footer", "monogramFrame", "monogram", "faviconText", "faviconLine"].map(name => name + ":stylex.attrs(s.box)").join(","));

const directories: string[] = [];
afterEach(async () => { await Promise.all(directories.splice(0).map(path => rm(path, { recursive: true, force: true }))); });

test("precompiler is deterministic and preserves last consistent generation after failure", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "rentemester-site-stylex-"));
  const root = join(temporary, "www");
  directories.push(temporary);
  await mkdir(join(root, "src"), { recursive: true });
  await writeFile(join(temporary, "DESIGN.md"), "---\nmarketing:\n  colors:\n    primary: '#ffffff'\n---\n");
  const source = 'import * as stylex from "@stylexjs/stylex"; const s=stylex.create({box:{color:"red",paddingLeft:{default:16,"@media (min-width: 768px)":64}}}); export const attributes={box:stylex.attrs(s.box)};';
  await writeFile(join(root, "src/site.stylex.ts"), source);
  await writeFile(join(root, "src/og.stylex.ts"), svgSource(source));
  await compileWebsite(root);
  const first = await readFile(join(root, ".stylex/site-stylex.ts"), "utf8");
  const href = first.match(/export const cssHref = "([^"]+)"/)![1];
  const css = await readFile(join(root, "public", href), "utf8");
  expect(css).toContain("color:red");
  expect(css).toContain("@media");
  expect(css).not.toContain("@layer");
  await compileWebsite(root);
  expect(await readFile(join(root, ".stylex/site-stylex.ts"), "utf8")).toBe(first);
  await writeFile(join(root, "src/site.stylex.ts"), source + "\nsyntax error");
  await expect(compileWebsite(root)).rejects.toThrow();
  expect(await readFile(join(root, ".stylex/site-stylex.ts"), "utf8")).toBe(first);
  expect(await readFile(join(root, "public", href), "utf8")).toBe(css);
  await writeFile(join(root, "src/site.stylex.ts"), source.replace('color:"red"', 'color:"blue"'));
  await compileWebsite(root);
  expect(await readFile(join(root, ".stylex/site-stylex.ts"), "utf8")).not.toBe(first);
  expect(await readFile(join(root, "public", href), "utf8")).toBe(css);
});

test("DESIGN palette updates change compiled colors and source identity", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "rentemester-site-palette-"));
  const root = join(temporary, "www");
  directories.push(temporary);
  await mkdir(join(root, "src"), { recursive: true });
  const source = 'import * as stylex from "@stylexjs/stylex"; declare const marketing: {colors:{primary:string}}; const s=stylex.create({box:{color:marketing.colors.primary}}); export const attributes={box:stylex.attrs(s.box)};';
  // The declaration is one line in real authoring modules; token injection removes it.
  const formatted = source.replace(" declare const marketing:", "\ndeclare const marketing:").replace("; const s=", ";\nconst s=");
  await writeFile(join(root, "src/site.stylex.ts"), formatted);
  await writeFile(join(root, "src/og.stylex.ts"), svgSource(formatted));
  const design = "---\nmarketing:\n  colors:\n    primary: '#ffffff'\n---\n";
  await writeFile(join(temporary, "DESIGN.md"), design);
  await compileWebsite(root);
  const first = await readFile(join(root, ".stylex/site-stylex.ts"), "utf8");
  await writeFile(join(temporary, "DESIGN.md"), design.replace("#ffffff", "#000000"));
  await compileWebsite(root);
  const second = await readFile(join(root, ".stylex/site-stylex.ts"), "utf8");
  expect(second).not.toBe(first);
  const href = second.match(/export const cssHref = "([^"]+)"/)![1];
  expect(await readFile(join(root, "public", href), "utf8")).toContain("color:#000");
});
