import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import type { Page } from "@playwright/test";

const saved = "/tmp/rentemester-rust-reports/native-workspace-parties.json";
type Party = { partyId: string; name: string; roles: Array<{ companySlug: string; role: string; defaults: unknown }>; assertions: Array<{ field: string; value: string; review_state: string }>; history: Array<{ event_type: string }>; [key: string]: unknown };
type Evidence = { partyId: string; canonical: { ok: boolean; party: Party }; knowledgeId: string; knowledge: unknown };
/** Own synthetic canonical identity; ordinary existing MFA only, no SQL/auth/grant or ledger mutation. */
export async function checkNativeWorkspaceParties(page: Page, base: string, restarted = false) {
 if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(base)) throw new Error("Owned synthetic loopback fixture required");
 const company = "synthetic", url = `${base}/api/companies/${company}`, headers = { origin: base, "content-type": "application/json" };
 const get = async (path: string) => { const r = await page.request.get(`${url}/${path}`); if (!r.ok()) throw new Error(`Party read refused ${path}: ${r.status()}`); return r.json(); };
 const post = async (path: string, data: unknown, status = 200) => { const r = await page.request.post(`${url}/${path}`, { headers, data }); if (r.status() !== status) throw new Error(`Party ${path}: expected ${status}, got ${r.status()}`); return r.json(); };
 const temp = mkdtempSync("/tmp/rm-workspace-party-clients-");
 try {
  const cookie = join(temp, "session.cookie"); writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
  const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
  const cli = (command: string, flags: string[] = [], body?: unknown) => { const r = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", command, ...common, "--company", company, ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body === undefined ? undefined : JSON.stringify(body), encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }); if (r.error || r.status !== 0) throw new Error(`Party CLI ${command} failed`); return JSON.parse(r.stdout).data; };
  const mcp = (name: string, args: Record<string, unknown>) => { const input = [{ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-workspace-party", version: "1" } } }, { jsonrpc: "2.0", method: "notifications/initialized" }, { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company, ...args } } }].map(v => JSON.stringify(v)).join("\n") + "\n"; const r = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }); if (r.error || r.status !== 0) throw new Error("Party MCP process failed"); const call = r.stdout.trim().split("\n").map(l => JSON.parse(l)).find(v => v.id === 2); if (call?.error || call?.result?.isError) throw new Error(`Party MCP ${name} failed`); return call.result.structuredContent.data; };
  let evidence: Evidence;
  if (!restarted) {
   const before = await get("journal");
   const initial = { kind: "organization", name: "Synthetic Native Workspace Party", aliases: ["Synthetic Native Party Alias"], identifiers: [{ country: "US", identifierKind: "non_eu", identifier: "SYN-NATIVE-PARTY-CLAIM" }], source: "synthetic-native-party-brief", observedAt: "2000-01-01", reviewAssertion: "Synthetic owner reviewed canonical source", role: "supplier", defaults: { currency: "DKK", paymentTermsDays: 21 }, confirm: true };
   await page.goto(`${base}/companies/${company}/workspace-register`);
   const panel = page.getByRole("region", { name: "Workspace-parter" });
   await panel.getByLabel("Party name", { exact: true }).fill(initial.name);
   await panel.getByLabel("Party aliases", { exact: true }).fill(JSON.stringify(initial.aliases));
   await panel.getByLabel("Party identifiers", { exact: true }).fill(JSON.stringify(initial.identifiers));
   await panel.getByLabel("Party source", { exact: true }).fill(initial.source);
   await panel.getByLabel("Party observed at", { exact: true }).fill(initial.observedAt);
   await panel.getByLabel("Party review assertion", { exact: true }).fill(initial.reviewAssertion);
   await panel.getByLabel("Party defaults", { exact: true }).fill(JSON.stringify(initial.defaults));
   await panel.getByLabel("Party confirm", { exact: true }).check();
   const created = page.waitForResponse(r => r.url() === `${url}/workspace-parties` && r.request().method() === "POST");
   await panel.getByRole("button", { name: "Registrér partændring", exact: true }).click();
   const response = await created; if (!response.ok()) throw new Error("Native UI party creation refused"); const partyId: string = (await response.json()).party.partyId;
   await panel.getByText(`Partens oprindelse og historik: ${partyId}`, { exact: true }).waitFor();
   await panel.getByLabel("Party action", { exact: true }).selectOption("assertion");
   await panel.getByLabel("Party value", { exact: true }).fill("Synthetic Native Party Reviewed Name");
   await panel.getByLabel("Party source", { exact: true }).fill("synthetic-native-party-reviewed-snapshot");
   await panel.getByLabel("Party observed at", { exact: true }).fill("2001-01-01");
   await panel.getByLabel("Party review state", { exact: true }).selectOption("approved");
   await panel.getByLabel("Party confirm", { exact: true }).check();
   const amended = page.waitForResponse(r => r.url() === `${url}/workspace-parties/${partyId}/assertion` && r.request().method() === "POST");
   await panel.getByRole("button", { name: "Registrér partændring", exact: true }).click(); if (!(await amended).ok()) throw new Error("Native UI party assertion refused");
   cli("assert-workspace-party-alias", ["--party-id", partyId], { alias: "Synthetic Native Party Reviewed Alias", source: "synthetic-native-party-reviewed-alias", observedAt: "2001-02-01", reviewState: "approved", confirm: true });
   mcp("link_workspace_party_role", { partyId, body: JSON.stringify({ role: "customer", defaults: { currency: "EUR" }, confirm: true }), confirm: true });
   const canonical = await get(`workspace-parties/${partyId}`) as { ok: boolean; party: Party };
   if (canonical.party.name !== initial.name || !canonical.party.assertions.some(a => a.field === "name" && a.value === "Synthetic Native Party Reviewed Name" && a.review_state === "approved") || canonical.party.roles.filter(r => r.companySlug === company).length !== 2 || canonical.party.history.some(h => h.event_type === "linked")) throw new Error("Canonical party provenance or scope projection differs");
   cli("create-workspace-party", [], { ...initial, partyId }); if (!isDeepStrictEqual(await get(`workspace-parties/${partyId}`), canonical)) throw new Error("Party create replay duplicated/rewrote assertions");
   await post(`workspace-parties/${partyId}/role`, { role: "supplier", defaults: { currency: "EUR" }, confirm: true }, 422);
   await post(`workspace-parties/${partyId}/role`, { role: "invented_role", confirm: true }, 422);
   await post(`workspace-parties/${partyId}/assertion`, { field: "name", value: "Forged", source: "synthetic", observedAt: "2001-01-01", reviewState: "approved", actor: "user:forged", confirm: true }, 400);
   if (!isDeepStrictEqual(await get(`workspace-parties/${partyId}`), canonical)) throw new Error("Refused party writes changed source history");
   const knowledgeInput = { predicate: "responsible_party", value: { purpose: "Synthetic native party source" }, source: { kind: "party", ref: partyId }, validFrom: "2000-01-01", confirm: true };
   const knowledgeId: string = (await post("knowledge/propose", knowledgeInput)).assertion.assertionId;
   mcp("review_company_knowledge", { body: JSON.stringify({ assertionId: knowledgeId, decision: "approved", confirm: true }), confirm: true });
   await post("knowledge/propose", { ...knowledgeInput, source: { kind: "party", ref: "missing-synthetic-native-party-source" } }, 422);
   const knowledge = (await get("knowledge?asOf=2026-06-01&includeProposed=true")).history.find((a: { assertionId: string }) => a.assertionId === knowledgeId);
   if (knowledge?.reviewState !== "approved" || knowledge?.source?.ref !== partyId) throw new Error("Visible canonical party source did not support knowledge review");
   if (!isDeepStrictEqual(await get("journal"), before)) throw new Error("Workspace party/knowledge flow changed journal");
   evidence = { partyId, canonical, knowledgeId, knowledge }; mkdirSync("/tmp/rentemester-rust-reports", { recursive: true }); writeFileSync(saved, JSON.stringify(evidence), { mode: 0o600 });
  } else {
   evidence = JSON.parse(readFileSync(saved, "utf8"));
   if (!isDeepStrictEqual(await get(`workspace-parties/${evidence.partyId}`), evidence.canonical)) throw new Error("Canonical party/history changed after restart");
   const knowledge = (await get("knowledge?asOf=2026-06-01&includeProposed=true")).history.find((a: { assertionId: string }) => a.assertionId === evidence.knowledgeId); if (!isDeepStrictEqual(knowledge, evidence.knowledge)) throw new Error("Party-backed knowledge history changed after restart");
  }
  const canonical = await get(`workspace-parties/${evidence.partyId}`), list = await get("workspace-parties?query=Synthetic%20Native%20Party%20Reviewed%20Alias&cursor=0&limit=25");
  if (!isDeepStrictEqual(cli("workspace-party", ["--party-id", evidence.partyId]), canonical) || !isDeepStrictEqual(mcp("workspace_party", { partyId: evidence.partyId }), canonical) || !isDeepStrictEqual(cli("workspace-parties", ["--query", "Synthetic Native Party Reviewed Alias", "--cursor", "0", "--limit", "25"]), list) || !isDeepStrictEqual(mcp("workspace_parties", { query: "Synthetic Native Party Reviewed Alias", cursor: 0, limit: 25 }), list)) throw new Error("Party HTTP/CLI/MCP full JSON differs");
  return { ok: true, restarted, ...evidence, accountingEffect: "none", currentHiddenScopeRevocationCoverage: "application tests; helper uses normal MFA principal", backupCoverage: "nonempty parties remain explicitly unsupported by current codec" };
 } finally { rmSync(temp, { recursive: true, force: true }); }
}
