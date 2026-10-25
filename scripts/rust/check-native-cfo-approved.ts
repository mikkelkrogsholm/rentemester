import { expect, type Page, type Response } from "@playwright/test";
import { randomUUID } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual as equal } from "node:util";

/** One fresh synthetic journal only. Never rerun after an unknown write outcome:
 * inspect the private intent/receipt and canonical journal first. Authentication,
 * verified MFA and company/document creation belong to the calling owner. */
export async function checkNativeCfoApprovedPagination(
  owner: Page, reviewer: Page, base: string, company: string, work: string,
) {
  function need(v: unknown, message: string): asserts v {
    if (!v) throw Error(`CFO718: ${message}`);
  }
  need(/^http:\/\/127\.0\.0\.1:\d+$/.test(base), "literal loopback origin required");
  need(/^synthetic[a-z0-9-]*$/.test(company), "dedicated synthetic company required");
  need(owner !== reviewer && owner.context() !== reviewer.context(), "independent authenticated contexts required");
  const root = `/api/companies/${company}/`, headers = { origin: base, "content-type": "application/json" };
  mkdirSync(work, { recursive: true, mode: 0o700 });
  const reportPath = join(work, "native-cfo-approved718-report.json");
  const report: any = { company, status: "started", effects: [] };
  const save = () => writeFileSync(reportPath, JSON.stringify(report, null, 2), { mode: 0o600 });
  // Exclusive creation prevents an accidental helper rerun from issuing another journal.
  writeFileSync(join(work, "native-cfo-approved718-start.json"), JSON.stringify({ company, base }), { mode: 0o600, flag: "wx" });
  async function get(path: string, page = owner): Promise<any> {
    const r = await page.request.get(base + path);
    need(r.ok(), `GET ${path} HTTP ${r.status()}`);
    return r.json();
  }
  async function post(path: string, data: unknown, page = owner, status = 200): Promise<any> {
    // No HTTP retries, including when a response is lost or cannot be decoded.
    const r = await page.request.post(base + path, { headers, data, maxRetries: 0 });
    need(r.status() === status, `POST ${path} expected ${status}, got ${r.status()}; inspect canonical state, do not retry`);
    const result = await r.json();
    report.effects.push({ path, status, result }); save();
    return result;
  }
  const observed: Array<{ url: URL; body: any }> = [];
  const pending = new Set<Promise<void>>();
  const observe = (r: Response) => {
    const url = new URL(r.url());
    if (url.origin !== base || url.pathname !== "/api/cfo-analytics" || r.request().method() !== "GET") return;
    const task = (async () => {
      try { observed.push({ url, body: r.ok() ? await r.json() : { httpStatus: r.status() } }); }
      catch { observed.push({ url, body: { unreadable: true } }); }
    })();
    pending.add(task); void task.finally(() => pending.delete(task));
  };
  try {
    save();
    const a = await get("/api/me"), b = await get("/api/me", reviewer);
    need(a.user?.id && b.user?.id && a.user.id !== b.user.id, "two distinct normal current MFA principals required");
    const asOf = new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
    const initial = (await get(root + "journal")).journal;
    need(initial.entries.length === 0 && initial.nativePosting?.ruleVersion, "empty current ledger and current posting rule required");
    const documents = (await get(root + "documents")).documents.documents;
    need(documents.length === 1 && /\.txt$/i.test(documents[0].filename), "exactly one registered synthetic TXT original required");
    const documentId = documents[0].id;
    let accounts = (await get(root + "accounts")).accounts.accounts;
    need(Array.isArray(accounts), "canonical account inventory missing");
    const choices: string[] = [];
    for (const [accountNo, type, normalBalance] of [["1718", "asset", "debit"], ["8718", "equity", "credit"]] as const) {
      let account = accounts.find((v: any) => v.type === type && v.normalBalance === normalBalance && v.active !== false && v.allowDirectPosting !== false && !v.defaultVatCode);
      if (!account) {
        need(!accounts.some((v: any) => v.accountNo === accountNo), `synthetic account ${accountNo} conflicts; no overwrite`);
        await post(root + "accounts", { accountNo, name: `Synthetic CFO718 ${type}`, type, normalBalance });
        accounts = (await get(root + "accounts")).accounts.accounts;
        account = accounts.find((v: any) => v.accountNo === accountNo && v.type === type && v.normalBalance === normalBalance);
      }
      need(account, `explicit typed ${type} account unavailable`); choices.push(account.accountNo);
    }
    const idempotencyKey = `synthetic-cfo718-${randomUUID()}`;
    const posting = { confirm: true, input: { format: "rentemester-manual-journal-post/1", expectedRuleVersion: initial.nativePosting.ruleVersion, idempotencyKey, request: {
      documentId: String(documentId), transactionDate: asOf, text: "Synthetic CFO718 102 source lines, no VAT", currency: "DKK",
      lines: [ ...Array.from({ length: 51 }, () => ({ accountNo: choices[0], debitAmount: "1.00", creditAmount: "0.00" })), ...Array.from({ length: 51 }, () => ({ accountNo: choices[1], debitAmount: "0.00", creditAmount: "1.00" })) ],
    } } };
    writeFileSync(join(work, "native-cfo-approved718-post-intent.json"), JSON.stringify({ company, posting }), { mode: 0o600, flag: "wx" });
    const receipt = await post(root + "manual-journal", posting);
    const journal = (await get(root + "journal")).journal;
    need(journal.entries.length === 1, "exactly one new journal required");
    const entry = journal.entries[0];
    need(entry.id === receipt.entryId && entry.lines.length === 102 && String(entry.documentId) === String(documentId), "exact receipt/document/102-line journal required");
    need(entry.lines.filter((l: any) => l.accountNo === choices[0] && (l.debit === 1 || l.debit === "1.00") && (l.credit === 0 || l.credit === "0.00")).length === 51 && entry.lines.filter((l: any) => l.accountNo === choices[1] && (l.credit === 1 || l.credit === "1.00") && (l.debit === 0 || l.debit === "0.00")).length === 51, "exact DKK51 asset/equity source required");
    const groupId = `cfo718-${randomUUID()}`, profileId = `${groupId}-profile`;
    const prior = await get(root + "intercompany");
    need(prior.manifest !== null || prior.manifestHash === null, "cannot replace hidden manifest");
    await post(root + "intercompany/manifest", { version: 1, groups: [...(prior.manifest?.groups ?? []), { id: groupId, name: "Synthetic CFO718 group", memberships: [{ id: "cfo718-member", companySlug: company, validFrom: asOf }], ownership: [] }], confirm: true });
    const reportingLines = [["assets", "Assets", "asset"], ["debt", "Debt", "liability"], ["equity", "Equity", "equity"], ["result", "Current result", "equity"], ["income", "Income", "income"], ["expense", "Expense", "expense"]].map(([id, label, section], displayOrder) => ({ id, label, section, displayOrder, ...(id === "result" ? { role: "current-result" } : {}) }));
    const profile = { version: 1, id: profileId, groupId, currency: "DKK", reportingLines, accountMappings: choices.map((accountNo, i) => ({ id: `cfo718-${i}`, companySlug: company, accountNo, reportingLineId: i === 0 ? "assets" : "equity", validFrom: asOf })), evidenceRefs: ["synthetic-cfo718-reviewed-asset-equity-original"], validFrom: asOf };
    const proposal = await post(root + "consolidation/propose", { profile, confirm: true });
    need(/^[a-f0-9]{64}$/.test(proposal.profileHash), "exact proposed profileHash missing");
    const approval = { profileId, profileHash: proposal.profileHash, confirm: true };
    await post(root + "consolidation/approve", approval, owner, 422);
    await post(root + "consolidation/approve", approval, reviewer);
    const approved = (await get(root + `consolidation?asOf=${asOf}`)).profiles.find((v: any) => v.profile.id === profileId);
    need(approved?.status === "approved" && approved.profileHash === proposal.profileHash && approved.actor === `user:${b.user.id}`, "exact independent reviewer approval required");
    const canonical = await get("/api/group-consolidated-report?" + new URLSearchParams({ profileId, from: asOf, asOf }));
    need(canonical.status === "ready" && canonical.appliedEliminations.length === 0 && canonical.sourceSnapshots.length === 1, "ready one-member canonical report required");
    for (const line of canonical.consolidatedFigures) {
      const expected = ["assets", "equity"].includes(line.lineId) ? "51.00" : "0.00";
      need(line.rawCompanySum === expected && line.eliminationAdjustment === "0.00" && line.consolidatedAmount === expected, `canonical ${line.lineId} figures differ`);
    }
    need(canonical.consolidatedFigures.length === 6, "exact six canonical lines required");
    const before = await get(root + "journal");
    owner.on("response", observe);
    await owner.goto(base + "/cfo");
    const controls = owner.getByRole("group", { name: "CFO-afgrænsning", exact: true });
    await controls.getByLabel("Fra dato", { exact: true }).fill(asOf);
    await controls.getByLabel("Pr. dato", { exact: true }).fill(asOf);
    await controls.getByLabel("Visning", { exact: true }).selectOption("group");
    await controls.getByLabel("Koncernprofil", { exact: true }).selectOption(profileId);
    const matches = (scope: string, cursor: string | null) => observed.filter(({ url }) => url.searchParams.get("scope") === scope && url.searchParams.get("from") === asOf && url.searchParams.get("to") === asOf && url.searchParams.get("cursor") === cursor && (scope === "group" ? url.searchParams.get("groupProfileId") === profileId : url.searchParams.get("companySlug") === company));
    await expect.poll(() => matches("group", null).length).toBeGreaterThan(0);
    const groupRead = matches("group", null).at(-1)!;
    need(groupRead.body.status === "ready" && groupRead.body.group.status === "ready" && equal(groupRead.body.group.consolidatedFigures, canonical.consolidatedFigures), "actual CFO group response must equal canonical ready figures");
    const region = owner.getByRole("region", { name: "Koncernrapport", exact: true });
    await expect(region.locator("tbody tr")).toHaveCount(6);
    for (let i = 0; i < canonical.consolidatedFigures.length; i++) {
      const line = canonical.consolidatedFigures[i];
      const display = (v: string) => v.replace(".", ",") + " kr.";
      await expect(region.locator("tbody tr").nth(i).locator("td")).toHaveText([line.label, display(line.rawCompanySum), display(line.eliminationAdjustment), display(line.consolidatedAmount)]);
    }
    await expect(region.getByRole("link", { name: "Se struktur, afstemning og kilde-evidens", exact: true })).toHaveAttribute("href", "/koncernstruktur");
    await controls.getByLabel("Visning", { exact: true }).selectOption("company");
    await controls.getByLabel("Virksomhed", { exact: true }).selectOption(company);
    await expect.poll(() => matches("company", null).length).toBeGreaterThan(0);
    const first = matches("company", null).at(-1)!;
    need(!first.url.searchParams.has("limit") && first.body.status === "ready" && first.body.page.limit === 100 && first.body.rows.length === 100 && first.body.page.nextCursor && first.body.reconciliation.rowCount === 102, "default 100 first rows, cursor and whole-universe 102 reconciliation required");
    const rowsRegion = owner.getByRole("region", { name: "Kildeposter", exact: true });
    async function rendered(rows: any[]) {
      await expect(rowsRegion.locator("tbody tr")).toHaveCount(rows.length);
      await expect(rowsRegion.locator("tbody summary")).toHaveText(rows.map(r => `Kildedetaljer ${r.sourceId}`));
    }
    await rendered(first.body.rows);
    const cursor = first.body.page.nextCursor;
    const [nextResponse] = await Promise.all([owner.waitForResponse(r => { const u = new URL(r.url()); return u.origin === base && u.pathname === "/api/cfo-analytics" && u.searchParams.get("scope") === "company" && u.searchParams.get("companySlug") === company && u.searchParams.get("cursor") === cursor; }), owner.getByRole("button", { name: "Næste kildeposter", exact: true }).click()]);
    need(nextResponse.ok(), "next cursor HTTP read failed");
    const second = await nextResponse.json();
    need(second.status === "ready" && second.rows.length === 2 && second.page.nextCursor === null && equal(second.reconciliation, first.body.reconciliation), "two remaining rows with unchanged whole-universe reconciliation required");
    const identity = (r: any) => `${r.companySlug}:${r.sourceType}:${r.sourceId}`;
    need(new Set([...first.body.rows, ...second.rows].map(identity)).size === 102, "102 distinct combined source identities required");
    await rendered(second.rows);
    const [resetResponse] = await Promise.all([owner.waitForResponse(r => { const u = new URL(r.url()); return u.origin === base && u.pathname === "/api/cfo-analytics" && u.searchParams.get("scope") === "company" && u.searchParams.get("companySlug") === company && !u.searchParams.has("cursor"); }), owner.getByRole("button", { name: "Første kildeposter", exact: true }).click()]);
    need(resetResponse.ok() && equal(await resetResponse.json(), first.body), "reset must return exact first page and reconciliation");
    await rendered(first.body.rows);
    const after = await get(root + "journal");
    need(equal(before, after), "browser report reads changed journal or source lines");
    const result = { status: "passed", company, profileId, profileHash: proposal.profileHash, journalEntryId: receipt.entryId, journalLines: 102, firstRows: 100, remainingRows: 2, combinedIdentities: 102, reconciliationRows: 102, journalUnchanged: true, reportPath };
    Object.assign(report, { status: "passed", result, asOf, canonical, groupRead: groupRead.body, first: first.body, second, before, after }); save();
    return result;
  } catch (error) {
    report.status = "failed"; report.error = error instanceof Error ? error.message : String(error); save();
    throw error;
  } finally {
    owner.off("response", observe); await Promise.all(pending);
  }
}
