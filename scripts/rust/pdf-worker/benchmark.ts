#!/usr/bin/env bun
/** Exact offline bridge for #691. Bun is a build/test oracle, never a runtime provider. */
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve, join } from "node:path";
import { syntheticTextPdf, SYNTHETIC_PDF_TEXT } from "../../../tests/fixtures/pdf-parser/synthetic-text-pdf";

const output = Bun.argv[2];
if (Bun.argv.length !== 3 || !output) throw new Error("usage: bun scripts/rust/pdf-worker/benchmark.ts NEW_CORPUS_DIRECTORY");
const root = resolve(import.meta.dir, "../../..");
const pin = (bytes: Uint8Array) => ({ sha256: createHash("sha256").update(bytes).digest("hex"), byteLength: bytes.length });
const sources = ["scripts/benchmark-pdf-parser.ts", "tests/fixtures/pdf-parser/synthetic-text-pdf.ts"].map(path => ({ path, ...pin(readFileSync(join(root, path))) }));
const inputs = Array.from({ length: 9 }, (_, index) => {
  const ordinal = index + 1;
  const bytes = ordinal === 4 ? Buffer.from("%PDF-1.4\nthis is deliberately malformed\n") : Buffer.concat([Buffer.from(syntheticTextPdf()), Buffer.from(`% benchmark-${ordinal}\n`)]);
  return { bytes, ordinal, file: `benchmark-${ordinal}.pdf`, ...pin(bytes), status: ordinal === 4 ? "malformed_pdf" : "ok" };
});
// Nonrecursive mkdir fails for every existing destination, including empty directories.
mkdirSync(resolve(output), { mode: 0o700 });
for (const { bytes, file } of inputs) writeFileSync(join(output, file), bytes, { flag: "wx", mode: 0o600 });
const manifest = { schema: "rentemester-original-pdf-benchmark-v1", sources, expectedText: SYNTHETIC_PDF_TEXT, width: 612, height: 792, inputs: inputs.map(({ bytes, ...entry }) => entry) };
writeFileSync(join(output, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n", { flag: "wx", mode: 0o600 });
console.log(JSON.stringify({ corpus: resolve(output), ...manifest }));
