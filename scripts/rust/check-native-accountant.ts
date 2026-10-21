import type { Page } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync, rmSync, existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";
import { readTar } from "../../src/core/tar";
const saved = "/tmp/rentemester-rust-reports/native-accountant.json";
type Evidence = { journal: unknown[]; documents: Array<{ id: number; sha256: string }>; audit: Array<{ id: number; [key: string]: unknown }>; periodStart: string; periodEnd: string };
export async function checkNativeAccountant(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic`;
  const headers = { origin: base, "content-type": "application/json" };
  const overview = (await (await page.request.get(`${url}/overview`)).json()).overview;
  if (overview.availability?.accountantExport !== true) throw new Error("Native accountant export still unavailable on company overview");
  const periodStart: string = overview.periodStart, periodEnd: string = overview.periodEnd;
  const journalBefore = (await (await page.request.get(`${url}/journal?year=${encodeURIComponent(overview.selectedYear)}`)).json()).journal.entries;
  for (const data of [{ periodStart: "2026-02-30", periodEnd, confirm: true }, { periodStart: periodEnd, periodEnd: periodStart, confirm: true }, { periodStart, periodEnd, confirm: false }]) {
    const denied = await page.request.post(`${url}/accountant-export`, { headers, data });
    if (denied.status() !== 422) throw new Error(`Invalid accountant export accepted: ${denied.status()}`);
  }
  const wrongCompany = await page.request.post(`${base}/api/companies/unselected/accountant-export`, { headers, data: { periodStart, periodEnd, confirm: true } });
  if (wrongCompany.status() !== 404) throw new Error("Accountant export crossed selected company boundary");
  await page.goto(`${base}/companies/synthetic`);
  await page.getByRole("heading", { name: "Revisor-eksport", exact: true }).waitFor();
  await page.getByLabel("Fra", { exact: true }).fill(periodStart);
  await page.getByLabel("Til", { exact: true }).fill(periodEnd);
  const [download, response] = await Promise.all([
    page.waitForEvent("download"),
    page.waitForResponse(r => r.url().endsWith("/accountant-export") && r.request().method() === "POST"),
    page.getByRole("button", { name: "Generér og download", exact: true }).click(),
  ]);
  if (!response.ok()) throw new Error(`Accountant download failed: ${await response.text()}`);
  const filename = `revisor-eksport-synthetic-${periodStart}-${periodEnd}.tar`;
  if (download.suggestedFilename() !== filename || response.headers()["content-type"] !== "application/x-tar" || response.headers()["cache-control"] !== "private, no-store") throw new Error("Accountant archive download contract is wrong");
  mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
  const path = `/tmp/rentemester-rust-reports/${restarted ? "reopened-" : ""}${filename}`;
  await download.saveAs(path);
  const entries = readTar(readFileSync(path));
  const roots = new Set(entries.map(e => e.path.split("/")[0]));
  if (roots.size !== 1 || ![...roots][0].startsWith("revisor-eksport-")) throw new Error("Accountant archive is not one portable handoff package");
  const root = [...roots][0];
  const files = new Map(entries.map(e => [e.path.slice(root.length + 1), Buffer.from(e.content)]));
  const manifest = JSON.parse(files.get("manifest.json")!.toString("utf8"));
  if (manifest.manifestVersion !== 2 || manifest.packageType !== "accountant_handoff_export" || manifest.periodStart !== periodStart || manifest.periodEnd !== periodEnd || manifest.handoffModel !== "local_export_package" || manifest.accessModel !== "no_runtime_access") throw new Error("Accountant manifest contract is wrong");
  if (manifest.provenance !== null || !manifest.limitations.includes("native_release_provenance_unavailable")) throw new Error("Accountant export invented native release provenance");
  for (const output of manifest.outputs as Array<{ path: string; sha256: string; sizeBytes: number }>) {
    const bytes = files.get(output.path);
    if (!bytes || bytes.length !== output.sizeBytes || createHash("sha256").update(bytes).digest("hex") !== output.sha256) throw new Error(`Accountant output hash/size mismatch: ${output.path}`);
  }
  if (manifest.outputs.length !== files.size - 1) throw new Error("Accountant manifest omitted an archive file");
  const journal = JSON.parse(files.get("machine-readable/journal-entries.json")!.toString("utf8"));
  const documents = JSON.parse(files.get("machine-readable/documents.json")!.toString("utf8"));
  const audit = JSON.parse(files.get("machine-readable/audit-log.json")!.toString("utf8"));
  const banks = JSON.parse(files.get("machine-readable/bank-transactions.json")!.toString("utf8"));
  if (journal.length !== journalBefore.length || !journal.length || !documents.length || !Array.isArray(banks) || !audit.length) throw new Error("Accountant archive has missing synthetic bookkeeping evidence");
  const liveBank = (await (await page.request.get(`${url}/bank?year=${encodeURIComponent(overview.selectedYear)}`)).json()).bank;
  if (banks.length !== liveBank.transactions.length) throw new Error("Accountant archive bank register disagrees with live selected-year bank reader");
  for (const key of ["journal-entries.csv", "trial-balance.csv", "accounts.json", "companies.json", "schema-migrations.json", "vat-report.json", "exceptions.json", "bank-journal-reconciliation-links.json"]) if (!files.get(`machine-readable/${key}`)?.length) throw new Error(`Accountant archive omitted ${key}`);
  for (const copy of manifest.copiedDocuments as Array<{ exportedPath: string; sha256: string; sizeBytes: number; documentId: number }>) {
    const bytes = files.get(copy.exportedPath);
    if (!bytes || createHash("sha256").update(bytes).digest("hex") !== copy.sha256) throw new Error("Accountant original does not match registered evidence");
    const original = await page.request.get(`${url}/documents/${copy.documentId}/file`);
    if (!original.ok() || !(await original.body()).equals(bytes)) throw new Error("Accountant original differs from actual document download");
  }
  if (!manifest.copiedDocuments.length || manifest.counts.copiedReadableDocuments !== manifest.copiedDocuments.length) throw new Error("Accountant archive contains no verified originals");
  for (const [header, count] of [["x-rentemester-journal-entries", journal.length], ["x-rentemester-documents", documents.length], ["x-rentemester-bank-transactions", banks.length]] as const) if (Number(response.headers()[header]) !== count) throw new Error("Accountant UI receipt counters disagree with archive");
  await page.getByText(`Hentede ${filename}`, { exact: false }).waitFor();
  const journalAfter = (await (await page.request.get(`${url}/journal?year=${encodeURIComponent(overview.selectedYear)}`)).json()).journal.entries;
  if (JSON.stringify(journalAfter) !== JSON.stringify(journalBefore)) throw new Error("Accountant export wrote bookkeeping effects");
  const evidence: Evidence = { journal, documents: documents.map((d: { id: number; sha256: string }) => ({ id: d.id, sha256: d.sha256 })), audit, periodStart, periodEnd };
  if (!restarted) writeFileSync(saved, JSON.stringify(evidence));
  else {
    const original: Evidence = JSON.parse(readFileSync(saved, "utf8"));
    if (JSON.stringify(original.journal) !== JSON.stringify(journal) || JSON.stringify(original.documents) !== JSON.stringify(evidence.documents)) throw new Error("Accountant bookkeeping/original evidence changed after restart");
    for (const row of original.audit) if (JSON.stringify(audit.find((a: { id: number }) => a.id === row.id)) !== JSON.stringify(row)) throw new Error("Accountant export lost original audit history after restart");
    if (!audit.some((a: { eventType: string }) => a.eventType === "accountant_handoff_export")) throw new Error("Accountant export audit event did not survive restart");
  }
  await page.screenshot({ path: "/tmp/rentemester-rust-accountant.png", fullPage: true });
  console.log(restarted ? "PASS: accountant browser download reopens after restart with original journal/documents/audit preserved" : "PASS: Overblik → Revisor-eksport browser download → portable manifest/JSON/CSV/VAT/audit and hash-verified original documents; period, confirmation and company guards hold; journal unchanged");
}

/** Request-context handover through the same normal-MFA standalone runner. */
export function createAuthorityExportProof(page: Page, base: string, work: string, member: Page) {
  const prefix = '/api/companies/synthetic/', ownerFile = join(work, 'authority-owner.cookie'), memberFile = join(work, 'authority-member.cookie');
  const need = (v: unknown, message: string) => { if (!v) throw Error(message); };
  const sha = (v: Buffer | string) => createHash('sha256').update(v).digest('hex');
  let input: any, stable: any, saved: any, documentId: number, originalPath: string, completed = false, exports = 0, sequence = 0;
  const original = Buffer.from('Synthetic authority handover source 100 DKK\n');
  async function get(path: string, actor = page) { const r = await actor.request.get(base + path); need(r.ok(), 'Authority canonical read ' + path); return r.json(); }
  async function post(path: string, data: unknown) { const r = await page.request.post(base + path, { headers: { origin: base }, data }); const v = await r.json(); need(r.ok() && v.ok !== false, 'Authority setup ' + path + ' ' + JSON.stringify(v)); return v; }
  async function cookies(actor: Page, file: string) { const c = await actor.context().cookies(base); need(c.some(c => c.httpOnly), 'Normal MFA export cookie'); writeFileSync(file, c.map(c => c.name + '=' + c.value).join('; '), { mode: 0o600 }); }
  async function books() { return { journal: await get(prefix + 'journal/entries?all=true'), trial: await get(prefix + 'trial-balance'), bank: await get(prefix + 'bank'), documents: await get(prefix + 'documents') }; }
  async function unchanged() { need(isDeepStrictEqual(await books(), stable), 'Handover must not change bookkeeping/source records'); }
  async function audit() { return (await get(prefix + 'audit-log?limit=5000')).rows.filter((r: any) => ['authority_export', 'accountant_handoff_export'].includes(r.eventType)); }
  async function auditCount() { need((await audit()).length === exports, 'Exactly one audit per confirmed accepted export; no failed-export effects'); }
  function verify(files: Map<string, Buffer>, profile: string) {
    const manifest = JSON.parse(files.get('manifest.json')!.toString('utf8'));
    need(manifest.manifestVersion === 2 && manifest.packageType === (profile === 'authority' ? 'authority_export' : 'accountant_handoff_export'), 'Recipient-correct manifest');
    need(manifest.requestedAt === new Date(input.requestedAt).toISOString() && manifest.generatedAt === manifest.requestedAt && manifest.generatedAtExplicit === true && manifest.requester === input.requester, 'Original request context and explicit timestamp');
    need(manifest.deadlineAt === new Date(Date.parse(input.requestedAt) + 28 * 86400000).toISOString(), 'Exactly four elapsed UTC weeks');
    need(manifest.periodStart === input.periodStart && manifest.periodEnd === input.periodEnd && manifest.counts.journalEntries === 1 && manifest.counts.documents === 1 && manifest.counts.bankTransactions === 0, 'Complete selected source scope');
    need(manifest.provenance === null && manifest.limitations.includes('native_release_provenance_unavailable'), 'No invented release attestation');
    for (const output of manifest.outputs) { const bytes = files.get(output.path); need(bytes && bytes.length === output.sizeBytes && sha(bytes) === output.sha256, 'Every handover output hash and size'); }
    need(manifest.outputs.length === files.size - 1, 'Manifest covers every non-manifest file');
    const copied = manifest.copiedDocuments.find((d: any) => d.documentId === documentId); need(copied && copied.sha256 === sha(original) && files.get(copied.exportedPath)?.equals(original), 'Exact readable original included'); originalPath = copied.sourcePathRelativeToCompany;
    const journal = JSON.parse(files.get('machine-readable/journal-entries.json')!.toString('utf8')); need(journal.length === 1 && journal[0].lines.length === 2, 'Whole journal source and lines');
    need(JSON.parse(files.get('machine-readable/audit-log.json')!.toString('utf8')).length > 0, 'Actual audit context included');
    if (profile === 'accountant_handoff') need(manifest.handoffModel === 'local_export_package' && manifest.accessModel === 'no_runtime_access', 'Accountant handoff model retained');
    return { packageType: manifest.packageType, requestedAt: manifest.requestedAt, generatedAt: manifest.generatedAt, deadlineAt: manifest.deadlineAt, requester: manifest.requester, periodStart: manifest.periodStart, periodEnd: manifest.periodEnd, journalSha256: sha(files.get('machine-readable/journal-entries.json')!), originalSha256: copied.sha256 };
  }
  function archive(bytes: Buffer, profile: string) {
    const entries = readTar(bytes), roots = new Set(entries.map(e => e.path.split('/')[0])); need(roots.size === 1, 'One portable export root'); const root = [...roots][0];
    need(root.startsWith(profile === 'authority' ? 'myndigheds-eksport-' : 'revisor-eksport-'), 'Recipient-correct directory name');
    return verify(new Map(entries.map(e => [e.path.slice(root.length + 1), Buffer.from(e.content)])), profile);
  }
  async function download(profile: string, actor = page) {
    const endpoint = prefix + (profile === 'authority' ? 'authority-export' : 'accountant-export');
    const r = await actor.request.post(base + endpoint, { headers: { origin: base }, data: input }); need(r.ok() && r.headers()['content-type'] === 'application/x-tar' && r.headers()['cache-control'] === 'private, no-store', 'Confirmed private archive download');
    const bytes = await r.body(); exports++; await auditCount(); return { projection: archive(bytes, profile), tarBase64: bytes.toString('base64') };
  }
  function local(result: any, out: string, profile: string) {
    need(result.ok && result.exportDir.startsWith(out + '/') && result.manifestPath === join(result.exportDir, 'manifest.json'), 'Original local export metadata');
    const manifest = JSON.parse(readFileSync(result.manifestPath, 'utf8')), files = new Map<string, Buffer>([['manifest.json', readFileSync(result.manifestPath)]]);
    for (const output of manifest.outputs) files.set(output.path, readFileSync(join(result.exportDir, output.path)));
    const projection = verify(files, profile); rmSync(out, { recursive: true }); return projection;
  }
  function cli(profile: string, file = ownerFile, denied = false) {
    const out = join(work, 'authority-cli-' + (++sequence)), command = profile === 'authority' ? 'export-authority' : 'export-accountant';
    const r = spawnSync(join(work, 'rentemester-rust'), ['system', command, '--server', base, '--cookie-file', file, '--allow-loopback-http', '--company', 'synthetic', '--from', input.periodStart, '--to', input.periodEnd, '--out', out, '--requested-at', input.requestedAt, '--requester', input.requester, '--json'], { encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!r.error && r.status === (denied ? 1 : 0) && !r.stdout.includes(readFileSync(file, 'utf8')) && !r.stderr.includes(readFileSync(file, 'utf8')), 'Authority CLI business exit/privacy');
    if (denied) { need(!existsSync(out), 'Refused CLI produces no local package'); return null; } exports++; return local(JSON.parse(r.stdout), out, profile);
  }
  function mcp(confirmed = true, file = ownerFile, denied = false) {
    const out = join(work, 'authority-mcp-' + (++sequence));
    const args = { company: 'synthetic', from: input.periodStart, to: input.periodEnd, out, requestedAt: input.requestedAt, requester: input.requester, confirm: confirmed };
    const wire = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-authority136', version: '1' } } }, { jsonrpc: '2.0', method: 'notifications/initialized' }, { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'system_export_authority', arguments: args } }].map(v => JSON.stringify(v)).join('\n') + '\n';
    const r = spawnSync(join(work, 'rentemester-mcp'), ['--server', base, '--cookie-file', file, '--allow-loopback-http'], { input: wire, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 }); need(!r.error && r.status === 0, 'Authority MCP transport');
    const call = r.stdout.trim().split('\n').map(v => JSON.parse(v)).find(v => v.id === 2), v = call?.result?.structuredContent; need(!call?.error && call.result.isError === denied && v?.ok === !denied && Array.isArray(v.errors), 'Original authority legacy envelope');
    need(isDeepStrictEqual(JSON.parse(call.result.content[0].text), v), 'Authority text/structured match');
    if (denied) { need(v.errors.length > 0 && !existsSync(out), 'Unconfirmed/refused MCP no package'); return null; }
    exports++; return local({ ...v.data, ok: v.ok, errors: v.errors, appliedRules: v.appliedRules }, out, 'authority');
  }
  async function projections() {
    const authority = await download('authority'), accountant = await download('accountant_handoff');
    need(isDeepStrictEqual(cli('authority'), authority.projection) && isDeepStrictEqual(cli('accountant_handoff'), accountant.projection) && isDeepStrictEqual(mcp(), authority.projection), 'Original HTTP/CLI/MCP same selected request/source context'); await auditCount(); await unchanged();
    return { authority, accountant };
  }
  return {
    async run() {
      need(!completed, 'Authority fixture only once'); await cookies(page, ownerFile); await cookies(member, memberFile);
      const today = (await get('/api/portfolio')).portfolio.asOf;
      input = { periodStart: today.slice(0, 4) + '-01-01', periodEnd: today, requestedAt: today + 'T12:34:56.789+02:00', requester: 'Synthetic authority requester', confirm: true };
      for (const [accountNo, type, normalBalance] of [['1736', 'asset', 'debit'], ['3736', 'income', 'credit']]) await post(prefix + 'accounts', { accountNo, name: 'Synthetic handover ' + accountNo, type, normalBalance });
      const uploaded = await post(prefix + 'documents/ingest', { confirm: true, fileName: 'synthetic-authority.txt', fileBase64: original.toString('base64'), metadata: { source: 'Synthetic authority handover', documentType: 'cash_register_receipt', issueDate: today } }); documentId = uploaded.document.id;
      await post(prefix + 'manual-journal', { confirm: true, input: { format: 'rentemester-manual-journal-post/1', expectedRuleVersion: (await get(prefix + 'journal')).journal.nativePosting.ruleVersion, idempotencyKey: 'synthetic-authority136', request: { transactionDate: today, text: 'Synthetic authority source', currency: 'DKK', documentId: String(documentId), lines: [{ accountNo: '1736', debitAmount: '100.00', creditAmount: '0.00' }, { accountNo: '3736', debitAmount: '0.00', creditAmount: '100.00' }] } } });
      stable = await books(); saved = await projections();
      for (const data of [{ ...input, confirm: false }, { ...input, requestedAt: '2026-02-30T12:00:00Z' }, { ...input, requester: 'bad\u0000requester' }, { ...input, packageProfile: 'accountant_handoff' }]) { const r = await page.request.post(base + prefix + 'authority-export', { headers: { origin: base }, data }); need(!r.ok(), 'Malformed/unconfirmed/mismatched recipient refuses'); }
      mcp(false, ownerFile, true); await auditCount();
      const path = join(work, 'workspace/synthetic', originalPath); writeFileSync(path, Buffer.from('altered synthetic bytes'));
      try { const r = await page.request.post(base + prefix + 'authority-export', { headers: { origin: base }, data: input }); need(!r.ok(), 'Altered registered original refuses handover'); } finally { writeFileSync(path, original); }
      await auditCount();
      const memberId = (await get('/api/me', member)).user.id; await post('/api/workspace/members/company', { userId: memberId, action: 'grant', companySlug: 'synthetic', role: 'reader' }); await download('authority', member);
      await post('/api/workspace/members/company', { userId: memberId, action: 'revoke', companySlug: 'synthetic' }); const refused = await member.request.post(base + prefix + 'authority-export', { headers: { origin: base }, data: input }); need(refused.status() === 403, 'Retained export session revocation enforced'); cli('authority', memberFile, true); mcp(true, memberFile, true); await auditCount(); await unchanged();
      const fullAudit = await get(prefix + 'audit-verify'); need(fullAudit.audit?.available && fullAudit.audit.ok && fullAudit.audit.entries === 1 && fullAudit.audit.components.length === 5 && fullAudit.audit.components.every((c: any) => c.available && c.ok), 'Complete source accounting audit after handover'); completed = true;
      return { qualified: true, ordinaryMfa: true, bookkeepingUnchanged: true, currentRevocationRefused: true, originalTamperRefused: true, confirmationBeforeEffects: true, projections: saved, exportAuditCount: exports, fullAudit, accountingFingerprint: sha(JSON.stringify(stable)), sourceFreeRestoreQualified: false, installedNativeQualified: false };
    },
    async reopen() { need(completed, 'Completed handover setup'); await cookies(page, ownerFile); await unchanged(); const current = await projections(); need(isDeepStrictEqual(current.authority.projection, saved.authority.projection) && isDeepStrictEqual(current.accountant.projection, saved.accountant.projection), 'Handover context and source survive normalMFA restart'); rmSync(ownerFile); rmSync(memberFile); return { qualified: true, ordinaryMfa: true, exactNormalMfaRestartState: true, projections: current, exportAuditCount: exports, accountingFingerprint: sha(JSON.stringify(stable)), sourceFreeRestoreQualified: false, installedNativeQualified: false }; },
  };
}
