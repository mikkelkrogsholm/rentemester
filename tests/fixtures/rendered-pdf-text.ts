import { parsePdfBytes } from "../../src/core/document-pdf-parser";

/** Inspect the actual selectable text layer, independent of renderer encoding. */
export async function pdfStrings(bytes: Uint8Array): Promise<string[]> {
  const parsed = await parsePdfBytes(bytes);
  if (parsed.status !== "ok") throw new Error(`Unreadable rendered PDF: ${parsed.status}`);
  return parsed.pages.flatMap(page => [
    ...page.layout.map(item => item.text), ...page.text.split("\n"),
    ...(page.text.match(/-?\d[\d.]*,\d{2} [A-Z]{3}/g) ?? []),
  ]);
}

export async function pdfText(bytes: Uint8Array): Promise<string> {
  const parsed = await parsePdfBytes(bytes);
  if (parsed.status !== "ok") throw new Error(`Unreadable rendered PDF: ${parsed.status}`);
  return parsed.pages.map(page => page.text).join("\n");
}
