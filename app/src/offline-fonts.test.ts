import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { expect, test } from "bun:test";

const appRoot = join(import.meta.dir, "..");
const forbiddenFontHosts = [
  ["fonts", "googleapis", "com"].join("."),
  ["fonts", "gstatic", "com"].join("."),
];

test("Cockpit source and built index have no external font requests", async () => {
  const sourceFiles = [join(appRoot, "index.html")];
  for await (const file of new Bun.Glob("src/**/*.{css,ts,tsx}").scan({
    cwd: appRoot,
    absolute: true,
  })) {
    sourceFiles.push(file);
  }

  for (const sourceFile of sourceFiles) {
    const source = await Bun.file(sourceFile).text();
    for (const host of forbiddenFontHosts) expect(source).not.toContain(host);
  }
  expect(await Bun.file(join(appRoot, "index.html")).text()).not.toMatch(
    /<link\b[^>]+href=["']https?:\/\//i,
  );

  const build = Bun.spawn(["bun", "run", "scripts/build.ts"], {
    cwd: appRoot,
    stderr: "pipe",
  });
  expect(await build.exited, await new Response(build.stderr).text()).toBe(0);

  const builtIndex = await readFile(join(appRoot, "dist/index.html"), "utf8");
  for (const host of forbiddenFontHosts) expect(builtIndex).not.toContain(host);
  expect(builtIndex).not.toMatch(/<link\b[^>]+href=["']https?:\/\//i);
});
