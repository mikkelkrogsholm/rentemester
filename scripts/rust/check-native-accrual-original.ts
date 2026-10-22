import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Work-Issue #717. Parent-only actual execution, normal intake/MFA and retained originals.
 * Each financial invocation is attempted exactly once. A lost/malformed response stops
 * the proof with its private profile retained; it is never a reason to resend a write.
 * Documentless/zero-slice/closed-period fixtures remain domain-test responsibilities;
 * this actual journey qualifies only the three documented types and explicit-date override.
 */
export function createOriginalAccrualProof(base: string, work: string, onStep: (name: string) => void) {
  const root = '/api/companies/synthetic/', api = root + 'accrual/original/';
  const cookie = join(work, 'accrual169.cookie'), key = join(work, 'accrual169.key');
  const observations: any[] = [], receipts: any[] = [], expectedEntries: any[] = [];
  let owner: Page, service: any, stable: any, originals: any[], today: string, ran = false;
  const need = (v: unknown, message: string) => { if (!v) throw Error('original_accrual: ' + message); };
  const hash = (v: Buffer) => createHash('sha256').update(v).digest('hex');
  function record(value: any) {
    observations.push(value);
    writeFileSync(join(work, 'accrual169-observations.json'), JSON.stringify(observations), { mode: 0o600 });
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
  function invoke(kind: 'cli' | 'mcp', op: 'register' | 'recognize' | 'register-report', args: any = {}, asService = false, mode: 'ok' | 'refused' | 'invalid' = 'ok') {
    onStep(kind + ':' + op + ':' + (asService ? 'service' : 'human'));
    const common = ['--server', base, '--allow-loopback-http', asService ? '--service-key-file' : '--cookie-file', asService ? key : cookie];
    let argv: string[], input: string | undefined;
    if (kind === 'cli') {
      const names: Record<string, string> = { accrualType: 'type', totalAmount: 'amount', recognitionPeriods: 'periods', firstRecognitionDate: 'first-date', resultAccountNo: 'result-account', balanceAccountNo: 'balance-account', settlementAccountNo: 'settlement-account' };
      argv = ['accrual', op, ...common, '--company', 'synthetic', '--json', ...Object.entries(args).filter(([k]) => k !== 'confirm').flatMap(([k, v]) => ['--' + (names[k] ?? k.replace(/[A-Z]/g, c => '-' + c.toLowerCase())), String(v)])];
    } else {
      argv = common;
      input = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'original-accrual169', version: '1' } } },
        { jsonrpc: '2.0', method: 'notifications/initialized' }, { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'accrual_' + op.replaceAll('-', '_'), arguments: { company: 'synthetic', ...args } } }].map(v => JSON.stringify(v)).join('\n') + '\n';
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
    for (const path of ['documents', 'journal', 'trial-balance', 'bank', 'vat', 'accruals']) state[path] = await http(root + path);
    state.report = await http(api + 'register-report');
    state.schedules = [];
    for (const row of state.report.accruals) state.schedules.push(await http(root + `accruals/${row.accrualId}/schedule`));
    return state;
  }
  async function parity() {
    const report = await http(api + 'register-report');
    need(equal(Object.keys(report).sort(), ['ok', 'accruals', 'totals', 'errors'].sort()), 'flat source report');
    for (const asService of [false, true]) for (const kind of ['cli', 'mcp'] as const) need(equal(invoke(kind, 'register-report', {}, asService), report), 'exact human/service CLI/MCP/API report');
    return report;
  }
  function receipt(value: any, recognized: boolean, amount: number) {
    const fields = ['ok', 'entryId', 'entryNo', 'entryHash', 'appliedRules', 'errors', 'accrualId', ...(recognized ? ['periodIndex', 'periodAmount', 'recognizedPeriods', 'totalPeriods', 'fullyRecognized'] : ['accrualType', 'totalPeriods', 'periodAmount'])];
    need(equal(Object.keys(value).sort(), fields.sort()), 'exact flat source receipt, no native schedule/key');
    need(value.ok && Number.isSafeInteger(value.entryId) && value.entryId > 0 && /^[a-f0-9]{64}$/.test(value.entryHash) && value.errors.length === 0 && value.appliedRules.includes('DK-BOOKKEEPING-ACCRUAL-001') && value.appliedRules.length > 1 && value.periodAmount === amount, 'immutable shared-writer receipt/rules/amount');
    receipts.push(value); return value;
  }
  async function run(page: Page, supplied: any[], date: string, reader: Page) {
    need(!ran && supplied.length === 2, 'one bounded run on two ordinary intake originals'); ran = true;
    owner = page; originals = supplied; today = date; await cookies();
    service = await admin('service-principals', { displayName: 'Synthetic accrual169', operationId: randomUUID(), confirm: true });
    writeFileSync(key, service.secret + '\n', { mode: 0o600, flag: 'wx' });
    await admin('members/access', { userId: service.serviceAccountId, action: 'set-role', workspaceRole: 'member' });
    await admin('members/company', { userId: service.serviceAccountId, companySlug: 'synthetic', action: 'grant', role: 'bookkeeper' });
    // Product defaults are exercised only after explicit source API account creation.
    for (const [accountNo, type, normalBalance] of [['1300', 'asset', 'debit'], ['7300', 'liability', 'credit'], ['7310', 'liability', 'credit'], ['2000', 'income', 'credit']]) await http(root + 'accounts', { accountNo, name: 'Synthetic accrual169 ' + accountNo, type, normalBalance });
    const inputs = [
      { accrualType: 'prepaid_expense', description: 'Synthetic prepaid169', totalAmount: 125.09, recognitionPeriods: 3, firstRecognitionDate: today, resultAccountNo: '3000', documentId: supplied[0].id },
      { accrualType: 'accrued_expense', description: 'Synthetic accrued169', totalAmount: 100, recognitionPeriods: 2, firstRecognitionDate: today, resultAccountNo: '3000', documentId: supplied[1].id },
      { accrualType: 'deferred_revenue', description: 'Synthetic deferred169', totalAmount: 90, recognitionPeriods: 2, firstRecognitionDate: today, resultAccountNo: '2000', documentId: supplied[1].id },
    ];
    const baseline = await snapshot();
    for (const confirm of [undefined, false]) { const refusal = invoke('mcp', 'register', { ...inputs[0], ...(confirm === undefined ? {} : { confirm }) }, false, 'refused'); need(refusal.code === 'CONFIRM_REQUIRED', 'local confirmation'); }
    for (const extra of [{ actor: 'forged' }, { principal: 'forged' }, { idempotencyKey: 'forbidden' }, { note: null }]) invoke('mcp', 'register', { ...inputs[0], confirm: true, ...extra }, true, 'invalid');
    invoke('cli', 'register', { ...inputs[0], balanceAccountNo: '3000' }, false, 'refused');
    await http(api + 'register', { ...inputs[0], confirm: true, expectedActor: 'user:forged' }, 403);
    await http(api + 'register', { ...inputs[0], confirm: true }, 403, reader);
    need(equal(await snapshot(), baseline), 'all initial refusals financially inert');
    const created: any[] = [];
    for (const [i, input] of inputs.entries()) {
      const r = receipt(invoke(i === 1 ? 'mcp' : 'cli', 'register', { ...input, confirm: true }, i === 1), false, i === 0 ? 41.69 : i === 1 ? 50 : 45); created.push(r);
      expectedEntries.push({ receipt: r, documentId: input.documentId, lines: i === 0 ? [['1300', 125.09, 0], ['1800', 0, 125.09]] : i === 1 ? [['3000', 100, 0], ['7300', 0, 100]] : [['1800', 90, 0], ['7310', 0, 90]] });
    }
    let report = await parity(); need(equal(report.totals, { totalAmount: 315.09, recognizedAmount: 0, remainingAmount: 315.09 }), 'registered total exposure');
    for (const [i, item] of created.entries()) {
      for (let period = 1; period <= (i === 0 ? 3 : 1); period++) {
        const amount = i === 0 ? (period === 3 ? 41.71 : 41.69) : i === 1 ? 50 : 45;
        // Later scheduled months explicitly recognize on current Copenhagen today.
        const r = receipt(invoke((period + i) % 2 ? 'mcp' : 'cli', 'recognize', { accrualId: item.accrualId, period, date: today, confirm: true }, (period + i) % 2 === 0), true, amount);
        need(r.periodIndex === period && r.recognizedPeriods === period && r.fullyRecognized === (i === 0 && period === 3), 'selected period progression');
        expectedEntries.push({ receipt: r, documentId: inputs[i].documentId, lines: i === 0 ? [['3000', amount, 0], ['1300', 0, amount]] : i === 1 ? [['7300', amount, 0], ['1800', 0, amount]] : [['7310', amount, 0], ['2000', 0, amount]] });
      }
    }
    report = await parity(); need(equal(report.totals, { totalAmount: 315.09, recognizedAmount: 220.09, remainingAmount: 95 }), 'exact recognized/remaining exposure');
    const expectedRows = inputs.map((input, i) => ({ accrualId: created[i].accrualId, accrualType: input.accrualType, description: input.description, totalAmount: input.totalAmount, recognitionPeriods: input.recognitionPeriods, recognizedPeriods: i === 0 ? 3 : 1, recognizedAmount: i === 0 ? 125.09 : i === 1 ? 50 : 45, remainingAmount: i === 0 ? 0 : i === 1 ? 50 : 45, fullyRecognized: i === 0, balanceAccountNo: ['1300', '7300', '7310'][i], resultAccountNo: input.resultAccountNo, firstRecognitionDate: today, periodStepMonths: 1 }));
    need(equal(report.accruals, expectedRows), 'exact source report rows/order/defaults');
    stable = await snapshot();
    const entries = stable.journal.journal.entries;
    need(entries.length === 8 && entries.reduce((n: number, e: any) => n + e.lines.length, 0) === 16, 'exact eight journals sixteen lines');
    for (const expected of expectedEntries) {
      const entry = entries.find((e: any) => e.id === expected.receipt.entryId);
      need(entry?.documentId === expected.documentId && equal(entry.lines.map((l: any) => [l.accountNo, l.debit, l.credit]).sort(), expected.lines.sort()), 'exact source/account/amount journal');
    }
    invoke('mcp', 'recognize', { accrualId: created[0].accrualId, period: 1, date: today, confirm: true }, true, 'refused');
    invoke('cli', 'recognize', { accrualId: created[1].accrualId, period: 3, date: today }, false, 'refused');
    invoke('mcp', 'recognize', { accrualId: created[1].accrualId, period: 2, date: today, settlementAccountNo: '3000', confirm: true }, true, 'refused');
    await http(api + created[1].accrualId + '/recognize', { period: 2, date: today, confirm: true }, 403, reader);
    await admin('members/company', { userId: service.serviceAccountId, companySlug: 'synthetic', action: 'grant', role: 'reviewer' });
    invoke('mcp', 'recognize', { accrualId: created[1].accrualId, period: 2, date: today, confirm: true }, true, 'refused');
    await admin('service-principals/revoke', { serviceAccountId: service.serviceAccountId, credentialId: service.credentialId, operationId: randomUUID(), confirm: true });
    invoke('cli', 'register-report', {}, true, 'refused');
    invoke('mcp', 'recognize', { accrualId: created[1].accrualId, period: 2, date: today, confirm: true }, true, 'refused');
    need(equal(await snapshot(), stable), 'duplicate/period/account/current authority refusals have no financial effects'); await sourceProof();
    return { receipts, report, expectedEntries, originals, journals: 8, lines: 16, noFinancialWriteReplay: true, explicitEarlierThanScheduleDateQualified: true, actualLimitations: ['documented originals only', 'zero-slice, documentless and closed-period refusal fixtures require targeted domain tests'] };
  }
  async function assertStable(page: Page) { owner = page; need(equal(await snapshot(), stable), 'exact report/header/schedule/books after restart/restore/reopen'); await sourceProof(); return { exact: true }; }
  async function reopen(page: Page) { await assertStable(page); await cookies(); invoke('mcp', 'register-report', {}, true, 'refused'); need(equal(invoke('cli', 'register-report'), stable.report), 'fresh MFA human source report'); return { noFinancialWriteReplay: true, revokedServiceRefused: true }; }
  return { run, assertStable, reopen };
}
