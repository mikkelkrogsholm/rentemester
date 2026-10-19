import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
const saved = "/tmp/rentemester-rust-reports/native-document-clients.json";

/** Own synthetic original only. No journal posting, forced import, local document files or guessed production metadata. */
export async function checkNativeDocumentClients(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const temp = mkdtempSync("/tmp/rm-native-document-clients-");
  try {
    const cookie = join(temp, "session.cookie");
    writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
    function cli(command: string, body?: string, id?: number, refused = false) {
      const run = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", command, ...common, "--company", "synthetic", ...(id ? ["--id", String(id)] : []), ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 128 * 1024 * 1024 });
      if (run.error) throw new Error("Native document CLI unavailable; build rentemester-cli or set RM_NATIVE_CLI_BIN");
      const value = JSON.parse(run.stdout);
      if (refused) { if (run.status !== 1 || value.code !== "server_refused") throw new Error("Expected definite native document refusal"); return value; }
      if (run.status !== 0) throw new Error(`Native document CLI ${command} failed`);
      return value.data;
    }
    function mcp(name: string, args: Record<string, unknown>, refused = false) {
      const input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-document-check", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company: "synthetic", ...args } } },
      ].map(v => JSON.stringify(v)).join("\n") + "\n";
      const run = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 128 * 1024 * 1024 });
      if (run.error || run.status !== 0) throw new Error("Native document MCP unavailable; build rentemester-mcp or set RM_NATIVE_MCP_BIN");
      const call = run.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
      if (refused) { if (call?.result?.isError !== true || call.result.structuredContent.error.code !== "server_refused" || call.result.structuredContent.error.status !== 422) throw new Error("Expected definite native document422 refusal"); return call.result; }
      if (call?.error || call?.result?.isError || !call?.result?.structuredContent?.data) throw new Error(`Native document MCP ${name} failed`);
      return call.result.structuredContent.data;
    }
    async function journal() {
      const years = (await (await page.request.get(`${url}/fiscal-years`)).json()).fiscalYears.years;
      const entries = [];
      for (const year of years) entries.push((await (await page.request.get(`${url}/journal?year=${encodeURIComponent(year.label)}`)).json()).journal.entries);
      return JSON.stringify(entries);
    }
    const before = await journal();
    let evidence: { id: number; documentNo: string; body: string; bytes: number[]; mime: string };
    if (!restarted) {
      const bytes = Buffer.from("Synthetic agent cash receipt original\nNo accounting posting requested.\n", "utf8");
      const metadata = { source: "synthetic-native-document-clients", documentType: "cash_register_receipt", issueDate: new Date().toISOString().slice(0, 10), deliveryDescription: "Synthetic receipt", amountIncVat: 125, vatAmount: 25, currency: "DKK" };
      const body = JSON.stringify({ confirm: true, fileName: "synthetic-agent-original.txt", fileBase64: bytes.toString("base64"), metadata });
      // The server must retain its MIME/source-validation gate; adapters do not infer/correct facts.
      cli("ingest-document", JSON.stringify({ confirm: true, fileName: "synthetic-spoofed.pdf", fileBase64: bytes.toString("base64"), metadata }), undefined, true);
      const document = cli("ingest-document", body).document;
      if (!Number.isSafeInteger(document.id) || !document.documentNo) throw new Error("Agent upload missing native document receipt");
      mcp("ingest_document", { body, confirm: true }, true);
      mcp("ingest_document", { body: JSON.stringify({ confirm: true, fileName: "synthetic-bad-source.txt", fileBase64: Buffer.from("Synthetic invalid metadata source\n").toString("base64"), metadata: { source: "" } }), confirm: true }, true);
      evidence = { id: document.id, documentNo: document.documentNo, body, bytes: [...bytes], mime: "text/plain" };
      writeFileSync(saved, JSON.stringify(evidence));
    } else evidence = JSON.parse(readFileSync(saved, "utf8"));
    const response = await page.request.get(`${url}/documents/${evidence.id}/file`);
    if (!response.ok() || response.headers()["content-type"] !== evidence.mime) throw new Error("Canonical native original unavailable or MIME changed");
    const original = await response.body();
    if (!original.equals(Buffer.from(evidence.bytes))) throw new Error("Stored original differs from agent-uploaded exact source bytes");
    for (const [name, value] of [["CLI", cli("document-original", undefined, evidence.id).original], ["MCP", mcp("document_original", { id: evidence.id }).original]] as const) {
      if (value?.contentType !== evidence.mime || !Array.isArray(value.bytes) || !value.bytes.every((b: unknown) => typeof b === "number" && Number.isInteger(b) && b >= 0 && b <= 255) || !Buffer.from(value.bytes).equals(original)) throw new Error(`${name} original differs from actual authenticated HTTP MIME/bytes`);
    }
    const documents = (await (await page.request.get(`${url}/documents`)).json()).documents.documents;
    if (documents.filter((d: { id: number }) => d.id === evidence.id).length !== 1) throw new Error("Duplicate upload changed native document identity");
    const anonymous = await page.context().browser()!.newContext();
    try { if ((await anonymous.request.get(`${url}/documents/${evidence.id}/file`)).ok()) throw new Error("Anonymous original read accepted"); } finally { await anonymous.close(); }
    if (await journal() !== before) throw new Error("Agent ingestion/original/refusals changed journal entries");
    console.log(restarted ? "PASS: agent-uploaded original MIME/bytes equal canonical HTTP/CLI/MCP after restart; no journal effects" : "PASS: CLI upload → exact authorized HTTP/CLI/MCP original; MCP duplicate and invalid metadata, CLI MIME spoof refused; no journal effects");
  } finally { rmSync(temp, { recursive: true, force: true }); }
}
