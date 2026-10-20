import { request as playwrightRequest, type Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { writeFileSync, unlinkSync } from "node:fs";
import { join } from "node:path";
import { connect } from "node:net";
import { checkNativeServiceSales } from "./check-native-service-sales";

/** Normal owner-issued service identity, real accounting and native client reconciliation. */
export async function checkNativeServiceCompany(owner: Page, base: string, work: string, cli: string, mcp: string) {
  const need = (value: unknown, label: string) => { if (!value) throw new Error(label); };
  const prefix = "/api/companies/synthetic/";
  const keyFile = join(work, "service-company.key");
  const secrets: string[] = [];
  async function admin(path: string, body: unknown) {
    const r = await owner.request.post(`${base}/api/workspace/${path}`, { headers: { origin: base }, data: body });
    need(r.ok(), `Normal owner ${path} refused HTTP${r.status()}`);
    return r.json();
  }
  const issued = await admin("service-principals", { displayName: "Synthetic accounting executor", operationId: crypto.randomUUID(), confirm: true });
  secrets.push(issued.secret);
  const makeHttp = (secret: string) => playwrightRequest.newContext({ baseURL: base, extraHTTPHeaders: { origin: base, "x-rentemester-service-key": secret } });
  let http = await makeHttp(issued.secret);
  const canonical = async () => {
    const r = await owner.request.get(`${base}${prefix}journal`); need(r.ok(), "Canonical owner journal readable"); return (await r.json()).journal;
  };
  const grant = (role: string) => admin("members/company", { userId: issued.serviceAccountId, companySlug: "synthetic", action: "grant", role });
  function client(kind: "cli" | "mcp", command: string, body?: unknown, success = true, id?: number) {
    const common = ["--server", base, "--allow-loopback-http", "--service-key-file", keyFile];
    const company = !["me", "companies"].includes(command);
    let args: string[]; let input: string;
    if (kind === "cli") {
      args = ["remote", command.replaceAll("_", "-"), ...common, ...(company ? ["--company", "synthetic"] : []), ...(id === undefined ? [] : ["--id", String(id)]), ...(body === undefined || ["post_journal", "reverse_journal", "preview_invoice"].includes(command) ? [] : ["--confirm"]), ...(command === "issue_invoice" ? ["--preview-reviewed"] : [])];
      input = body === undefined ? "" : JSON.stringify(body);
    } else {
      args = common;
      input = [
        { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-service-accounting", version: "1" } } },
        { jsonrpc: "2.0", method: "notifications/initialized" },
        { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: command, arguments: { ...(company ? { company: "synthetic" } : {}), ...(id === undefined ? {} : { id }), ...(body === undefined ? {} : ["post_journal", "reverse_journal"].includes(command) ? { envelope: JSON.stringify(body) } : { body: JSON.stringify(body), ...(command === "preview_invoice" ? {} : { confirm: true }), ...(command === "issue_invoice" ? { previewReviewed: true } : {}) }) } } },
      ].map((v) => JSON.stringify(v)).join("\n") + "\n";
    }
    const out = spawnSync(kind === "cli" ? cli : mcp, args, { input, encoding: "utf8", timeout: 30000, maxBuffer: 4 * 1024 * 1024 });
    need(!out.error && !secrets.some((s) => out.stdout?.includes(s) || out.stderr?.includes(s)), `${kind} transport or secret disclosure failure`);
    if (kind === "cli") {
      need(out.status === (success ? 0 : 1), `Service CLI ${command} unexpected exit${out.status}`);
      return success ? JSON.parse(out.stdout).data : { refused: true };
    }
    need(out.status === 0, `Service MCP ${command} process failure`);
    const r = out.stdout.trim().split("\n").map((line) => JSON.parse(line)).find((v) => v.id === 2);
    need(success ? r?.result?.isError === false : r?.result?.isError === true || r?.error, `Service MCP ${command} unexpected outcome`);
    return success ? r.result.structuredContent.data : { refused: true };
  }
  async function discardAcceptedReply(body: unknown, secret: string) {
    const url = new URL(base); need(url.protocol === "http:" && url.hostname === "127.0.0.1", "Owned literal loopback fixture required");
    await new Promise<void>((resolve, reject) => {
      let done = false; let prefixBytes = "";
      const socket = connect({ host: url.hostname, port: Number(url.port) }, () => {
        const bytes = JSON.stringify(body);
        socket.write(`POST ${prefix}manual-journal HTTP/1.1\r\nHost: ${url.host}\r\nOrigin: ${base}\r\nx-rentemester-service-key: ${secret}\r\nContent-Type: application/json\r\nContent-Length: ${Buffer.byteLength(bytes)}\r\nConnection: close\r\n\r\n${bytes}`);
      });
      socket.setTimeout(30000, () => socket.destroy(new Error("Bounded service response timeout")));
      socket.on("data", (bytes) => {
        prefixBytes = (prefixBytes + bytes.toString()).slice(0, 64);
        if (prefixBytes.length < 13) return;
        done = true; socket.destroy();
        if (/^HTTP\/1\.1 200 /.test(prefixBytes)) resolve(); else reject(new Error("Service posting was not accepted before reply loss"));
      });
      socket.on("error", reject);
      socket.on("close", () => { if (!done) reject(new Error("Service reply closed before accepted status")); });
    });
  }
  try {
    need((await http.get("/api/me")).status() === 401, "Issued credential must not implicitly activate workspace access");
    await admin("members/access", { userId: issued.serviceAccountId, action: "set-role", workspaceRole: "member" });
    await grant("reader");
    writeFileSync(keyFile, issued.secret + "\n", { mode: 0o600, flag: "wx" });
    const companies = await (await http.get("/api/companies")).json();
    need(companies.companies.length === 1 && companies.companies[0].slug === "synthetic", "Service catalog must exclude ungranted second company");
    need((await http.get("/api/companies/synthetic-second-ledger/journal")).status() === 403, "Service cannot read another ledger");
    need((await owner.request.get(`${base}${prefix}journal`, { headers: { "x-rentemester-service-key": issued.secret } })).status() === 401, "Mixed cookie/key authentication refused");
    need((await http.get("/api/workspace/service-principals")).status() === 401, "Service cannot administer credentials");
    const before = await canonical();
    const original = { confirm: true, input: { format: "rentemester-manual-journal-post/1", expectedRuleVersion: before.nativePosting.ruleVersion, idempotencyKey: crypto.randomUUID(), request: {
      transactionDate: new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" }), text: "Synthetic service lost-response accounting", currency: "DKK", lines: [{ accountNo: "1759", debitAmount: "42.75", creditAmount: "0.00" }, { accountNo: "8759", debitAmount: "0.00", creditAmount: "42.75" }],
    } } };
    const malformed = structuredClone(original); malformed.input.format = "unsupported";
    need((await http.post(`${prefix}manual-journal`, { data: malformed })).status() === 400, "Malformed service envelope refused before owner attempt");
    need((await http.get(prefix + "journal")).status() === 200, "Malformed input must not retire the shared owner");
    const denied = await http.post(`${prefix}manual-journal`, { data: original });
    need(denied.status() === 403 && (await denied.json()).usage === "not_committed", "Reader posting must refuse before charging or reserving a receipt");
    need(JSON.stringify(await canonical()) === JSON.stringify(before), "Reader refusal leaves canonical ledger unchanged");
    await grant("bookkeeper");
    await discardAcceptedReply(original, issued.secret);
    const once = await canonical();
    need(once.entries.length === before.entries.length + 1 && once.entries.filter((e: { text: string }) => e.text === original.input.request.text).length === 1, "Accepted lost response must create exactly one service posting");
    const replay = client("cli", "post_journal", original);
    const same = client("mcp", "post_journal", original);
    need(replay.receipt?.replayed === true && same.receipt?.replayed === true && replay.entryId === same.entryId && replay.entryHash === same.entryHash, "CLI and MCP must recover the same service receipt");
    need(JSON.stringify(await canonical()) === JSON.stringify(once), "Cross-client replay creates no duplicate");
    const rotated = await admin("service-principals/rotate", { serviceAccountId: issued.serviceAccountId, credentialId: issued.credentialId, operationId: crypto.randomUUID(), confirm: true });
    secrets.push(rotated.secret);
    need((await http.post(`${prefix}manual-journal`, { data: original })).status() === 401, "Old rotated key cannot replay an existing result");
    await http.dispose(); http = await makeHttp(rotated.secret);
    writeFileSync(keyFile, rotated.secret + "\n", { mode: 0o600 });
    const rotatedReplay = client("mcp", "post_journal", original);
    need(rotatedReplay.entryHash === replay.entryHash && rotatedReplay.receipt?.replayed === true, "New key on stable service subject retains receipt identity");
    const reversal = { confirm: true, input: { format: "rentemester-manual-journal-reversal/1", expectedRuleVersion: before.nativePosting.ruleVersion, idempotencyKey: crypto.randomUUID(), request: { entryId: String(replay.entryId), transactionDate: original.input.request.transactionDate, reason: "Synthetic service exact reversal" } } };
    const reversed = client("mcp", "reverse_journal", reversal);
    const reverseReplay = client("cli", "reverse_journal", reversal);
    need(reversed.ok === true && reversed.receipt?.replayed === false && reverseReplay.receipt?.replayed === true, "Service reversal applies once across native clients");
    let after = await canonical(); need(after.entries.length === before.entries.length + 2, "Exactly one service posting and one reversal");
    for (const [resource, envelope, command] of [["journal", "journal", "journal"], ["trial-balance", "trialBalance", "trial_balance"], ["income-statement", "incomeStatement", "income_statement"], ["balance", "balance", "balance"], ["overview", "overview", "overview"]]) {
      const human = await (await owner.request.get(`${base}${prefix}${resource}`)).json();
      const service = await (await http.get(prefix + resource)).json();
      need(JSON.stringify(human[envelope]) === JSON.stringify(service[envelope]), `Human/service ${resource} parity`);
      for (const kind of ["cli", "mcp"] as const) need(JSON.stringify(client(kind, command)[envelope]) === JSON.stringify(service[envelope]), `${kind} service ${resource} parity`);
    }
    const sales = await checkNativeServiceSales(owner, http, base, client);
    after = await canonical();
    await admin("members/company", { userId: issued.serviceAccountId, companySlug: "synthetic", action: "revoke" });
    need((await http.get(prefix + "journal")).status() === 403 && (await http.post(prefix + "manual-journal", { data: original })).status() === 403, "Revoked membership blocks reads and existing receipt replay");
    await grant("reader");
    await admin("service-principals/revoke", { serviceAccountId: issued.serviceAccountId, credentialId: rotated.credentialId, operationId: crypto.randomUUID(), confirm: true });
    client("cli", "me", undefined, false); client("mcp", "journal", undefined, false);
    need(JSON.stringify(await canonical()) === JSON.stringify(after), "Revocation must not change accounting");
    return { sales, normalOwnerIssuedAndExplicitlyGranted: true, companyIsolation: true, readerWriteRefusedBeforeCharge: true, acceptedLostReply: true, cliMcpExactReplay: true, stableReceiptAcrossRotation: true, reversalExactlyOnce: true, canonicalFiveReportParity: true, membershipAndCredentialRevocation: true, servicePostingEntryHash: replay.entryHash, finalEntries: after.entries.length };
  } finally { await http.dispose(); try { unlinkSync(keyFile); } catch {} }
}
