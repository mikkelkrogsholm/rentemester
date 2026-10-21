import { request, type Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";

type Query = { limit?: number; offset?: number; fromDate?: string; toDate?: string; eventTypeLike?: string; actorLike?: string };
/** A single supported reversal followed by read-only inspection of actual audit events. */
export function createAuditLogProof(page: Page, base: string, work: string, nonAdminPage?: Page) {
  const prefix = "/api/companies/synthetic/", endpoint = base + prefix + "audit-log";
  const need = (value: unknown, message: string) => { if (!value) throw Error(message); };
  const hash = (value: unknown) => createHash("sha256").update(JSON.stringify(value)).digest("hex");
  const cookieFile = join(work, "audit-owner.cookie"), serviceFile = join(work, "backup-lock-service.key");
  let baseline: any, all: any, ownerId = "", completed = false;
  const url = (query: Query) => endpoint + "?" + new URLSearchParams(Object.entries(query).map(([key, value]) => [key, String(value)]));
  async function get(resource: string) { const response = await page.request.get(base + prefix + resource); need(response.ok(), "Audit fixture authorized read refused"); return response.json(); }
  async function books() { return { journal: (await get("journal")).journal, bank: (await get("bank")).bank, trial: (await get("trial-balance")).trialBalance, balance: (await get("balance")).balance }; }
  async function cookies() { writeFileSync(cookieFile, (await page.context().cookies(base)).map(cookie => `${cookie.name}=${cookie.value}`).join("; ") + "\n", { mode: 0o600 }); }
  async function read(query: Query = {}, service = false) {
    const context = service ? await request.newContext({ extraHTTPHeaders: { "x-rentemester-service-key": readFileSync(serviceFile, "utf8").trim() } }) : page.request;
    try { const response = await context.get(url(query)); need(response.ok(), "Audit authorized HTTP read refused"); return response.json(); }
    finally { if (service) await context.dispose(); }
  }
  function client(kind: "cli" | "mcp", query: Query, service = false) {
    const flags = { limit: "--limit", offset: "--offset", fromDate: "--from-date", toDate: "--to-date", eventTypeLike: "--event-type-like", actorLike: "--actor-like" };
    const common = ["--server", base, "--allow-loopback-http", service ? "--service-key-file" : "--cookie-file", service ? serviceFile : cookieFile];
    const args = kind === "cli" ? ["remote", "audit-log-list", ...common, "--company", "synthetic", ...Object.entries(query).flatMap(([key, value]) => [flags[key as keyof Query], String(value)])] : common;
    const input = kind === "cli" ? "" : [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-audit-proof", version: "1" } } },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "audit_log_list", arguments: { company: "synthetic", ...query } } },
    ].map(value => JSON.stringify(value)).join("\n") + "\n";
    const result = spawnSync(join(work, kind === "cli" ? "rentemester-rust" : "rentemester-mcp"), args, { input, encoding: "utf8", timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    const secret = readFileSync(serviceFile, "utf8").trim();
    need(!result.error && result.status === 0 && !result.stdout.includes(secret) && !result.stderr.includes(secret), "Audit native client/credential safety refused");
    if (kind === "cli") { const value = JSON.parse(result.stdout); need(value.ok === true, "Audit CLI envelope refused"); return value.data; }
    const value = result.stdout.trim().split("\n").map(line => JSON.parse(line)).find(reply => reply.id === 2);
    need(value?.result?.isError === false, "Audit MCP envelope refused"); return value.result.structuredContent.data;
  }
  async function parity(query: Query, service = false) {
    const value = await read(query, service);
    for (const kind of ["cli", "mcp"] as const) need(isDeepStrictEqual(client(kind, query, service), value), "Audit exact HTTP/CLI/MCP rows/envelope differ");
    return value;
  }
  const auditTime = (value: string) => Date.parse(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(?:\.\d+)?$/.test(value) ? value.replace(" ", "T") + "Z" : value);
  function shape(value: any) {
    need(value.ok === true && value.slug === "synthetic" && value.count === value.rows.length && value.total >= value.count, "Audit page shape differs");
    for (const row of value.rows) need(Number.isSafeInteger(row.id) && row.id > 0 && typeof row.actor === "string" && typeof row.eventType === "string"
      && typeof row.entityType === "string" && (row.entityId === null || typeof row.entityId === "string") && typeof row.message === "string" && Number.isFinite(auditTime(row.createdAt)), "Audit row identity/text/timestamp malformed");
    for (let index = 1; index < value.rows.length; index++) {
      const previous = value.rows[index - 1], row = value.rows[index];
      need(auditTime(previous.createdAt) > auditTime(row.createdAt) || previous.createdAt === row.createdAt && previous.id > row.id, "Audit stable descending timestamp/id order differs");
    }
    need(value.hasMore ? value.nextOffset === value.offset + value.count : !("nextOffset" in value), "Audit pagination cursor contract differs");
  }
  async function ui() {
    await page.goto(base + "/companies/synthetic/integritet");
    const card = page.getByRole("region", { name: "Revisionsspor", exact: true });
    await card.getByLabel("Rækker pr. side", { exact: true }).fill("1");
    const firstResponse = page.waitForResponse(response => response.url().startsWith(endpoint) && response.request().method() === "GET");
    await card.getByRole("button", { name: "Hent revisionsspor", exact: true }).click();
    const first = await (await firstResponse).json(); shape(first); need(first.count === 1 && first.hasMore, "Actual UI first page must be bounded");
    // Unsaved filter must not alter the next page's last-submitted request.
    await card.getByLabel("Hændelse indeholder", { exact: true }).fill("unsaved-no-match-118");
    const nextResponse = page.waitForResponse(response => response.url().startsWith(endpoint) && response.request().method() === "GET");
    await card.getByRole("button", { name: "Næste side", exact: true }).click();
    const nextWire = await nextResponse, next = await nextWire.json(); shape(next);
    need(next.offset === 1 && next.rows[0].id !== first.rows[0].id && !new URL(nextWire.url()).searchParams.has("eventTypeLike"), "UI paging must use submitted filters without overlap");
    const previousResponse = page.waitForResponse(response => response.url().startsWith(endpoint) && response.request().method() === "GET");
    await card.getByRole("button", { name: "Forrige side", exact: true }).click();
    const previous = await (await previousResponse).json(); need(previous.offset === 0 && isDeepStrictEqual(previous.rows, first.rows), "Actual UI previous page must restore exact initial rows");
    await card.getByLabel("Hændelse indeholder", { exact: true }).fill("JOURNAL_REVERSE");
    await card.getByLabel("Aktør indeholder", { exact: true }).fill(ownerId.toUpperCase());
    const today = all.rows.find((row: any) => row.eventType === "journal_reverse").createdAt.slice(0, 10);
    await card.getByLabel("Fra dato", { exact: true }).fill(today); await card.getByLabel("Til dato", { exact: true }).fill(today);
    const filterResponse = page.waitForResponse(response => response.url().startsWith(endpoint) && response.request().method() === "GET");
    await card.getByRole("button", { name: "Hent revisionsspor", exact: true }).click();
    const filtered = await (await filterResponse).json(); shape(filtered);
    need(filtered.offset === 0 && filtered.count === 1 && filtered.rows[0].eventType === "journal_reverse", "Actual UI submitted actor/type/date filters differ");
    const row = filtered.rows[0];
    await card.getByRole("cell", { name: String(row.id), exact: true }).waitFor();
    for (const text of [row.actor, row.eventType, row.createdAt, row.entityType, row.entityId, row.message]) if (text !== null) await card.getByText(String(text), { exact: false }).first().waitFor();
    return { actualWebPaging: true, unsavedFiltersDoNotAffectPaging: true, submittedFiltersResetOffset: true, exactAuditTextVisible: true };
  }
  async function unchanged() { need(isDeepStrictEqual(await books(), baseline), "Audit inspection changed accounting"); }
  return {
    async run() {
      need(!completed, "Audit proof executes once"); await cookies();
      const me = await page.request.get(base + "/api/me"); need(me.ok(), "Current audit owner identity required"); ownerId = (await me.json()).user.id;
      const before = await books(); need(before.journal.entries.length === 1, "Existing backup-lock single human posting required");
      const original = before.journal.entries[0], today = new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
      const body = { confirm: true, input: { format: "rentemester-manual-journal-reversal/1", expectedRuleVersion: before.journal.nativePosting.ruleVersion, idempotencyKey: "synthetic-audit-reversal-118", request: { entryId: String(original.id), transactionDate: today, reason: "Synthetic audit inspection reversal" } } };
      const response = await page.request.post(base + prefix + "manual-journal-reversal", { headers: { origin: base }, data: body }); need(response.ok(), "One normal human audit reversal refused");
      const reversal = await response.json(); need(reversal.ok === true && reversal.originalEntryId === original.id && reversal.receipt?.replayed === false, "Actual reversal receipt differs");
      baseline = await books(); need(baseline.journal.entries.length === 2 && baseline.trial.balanced && isDeepStrictEqual(before.bank, baseline.bank), "Exactly one append-only balanced reversal required");
      all = await parity({ limit: 5000 }); shape(all); need(all.hasMore === false && all.total === all.rows.length, "All synthetic rows must fit explicit supported page without truncation");
      const posted = all.rows.find((row: any) => row.eventType === "journal_post" && row.entityId === String(original.id));
      const reversed = all.rows.find((row: any) => row.eventType === "journal_reverse" && row.entityId === String(reversal.entryId));
      const actor = `user:${ownerId} via rentemester-rust`;
      need(posted?.actor === actor && reversed?.actor === actor && posted.entityType === "journal_entry" && reversed.entityType === "journal_entry"
        && posted.message === `Posted journal entry ${original.entryNo}` && reversed.message === `Reversed journal entry ${original.entryNo} with ${reversal.entryNo}`, "Actual post/reverse actor/entity/message differs");
      const pages = [];
      for (let offset = 0; offset < all.total; offset += 2) { const value = await parity({ limit: 2, offset }); shape(value); pages.push(...value.rows); }
      need(isDeepStrictEqual(pages, all.rows) && new Set(pages.map((row: any) => row.id)).size === all.total, "Offset pages must reproduce every exact audit row without overlap");
      const from = reversed.createdAt.slice(0, 10), query = { limit: 5000, fromDate: from, toDate: from, eventTypeLike: "JOURNAL_REVERSE", actorLike: ownerId.toUpperCase() };
      const filtered = await parity(query); need(filtered.total === 1 && isDeepStrictEqual(filtered.rows[0], reversed), "ANDed case-insensitive actor/type/inclusive date filters differ");
      const exactTimestamp = new Date(auditTime(reversed.createdAt)).toISOString();
      const exactTime = await parity({ limit: 5000, fromDate: exactTimestamp, toDate: exactTimestamp, eventTypeLike: "journal_reverse" }); need(exactTime.rows.some((row: any) => row.id === reversed.id), "Inclusive RFC3339 timestamp bounds lost event");
      const wildcard = await parity({ limit: 5000, eventTypeLike: "journal_%" }); need(wildcard.rows.some((row: any) => row.id === posted.id) && wildcard.rows.some((row: any) => row.id === reversed.id), "Documented SQL LIKE wildcards differ");
      need(isDeepStrictEqual(await parity({ limit: 5000 }, true), all), "Scoped service audit read differs from human");
      const malformed = ["limit=0", "limit=1&limit=2", "offset=-1", "offset=9007199254740992", "limit=1e2", "unknown=x", "fromDate=not-a-date", "fromDate=2026-02-02&toDate=2026-01-01", "actorLike=%00"];
      for (const query of malformed) { const denied = await page.request.get(endpoint + "?" + query); need(denied.status() === 400 && (await denied.json()).ok === false, "Malformed audit query must refuse"); }
      const ambiguous = await page.request.get(endpoint, { headers: { "x-rentemester-service-key": readFileSync(serviceFile, "utf8").trim() } }); need(!ambiguous.ok() && (await ambiguous.json()).ok === false, "Ambiguous cookie/service authority admitted");
      need(nonAdminPage, "Existing ordinary MFA reader fixture required");
      const second = await nonAdminPage!.request.get(base + "/api/me"); need(second.ok(), "Existing second current human required"); const readerId = (await second.json()).user.id;
      const members = await page.request.get(base + "/api/workspace/members"); need(members.ok(), "Normal owner membership read required");
      const member = (await members.json()).members.find((row: any) => row.userId === readerId), previousRole = member?.memberships.find((row: any) => row.companySlug === "synthetic")?.role;
      const change = async (action: string, role?: string) => { const response = await page.request.post(base + "/api/workspace/members/company", { headers: { origin: base }, data: { userId: readerId, companySlug: "synthetic", action, ...(role ? { role } : {}) } }); need(response.ok(), "Normal audit reader grant/revocation refused"); };
      try {
        await change("grant", "reader"); const allowed = await nonAdminPage!.request.get(url({ limit: 5000 })); need(allowed.ok() && isDeepStrictEqual(await allowed.json(), all), "Current reader must be able to inspect audit");
        await change("revoke"); const denied = await nonAdminPage!.request.get(endpoint); need(denied.status() === 403 && (await denied.json()).ok === false, "Revoked current reader must lose audit access");
      } finally { if (previousRole) await change("grant", previousRole); else await change("revoke"); }
      const web = await ui(); await unchanged(); need(isDeepStrictEqual(await read({ limit: 5000 }), all), "Read-side tests modified audit rows"); completed = true;
      return { qualified: true, expectedFinalJournalCount: 2, postAuditId: posted.id, reversalAuditId: reversed.id, originalEntryId: original.id, reversalEntryId: reversal.entryId, auditFingerprint: hash(all), accountingFingerprint: hash(baseline), allRowsExactHttpCliMcp: true, completeOffsetPagination: true, exactActorTimeTypeEntityMessage: true, dateActorTypeAndWildcardFilters: true, serviceReadParity: true, malformedQueryRefused: true, ambiguousAuthorityRefused: true, currentReaderAllowedThenRevoked: true, readSideAccountingUnchanged: true, web };
    },
    async reopen() {
      need(completed, "Completed audit proof required before restart"); await cookies(); await unchanged();
      need(isDeepStrictEqual(await parity({ limit: 5000 }), all) && isDeepStrictEqual(await parity({ limit: 5000 }, true), all), "Exact audit actor/time/text/order must survive fresh MFA restart");
      const web = await ui(); await unchanged();
      return { sameAuditAfterNormalMfaRestart: true, expectedFinalJournalCount: 2, auditFingerprint: hash(all), accountingFingerprint: hash(baseline), httpCliMcpAndServiceParity: true, readSideAccountingUnchanged: true, web };
    },
  };
}
