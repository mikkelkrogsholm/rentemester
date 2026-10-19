import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";
import type { Page } from "@playwright/test";

const evidencePath = "/tmp/rentemester-rust-reports/native-workspace-inbox-clients.json";
/** Ordinary existing MFA only; own API-created company/source, no identity/grant seeds or global-list snapshots. */
export async function checkNativeWorkspaceInboxClients(page: Page, base: string, restarted = false) {
 if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(base)) throw Error("Synthetic literal loopback fixture required");
 const cliBinary = process.env.RM_NATIVE_CLI_BIN, mcpBinary = process.env.RM_NATIVE_MCP_BIN;
 if (!cliBinary || !mcpBinary) throw Error("Explicit qualified CLI/MCP artifacts required");
 const anchor = "synthetic", target = "synthetic-workspace-inbox-clients-target", sourceId = "inbox-synthetic-client-parity";
 const api = `${base}/api/companies/${anchor}`, headers = { origin: base, "content-type": "application/json" };
 const bytes = Buffer.from("Synthetic native workspace inbox client original\nNo personal or production data\n");
 const sha256 = (b: Buffer) => createHash("sha256").update(b).digest("hex");
 const get = async (slug: string, path: string) => { const r = await page.request.get(`${base}/api/companies/${slug}/${path}`); if (!r.ok()) throw Error(`Inbox client HTTP read refused: ${r.status()}`); return r.json(); };
 const journal = async () => ({ anchor: (await get(anchor, "journal")).journal.entries, target: (await get(target, "journal")).journal.entries });
 if (!restarted) {
  const created = await page.request.post(`${base}/api/companies`, { headers, data: { name: "Synthetic workspace inbox clients target", slug: target } });
  if (created.status() !== 201) throw Error(`Own inbox client company creation refused: ${created.status()}`);
 }
 const targetUuid = (await get(target, "journal")).journal.ledgerUuid;
 const before = await journal();
 const temp = mkdtempSync("/tmp/rm-workspace-inbox-clients-");
 try {
  const cookie = join(temp, "session.cookie"); writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
  const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
  const cli = (command: string, flags: string[] = [], body?: string) => {
   const r = spawnSync(cliBinary, ["remote", command, ...common, "--company", anchor, ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024, timeout: 30000 });
   if (r.error || r.status !== 0) throw Error(`Inbox CLI ${command} failed`); return JSON.parse(r.stdout).data;
  };
  const mcp = (name: string, args: Record<string, unknown>) => {
   const messages = [{ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-inbox-clients", version: "1" } } }, { jsonrpc: "2.0", method: "notifications/initialized" }, { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company: anchor, ...args } } }];
   const r = spawnSync(mcpBinary, common, { input: messages.map(m => JSON.stringify(m)).join("\n") + "\n", encoding: "utf8", maxBuffer: 16 * 1024 * 1024, timeout: 30000 });
   if (r.error || r.status !== 0) throw Error(`Inbox MCP ${name} process failed`);
   const call = r.stdout.trim().split("\n").map(row => JSON.parse(row)).find(row => row.id === 2);
   if (call?.error || call?.result?.isError) throw Error(`Inbox MCP ${name} refused`); return call.result.structuredContent.data;
  };
  let evidence: any;
  if (!restarted) {
   const body = JSON.stringify({ confirm: true, sourceId, idempotencyKey: "synthetic-client-inbox-source-v1", bytesBase64: bytes.toString("base64"), filename: "synthetic-agent-workspace-inbox.txt", mimeType: "text/plain", transport: "upload", receivedAt: "2000-01-01", metadata: { documentType: "cash_register_receipt", currency: "DKK" }, candidates: [] });
   const created = cli("ingest-workspace-inbox", [], body);
   if (created.source?.sourceId !== sourceId || !isDeepStrictEqual(mcp("ingest_workspace_inbox", { body, confirm: true }), created)) throw Error("Native same-source explicit replay changed source identity/history");
   const destination = JSON.stringify({ companySlug: target, confirm: true });
   mcp("assign_workspace_inbox", { sourceId, body: destination, confirm: true });
   const assigned = await get(anchor, `workspace-inbox/${sourceId}`);
   if (!assigned.source.assignments.some((a: any) => a.companySlug === target && a.state === "approved")) throw Error("Explicit destination assignment missing before separate completion");
   const completed = cli("complete-workspace-inbox", ["--source-id", sourceId], destination);
   const assignment = completed.source?.assignments.find((a: any) => a.companySlug === target && a.state === "completed");
   if (!assignment || completed.source.history.filter((h: any) => h.event_type === "handoff_completed").length !== 1) throw Error("Exactly one completed source handoff missing");
   const refusal = await page.request.post(`${api}/workspace-inbox`, { headers, data: { ...JSON.parse(body), filename: "changed-intent.txt" } });
   if (refusal.status() !== 422 || !isDeepStrictEqual(await get(anchor, `workspace-inbox/${sourceId}`), completed)) throw Error("Changed source intent was accepted or changed canonical source");
   evidence = { sourceId, source: completed, documentId: assignment.documentId, targetUuid, originalSha256: sha256(bytes) };
   mkdirSync("/tmp/rentemester-rust-reports", { recursive: true }); writeFileSync(evidencePath, JSON.stringify(evidence), { mode: 0o600 });
  } else evidence = JSON.parse(readFileSync(evidencePath, "utf8"));
  if (evidence.targetUuid !== targetUuid) throw Error("Own inbox client target UUID changed");
  const canonical = await get(anchor, `workspace-inbox/${sourceId}`), list = await get(anchor, "workspace-inbox?cursor=0&limit=100");
  if (!isDeepStrictEqual(canonical, evidence.source) || !isDeepStrictEqual(cli("workspace-inbox-source", ["--source-id", sourceId]), canonical) || !isDeepStrictEqual(mcp("workspace_inbox_source", { sourceId }), canonical) || !isDeepStrictEqual(cli("workspace-inbox", ["--cursor", "0", "--limit", "100"]), list) || !isDeepStrictEqual(mcp("workspace_inbox", { cursor: 0, limit: 100 }), list)) throw Error("Inbox canonical source/list HTTP/CLI/MCP differs or changed after restart");
  const docs = (await get(target, "documents")).documents.documents.filter((d: any) => d.source === `workspace-inbox:${sourceId}`);
  if (docs.length !== 1 || docs[0].id !== evidence.documentId || docs[0].documentType !== "cash_register_receipt") throw Error("Canonical target document/source identity differs");
  const original = await page.request.get(`${base}/api/companies/${target}/documents/${evidence.documentId}/file`);
  if (!original.ok() || sha256(await original.body()) !== evidence.originalSha256 || !isDeepStrictEqual(await journal(), before)) throw Error("Inbox handoff changed original or journal");
  return { ok: true, restarted, ...evidence, fullCanonicalClientParity: true, accountingEffect: "none" };
 } finally { rmSync(temp, { recursive: true, force: true }); }
}
