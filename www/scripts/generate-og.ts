#!/usr/bin/env bun
import { join } from "node:path";
import { $ } from "bun";
import { compileWebsite } from "./stylex-build";
import { ogImageNames } from "./svg-templates";

// compileWebsite publishes current SVGs from the same StyleX generation.
await compileWebsite();
const publicDir = join(import.meta.dir, "..", "public");
for (const name of ogImageNames) {
  const svgPath = join(publicDir, name + ".svg");
  const pngPath = join(publicDir, name + ".png");
  await $`rsvg-convert -w 1200 -h 630 ${svgPath} -o ${pngPath}`.quiet();
  console.log(`✓ ${name}.png`);
}
