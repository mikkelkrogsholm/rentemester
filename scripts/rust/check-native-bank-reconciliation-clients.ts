import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { isDeepStrictEqual } from "node:util";
const saved = "/tmp/rentemester-rust-reports/native-bank-reconciliation-clients.json";
type Row = { bankId: number; journalId: number; entryNo: string; body: string; link: string; proof: string };
type Evidence = { year: string; rows: Row[]; journal: Array<{ id: number }>; payables: unknown };

/** Dedicated synthetic accounts/statement/journals, separate from the bank web fixture. Normal current cookie authority only. */
export async function checkNativeBankReconciliationClients(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const temp = mkdtempSync("/tmp/rm-native-bank-link-clients-");
  try {
    const cookie = join(temp, "session.cookie");
    writeFileSync(cookie, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookie];
    function cli(command: string, body?: string, flags: string[] = [], refused = false) {
      const run = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", command, ...common, "--company", "synthetic", ...flags, ...(body === undefined ? [] : ["--confirm"])], { input: body, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error) throw new Error("Native bank-link CLI unavailable; build rentemester-cli or set RM_NATIVE_CLI_BIN");
      const value = JSON.parse(run.stdout);
      if (refused) { if (run.status !== 1 || value.code !== "server_refused") throw new Error("Expected definite native bank-link CLI refusal"); return value; }
      if (run.status !== 0) throw new Error(`Native bank-link CLI ${command} failed`);
      return value.data;
    }
    function mcp(name: string, args: Record<string, unknown>, refused = false) {
      const input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-bank-link-client-check", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name, arguments: { company: "synthetic", ...args } } },
      ].map(v => JSON.stringify(v)).join("\n") + "\n";
      const run = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
      if (run.error || run.status !== 0) throw new Error("Native bank-link MCP unavailable; build rentemester-mcp or set RM_NATIVE_MCP_BIN");
      const call = run.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
      if (refused) { if (call?.result?.isError !== true || call.result.structuredContent.error.code !== "server_refused" || call.result.structuredContent.error.status !== 422) throw new Error("Expected definite native bank-link MCP422 refusal"); return call.result; }
      if (call?.error || call?.result?.isError || !call?.result?.structuredContent?.data) throw new Error(`Native bank-link MCP ${name} failed`);
      return call.result.structuredContent.data;
    }
    async function get(path: string) {
      const response = await page.request.get(`${url}/${path}`);
      if (!response.ok()) throw new Error(`Canonical bank-link read ${path} failed (${response.status()})`);
      return response.json();
    }
    async function post(path: string, data: unknown) {
      const response = await page.request.post(`${url}/${path}`, { headers: { origin: base, "content-type": "application/json" }, data });
      if (!response.ok()) throw new Error(`Synthetic bank-link setup ${path} failed: ${await response.text()}`);
      return response.json();
    }
    let evidence: Evidence;
    if (!restarted) {
      for (const [accountNo, name, type, normalBalance] of [["1763", "Synthetic client reconciliation bank", "asset", "debit"], ["8763", "Synthetic client reconciliation equity", "equity", "credit"]]) await post("accounts", { accountNo, name, type, normalBalance });
      cli("create-bank-account", JSON.stringify({ name: "Synthetic client-only reconciliation bank", slug: "synthetic-client-bank-link", currency: "DKK", ledgerAccountNo: "1763" }));
      const date = new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
      const fixtures = [{ text: "Synthetic CLI existing journal link", amount: "17.63" }, { text: "Synthetic MCP existing journal link", amount: "18.67" }];
      const state = JSON.parse(readFileSync(process.env.RM_NATIVE_WEB_STATE!, "utf8"));
      for (const [index, fixture] of fixtures.entries()) {
        const posting = JSON.parse(state.postedBody);
        // The established manual journal contract requires an explicit fixture key. None is used for link operations.
        posting.input.idempotencyKey = `synthetic-client-bank-link-journal-${index + 1}`;
        posting.input.expectedRuleVersion = (await get("journal")).journal.nativePosting.ruleVersion;
        posting.input.request = { transactionDate: date, text: fixture.text, currency: "DKK", lines: [{ accountNo: "1763", debitAmount: fixture.amount, creditAmount: "0.00" }, { accountNo: "8763", debitAmount: "0.00", creditAmount: fixture.amount }] };
        await post("manual-journal", posting);
      }
      cli("import-bank", JSON.stringify({ csvContent: `transaction_date,text,amount,currency\n${fixtures.map(f => `${date},${f.text},${f.amount},DKK`).join("\n")}\n`, account: "synthetic-client-bank-link", confirm: true }));
      const bank = (await get("bank")).bank;
      const journal = (await get(`journal?year=${encodeURIComponent(bank.selectedYear)}`)).journal.entries;
      const plans = [];
      const rows: Row[] = [];
      for (const fixture of fixtures) {
        const transaction = bank.transactions.find((t: { text: string }) => t.text === fixture.text);
        const entry = journal.find((e: { text: string }) => e.text === fixture.text);
        if (!transaction || !entry || transaction.reconciliationStatus !== "unmatched") throw new Error("Dedicated unlinked client fixture missing");
        const http = await get(`bank/reconciliation-candidates?bankTransactionId=${transaction.id}`);
        const viaCli = cli("bank-reconciliation-candidates", undefined, ["--bank-transaction-id", String(transaction.id)]);
        const viaMcp = mcp("bank_reconciliation_candidates", { bankTransactionId: transaction.id });
        if (!isDeepStrictEqual(viaCli, http) || !isDeepStrictEqual(viaMcp, http)) throw new Error("Full reconciliation candidate JSON differs across HTTP/CLI/MCP");
        const plan = http.reconciliationCandidates.plans.find((p: { journalEntryId: number }) => p.journalEntryId === entry.id);
        if (!plan || plan.effect !== "link_only_no_posting" || plan.bankAccountNo !== "1763" || !/^[a-f0-9]{64}$/.test(plan.planHash) || !/^[a-f0-9]{64}$/.test(plan.sourceHash)) throw new Error("Actual reviewed source/account/journal proof unavailable");
        plans.push(plan);
        rows.push({ bankId: transaction.id, journalId: entry.id, entryNo: entry.entryNo, body: JSON.stringify({ bankTransactionId: transaction.id, journalEntryId: entry.id, planHash: plan.planHash, note: "Synthetic client reviewer inspected archived statement and exact existing accounting.", confirm: true }), link: "", proof: "" });
      }
      evidence = { year: bank.selectedYear, rows, journal, payables: (await get("payables?status=all")).payables.rows };
      for (const [index, row] of rows.entries()) {
        // A different row's actual server proof cannot authorize this selected journal/bank pair.
        mcp("bank_reconciliation_link", { body: JSON.stringify({ ...JSON.parse(row.body), planHash: plans[1 - index].planHash }), confirm: true }, true);
        const response = index === 0 ? cli("bank-reconciliation-link", row.body) : mcp("bank_reconciliation_link", { body: row.body, confirm: true });
        if (response.reconciliation?.effect !== "link_only_no_posting" || response.reconciliation.journalEntryId !== row.journalId) throw new Error("Client link returned unexpected effect/target");
        cli("bank-reconciliation-link", row.body, [], true);
        mcp("bank_reconciliation_link", { body: row.body, confirm: true }, true);
        const current = (await get(`bank?year=${encodeURIComponent(evidence.year)}`)).bank.transactions.find((t: { id: number }) => t.id === row.bankId);
        row.link = current.reconciliationId;
        row.proof = current.reconciliationProof;
      }
      writeFileSync(saved, JSON.stringify(evidence));
    } else evidence = JSON.parse(readFileSync(saved, "utf8"));
    // Other combined journeys may legitimately post/pay after initial verification. Across restart,
    // pin our own journal facts and compare all current facts before/after these client reads.
    const journalBefore = restarted ? (await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries : evidence.journal;
    const payablesBefore = restarted ? (await get("payables?status=all")).payables.rows : evidence.payables;
    const bank = (await get(`bank?year=${encodeURIComponent(evidence.year)}`)).bank;
    const cliBank = cli("bank", undefined, ["--year", evidence.year]).bank;
    const mcpBank = mcp("bank", { year: evidence.year }).bank;
    for (const row of evidence.rows) {
      const current = bank.transactions.find((t: { id: number }) => t.id === row.bankId);
      if (current?.reconciliationStatus !== "matched" || current.journalEntryNo !== row.entryNo || current.reconciliationId !== row.link || current.reconciliationNote !== JSON.parse(row.body).note || current.reconciliationProof !== row.proof || current.reconciliationProof !== `native-plan:${JSON.parse(row.body).planHash}`) throw new Error("Canonical client link/proof not retained");
      for (const view of [cliBank, mcpBank]) if (!isDeepStrictEqual(view.transactions.find((t: { id: number }) => t.id === row.bankId), current)) throw new Error("CLI/MCP canonical bank proof differs from actual HTTP");
      const http = await get(`bank/reconciliation-candidates?bankTransactionId=${row.bankId}`);
      if (!isDeepStrictEqual(cli("bank-reconciliation-candidates", undefined, ["--bank-transaction-id", String(row.bankId)]), http) || !isDeepStrictEqual(mcp("bank_reconciliation_candidates", { bankTransactionId: row.bankId }), http)) throw new Error("Linked-row current candidates differ across transports");
    }
    const journalAfter = (await get(`journal?year=${encodeURIComponent(evidence.year)}`)).journal.entries;
    if (!isDeepStrictEqual(journalAfter, journalBefore) || !isDeepStrictEqual((await get("payables?status=all")).payables.rows, payablesBefore)) throw new Error("Client linking/reads changed journal/payable effects");
    for (const row of evidence.rows) if (!isDeepStrictEqual(journalAfter.find((e: {id: number}) => e.id === row.journalId), evidence.journal.find(e => e.id === row.journalId))) throw new Error("Own linked journal facts changed after restart");
    console.log(restarted ? "PASS: CLI/MCP immutable reconciliation proof and unchanged journal/payables survive restart" : "PASS: own statement/existing journals → full HTTP/CLI/MCP proof review → CLI and MCP links; wrong-proof/duplicate refused; journal/payables unchanged");
  } finally { rmSync(temp, { recursive: true, force: true }); }
}
