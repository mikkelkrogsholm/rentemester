import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
import type { Page } from "@playwright/test";

const saved = "/tmp/rentemester-rust-reports/native-company-knowledge.json";
type Knowledge = { ok: boolean; context: { assertions: Array<{ assertionId: string; reviewState: string }>; conflicts: string[]; safeForProductBehavior: boolean }; history: Array<{ assertionId: string; reviewState: string; value: unknown; history: unknown[] }> };
type Evidence = { firstId: string; replacementId: string; conflictId: string; canonical: Knowledge };
/** Ordinary existing MFA context. Own synthetic assertions only; no identities, grants or ledger writes. */
export async function checkNativeCompanyKnowledge(page: Page, base: string, restarted = false) {
 if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(base)) throw new Error("Owned synthetic loopback fixture required");
 const company = "synthetic", url = `${base}/api/companies/${company}`, headers = { origin: base, "content-type": "application/json" };
 const get = async (path: string) => { const r = await page.request.get(`${url}/${path}`); if (!r.ok()) throw new Error(`Knowledge read refused ${path}: ${r.status()}`); return r.json(); };
 const post = async (path: string, data: unknown, status = 200) => { const r = await page.request.post(`${url}/${path}`, { headers, data }); if (r.status() !== status) throw new Error(`Knowledge ${path}: expected ${status}, got ${r.status()}`); return r.json(); };
 const future = "2099-06-01", selectors = `knowledge?asOf=${future}&includeProposed=true`;
 const temp = mkdtempSync("/tmp/rm-company-knowledge-clients-");
 try {
  const cookie = join(temp, "session.cookie"); writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
  const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
  function cli(command: string, body?: unknown) { const r = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", command, ...common, "--company", company, ...(body === undefined ? ["--as-of", future, "--include-proposed", "true"] : ["--confirm"])], { input: body === undefined ? undefined : JSON.stringify(body), encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }); if (r.error || r.status !== 0) throw new Error(`Knowledge CLI ${command} failed`); return JSON.parse(r.stdout).data; }
  function mcp(name: string, args: Record<string, unknown>) { const input = [{ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-knowledge", version: "1" } } }, { jsonrpc: "2.0", method: "notifications/initialized" }, { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company, ...args } } }].map(v => JSON.stringify(v)).join("\n") + "\n"; const r = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }); if (r.error || r.status !== 0) throw new Error("Knowledge MCP process failed"); const call = r.stdout.trim().split("\n").map(l => JSON.parse(l)).find(v => v.id === 2); if (call?.error || call?.result?.isError) throw new Error(`Knowledge MCP ${name} failed`); return call.result.structuredContent.data; }
  let evidence: Evidence;
  if (!restarted) {
   const before = await get("journal");
   await page.goto(`${base}/companies/${company}/workspace-register`);
   const panel = page.getByRole("region", { name: "Virksomhedsviden" });
   await panel.getByLabel("Knowledge value", { exact: true }).fill('"Synthetic native knowledge initial"');
   await panel.getByLabel("Knowledge predicate", { exact: true }).selectOption("operating_status");
   await panel.getByLabel("Knowledge source ref", { exact: true }).fill("synthetic-native-knowledge-brief-1");
   await panel.getByLabel("Knowledge valid from", { exact: true }).fill("2000-01-01");
   await panel.getByLabel("Knowledge confirm", { exact: true }).check();
   await panel.getByRole("button", { name: "Registrér vidensforslag", exact: true }).click();
   await panel.getByText("Ændringen er registreret. Læs den aktuelle tilstand og historik.", { exact: true }).waitFor();
   const proposed = (await get(selectors) as Knowledge).history.find(a => a.value === "Synthetic native knowledge initial"); if (!proposed || proposed.reviewState !== "proposed") throw new Error("Native proposal missing");
   const firstId = proposed.assertionId;
   await panel.locator("summary").filter({ hasText: firstId }).click();
   await panel.getByLabel("Knowledge confirm", { exact: true }).check();
   await panel.getByRole("button", { name: `Godkend ${firstId}`, exact: true }).click();
   await panel.locator("summary").filter({ hasText: `${firstId} · operating_status · approved` }).waitFor();
   await panel.getByRole("button", { name: `Forbered erstatning af ${firstId}`, exact: true }).click();
   await panel.getByLabel("Knowledge value", { exact: true }).fill('"Synthetic native knowledge future"');
   await panel.getByLabel("Knowledge source ref", { exact: true }).fill("synthetic-native-knowledge-brief-2");
   await panel.getByLabel("Knowledge valid from", { exact: true }).fill("2099-01-01");
   await panel.getByLabel("Knowledge confirm", { exact: true }).check();
   await panel.getByRole("button", { name: "Registrér erstatningsforslag", exact: true }).click();
   await panel.getByRole("button", { name: "Registrér vidensforslag", exact: true }).waitFor();
   const changed = await get(selectors) as Knowledge, replacement = changed.history.find(a => a.value === "Synthetic native knowledge future");
   if (!replacement || replacement.reviewState !== "proposed" || changed.history.find(a => a.assertionId === firstId)?.reviewState !== "superseded") throw new Error("Native supersession history differs");
   const replacementId = replacement.assertionId;
   mcp("review_company_knowledge", { body: JSON.stringify({ assertionId: replacementId, decision: "approved", reason: "Synthetic source reviewed", confirm: true }), confirm: true });
   const current = await get("knowledge?asOf=2026-06-01&includeProposed=true") as Knowledge; if (current.context.assertions.some(a => a.assertionId === firstId || a.assertionId === replacementId)) throw new Error("Superseded/future knowledge leaked into current context");
   const proposal = { predicate: "operating_status", value: "Synthetic native knowledge future", source: { kind: "user", ref: "synthetic-native-knowledge-brief-2" }, validFrom: "2099-01-01", certainty: "confirmed", confirm: true };
   const duplicate = cli("propose-company-knowledge", proposal); if (duplicate.assertion.assertionId !== replacementId || duplicate.assertion.reviewState !== "approved") throw new Error("Knowledge source dedup changed reviewed state");
   const stable = await get(selectors);
   const { confirm: _, ...replacementBody } = proposal;
   await post("knowledge/supersede", { assertionId: replacementId, replacement: replacementBody, confirm: true }, 422);
   if (!isDeepStrictEqual(await get(selectors), stable)) throw new Error("Self-supersession changed knowledge history");
   await post("knowledge/review", { assertionId: firstId, decision: "approved", confirm: true }, 422);
   await post("knowledge/propose", { ...proposal, source: { kind: "party", ref: "missing-synthetic-party" } }, 422);
   await post("knowledge/propose", { ...proposal, actor: "user:forged" }, 400);
   await post("knowledge/propose", { ...proposal, validToExclusive: "2000-01-01" }, 422);
   const unknown = await page.request.get(`${url}/knowledge?asOf=2026-01-01&asOf=2027-01-01`); if (unknown.status() !== 400) throw new Error("Mixed knowledge date selectors accepted");
   const conflict = cli("propose-company-knowledge", { ...proposal, value: "Synthetic conflicting future knowledge", source: { kind: "registry", ref: "synthetic-native-knowledge-snapshot-3" } });
   const conflictId = conflict.assertion.assertionId;
   mcp("review_company_knowledge", { body: JSON.stringify({ assertionId: conflictId, decision: "approved", confirm: true }), confirm: true });
   const canonical = await get(selectors) as Knowledge;
   if (!canonical.context.conflicts.includes("operating_status") || canonical.context.safeForProductBehavior !== false || canonical.context.assertions.filter(a => [replacementId, conflictId].includes(a.assertionId)).some(a => a.reviewState !== "conflict")) throw new Error("Knowledge conflict projection missing");
   if (!isDeepStrictEqual(await get("journal"), before)) throw new Error("Advisory knowledge changed journal");
   evidence = { firstId, replacementId, conflictId, canonical }; mkdirSync("/tmp/rentemester-rust-reports", { recursive: true }); writeFileSync(saved, JSON.stringify(evidence), { mode: 0o600 });
  } else {
   evidence = JSON.parse(readFileSync(saved, "utf8"));
   if (!isDeepStrictEqual(await get(selectors), evidence.canonical)) throw new Error("Knowledge effective context/history changed after restart");
  }
  const canonical = await get(selectors);
  if (!isDeepStrictEqual(cli("company-knowledge"), canonical) || !isDeepStrictEqual(mcp("company_knowledge", { asOf: future, includeProposed: true }), canonical)) throw new Error("HTTP/CLI/MCP knowledge full JSON differs");
  return { ok: true, restarted, ...evidence, accountingEffect: "none", actualCurrentRoleRevocationCoverage: "application tests; helper uses existing normal MFA principal" };
 } finally { rmSync(temp, { recursive: true, force: true }); }
}
