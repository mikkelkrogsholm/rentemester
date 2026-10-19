import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";
import type { Page } from "@playwright/test";

const evidencePath = "/tmp/rentemester-rust-reports/native-corporate-record-clients.json";
/** Existing normal MFA, own API-created company/records; no identity/grant seeds or saved global-list counts. */
export async function checkNativeCorporateClients(page: Page, base: string, restarted = false) {
 if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(base)) throw Error("Synthetic literal loopback fixture required");
 const cliBinary = process.env.RM_NATIVE_CLI_BIN, mcpBinary = process.env.RM_NATIVE_MCP_BIN;
 if (!cliBinary || !mcpBinary) throw Error("Explicit qualified CLI/MCP artifacts required");
 const anchor = "synthetic", target = "synthetic-corporate-clients-target";
 const ids = ["record-client-corporate-original", "record-client-corporate-replacement"];
 const originals = [Buffer.from("Synthetic client corporate original\nNo personal or production data\n"), Buffer.from("Synthetic client corporate replacement\nExplicit revised source\n")];
 const headers = { origin: base, "content-type": "application/json" };
 const url = (slug: string, path: string) => `${base}/api/companies/${slug}/${path}`;
 const get = async (slug: string, path: string) => { const r = await page.request.get(url(slug, path)); if (!r.ok()) throw Error(`Corporate HTTP read refused: ${r.status()}`); return r.json(); };
 const journal = async () => ({ anchor: (await get(anchor, "journal")).journal.entries, target: (await get(target, "journal")).journal.entries });
 if (!restarted) {
  const created = await page.request.post(`${base}/api/companies`, { headers, data: { name: "Synthetic corporate client target", slug: target } });
  if (created.status() !== 201) throw Error(`Own corporate client company creation refused: ${created.status()}`);
 }
 const targetUuid = (await get(target, "journal")).journal.ledgerUuid, before = await journal();
 const temp = mkdtempSync("/tmp/rm-corporate-clients-");
 try {
  const cookie = join(temp, "session.cookie"); writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
  const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
  const cli = (command: string, flags: string[] = [], body?: string) => {
   const r = spawnSync(cliBinary, ["remote", command, ...common, "--company", anchor, ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 64 * 1024 * 1024, timeout: 30000 });
   if (r.error || r.status !== 0) throw Error(`Corporate CLI ${command} failed`); return JSON.parse(r.stdout).data;
  };
  const mcp = (name: string, args: Record<string, unknown>) => {
   const messages = [{ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-corporate-clients", version: "1" } } }, { jsonrpc: "2.0", method: "notifications/initialized" }, { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company: anchor, ...args } } }];
   const r = spawnSync(mcpBinary, common, { input: messages.map(m => JSON.stringify(m)).join("\n") + "\n", encoding: "utf8", maxBuffer: 64 * 1024 * 1024, timeout: 30000 });
   if (r.error || r.status !== 0) throw Error(`Corporate MCP ${name} process failed`); const call = r.stdout.trim().split("\n").map(row => JSON.parse(row)).find(row => row.id === 2);
   if (call?.error || call?.result?.isError) throw Error(`Corporate MCP ${name} refused`); return call.result.structuredContent.data;
  };
  let evidence: any;
  if (!restarted) {
   const body = (index: number) => JSON.stringify({ recordId: ids[index], type: "articles", bytesBase64: originals[index].toString("base64"), filename: `synthetic-corporate-client-${index}.txt`, source: "Synthetic client explicitly reviewed source", receivedAt: "2000-01-01", uploader: "Synthetic client selected uploader", sensitivity: "normal", links: [{ type: "company", id: anchor }], confirm: true });
   cli("ingest-corporate-record", [], body(0)); mcp("ingest_corporate_record", { body: body(1), confirm: true });
   mcp("link_corporate_record", { recordId: ids[0], body: JSON.stringify({ type: "company", id: target, confirm: true }), confirm: true });
   cli("enrich-corporate-record", ["--record-id", ids[0]], JSON.stringify({ assertion: "Synthetic client reviewed explicit additional source context", confirm: true }));
   mcp("supersede_corporate_record", { recordId: ids[0], body: JSON.stringify({ replacementRecordId: ids[1], reason: "Synthetic reviewed revised source", confirm: true }), confirm: true });
   const records = await Promise.all(ids.map(id => get(anchor, `corporate-records/${id}`)));
   if (!records[0].record.links.some((l: any) => l.type === "company" && l.id === target) || !["ingested", "linked", "enriched", "superseded"].every(type => records[0].record.history.some((h: any) => h.event_type === type))) throw Error("Corporate explicit company scope/source lifecycle missing");
   if (!isDeepStrictEqual(cli("ingest-corporate-record", [], body(0)), records[0])) throw Error("Explicit same-id/same-bytes source replay changed current record");
   const refused = await page.request.post(url(anchor, "corporate-records"), { headers, data: { ...JSON.parse(body(0)), bytesBase64: Buffer.from("Synthetic changed immutable bytes").toString("base64") } });
   if (refused.status() !== 422 || !isDeepStrictEqual(await get(anchor, `corporate-records/${ids[0]}`), records[0])) throw Error("Immutable original changed or refusal mutated history");
   evidence = { targetUuid, records, ids, originalSha256: originals.map(b => createHash("sha256").update(b).digest("hex")) };
   mkdirSync("/tmp/rentemester-rust-reports", { recursive: true }); writeFileSync(evidencePath, JSON.stringify(evidence), { mode: 0o600 });
  } else evidence = JSON.parse(readFileSync(evidencePath, "utf8"));
  if (targetUuid !== evidence.targetUuid) throw Error("Own corporate client target UUID changed");
  const list = await get(anchor, "corporate-records?cursor=0&limit=100");
  if (!ids.every(id => list.rows.some((r: any) => r.recordId === id)) || !isDeepStrictEqual(cli("corporate-records", ["--cursor", "0", "--limit", "100"]), list) || !isDeepStrictEqual(mcp("corporate_records", { cursor: 0, limit: 100 }), list)) throw Error("Corporate full canonical list HTTP/CLI/MCP differs");
  for (let i = 0; i < ids.length; i++) {
   const canonical = await get(anchor, `corporate-records/${ids[i]}`), original = await get(anchor, `corporate-records/${ids[i]}/download`);
   if (!isDeepStrictEqual(canonical, evidence.records[i]) || !isDeepStrictEqual(cli("corporate-record", ["--record-id", ids[i]]), canonical) || !isDeepStrictEqual(mcp("corporate_record", { recordId: ids[i] }), canonical) || !isDeepStrictEqual(cli("corporate-record-original", ["--record-id", ids[i]]), original) || !isDeepStrictEqual(mcp("corporate_record_original", { recordId: ids[i] }), original)) throw Error("Corporate own full record/history/original client parity or restart differs");
   const bytes = Buffer.from(original.bytesBase64, "base64"), hash = createHash("sha256").update(bytes).digest("hex");
   if (!bytes.equals(originals[i]) || hash !== evidence.originalSha256[i] || original.sha256 !== hash || canonical.record.sha256 !== hash) throw Error("Corporate immutable source bytes differ");
  }
  if (!isDeepStrictEqual(await journal(), before)) throw Error("Corporate source lifecycle changed accounting");
  return { ok: true, restarted, ...evidence, fullCanonicalClientParity: true, accountingEffect: "none" };
 } finally { rmSync(temp, { recursive: true, force: true }); }
}
