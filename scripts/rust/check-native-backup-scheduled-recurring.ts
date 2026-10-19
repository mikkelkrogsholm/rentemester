import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { realpathSync, statSync, unlinkSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { isDeepStrictEqual } from "node:util";
import type { Page } from "@playwright/test";

type Context = { page: Page; base: string; work: string; company: string; clients: boolean };
type State = {
  company: string;
  sourceOwner: string;
  ids: Record<string, number>;
  source: any;
  sourceBooks: any;
  originals: any[];
  postRestore?: any;
  restoredOwner?: string;
  evidence: Record<string, unknown>;
};
const digest = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");
function api(ctx: Context) {
  if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(ctx.base) || ctx.company !== "synthetic" ||
      !basename(ctx.work).startsWith("Rentemester-Synthetic-Portable-")) throw Error("Owned synthetic portable scheduler fixture required");
  const prefix = `${ctx.base}/api/companies/${ctx.company}/`;
  const headers = { origin: ctx.base, "content-type": "application/json" };
  const get = async (path: string) => {
    const response = await ctx.page.request.get(prefix + path);
    if (!response.ok()) throw Error(`Portable scheduler read ${path}: HTTP${response.status()}`);
    return response.json();
  };
  const post = async (action: string, data: unknown, status = 200) => {
    const response = await ctx.page.request.post(prefix + action, { headers, data });
    if (response.status() !== status) throw Error(`Portable scheduler ${action}: HTTP${response.status()}, expected${status}; no automatic retry`);
    return response.json();
  };
  const current = async () => (await get("scheduled-recurring")).scheduledRecurring;
  const plan = async (id: number) => (await get(`scheduled-recurring/${id}/plan`)).plan;
  return { prefix, headers, get, post, current, plan };
}
async function views(ctx: Context, ids: Record<string, number>) {
  const { current, plan, get } = api(ctx);
  const plans: Record<string, any> = {};
  for (const [kind, id] of Object.entries(ids)) plans[kind] = await plan(id);
  return { current: await current(), plans, recurring: await get("recurring-invoices") };
}
function row(view: any, id: number) {
  const matches = view.current.rows.filter((item: any) => item.templateId === id);
  if (matches.length !== 1) throw Error("Exact portable scheduler template row missing");
  return matches[0];
}
async function original(ctx: Context, documentId: number) {
  const { prefix } = api(ctx);
  const json = await ctx.page.request.get(prefix + `documents/${documentId}/file`);
  const pdf = await ctx.page.request.get(prefix + `invoices/${documentId}/pdf`);
  if (!json.ok() || !pdf.ok()) throw Error("Portable scheduled invoice JSON/PDF missing");
  const bytes = await json.body(), pdfBytes = await pdf.body();
  if (!pdfBytes.subarray(0, 5).equals(Buffer.from("%PDF-"))) throw Error("Issued PDF signature missing");
  return { documentId, jsonBase64: bytes.toString("base64"), jsonSha256: digest(bytes), pdfBase64: pdfBytes.toString("base64"), pdfSha256: digest(pdfBytes) };
}
async function checkOriginals(ctx: Context, state: State) {
  const invoices = (await api(ctx).get("invoices")).invoices.invoices;
  for (const saved of state.originals) {
    if (!isDeepStrictEqual(await original(ctx, saved.documentId), saved)) throw Error("Restored generated JSON/PDF bytes differ");
    const invoice = invoices.find((item: any) => item.documentId === saved.documentId);
    if (!invoice || invoice.posted !== false || invoice.lastEmailedAt !== null) throw Error("Scheduled generation was posted or sent");
  }
}
/** Ordinary API mutations only. Caller has completed first-owner verification/MFA and explicit seller/customer prerequisites. */
export async function prepareBackupScheduledRecurring(ctx: Context): Promise<State> {
  const { get, post, current, plan, prefix, headers } = api(ctx);
  const me = await (await ctx.page.request.get(ctx.base + "/api/me")).json();
  const sourceBooks = await get("journal");
  const today = (await current()).asOfDate;
  if (!me.user?.id || !/^\d{4}-\d{2}-\d{2}$/.test(today)) throw Error("Normal source MFA principal/server date missing");
  const before = await views(ctx, {});
  if (before.current.rows.length) throw Error("Scheduler backup fixture must start without templates");
  // Synthetic date selection only. Server plan supplies every actual occurrence/index.
  const previous = new Date(`${today}T12:00:00Z`); previous.setUTCDate(previous.getUTCDate() - 21);
  const firstIssueDate = previous.toISOString().slice(0, 10);
  const ids: Record<string, number> = {};
  const buyers: Record<string, number> = {};
  for (const [index, kind] of ["browser", "cli", "mcp", "cancelled", "failed"].entries()) {
    const customer = await post("customers", { name: `Synthetic Portable Scheduled ${kind}`, address: "Synthetic Scheduled Buyer Street 9", vatOrCvr: `DK${12345670 + index}`, defaultCurrency: "DKK", paymentTermsDays: 14 });
    buyers[kind] = customer.customer.id;
    const created = await post("recurring-invoices", {
      name: `Synthetic Portable Scheduled ${kind}`, interval: "weekly", intervalCount: 1,
      deliveryChannel: "manual", firstIssueDate: kind === "failed" ? today : firstIssueDate,
      paymentTermsDays: 14, deliveryPeriodMode: "interval_window", currency: "DKK", customerId: buyers[kind], vatRatePercent: 25,
      lines: [{ description: `Synthetic portable ${kind} scheduled service`, quantity: 1, unitPriceExVat: 100 }], confirm: true,
    });
    ids[kind] = created.template.templateId;
    if (!Number.isSafeInteger(ids[kind]) || ids[kind] <= 0) throw Error("Template identity missing");
    const p = await plan(ids[kind]);
    await post("scheduled-recurring/enable", { templateId: ids[kind], templateHash: p.templateHash, confirm: true });
  }
  let p = await plan(ids.browser);
  if (p.missed.length !== 3 || p.missedCount !== 3 || !p.normalDue) throw Error("Three exact weekly historical source occurrences required");
  const [succeeded, skipped, queued] = p.missed;
  await post("scheduled-recurring/catch-up", { templateId: ids.browser, planHash: p.planHash, periodIndexes: [succeeded.periodIndex, queued.periodIndex], confirm: true });
  p = await plan(ids.browser);
  await post("scheduled-recurring/skip", { templateId: ids.browser, planHash: p.planHash, periodIndexes: [skipped.periodIndex], reason: "Explicit source-period skip retained through restore", confirm: true });
  p = await plan(ids.browser);
  const issued = await post("scheduled-recurring/dispatch", { templateId: ids.browser, planHash: p.planHash, periodIndex: succeeded.periodIndex, confirm: true });
  if (issued.dispatch?.status !== "succeeded" || !issued.generation?.documentId || !issued.generation?.invoiceNumber) throw Error("Source scheduler did not issue actual invoice JSON/PDF");
  for (const kind of ["cli", "mcp"]) {
    p = await plan(ids[kind]);
    if (!p.missed.length) throw Error("Client source historical queue requires a server occurrence");
    await post("scheduled-recurring/catch-up", { templateId: ids[kind], planHash: p.planHash, periodIndexes: [p.missed[0].periodIndex], confirm: true });
    if (kind === "cli") {
      const status = row(await views(ctx, ids), ids[kind]);
      await post("scheduled-recurring/pause", { templateId: ids[kind], expectedStateHash: status.stateHash, reason: "Explicit source pause retained as immutable history", confirm: true });
    }
  }
  let status = row(await views(ctx, ids), ids.cancelled);
  await post("scheduled-recurring/cancel", { templateId: ids.cancelled, expectedStateHash: status.stateHash, reason: "Explicit source cancellation remains terminal", confirm: true });
  const deleted = await ctx.page.request.delete(prefix + `customers/${buyers.failed}`, { headers, data: { confirm: true } });
  if (!deleted.ok()) throw Error("Own unused failure customer deletion refused");
  p = await plan(ids.failed);
  const failed = await post("scheduled-recurring/dispatch", { templateId: ids.failed, planHash: p.planHash, periodIndex: p.normalDue.periodIndex, confirm: true });
  if (failed.dispatch?.status !== "failed") throw Error("Known rolled-back issuance failure was not preserved");
  const source = await views(ctx, ids);
  if (row(source, ids.browser).state !== "enabled" || row(source, ids.cli).state !== "paused" || row(source, ids.cancelled).state !== "cancelled" || row(source, ids.failed).lastRun?.status !== "failed") throw Error("Source state/history inventory differs");
  const generations = source.recurring.recurringInvoices.templates.flatMap((template: any) => template.generations);
  if (generations.length !== 1 || generations[0].documentId !== issued.generation.documentId) throw Error("Source generated invoice inventory differs");
  const state: State = { company: ctx.company, sourceOwner: me.user.id, ids, source, sourceBooks,
    originals: [await original(ctx, issued.generation.documentId)],
    evidence: { templateIds: ids, states: ["enabled", "paused", "cancelled"], realIssuedUnpostedUnsent: 1, knownFailedIssuance: true,
      sourceCanonical: source, sourceHistorySha256: digest(Buffer.from(JSON.stringify(source))), originalSha256: [], sourceLedgerUuid: sourceBooks.journal.ledgerUuid,
      resolvedPeriods: { succeeded: succeeded.periodIndex, skipped: skipped.periodIndex, inactiveHistoricalQueue: queued.periodIndex },
      repeatedRestoreAfterNewConsent: "unit coverage only", unresolvedAttemptRestore: "unit coverage only", reducedRoleRefusal: "existing authorization tests only; fixture has one ordinary restored owner" } };
  state.evidence.originalSha256 = state.originals.map(value => ({ documentId: value.documentId, json: value.jsonSha256, pdf: value.pdfSha256 }));
  const issuedJson = JSON.parse(Buffer.from(state.originals[0].jsonBase64,"base64").toString());
  if (issuedJson.recurringTemplateId !== ids.browser || issuedJson.recurringPeriodIndex !== succeeded.periodIndex) throw Error("Issued original is not bound to exact scheduled template/period");
  await checkOriginals(ctx, state);
  if (!isDeepStrictEqual(await get("journal"), sourceBooks)) throw Error("Scheduler generation changed the ledger");
  return state;
}

