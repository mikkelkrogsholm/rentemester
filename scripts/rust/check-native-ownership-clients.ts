import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";
import type { Page } from "@playwright/test";

const evidencePath = "/tmp/rentemester-rust-reports/native-ownership-clients.json";
/** Root supplies an isolated company and two ordinary MFA sessions with current endpoint authority. No auth/grant seeds. */
export async function checkNativeOwnershipClients(author: Page, reviewer: Page, base: string, company: string, restarted = false) {
 if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(base) || !/^synthetic-ownership-clients(?:-[a-z0-9]+)?$/.test(company)) throw Error("Explicit isolated synthetic ownership client company/loopback required");
 const cliBinary = process.env.RM_NATIVE_CLI_BIN, mcpBinary = process.env.RM_NATIVE_MCP_BIN;
 if (!cliBinary || !mcpBinary) throw Error("Explicit qualified CLI/MCP artifacts required");
 const me = async (page: Page) => { const r = await page.request.get(`${base}/api/me`); if (!r.ok()) throw Error("Current ordinary MFA session required"); return r.json(); };
 const a = await me(author), b = await me(reviewer);
 if (!a.user?.id || !b.user?.id || a.user.id === b.user.id || ![a, b].every(who => who.companies.some((c: any) => c.slug === company))) throw Error("Two distinct current MFA principals and explicit company grants required");
 const url = `${base}/api/companies/${company}`, headers = { origin: base, "content-type": "application/json" };
 const get = async (page: Page, path: string) => { const r = await page.request.get(`${url}/${path}`); if (!r.ok()) throw Error(`Ownership HTTP read refused: ${r.status()}`); return r.json(); };
 const before = await get(author, "journal"), uuid = before.journal.ledgerUuid, year = before.journal.selectedYear;
 const trial = await get(author, `trial-balance?year=${year}`);
 const ids = ["ownership-client-initial", "ownership-client-later"], partyId = "party-ownership-client";
 const dates = ["2000-06-01", "2001-01-01"];
 const temp = mkdtempSync("/tmp/rm-ownership-clients-");
 try {
  for (const [name, page] of [["author", author], ["reviewer", reviewer]] as const) writeFileSync(join(temp, name), (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
  const common = (who: string) => ["--server", base, "--allow-loopback-http", "--cookie-file", join(temp, who)];
  const cli = (who: string, command: string, flags: string[] = [], body?: string) => {
   const r = spawnSync(cliBinary, ["remote", command, ...common(who), "--company", company, ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024, timeout: 30000 });
   if (r.error || r.status !== 0) throw Error(`Ownership CLI ${command} failed`); return JSON.parse(r.stdout).data;
  };
  const mcp = (who: string, name: string, args: Record<string, unknown>, refusal = false) => {
   const messages = [{ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-ownership-clients", version: "1" } } }, { jsonrpc: "2.0", method: "notifications/initialized" }, { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company, ...args } } }];
   const r = spawnSync(mcpBinary, common(who), { input: messages.map(m => JSON.stringify(m)).join("\n") + "\n", encoding: "utf8", maxBuffer: 16 * 1024 * 1024, timeout: 30000 });
   if (r.error || r.status !== 0) throw Error(`Ownership MCP ${name} process failed`); const call = r.stdout.trim().split("\n").map(row => JSON.parse(row)).find(row => row.id === 2);
   if (refusal) { const error = call?.result?.structuredContent?.error; if (!call?.result?.isError || error?.code !== "server_refused" || error.status !== 422) throw Error("Expected definite native self-review/stale-proof refusal"); return error; }
   if (call?.error || call?.result?.isError) throw Error(`Ownership MCP ${name} refused`); return call.result.structuredContent.data;
  };
  let evidence: any;
  if (!restarted) {
   const registered = await author.request.post(`${url}/workspace-parties`, { headers, data: { partyId, kind: "organization", name: "Synthetic ownership client source party", aliases: [], identifiers: [], source: "Synthetic ownership client reviewed brief", observedAt: "2000-01-01", reviewAssertion: "Explicit source identity reviewed", role: "owner", confirm: true } });
   if (!registered.ok()) throw Error("Own visible party source registration refused");
   const body = (index: number) => JSON.stringify({ snapshotId: ids[index], source: "Synthetic ownership client reviewed source", observedAt: "2000-01-01", facts: [{ owner: { kind: "party", partyId }, ownedCompanySlug: company, validFrom: index === 0 ? "2000-01-01" : "2001-01-01", economicBasisPoints: index === 0 ? 1000 : 2000, votingBasisPoints: index === 0 ? 1000 : 2000, controlType: "equity", jurisdiction: "DK", evidenceRefs: ["synthetic-client-ownership-source-brief"] }], confirm: true });
   const initial = cli("author", "propose-ownership", [], body(0));
   const proposed = await get(author, `ownership/history?snapshotId=${ids[0]}`);
   mcp("author", "review_ownership", { body: JSON.stringify({ snapshotId: ids[0], decision: "approved", confirm: true }), confirm: true }, true);
   if (!isDeepStrictEqual(await get(author, `ownership/history?snapshotId=${ids[0]}`), proposed)) throw Error("Refused self-review changed source history");
   const approved = mcp("reviewer", "review_ownership", { body: JSON.stringify({ snapshotId: ids[0], decision: "approved", confirm: true }), confirm: true });
   if (approved.snapshot.state !== "approved" || approved.snapshot.snapshotHash !== initial.snapshot.snapshotHash || approved.snapshot.diffHash !== initial.snapshot.diffHash) throw Error("Independent review changed canonical proposal hashes");
   const apply = JSON.stringify({ snapshotId: ids[0], snapshotHash: approved.snapshot.snapshotHash, diffHash: approved.snapshot.diffHash, confirm: true });
   if (cli("author", "apply-ownership", [], apply).status !== "applied") throw Error("Exact approved source diff was not applied");
   const applied = await get(author, `ownership/history?snapshotId=${ids[0]}`);
   if (mcp("author", "apply_ownership", { body: apply, confirm: true }).status !== "unchanged" || !isDeepStrictEqual(await get(author, `ownership/history?snapshotId=${ids[0]}`), applied)) throw Error("Explicit known applied snapshot replay changed history");
   mcp("author", "propose_ownership", { body: body(1), confirm: true });
   const later = cli("reviewer", "review-ownership", [], JSON.stringify({ snapshotId: ids[1], decision: "approved", confirm: true }));
   const laterApply = { snapshotId: ids[1], snapshotHash: later.snapshot.snapshotHash, diffHash: later.snapshot.diffHash, confirm: true };
   mcp("author", "apply_ownership", { body: JSON.stringify(laterApply), confirm: true });
   const histories = await Promise.all(ids.map(id => get(author, `ownership/history?snapshotId=${id}`)));
   mcp("author", "apply_ownership", { body: JSON.stringify({ ...laterApply, diffHash: "0".repeat(64) }), confirm: true }, true);
   if (!isDeepStrictEqual(await Promise.all(ids.map(id => get(author, `ownership/history?snapshotId=${id}`))), histories)) throw Error("Wrong proof changed canonical ownership history");
   const facts = await Promise.all(dates.map(async date => (await get(author, `ownership?asOf=${date}`)).facts.filter((f: any) => f.ownedCompanySlug === company)));
   if (facts[0].length !== 1 || facts[1].length !== 1 || facts[0][0].economicBasisPoints !== 1000 || facts[0][0].validToExclusive !== "2001-01-01" || facts[1][0].economicBasisPoints !== 2000 || facts[1][0].owner.partyId !== partyId) throw Error("Authoritative effective ownership slices differ");
   evidence = { company, uuid, ids, histories, facts, principals: [a.user.id, b.user.id] };
   mkdirSync("/tmp/rentemester-rust-reports", { recursive: true }); writeFileSync(evidencePath, JSON.stringify(evidence), { mode: 0o600 });
  } else evidence = JSON.parse(readFileSync(evidencePath, "utf8"));
  if (evidence.company !== company || evidence.uuid !== uuid || !isDeepStrictEqual(evidence.principals, [a.user.id, b.user.id])) throw Error("Ownership fixture company/ledger/normal principals changed");
  for (let i = 0; i < ids.length; i++) {
   const history = await get(author, `ownership/history?snapshotId=${ids[i]}`), graph = await get(author, `ownership?asOf=${dates[i]}`);
   if (!isDeepStrictEqual(history, evidence.histories[i]) || !isDeepStrictEqual(graph.facts.filter((f: any) => f.ownedCompanySlug === company), evidence.facts[i]) || !isDeepStrictEqual(cli("author", "ownership-history", ["--snapshot-id", ids[i]]), history) || !isDeepStrictEqual(mcp("author", "ownership_history", { snapshotId: ids[i] }), history) || !isDeepStrictEqual(cli("author", "ownership", ["--as-of", dates[i]]), graph) || !isDeepStrictEqual(mcp("author", "ownership", { asOf: dates[i] }), graph)) throw Error("Ownership own facts/history or full live HTTP/CLI/MCP parity changed after restart");
  }
  if (!isDeepStrictEqual(await get(author, "journal"), before) || !isDeepStrictEqual(await get(author, `trial-balance?year=${year}`), trial)) throw Error("Ownership source workflow changed accounting");
  return { ok: true, restarted, ...evidence, independentNormalMfaReview: true, fullCanonicalClientParity: true, accountingEffect: "none" };
 } finally { rmSync(temp, { recursive: true, force: true }); }
}
