import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Work-Issue #717. Parent-only actual proof; never retry a financial write.
 * Registration is metadata only. The stricter existing unallocated-capitalisation
 * proof is intentionally retained, even though TypeScript only checks the document.
 * Explicit gross costs do not infer VAT deduction or a new legal assessment.
 * Threshold edges/zero slices/physical-integrity/closed-period domain fixtures are
 * separately mandatory; this actual run claims only its explicit cases.
 */
export function createOriginalAssetProof(base: string, work: string, onStep: (name: string) => void) {
  const root = '/api/companies/synthetic/', api = root + 'asset/original/';
  const cookie = join(work, 'asset170.cookie'), key = join(work, 'asset170.key');
  const observations: any[] = [], receipts: any[] = [], expectedEntries: any[] = [];
  let owner: Page, service: any, stable: any, originals: any[], today: string, ran = false;
  const need = (v: unknown, message: string) => { if (!v) throw Error('original_asset: ' + message); };
  const hash = (v: Buffer) => createHash('sha256').update(v).digest('hex');
  function record(value: any) {
    observations.push(value);
    writeFileSync(join(work, 'asset170-observations.json'), JSON.stringify(observations), { mode: 0o600 });
  }
  async function http(path: string, data?: any, expected = 200, page = owner) {
    onStep((data === undefined ? 'GET:' : 'POST:') + path);
    const r = data === undefined ? await page.request.get(base + path) : await page.request.post(base + path, { headers: { origin: base }, data });
    const text = await r.text();
    // The one-time service credential must never enter a saved proof.
    if (path === '/api/workspace/service-principals') {
      const value = JSON.parse(text); record({ path, status: r.status(), value: { ...value, secret: '[redacted]' } });
      need(r.status() === expected, 'service creation HTTP' + r.status()); return value;
    }
    record({ path, status: r.status(), text });
    need(r.status() === expected, path + ' HTTP' + r.status()); return JSON.parse(text);
  }
  const admin = (path: string, data: any) => http('/api/workspace/' + path, data, path === 'service-principals' ? 201 : 200);
  async function cookies() { writeFileSync(cookie, (await owner.context().cookies(base)).map(c => c.name + '=' + c.value).join('; '), { mode: 0o600 }); }
  function invoke(kind: 'cli' | 'mcp', op: 'register' | 'depreciate' | 'write-off' | 'register-report', args: any = {}, asService = false, mode: 'ok' | 'refused' | 'invalid' = 'ok') {
    onStep(kind + ':' + op + ':' + (asService ? 'service' : 'human'));
    const common = ['--server', base, '--allow-loopback-http', asService ? '--service-key-file' : '--cookie-file', asService ? key : cookie];
    let argv: string[], input: string | undefined;
    if (kind === 'cli') {
      const names: Record<string, string> = { thresholdRuleSource: 'threshold-source' };
      argv = ['asset', op, ...common, '--company', 'synthetic', '--json', ...Object.entries(args).filter(([k]) => k !== 'confirm' && k !== 'confirmImmediateWriteOff').flatMap(([k, v]) => ['--' + (names[k] ?? k.replace(/[A-Z]/g, c => '-' + c.toLowerCase())), String(v)]), ...(op === 'write-off' ? ['--confirm', args.confirmImmediateWriteOff === true ? 'yes' : 'no'] : [])];
    } else {
      argv = common;
      input = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'original-asset170', version: '1' } } },
        { jsonrpc: '2.0', method: 'notifications/initialized' }, { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'asset_' + op.replaceAll('-', '_'), arguments: { company: 'synthetic', ...args } } }].map(v => JSON.stringify(v)).join('\n') + '\n';
    }
    const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), argv, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    for (const file of [cookie, key]) { const secret = readFileSync(file, 'utf8').trim(); need(!r.stdout.includes(secret) && !r.stderr.includes(secret), 'credential privacy'); }
    record({ kind, op, args, asService, exit: r.status, stdout: r.stdout, stderr: r.stderr, terminalError: r.error?.name });
    need(!r.error && r.status === (kind === 'cli' && mode !== 'ok' ? (mode === 'invalid' ? 2 : 1) : 0), 'terminal status; never resend unknown write');
    if (kind === 'cli') {
      if (mode === 'invalid') return;
      const value = JSON.parse(r.stdout); need(value.ok === (mode === 'ok'), 'CLI flat outcome'); return value;
    }
    const response = r.stdout.trim().split('\n').map(s => JSON.parse(s)).find(v => v.id === 2);
    if (mode === 'invalid') { need(response?.error?.code === -32602, 'RPC strict arguments'); return; }
    const result = response?.result, value = result?.structuredContent;
    need(!response?.error && result.content.length === 1 && equal(JSON.parse(result.content[0].text), value), 'MCP text/structured equality');
    need(value?.ok === (mode === 'ok') && result.isError === (mode !== 'ok') && Array.isArray(value.errors), 'MCP source envelope');
    if (mode !== 'ok') { need(!Object.hasOwn(value, 'data') && value.errors.length > 0, 'failure has no success data'); return value; }
    return { ok: true, ...value.data, errors: value.errors, ...(value.appliedRules ? { appliedRules: value.appliedRules } : {}) };
  }
  async function sourceProof() {
    for (const original of originals) {
      const r = await owner.request.get(base + root + `documents/${original.id}/file`);
      need(r.ok() && hash(await r.body()) === original.sha256, 'retained immutable original bytes');
    }
  }
  async function snapshot() {
    const state: any = {};
    for (const path of ['documents', 'journal', 'trial-balance', 'bank', 'vat', 'assets', 'exceptions/original?status=all&includeArchived=true']) state[path] = await http(root + path);
    state.report = await http(api + 'register-report');
    return state;
  }
  async function parity() {
    const report = await http(api + 'register-report');
    need(equal(Object.keys(report).sort(), ['ok', 'assets', 'totals', 'errors'].sort()), 'flat source report');
    for (const asService of [false, true]) for (const kind of ['cli', 'mcp'] as const) need(equal(invoke(kind, 'register-report', {}, asService), report), 'exact human/service CLI/MCP/API report');
    return report;
  }
  function exactFields(value: any, fields: string[]) {
    need(equal(Object.keys(value).sort(), fields.sort()), 'exact source DTO fields');
  }
  function receipt(value: any, op: 'depreciate' | 'write-off', amount: number) {
    exactFields(value, ['ok', 'entryId', 'entryNo', 'entryHash', 'appliedRules', 'errors', ...(op === 'depreciate' ? ['assetId', 'periodIndex', 'periodAmount'] : ['writeOffId', 'cost', 'thresholdDkk'])]);
    need(value.ok && Number.isSafeInteger(value.entryId) && value.entryId > 0 && Number.isSafeInteger(value.entryNo) && value.entryNo > 0 && /^[a-f0-9]{64}$/.test(value.entryHash) && equal(value.errors, []), 'shared-writer immutable receipt');
    need(value.appliedRules.includes(op === 'depreciate' ? 'DK-ASSET-DEPR-001' : 'DK-ASSET-WRITEOFF-001') && value.appliedRules.length > 1, 'exact source and shared rules');
    need((op === 'depreciate' ? value.periodAmount : value.cost) === amount, 'exact integer-ore amount');
    receipts.push(value); return value;
  }
  function failedCore(value: any) {
    exactFields(value, ['ok', 'appliedRules', 'errors']);
    need(value.ok === false && equal(value.appliedRules, ['DK-ASSET-WRITEOFF-001']) && value.errors.length > 0 && value.errors.every((e: any) => typeof e === 'string'), 'definite source business refusal, not fabricated success/exception receipt');
  }
  async function run(page: Page, supplied: any[], date: string, reader: Page) {
    need(!ran && supplied.length === 2 && equal(supplied.map(d => d.gross), [125, 250]), 'one bounded run on two ordinary originals'); ran = true;
    owner = page; originals = supplied; today = date; await cookies();
    need(today.slice(0, 4) >= '2026', '2026 threshold fixture requires current date at least 2026');
    service = await admin('service-principals', { displayName: 'Synthetic asset170', operationId: randomUUID(), confirm: true });
    writeFileSync(key, service.secret + '\n', { mode: 0o600, flag: 'wx' });
    await admin('members/access', { userId: service.serviceAccountId, action: 'set-role', workspaceRole: 'member' });
    await admin('members/company', { userId: service.serviceAccountId, companySlug: 'synthetic', action: 'grant', role: 'bookkeeper' });
    for (const [accountNo, type, normalBalance] of [['5800', 'asset', 'debit'], ['5810', 'asset', 'credit'], ['5820', 'expense', 'debit']]) await http(root + 'accounts', { accountNo, name: 'Synthetic asset170 ' + accountNo, type, normalBalance });
    const registration = { name: 'Synthetic capitalised equipment170', category: 'equipment', acquisitionDate: today, cost: 125, usefulLifeMonths: 3, documentId: originals[0].id };
    const writeoff = { name: 'Synthetic small equipment170', category: 'equipment', acquisitionDate: today, cost: 250, documentId: originals[1].id, expenseAccount: '3000', date: today, thresholdRuleSource: 'Synthetic user supplied 2026 workflow guardrail; explicit gross cost, no VAT deduction inferred', confirmImmediateWriteOff: true };
    let before = await snapshot();
    for (const confirm of [undefined, false]) {
      const result = invoke('mcp', 'register', { ...registration, ...(confirm === undefined ? {} : { confirm }) }, false, 'refused');
      need(result.code === 'CONFIRM_REQUIRED', 'outer confirmation required locally');
    }
    for (const extra of [{ principal: 'forged' }, { actor: 'forged' }, { idempotencyKey: 'forbidden' }, { note: null }]) invoke('mcp', 'register', { ...registration, confirm: true, ...extra }, true, 'invalid');
    // Unlike outer confirmation, a boolean false inner acknowledgment is a core refusal.
    const ack = await http(api + 'write-off', { ...writeoff, confirm: true, confirmImmediateWriteOff: false }, 422); failedCore(ack);
    failedCore(invoke('mcp', 'write-off', { ...writeoff, confirm: true, confirmImmediateWriteOff: false }, true, 'refused'));
    await http(api + 'register', { ...registration, confirm: true, expectedActor: 'user:forged' }, 403);
    invoke('cli', 'register', registration, false, 'refused'); // no capitalisation proof yet
    need(equal(await snapshot(), before), 'ack/input/unallocated-capitalisation refusals are inert');
    const posting = { confirm: true, input: { format: 'rentemester-manual-journal-post/1', expectedRuleVersion: before.journal.journal.nativePosting.ruleVersion, idempotencyKey: 'synthetic-asset170-capitalisation', request: { transactionDate: today, text: 'Synthetic asset170 explicit gross capitalisation', currency: 'DKK', documentId: String(originals[0].id), lines: [{ accountNo: '5800', debitAmount: '125.00', creditAmount: '0.00' }, { accountNo: '1800', debitAmount: '0.00', creditAmount: '125.00' }] } } };
    await http(root + 'manual-journal', posting); // one attempt, never replayed
    before = await snapshot();
    need(before.journal.journal.entries.length === 1, 'exactly one capitalisation journal');
    expectedEntries.push({ receipt: { entryId: before.journal.journal.entries[0].id }, documentId: originals[0].id, lines: [['5800', 125, 0], ['1800', 0, 125]] });
    invoke('cli', 'register', { ...registration, depreciationAccount: '5800' }, false, 'refused');
    const registered = invoke('cli', 'register', registration);
    exactFields(registered, ['ok', 'assetId', 'totalPeriods', 'periodAmount', 'appliedRules', 'errors']);
    need(registered.ok && Number.isSafeInteger(registered.assetId) && registered.assetId > 0 && registered.totalPeriods === 3 && registered.periodAmount === 41.66 && equal(registered.appliedRules, ['DK-ASSET-DEPR-001']) && equal(registered.errors, []), 'source registration plan without journal receipt');
    need(equal((await snapshot()).journal, before.journal), 'registration creates NO journal');
    invoke('cli', 'register', registration, false, 'refused');
    for (let period = 1; period <= 3; period++) {
      const amount = period === 3 ? 41.68 : 41.66;
      const r = receipt(invoke('mcp', 'depreciate', { assetId: registered.assetId, period, date: today, confirm: true }, period === 2), 'depreciate', amount);
      need(r.assetId === registered.assetId && r.periodIndex === period, 'explicit selected period');
      expectedEntries.push({ receipt: r, documentId: originals[0].id, lines: [['5820', amount, 0], ['5810', 0, amount]] });
    }
    const written = receipt(invoke('mcp', 'write-off', { ...writeoff, confirm: true }, true), 'write-off', 250);
    need(written.writeOffId > 0 && written.thresholdDkk === 36000, 'source eligible writeoff and threshold');
    expectedEntries.push({ receipt: written, documentId: originals[1].id, lines: [['3000', 250, 0], ['1800', 0, 250]] });
    const report = await parity();
    need(equal(report, { ok: true, assets: [{ assetId: registered.assetId, name: registration.name, category: registration.category, acquisitionDate: today, cost: 125, usefulLifeMonths: 3, postedPeriods: 3, accumulatedDepreciation: 125, netBookValue: 0 }], totals: { cost: 125, accumulatedDepreciation: 125, netBookValue: 0 }, errors: [] }), 'exact register excludes immediate writeoff');
    before = await snapshot();
    const documents = before.documents.documents.documents;
    const absent = Math.max(...documents.map((d: any) => d.id)) + 1000;
    need(Number.isSafeInteger(absent) && !documents.some((d: any) => d.id === absent), 'positive proven absent ID');
    // Source original is retained, but this synthetic threshold gross cost is not
    // prior capitalisation and makes no claim about tax eligibility or VAT.
    const over = { ...writeoff, name: 'Synthetic uncertain equipment170', acquisitionDate: '2026-01-01', cost: 36000.01 };
    const missing = { ...writeoff, name: 'Synthetic missing documentation170', documentId: absent };
    for (const input of [missing, over]) {
      failedCore(await http(api + 'write-off', { ...input, confirm: true }, 422));
      const once = await snapshot();
      failedCore(invoke('mcp', 'write-off', { ...input, confirm: true }, true, 'refused'));
      need(equal(await snapshot(), once), 'durable business refusal deduplicates without financial effects');
    }
    const afterExceptions = await snapshot();
    for (const field of ['journal', 'assets', 'report', 'trial-balance', 'vat', 'bank', 'documents']) need(equal(afterExceptions[field], before[field]), 'exceptions only, no financial mutations');
    const exceptionRows = afterExceptions['exceptions/original?status=all&includeArchived=true'].rows;
    need(exceptionRows.length === 2 && equal(exceptionRows.map((r: any) => r.type).sort(), ['ASSET_WRITEOFF_ELIGIBILITY_UNCERTAIN', 'ASSET_WRITEOFF_MISSING_DOCUMENTATION'].sort()), 'two durable source exceptions only');
    for (const row of exceptionRows) {
      const isMissing = row.type === 'ASSET_WRITEOFF_MISSING_DOCUMENTATION';
      need(row.severity === 'high' && row.status === 'open' && row.relatedDocumentId === (isMissing ? null : originals[1].id), 'source severity/status/document relation');
      need(equal(row.sourceEvidence, isMissing ? { name: missing.name, cost: 250, purchaseDocumentId: absent } : { name: over.name, cost: 36000.01, thresholdDkk: 36000, acquisitionYear: 2026, thresholdRuleSource: over.thresholdRuleSource }), 'exact source evidence');
      need(row.requiredAction === (isMissing ? 'Attach the purchase document, then re-run the immediate write-off.' : 'Advisor must confirm straksafskrivning eligibility or capitalise and depreciate the asset instead.'), 'source manual resolution action');
    }
    stable = afterExceptions;
    const entries = stable.journal.journal.entries;
    need(entries.length === 5 && entries.reduce((n: number, e: any) => n + e.lines.length, 0) === 10, 'exact five journals ten lines');
    for (const expected of expectedEntries) {
      const entry = entries.find((e: any) => e.id === expected.receipt.entryId);
      need(entry?.documentId === expected.documentId && equal(entry.lines.map((l: any) => [l.accountNo, l.debit, l.credit]).sort(), expected.lines.sort()), 'exact source-linked journal accounts and amounts');
    }
    invoke('cli', 'depreciate', { assetId: registered.assetId, period: 1, date: today }, false, 'refused');
    invoke('mcp', 'depreciate', { assetId: registered.assetId, period: 4, date: today, confirm: true }, true, 'refused');
    invoke('cli', 'write-off', writeoff, false, 'refused');
    await http(api + registered.assetId + '/depreciate', { period: 1, date: today, confirm: true }, 403, reader);
    await admin('members/company', { userId: service.serviceAccountId, companySlug: 'synthetic', action: 'grant', role: 'reader' });
    invoke('mcp', 'write-off', { ...missing, name: 'Must not create unauthorized exception', confirm: true }, true, 'refused');
    await admin('service-principals/revoke', { serviceAccountId: service.serviceAccountId, credentialId: service.credentialId, operationId: randomUUID(), confirm: true });
    invoke('cli', 'register-report', {}, true, 'refused');
    invoke('mcp', 'write-off', { ...over, name: 'Must not create revoked exception', confirm: true }, true, 'refused');
    need(equal(await snapshot(), stable), 'duplicate/account/period/current role/key refusals leave canonical state exact');
    await sourceProof();
    return { registered, receipts, report, expectedEntries, originals, journals: 5, lines: 10, exceptions: 2, noFinancialWriteReplay: true, actualLimitations: ['Closed-period, threshold edge years, zero slices and physical-integrity refusals require targeted domain fixtures', 'Synthetic gross costs do not infer VAT deduction or legal eligibility'] };
  }
  async function assertStable(page: Page) {
    owner = page; need(equal(await snapshot(), stable), 'exact source/register/depreciations/writeoffs/exceptions/books after restart/restore/reopen'); await sourceProof(); return { exact: true };
  }
  async function reopen(page: Page) {
    await assertStable(page); await cookies(); invoke('mcp', 'register-report', {}, true, 'refused');
    need(equal(invoke('cli', 'register-report'), stable.report), 'fresh MFA human source report'); return { noFinancialWriteReplay: true, revokedServiceRefused: true };
  }
  return { run, assertStable, reopen };
}
