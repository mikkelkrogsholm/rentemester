import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { render } from "takumi-pdf";
import { documentCss } from "./document-styles.generated";
import { documentFonts } from "./document-fonts";
import type { PdfDocument } from "./pdf-render";

try {
  const chunks: Uint8Array[] = [];
  let length = 0;
  for await (const chunk of Bun.stdin.stream()) {
    length += chunk.length;
    if (length > 8 * 1024 * 1024) throw new Error("input limit");
    chunks.push(chunk);
  }
  const document = JSON.parse(Buffer.concat(chunks).toString("utf8")) as PdfDocument;
  if (typeof document.html !== "string" || typeof document.footer !== "string" ||
      typeof document.title !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(document.date)) throw new Error("invalid document");
  // Assets are packaged locally; disable network even if a future template
  // accidentally adds a remote image or font URL.
  globalThis.fetch = Object.assign(() => Promise.reject(new Error("PDF rendering is offline")), { preconnect: () => {} }) as typeof fetch;
  const fonts = documentFonts.map(({ name, weight, file, subsetOf }) => ({
    name, weight, ...(subsetOf ? { subsetOf } : {}), data: readFileSync(resolve(import.meta.dir, "fonts", file)),
  }));
  const bytes = await render(document.html, {
    size: "a4", margin: { top: 48, bottom: 48, left: 56, right: 56 },
    css: documentCss, fonts, footer: document.footer, lang: "da",
    metadata: { title: document.title, creator: "Rentemester", creationDate: document.date },
    fontFamilies: ["IBM Plex Sans", "Source Serif 4", "IBM Plex Mono"],
    uncoveredText: "error",
    outline: true,
  });
  await Bun.write(Bun.stdout, bytes);
} catch {
  // Never log legal document contents or environment information.
  process.stderr.write("PDF renderer failed\n");
  process.exitCode = 1;
}
