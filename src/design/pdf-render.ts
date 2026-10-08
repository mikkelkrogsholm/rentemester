import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

export type PdfDocument = { html: string; title: string; date: string; footer: string };
export const PDF_RENDER_TIMEOUT_MS = 15_000;
const MAX_INPUT = 8 * 1024 * 1024;
const MAX_OUTPUT = 32 * 1024 * 1024;

/** Synchronous boundary preserves the ledger transaction and rollback contract.
 * A renderer child never receives ledger paths, credentials or mutable state.
 */
export function renderDocumentPdf(document: PdfDocument, options: { timeoutMs?: number } = {}): Buffer {
  const input = JSON.stringify(document);
  if (Buffer.byteLength(input) > MAX_INPUT) throw new Error("PDF render input exceeds 8 MiB");
  const timeout = options.timeoutMs ?? PDF_RENDER_TIMEOUT_MS;
  if (!Number.isInteger(timeout) || timeout < 1 || timeout > PDF_RENDER_TIMEOUT_MS) throw new Error("PDF render timeout must be between 1 and 15000 ms");
  const result = spawnSync(process.execPath, [resolve(import.meta.dir, "pdf-render-child.ts")], {
    input,
    timeout,
    killSignal: "SIGKILL",
    maxBuffer: MAX_OUTPUT,
    env: { LANG: "C.UTF-8", TZ: "UTC" },
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  });
  if (result.error) {
    const code = (result.error as NodeJS.ErrnoException).code;
    throw new Error(code === "ETIMEDOUT" ? "PDF render timed out" : code === "ENOBUFS" ? "PDF render output limit exceeded" : "PDF renderer could not start");
  }
  if (result.status !== 0 || result.signal) throw new Error("PDF renderer failed");
  if (!result.stdout?.subarray(0, 5).equals(Buffer.from("%PDF-"))) throw new Error("PDF renderer returned invalid output");
  return result.stdout;
}
