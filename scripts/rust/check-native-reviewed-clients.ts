import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
type Fixture = { bankTransactionId: number; counterAccount: string; documentId?: number };
type Evidence = { draftId: string; submittedHash: string; journalEntryId: number };

/** Dedicated owned fixture only: normal two-subject MFA contexts, replacing the web post of its sole row.
 * Pass saved evidence after restart to reconcile canonical reads and explicitly test the durable receipt.
 * Never grants roles, seeds authentication, generates hashes or adds another journal effect.
 */
export async function checkNativeReviewedClients(author: Page, reviewer: Page, base: string, fixture: Fixture, restored?: Evidence): Promise<Evidence> {
  const url = `${base}/api/companies/synthetic`;
  const a = await (await author.request.get(`${base}/api/me`)).json();
  const b = await (await reviewer.request.get(`${base}/api/me`)).json();
  if (!a.user?.id || !b.user?.id || a.user.id === b.user.id) throw new Error("Reviewed client fixture requires two distinct current MFA subjects");
  const temp = mkdtempSync("/tmp/rm-native-reviewed-clients-");
  try {
    for (const [name, page] of [["author", author], ["reviewer", reviewer]] as const) writeFileSync(join(temp, name), (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
    const common = (who: string) => ["--server", base, "--allow-loopback-http", "--cookie-file", join(temp, who)];
    function cli(who: string, command: string, body?: string, draftId?: string, refused = false) {
      const flags = draftId ? ["--draft-id", draftId] : [];
      const run = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", command, ...common(who), "--company", "synthetic", ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error) throw new Error("Native reviewed CLI unavailable; build rentemester-cli or configure RM_NATIVE_CLI_BIN");
      const value = JSON.parse(run.stdout);
      if (refused) { if (run.status !== 1 || value.code !== "server_refused") throw new Error("Reviewed CLI expected a definite native refusal"); return value; }
      if (run.status !== 0) throw new Error(`Reviewed CLI ${command} failed`);
      return value.data;
    }
    function mcp(who: string, name: string, args: Record<string, unknown>, refused = false) {
      const input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-reviewed-check", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company: "synthetic", ...args } } },
      ].map(v => JSON.stringify(v)).join("\n") + "\n";
      const run = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common(who), { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error || run.status !== 0) throw new Error("Native reviewed MCP unavailable; build rentemester-mcp or configure RM_NATIVE_MCP_BIN");
      const call = run.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
      if (refused) { if (call?.result?.isError !== true || call.result.structuredContent.error.code !== "server_refused" || call.result.structuredContent.error.status !== 422) throw new Error("Reviewed MCP expected definite native422 refusal"); return call.result.structuredContent; }
      if (call?.error || call?.result?.isError || !call?.result?.structuredContent?.data) throw new Error(`Reviewed MCP ${name} failed`);
      return call.result.structuredContent.data;
    }
    let evidence = restored;
    if (!evidence) {
      const journal = (await (await author.request.get(`${url}/journal`)).json()).journal;
      if (journal.entries.length !== 0) throw new Error("Reviewed client fixture requires a pristine sole-row journal");
      const queue = cli("author", "reviewed-bank").queue;
      const row = queue.rows.find((r: { bankTransactionId: number }) => r.bankTransactionId === fixture.bankTransactionId);
      if (!row || !row.bankAccountNo || row.currency !== "DKK") throw new Error("Mapped unposted synthetic DKK bank row required");
      if (!isDeepStrictEqual(queue, (await (await author.request.get(`${url}/reviewed-bank`)).json()).queue)) throw new Error("CLI queue differs from canonical HTTP read");
      const rules = mcp("author", "posting_rules", {}).postingRules;
      if (!isDeepStrictEqual(rules, (await (await author.request.get(`${url}/posting-rules`)).json()).postingRules)) throw new Error("MCP rule read differs from actual HTTP");
      const draftId = `bank-${fixture.bankTransactionId}`;
      const create = { bankTransactionId: fixture.bankTransactionId, expectedSourceHash: row.sourceHash, accountNo: fixture.counterAccount, draftId, confirm: true, ...(fixture.documentId ? { documentId: fixture.documentId } : {}) };
      // Malformed/stale evidence is deliberately synthetic; positive hashes always come from canonical reads.
      cli("author", "create-bank-draft", JSON.stringify({ ...create, expectedSourceHash: "synthetic-stale-evidence" }), undefined, true);
      const created = cli("author", "create-bank-draft", JSON.stringify(create)).accountingDraft;
      const submitted = mcp("author", "submit_draft", { draftId, body: JSON.stringify({ expectedEventHash: created.eventHash }), confirm: true }).accountingDraft;
      if (submitted.status !== "submitted") throw new Error("MCP did not submit the stored source draft");
      mcp("reviewer", "approve_draft", { draftId, body: JSON.stringify({ expectedEventHash: created.eventHash, confirm: true }), confirm: true }, true);
      const body = JSON.stringify({ expectedEventHash: submitted.eventHash, confirm: true });
      cli("author", "approve-draft", body, draftId, true);
      const posted = mcp("reviewer", "approve_draft", { draftId, body, confirm: true }).accountingDraft;
      if (posted.status !== "approved_posted" || !Number.isSafeInteger(posted.journalEntryId)) throw new Error("Independent MCP reviewer did not return a durable posting");
      evidence = { draftId, submittedHash: submitted.eventHash, journalEntryId: posted.journalEntryId };
    }
    const drafts = cli("reviewer", "accounting-drafts").accountingDrafts;
    const canonical = (await (await reviewer.request.get(`${url}/accounting-drafts`)).json()).accountingDrafts;
    if (!isDeepStrictEqual(drafts, canonical) || !isDeepStrictEqual(mcp("author", "accounting_drafts", {}).accountingDrafts, canonical)) throw new Error("CLI/MCP drafts differ from actual HTTP");
    const posted = drafts.find((d: { id: string }) => d.id === evidence.draftId);
    if (posted?.status !== "approved_posted" || posted.journalEntryId !== evidence.journalEntryId) throw new Error("Reviewed client canonical posting missing");
    // Explicit fixture assertion of the server's same-actor durable receipt; never an adapter retry.
    const receipt = cli("reviewer", "approve-draft", JSON.stringify({ expectedEventHash: evidence.submittedHash, confirm: true }), evidence.draftId).accountingDraft;
    if (!isDeepStrictEqual(receipt, posted)) throw new Error("Explicit CLI receipt read differs from canonical draft");
    const queue = mcp("author", "reviewed_bank", {}).queue;
    if (queue.rows.some((r: { bankTransactionId: number }) => r.bankTransactionId === fixture.bankTransactionId)) throw new Error("Posted bank row remains available");
    const journal = (await (await reviewer.request.get(`${url}/journal`)).json()).journal;
    if (journal.entries.length !== 1 || journal.entries[0].id !== evidence.journalEntryId) throw new Error("Reviewed clients violated exact one journal invariant");
    console.log(restored ? "PASS: distinct MFA CLI/MCP canonical draft/queue reads and exact durable receipt survive restart; exactly1journal" : "PASS: CLI source draft → MCP submit → stale and self review refused → independent MCP approval → explicit CLI durable receipt; exactly1journal");
    return evidence;
  } finally { rmSync(temp, { recursive: true, force: true }); }
}