/** Current normal restored owner only; historical labels are evidence, never new authority. */
export async function checkBackupScheduledRecurring(ctx: Context, state: State, reopened: boolean) {
  const { get, post } = api(ctx);
  const me = await (await ctx.page.request.get(ctx.base + "/api/me")).json();
  const books = await get("journal");
  if (state.company !== ctx.company || !me.user?.id || me.user.id === state.sourceOwner || books.journal.ledgerUuid !== state.sourceBooks.journal.ledgerUuid) throw Error("Fresh owner/company/ledger identity boundary failed");
  await checkOriginals(ctx, state);
  let now = await views(ctx, state.ids);
  if (!isDeepStrictEqual(now.recurring, reopened ? state.postRestore?.recurring : state.source.recurring)) throw Error("Restored source/post-action template and generation history differs");
  if (reopened) {
    if (!state.postRestore || state.restoredOwner !== me.user.id || !isDeepStrictEqual(now, state.postRestore)) throw Error("Reopened scheduler consent/history/projection changed or dispatched");
  } else {
    if (!isDeepStrictEqual(books, state.sourceBooks)) throw Error("Source-free restored books differ before new intent");
    for (const [kind, id] of Object.entries(state.ids)) {
      const actual = row(now, id), source = row(state.source, id);
      if (!isDeepStrictEqual(actual.history, source.history) || !isDeepStrictEqual(actual.lastRun, source.lastRun) || actual.templateHash !== source.templateHash) throw Error("Imported raw scheduler history/hash changed");
      if (source.state === "enabled" || source.state === "paused") {
        const consent = actual.restoreConsent;
        if (actual.state !== "paused" || actual.ownerUserId !== null || actual.queued.length !== 0 || !isDeepStrictEqual(actual.nextOccurrence, source.nextOccurrence) || consent?.required !== true || !/^[0-9a-f]{64}$/.test(consent.boundaryHash) || consent.historicalOwnerUserId !== source.ownerUserId || !isDeepStrictEqual(consent.historicalQueued, source.queued)) throw Error("Imported authority/queue became live or provenance was lost");
        const p = now.plans[kind];
        if (p.normalDue !== null || p.queued.length || !isDeepStrictEqual(p.restoreConsent, consent)) throw Error("Restored plan enables imported work before reconsent");
      } else if (actual.state !== source.state) throw Error("Cancelled source state changed");
    }
    const beforeRefusal = await views(ctx, state.ids);
    await post("scheduled-recurring/reconsent", { templateId: state.ids.browser, planHash: state.source.plans.browser.planHash, reason: "Stale source-plan consent must refuse", confirm: true }, 422);
    if (!isDeepStrictEqual(await views(ctx, state.ids), beforeRefusal)) throw Error("Stale source plan changed history");
    const endpoint = `/api/companies/${ctx.company}/scheduled-recurring/`;
    const writes: string[] = [];
    const observe = (request: any) => { if (request.method() === "POST" && new URL(request.url()).pathname.startsWith(endpoint)) writes.push(new URL(request.url()).pathname); };
    ctx.page.on("request", observe);
    try {
      await ctx.page.goto(`${ctx.base}/companies/${ctx.company}/faktura-skabeloner`);
      const panel = ctx.page.getByRole("region", { name: "Planlagte fakturaer", exact: true });
      await panel.getByLabel("Planlagt fakturaskabelon", { exact: true }).selectOption(String(state.ids.browser));
      await panel.getByRole("region", { name: "Planlægning efter gendannelse", exact: true }).waitFor();
      const response = ctx.page.waitForResponse(r => r.url() === ctx.base + endpoint + `${state.ids.browser}/plan` && r.request().method() === "GET");
      await panel.getByRole("button", { name: "Gennemgå forekomster", exact: true }).click();
      const p = (await (await response).json()).plan;
      await panel.getByRole("region", { name: "Gennemgå fakturaskabelon", exact: true }).getByRole("heading", { name: p.template.name, exact: true }).waitFor();
      await panel.getByLabel("Planlægningshandling", { exact: true }).selectOption("reconsent");
      const button = panel.getByRole("button", { name: "Godkend planlægning efter gendannelse", exact: true });
      if (!(await button.isDisabled())) throw Error("Reconsent admitted without reason/confirmation");
      await panel.getByLabel("Planlægningsbegrundelse", { exact: true }).fill("Fresh ordinary restored owner explicitly reviews this frozen specification");
      await panel.getByLabel("Jeg bekræfter den valgte handling og de viste forekomster.", { exact: true }).check();
      const accepted = ctx.page.waitForResponse(r => r.url() === ctx.base + endpoint + "reconsent" && r.request().method() === "POST");
      await button.click();
      const receipt = await accepted;
      if (!receipt.ok()) throw Error(`UI reconsent refused HTTP${receipt.status()}`);
      const native = await receipt.json();
      assertReconsent(native.scheduledRecurring, state.ids.browser, me.user.id, p.restoreConsent.boundaryHash);
      if (!isDeepStrictEqual(row(await views(ctx,state.ids),state.ids.browser),native.scheduledRecurring)) throw Error("UI consent full receipt differs from canonical readback");
      await panel.getByRole("status").getByText(/Dit samtykke er gemt/).waitFor();
      await panel.getByText("Ikke startet", { exact: true }).waitFor();
      if (writes.length !== 1 || writes[0] !== endpoint + "reconsent") throw Error("UI consent automatically dispatched/started another mutation");
    } finally { ctx.page.off("request", observe); }
    if (ctx.clients) await clientReconsents(ctx, state, me.user.id);
    now = await views(ctx, state.ids);
    for (const kind of ["browser", ...(ctx.clients ? ["cli", "mcp"] : [])]) {
      const actual = row(now, state.ids[kind]), source = row(state.source, state.ids[kind]);
      assertReconsent(actual, state.ids[kind], me.user.id, row(beforeRefusal, state.ids[kind]).restoreConsent.boundaryHash);
      if (!isDeepStrictEqual(actual.restoreConsent.historicalQueued,source.queued) || actual.restoreConsent.historicalOwnerUserId !== source.ownerUserId) throw Error("New consent erased original queue/owner evidence");
      if (!isDeepStrictEqual(actual.history.slice(0, source.history.length), source.history) || actual.history.length !== source.history.length + 1) throw Error("Consent rewrote or duplicated historical events");
      const reviewed = now.plans[kind];
      const before = await views(ctx, state.ids);
      await post("scheduled-recurring/reconsent", { templateId: state.ids[kind], planHash: reviewed.planHash, reason: "Same boundary cannot be consented twice", confirm: true }, 422);
      if (!isDeepStrictEqual(await views(ctx, state.ids), before)) throw Error("Repeated consent changed scheduler history");
    }
    const resolved = state.evidence.resolvedPeriods as {succeeded:number;skipped:number};
    for (const periodIndex of [resolved.succeeded, resolved.skipped]) {
      const before = await views(ctx, state.ids);
      await post("scheduled-recurring/dispatch", {templateId:state.ids.browser,planHash:before.plans.browser.planHash,periodIndex,confirm:true},422);
      if (!isDeepStrictEqual(await views(ctx,state.ids),before)) throw Error("Resolved historical period became executable after restored consent");
    }
    if (!isDeepStrictEqual(now.recurring,state.source.recurring)) throw Error("Consent issued an additional invoice or changed generation source history");
    const consentOnly = now;
    // The historical queue remains inactive until this separate new-owner decision.
    const oldQueue = row(state.source,state.ids.browser).queued;
    if (oldQueue.length !== 1) throw Error("One explicit source historical queued occurrence required");
    const selected = oldQueue[0];
    const reviewed = now.plans.browser;
    if (!reviewed.missed.some((occurrence:any)=>isDeepStrictEqual(occurrence,selected)) || reviewed.queued.length) throw Error("Historical occurrence was automatically queued or is not eligible for fresh review");
    await post("scheduled-recurring/catch-up",{templateId:state.ids.browser,planHash:reviewed.planHash,periodIndexes:[selected.periodIndex],confirm:true});
    const queued = await views(ctx,state.ids);
    if (!isDeepStrictEqual(queued.plans.browser.queued,[selected]) || !isDeepStrictEqual(queued.recurring,state.source.recurring)) throw Error("Fresh catch-up did not only queue the chosen historical occurrence");
    const dispatched = await post("scheduled-recurring/dispatch",{templateId:state.ids.browser,planHash:queued.plans.browser.planHash,periodIndex:selected.periodIndex,confirm:true});
    const documentId = dispatched.generation?.documentId;
    if (dispatched.dispatch?.status !== "succeeded" || !Number.isSafeInteger(documentId) || !dispatched.generation.invoiceNumber || state.originals.some(item=>item.documentId===documentId)) throw Error("Fresh restored-owner dispatch did not issue exactly one distinct invoice");
    const issuedOriginal = await original(ctx,documentId);
    const payload = JSON.parse(Buffer.from(issuedOriginal.jsonBase64,"base64").toString());
    if (payload.recurringTemplateId !== state.ids.browser || payload.recurringPeriodIndex !== selected.periodIndex) throw Error("Restored-owner original is not bound to reviewed template/period");
    state.originals.push(issuedOriginal);
    now = await views(ctx,state.ids);
    const generations = now.recurring.recurringInvoices.templates.flatMap((template:any)=>template.generations);
    if (generations.length !== 2 || generations.filter((generation:any)=>generation.documentId===documentId).length !== 1 || row(now,state.ids.browser).ownerUserId !== me.user.id) throw Error("Restored-owner generation inventory/authority differs");
    await checkOriginals(ctx,state);
    await post("scheduled-recurring/dispatch",{templateId:state.ids.browser,planHash:now.plans.browser.planHash,periodIndex:selected.periodIndex,confirm:true},422);
    if (!isDeepStrictEqual(await views(ctx,state.ids),now)) throw Error("Repeated restored period created another invoice/history event");
    state.restoredOwner = me.user.id;
    state.postRestore = now;
    state.evidence = { ...state.evidence, browserReconsent: true, cliReconsent: ctx.clients, mcpReconsent: ctx.clients,
      importedConsentInactive: true, immutableHistoricalQueue: true, staleAndRepeatedConsentRefused: true, resolvedHistoricalDispatchRefused:true,
      restoredOwnerDiffers: true, consentDidNotIssuePostOrSend: true, postConsentCanonical: consentOnly, postConsentSha256: digest(Buffer.from(JSON.stringify(consentOnly))),
      explicitRestoredOwnerCatchUpAndDispatch:{templateId:state.ids.browser,periodIndex:selected.periodIndex,receipt:dispatched,original:issuedOriginal,repeatedPeriodRefused:true,unpostedUnsent:true},
      postActionCanonical:now,postActionSha256:digest(Buffer.from(JSON.stringify(now))) };
  }
  await ctx.page.goto(`${ctx.base}/companies/${ctx.company}/faktura-skabeloner`);
  const panel = ctx.page.getByRole("region", { name: "Planlagte fakturaer", exact: true });
  await panel.getByLabel("Planlagt fakturaskabelon", { exact: true }).selectOption(String(state.ids.browser));
  await panel.getByText("Ikke startet", { exact: true }).waitFor();
  if (!isDeepStrictEqual(await views(ctx, state.ids), state.postRestore) || !isDeepStrictEqual(await get("journal"), books)) throw Error("Reopen/status/UI issued work or changed books");
  await checkOriginals(ctx, state);
  state.evidence[reopened ? "exactAfterReopen" : "exactAfterRestore"] = true;
  return state;
}
function assertReconsent(actual: any, templateId: number, owner: string, boundary: string) {
  if (actual?.templateId !== templateId || actual.state !== "enabled" || actual.ownerUserId !== owner || actual.queued?.length !== 0 || actual.restoreConsent?.required !== false || actual.restoreConsent.boundaryHash !== boundary) throw Error("Canonical explicit new-owner consent receipt differs");
}
async function clientReconsents(ctx: Context, state: State, owner: string) {
  const { plan } = api(ctx);
  const cookie = join(ctx.work, "restored-scheduled-clients.cookie");
  writeFileSync(cookie, (await ctx.page.context().cookies(ctx.base)).map(c => `${c.name}=${c.value}`).join("; "), { flag: "wx", mode: 0o600 });
  const common = ["--server", ctx.base, "--allow-loopback-http", "--cookie-file", cookie];
  try {
    for (const kind of ["cli", "mcp"]) {
      const binary = join(ctx.work, kind === "cli" ? "rentemester-rust" : "rentemester-mcp");
      if (realpathSync(binary) !== binary || !statSync(binary).isFile()) throw Error("Pinned private scheduled client executable required");
      const id = state.ids[kind], p = await plan(id);
      const body = { templateId: id, planHash: p.planHash, reason: `Fresh restored owner explicitly reconsents through ${kind}`, confirm: true };
      const messages = [{ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-portable-scheduler", version: "1" } } }, { jsonrpc: "2.0", method: "notifications/initialized" }, { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "reconsent_scheduled_recurring", arguments: { company: ctx.company, body: JSON.stringify(body), confirm: true } } }];
      const process = spawnSync(binary, kind === "cli" ? ["remote", "reconsent-scheduled-recurring", ...common, "--company", ctx.company, "--confirm"] : common,
        { input: kind === "cli" ? JSON.stringify(body) : messages.map(value => JSON.stringify(value)).join("\n") + "\n", encoding: "utf8", timeout: 30000, maxBuffer: 32 * 1024 * 1024 });
      if (process.error || process.status !== 0) throw Error(`Pinned ${kind} reconsent failed; do not replay`);
      const native = kind === "cli" ? JSON.parse(process.stdout).data : (() => {
        const response = process.stdout.trim().split("\n").map(line => JSON.parse(line)).find(value => value.id === 2);
        if (response?.error || response?.result?.isError) throw Error("MCP restored consent refused");
        return response.result.structuredContent.data;
      })();
      assertReconsent(native.scheduledRecurring, id, owner, p.restoreConsent.boundaryHash);
      const canonical = row(await views(ctx,state.ids),id);
      if (!isDeepStrictEqual(canonical,native.scheduledRecurring)) throw Error("Client reconsent full receipt differs from canonical readback");
    }
  } finally { unlinkSync(cookie); }
}
