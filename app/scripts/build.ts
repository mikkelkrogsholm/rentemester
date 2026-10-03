import { stylexBunPlugin } from "./stylex-plugin";
import { stylexConfig } from "../stylex.config";
import { rm, mkdir, copyFile, readFile, writeFile, rename } from "node:fs/promises";
import { createHash } from "node:crypto";
import { resolve } from "node:path";

const appRoot = resolve(import.meta.dir, "..");
const outdir = resolve(appRoot, "dist");

await rm(outdir, { recursive: true, force: true });
const result = await Bun.build({
  entrypoints: [resolve(appRoot, "index.html")],
  outdir,
  minify: true,
  publicPath: "/",
  sourcemap: "linked",
  target: "browser",
  metafile: true,
  plugins: [stylexBunPlugin({ ...stylexConfig, dev: false, bunDevCssOutput: resolve(outdir, "stylex.css") })],
});

if (!result.success) {
  for (const log of result.logs) console.error(log);
  process.exit(1);
}

// Bun's HTML builder does not attach plugin-emitted CSS. Link the native
// StyleX compiler's complete output explicitly; production has no injection.
const css = await readFile(resolve(outdir, "stylex.css"), "utf8");
if (!css.includes("@layer stylex") || !css.includes("Source Serif 4") || !css.includes("44px")) throw new Error("StyleX compiler emitted incomplete styles or design variables");
const cssName = `stylex-${createHash("sha256").update(css).digest("hex").slice(0, 16)}.css`;
await rename(resolve(outdir, "stylex.css"), resolve(outdir, cssName));
const htmlPath = resolve(outdir, "index.html");
const html = await readFile(htmlPath, "utf8");
await writeFile(htmlPath, html.replace("</head>", `<link rel="stylesheet" href="/${cssName}"></head>`));

// Font license notices accompany every distributed build.
const fontLicenseDirectory = resolve(outdir, "licenses");
await mkdir(fontLicenseDirectory, { recursive: true });
for (const font of ["ibm-plex-sans", "ibm-plex-mono", "source-serif-4"]) {
  await copyFile(resolve(appRoot, "../node_modules/@fontsource", font, "LICENSE"), resolve(fontLicenseDirectory, `${font}.txt`));
}
