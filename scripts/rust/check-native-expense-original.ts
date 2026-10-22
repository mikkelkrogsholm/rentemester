import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, lstatSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Work-Issue #717: bounded real clients; only the parent runs actual acceptance.
 * No financial write retries. Source failures are retained before assertions.
 * Actual coverage is DK standard/EU service; six-treatment/FX proof belongs to domain tests.
 */
export function createOriginalExpenseProof(base: string, work: string, onStep: (name: string) => void) {
  const root = '/api/companies/synthetic/', api = root + 'expense/original/';
  const cookie = join(work, 'expense168.cookie'), key = join(work, 'expense168.key');
  const observations: any[] = [], originals: any[] = [], receipts: any[] = [];
  let owner: Page, service: any, today: string, stable: any, keyed: any, keyedReceipt: any;
  let heldId: number, pending: Promise<any> | undefined;
  const hash = (v: Buffer | string) => createHash('sha256').update(v).digest('hex');
  const need = (v: unknown, label: string) => { if (!v) throw Error('original_expense: ' + label); };
  function record(v: any) { observations.push(v); writeFileSync(join(work, 'expense168-observations.json'), JSON.stringify(observations), { mode: 0o600 }); }
  async function http(path: string, data?: any, method = 'POST', expected = 200, page = owner) {
    onStep(method + ':' + path);
    const r = data === undefined ? await page.request.get(base + path) : await page.request.fetch(base + path, { method, headers: { origin: base }, data });
    const value = await r.json(); record({ transport: 'http', path, status: r.status(), value: path === '/api/workspace/service-principals' ? { ...value, secret: '[redacted]' } : value });
    need(r.status() === expected, path + ' HTTP' + r.status()); return value;
  }
  const admin = (path: string, data: any) => http('/api/workspace/' + path, data, 'POST', path === 'service-principals' ? 201 : 200);
  async function cookies() { writeFileSync(cookie, (await owner.context().cookies(base)).map(c => c.name + '=' + c.value).join('; '), { mode: 0o600 }); }
  function invoke(kind: 'cli' | 'mcp', op: 'vat-preflight' | 'vat-preflight-apply' | 'book', args: any, asService = false, mode: 'ok' | 'refused' | 'invalid' = 'ok') {
    onStep(kind + ':' + op + ':' + (asService ? 'service' : 'human'));
    const common = ['--server', base, '--allow-loopback-http', asService ? '--service-key-file' : '--cookie-file', asService ? key : cookie];
    let argv: string[], input: string | undefined;
    if (kind === 'cli') {
      const flags = { ...args }; delete flags.confirm; delete flags.idempotencyKey;
      argv = ['expense', op === 'vat-preflight-apply' ? 'vat-preflight' : op, ...common, '--company', 'synthetic', '--json',
        ...(op === 'vat-preflight-apply' ? ['--apply', 'yes'] : []),
        ...Object.entries(flags).flatMap(([k, v]) => ['--' + k.replace(/[A-Z]/g, c => '-' + c.toLowerCase()), String(v)])];
    } else {
      argv = common;
      input = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'original-expense168', version: '1' } } },
        { jsonrpc: '2.0', method: 'notifications/initialized' }, { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'expense_' + op.replaceAll('-', '_'), arguments: { company: 'synthetic', ...args } } }].map(v => JSON.stringify(v)).join('\n') + '\n';
    }
    const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), argv, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    const secret = readFileSync(asService ? key : cookie, 'utf8').trim();
    need(!r.stdout.includes(secret) && !r.stderr.includes(secret), 'credential privacy');
    // Preserve the exact sanitized response before checking any result shape.
    record({ kind, op, asService, args, exit: r.status, stdout: r.stdout, stderr: r.stderr, terminalError: r.error?.name });
    need(!r.error && r.status === (kind === 'cli' && mode !== 'ok' ? (mode === 'invalid' ? 2 : 1) : 0), 'terminal status');
    if (kind === 'cli') { if (mode === 'invalid') return; const value = JSON.parse(r.stdout); need(value.ok === (mode === 'ok'), 'CLI flat outcome'); return value; }
    const reply = r.stdout.trim().split('\n').map(s => JSON.parse(s)).find(v => v.id === 2);
    if (mode === 'invalid') { need(reply?.error?.code === -32602, 'RPC invalid arguments'); return; }
    const value = reply?.result?.structuredContent;
    need(!reply?.error && equal(JSON.parse(reply.result.content[0].text), value), 'MCP text/structured parity');
    need(value?.ok === (mode === 'ok') && reply.result.isError === (mode !== 'ok') && Array.isArray(value.errors), 'MCP source envelope');
    if (mode !== 'ok') { need(!Object.hasOwn(value, 'data') && value.errors.length > 0, 'no success data on failure'); return value; }
    return { ok: true, ...value.data, errors: value.errors, ...(value.appliedRules ? { appliedRules: value.appliedRules } : {}) };
  }
  function safeSnapshot(v: any) {
    need(equal(Object.keys(v).sort(), 'ok,documentId,classification,cached,evidenceExpiresAt,wouldCallProvider,errors,derivedRegion,requiredValidation,cache,applyWouldCallProvider,evidence,exception'.split(',').sort()), 'exact safe preflight snapshot');
    need(v.evidence.length <= 20, 'bounded evidence');
    for (const row of v.evidence) need(equal(Object.keys(row).sort(), 'id,event_type,classification,provider_status,evidence_expires_at,created_at,actor'.split(',').sort()), 'safe evidence only');
    if (v.exception) need(equal(Object.keys(v.exception).sort(), 'id,status,severity,message,requiredAction,createdAt'.split(',').sort()), 'safe resumable exception');
    need(!/rawResponse|detail_json|provider_name|provider_address|expense168\.key/.test(JSON.stringify(v)), 'private provider data excluded');
    return v;
  }
  async function inspect(id: number) { return safeSnapshot(await http(api + 'vat-preflight/' + id)); }
  async function books(page = owner) {
    const result: any = {};
    for (const path of ['documents', 'journal', 'trial-balance', 'bank', 'vat']) result[path] = await http(root + path, undefined, 'GET', 200, page);
    return result;
  }
  async function sourceProof(page = owner) {
    for (const original of originals) {
      const r = await page.request.get(base + root + `documents/${original.id}/file`);
      need(r.status() === 200 && hash(await r.body()) === original.sha256, 'immutable original bytes');
    }
  }
  async function ingest(name: string, vatNumber: string, gross = 100, vat = 0) {
    const bytes = Buffer.from('Synthetic original expense168 ' + name + '\n' + vatNumber + '\nGross ' + gross + ' VAT ' + vat + ' DKK\n');
    const metadata = { source: 'Synthetic original expense168', documentType: 'purchase_sale', issueDate: today, invoiceNo: 'SYN-168-' + name, deliveryDescription: 'Synthetic purchased services', amountIncVat: gross, vatAmount: vat, currency: 'DKK', sender: { name: 'Synthetic EU supplier168', address: 'Synthetic Supplier Street 168', vatOrCvr: vatNumber, countryCode: 'DE', identifierKind: 'eu_vat' }, recipient: { name: 'Synthetic Batch Company', address: 'Synthetic Batch Street 1', vatOrCvr: 'DK87654321' } };
    const result = await http(root + 'documents/ingest', { confirm: true, fileName: 'synthetic-expense168-' + name + '.txt', fileBase64: bytes.toString('base64'), metadata });
    need(result.ok && Number.isSafeInteger(result.document.id), 'source-created original');
    const original = { id: result.document.id, sha256: hash(bytes), filename: 'synthetic-expense168-' + name + '.txt', gross, vat };
    originals.push(original); return original;
  }
  async function parity(id: number, expectedOk: boolean) {
    const value = await inspect(id);
    for (const asService of [false, true]) for (const kind of ['cli', 'mcp'] as const) {
      const actual = invoke(kind, 'vat-preflight', { documentId: id }, asService, expectedOk ? 'ok' : 'refused');
      if (kind === 'cli' || expectedOk) need(equal(actual, value), 'exact source inspection parity');
      else need(equal(actual.errors, value.errors), 'source failure errors retained');
    }
    return value;
  }
  async function run(page: Page, supplied: any[], date: string) {
    owner = page; today = date; originals.push(...supplied); await cookies();
    service = await admin('service-principals', { displayName: 'Synthetic expense168', operationId: randomUUID(), confirm: true });
    writeFileSync(key, service.secret + '\n', { mode: 0o600, flag: 'wx' });
    // Never persist the one-time credential in the observation report.
    record({ serviceAccountId: service.serviceAccountId, credentialId: service.credentialId });
    await admin('members/access', { userId: service.serviceAccountId, action: 'set-role', workspaceRole: 'member' });
    await admin('members/company', { userId: service.serviceAccountId, companySlug: 'synthetic', action: 'grant', role: 'bookkeeper' });
    await http(root + 'accounts', { accountNo: '4710', name: 'Synthetic reverse charge VAT168', type: 'vat', normalBalance: 'credit' });
    await http(root + 'account-roles', { role: 'reverse_charge_vat', accountNo: '4710', confirm: true });
    const eu = [await ingest('eu-human', 'DE168000001'), await ingest('eu-service', 'DE987654321')];
    heldId = (await ingest('held', 'DE168000003')).id;
    const csv = `dato;tekst;beløb;valuta;saldo\n${today};SYN-168-eu-human;-100,00;DKK;-475,00\n${today};SYN-168-eu-service;-100,00;DKK;-575,00\n`;
    await http(root + 'bank/import', { confirm: true, csvContent: csv, account: 'synthetic-batch-bank' });
    let before = await books();
    for (const original of originals) await parity(original.id, supplied.some(v => v.id === original.id));
    need(equal(await books(), before) && trace().length === 0, 'inspection no writes/provider');

    // Real extraction request against a retained PDF. Empty provider fields retain
    // unresolved evidence, which preflight must reject before any VIES request.
    const stream = 'BT /F1 12 Tf 72 720 Td (SYN168-CONFLICT) Tj ET\n';
    const objects = ['<< /Type /Catalog /Pages 2 0 R >>', '<< /Type /Pages /Kids [3 0 R] /Count 1 >>', '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>', `<< /Length ${Buffer.byteLength(stream)} >>\nstream\n${stream}endstream`, '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'];
    let pdf = '%PDF-1.4\n'; const offsets: number[] = [];
    objects.forEach((o, i) => { offsets.push(Buffer.byteLength(pdf)); pdf += `${i + 1} 0 obj\n${o}\nendobj\n`; });
    const xref = Buffer.byteLength(pdf);
    pdf += `xref\n0 6\n0000000000 65535 f \n` + offsets.map(o => String(o).padStart(10, '0') + ' 00000 n \n').join('') + `trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
    const pdfBytes = Buffer.from(pdf);
    const conflict = await http(root + 'documents/ingest', { confirm: true, fileName: 'synthetic-expense168-conflict.pdf', fileBase64: pdfBytes.toString('base64'), metadata: { source: 'Synthetic extraction fence168', documentType: 'purchase_sale', issueDate: today, invoiceNo: 'SYN168-CONFLICT', amountIncVat: 100, vatAmount: 0, currency: 'DKK', sender: { name: 'Synthetic unresolved supplier168', address: 'Synthetic Street 168', vatOrCvr: 'DE168000004', countryCode: 'DE', identifierKind: 'eu_vat' } } });
    const conflictId = conflict.document.id;
    originals.push({ id: conflictId, sha256: hash(pdfBytes), gross: 100, vat: 0 });
    const extracted = await http(root + `documents/${conflictId}/extract-invoice`, { confirm: true });
    need(extracted.extraction.status !== 'completed', 'source extraction remains unresolved');
    const conflictSnapshot = await inspect(conflictId);
    need(conflictSnapshot.classification === 'CONFLICT' && !conflictSnapshot.wouldCallProvider, 'latest unresolved extraction wins EU identity');
    invoke('mcp', 'vat-preflight-apply', { documentId: conflictId, confirm: true }, true, 'refused');
    const conflictFailed = await inspect(conflictId);
    need(conflictFailed.exception && conflictFailed.evidence.length === 1 && conflictFailed.evidence[0].event_type === 'preflight_failed' && trace().length === 0, 'conflict retained without provider');

    before = await books();
    for (const asService of [false, true]) {
      for (const confirm of [undefined, false]) { const r = invoke('mcp', 'vat-preflight-apply', { documentId: eu[0].id, ...(confirm === undefined ? {} : { confirm }) }, asService, 'refused'); need(r.code === 'CONFIRM_REQUIRED', 'local confirmation refusal'); }
      invoke('mcp', 'book', { documentId: supplied[0].id, bankTransactionId: 1, expenseAccount: '3000', confirm: true, actor: 'forged' }, asService, 'invalid');
    }
    for (const original of supplied) for (const asService of [false, true]) for (const kind of ['cli', 'mcp'] as const) {
      need(equal(invoke(kind, 'vat-preflight-apply', { documentId: original.id, confirm: true }, asService), await inspect(original.id)), 'DK apply unchanged/no provider');
    }
    need(trace().length === 0, 'DK local no provider');
    for (const [i, original] of eu.entries()) {
      invoke(i === 0 ? 'cli' : 'mcp', 'vat-preflight-apply', { documentId: original.id, confirm: true }, i === 1, 'refused');
      const failed = await inspect(original.id); need(!failed.ok && failed.exception?.status === 'open' && failed.evidence.some((e: any) => e.event_type === 'preflight_failed' && e.provider_status === 'unavailable'), 'durable resumable provider failure');
      writeFileSync(join(work, 'expense168-recover-' + (i === 0 ? '168000001' : '987654321')), 'explicit recovery', { flag: 'wx', mode: 0o600 });
      const recovered = invoke(i === 0 ? 'mcp' : 'cli', 'vat-preflight-apply', { documentId: original.id, confirm: true }, i === 1);
      safeSnapshot(recovered); need(recovered.ok && recovered.cached && recovered.exception?.id === failed.exception.id && recovered.exception.status === failed.exception.status, 'explicit successful recovery preserves source exception lifecycle');
      await parity(original.id, true);
      for (const asService of [false, true]) for (const kind of ['cli', 'mcp'] as const) need(equal(invoke(kind, 'vat-preflight-apply', { documentId: original.id, confirm: true }, asService), recovered), 'cache reused unchanged');
    }
    need(trace().length === 4 && equal(await books(), before), 'exact failure/recovery provider requests; no posting from preflight');
    const bank = (await http(root + 'bank')).bank.transactions;
    for (const [i, original] of [...supplied, ...eu].entries()) {
      const row = bank.find((b: any) => b.text === (i < 2 ? 'SYN-BATCH-' + (i + 1) : 'SYN-168-' + (i === 2 ? 'eu-human' : 'eu-service')));
      need(row && row.amount === -original.gross, 'exact source bank row');
      const input = { documentId: original.id, bankTransactionId: row.id, expenseAccount: '3000', ...(i < 2 ? {} : { vatTreatment: 'reverse_charge' }), confirm: true, ...(i === 1 ? { idempotencyKey: 'synthetic-expense168-key' } : {}) };
      const receipt = invoke(i === 0 || i === 3 ? 'cli' : 'mcp', 'book', input, i === 1 || i === 3);
      need(receipt.grossAmount === original.gross && receipt.netAmount === original.gross - original.vat && receipt.vatAmount === original.vat, 'source expected expense amounts');
      if (i >= 2) need(receipt.appliedRules.includes('DK-VAT-REVERSE-CHARGE-001'), 'source EU rule code');
      receipts.push(receipt);
      if (i === 1) {
        keyed = input; keyedReceipt = receipt;
        const beforeReplay = await books(), replay = invoke('mcp', 'book', input, true);
        need(replay.idempotency.replayed && replay.idempotency.receiptId === receipt.idempotency.receiptId && equal(await books(), beforeReplay), 'optional caller key receipt only replay');
        invoke('mcp', 'book', { ...input, text: 'conflicting changed payload168' }, true, 'refused');
        need(equal(await books(), beforeReplay), 'key conflict has no effects');
      } else need(!receipt.idempotency, 'unkeyed source invocation no generated key');
    }
    stable = await books();
    need(stable.journal.journal.entries.length === 4 && stable.bank.bank.matchedCount === 4 && stable.bank.bank.difference === 0 && stable.bank.bank.bookedBalance === -575, 'exact four atomic journals and bank reconciliation');
    for (const [i, receipt] of receipts.entries()) {
      const entry = stable.journal.journal.entries.find((e: any) => e.id === receipt.journalEntryId), original = [...supplied, ...eu][i];
      const wanted = i < 2 ? [['3000', original.gross - original.vat, 0], ['4700', original.vat, 0], ['1800', 0, original.gross]] : [['3000', 100, 0], ['4700', 25, 0], ['1800', 0, 100], ['4710', 0, 25]];
      need(entry?.documentId === original.id && equal(entry.lines.map((l: any) => [l.accountNo, l.debit, l.credit]).sort(), wanted.sort()), 'exact source VAT/account allocations');
    }
    need(stable.vat.vat.inputVat === 125 && stable.vat.vat.payable === -75 && stable.vat.vat.vatReportErrors.length === 0, 'source expected 75 DK plus 50 reverse charge input/output');
    await sourceProof();
    // A current physical source fence must precede the keyed receipt lookup.
    const statement = join(work, 'workspace/synthetic/bank/processed', hash(Buffer.from(`dato;tekst;beløb;valuta;saldo\n${today};SYN-BATCH-1;-125,00;DKK;-125,00\n${today};SYN-BATCH-2;-250,00;DKK;-375,00\n`)) + '.txt');
    const saved = readFileSync(statement); writeFileSync(statement, 'Synthetic corrupted statement168');
    try { invoke('mcp', 'book', keyed, true, 'refused'); } finally { writeFileSync(statement, saved); }
    const candidates: string[] = [];
    function findOriginal(dir: string) {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = join(dir, entry.name); need(!lstatSync(path).isSymbolicLink(), 'owned regular source namespace');
        if (entry.isDirectory()) findOriginal(path);
        else if (entry.isFile() && hash(readFileSync(path)) === supplied[1].sha256) candidates.push(path);
      }
    }
    findOriginal(join(work, 'workspace/synthetic'));
    need(candidates.length === 1, 'one retained physical keyed original');
    const physical = readFileSync(candidates[0]); writeFileSync(candidates[0], 'Synthetic corrupted original168');
    try { invoke('mcp', 'book', keyed, true, 'refused'); } finally { writeFileSync(candidates[0], physical); }
    await admin('members/company', { userId: service.serviceAccountId, companySlug: 'synthetic', action: 'grant', role: 'reviewer' });
    invoke('mcp', 'book', keyed, true, 'refused');
    invoke('cli', 'vat-preflight-apply', { documentId: eu[0].id, confirm: true }, true, 'refused');
    await admin('members/company', { userId: service.serviceAccountId, companySlug: 'synthetic', action: 'grant', role: 'bookkeeper' });
    need(equal(await books(), stable), 'source/current authority refusals leave books unchanged');
    return { representativeTreatments: ['standard', 'reverse_charge'], sixTreatmentFxRegistrationActualQualified: false, receipts, originals, optionalKeyReplayConflict: true, exactBankAndVat: true };
  }
  function trace(): any[] { const path = join(work, 'expense168-provider-trace.json'); return existsSync(path) ? JSON.parse(readFileSync(path, 'utf8')) : []; }
  async function holdBegin() {
    const before = await inspect(heldId); need(before.evidence.length === 0, 'held original has no prior evidence');
    pending = fetch(base + api + 'vat-preflight/' + heldId, { method: 'POST', headers: { origin: base, 'content-type': 'application/json', 'x-rentemester-service-key': service.secret }, body: JSON.stringify({ confirm: true }), signal: AbortSignal.timeout(45000) }).then(async r => ({ status: r.status, value: await r.json() }));
    void pending.catch(() => {}); return { heldId, requestStarted: true };
  }
  async function revoke() { await admin('service-principals/revoke', { serviceAccountId: service.serviceAccountId, credentialId: service.credentialId, operationId: randomUUID(), confirm: true }); return { credentialRevoked: true }; }
  async function holdFinish() {
    need(pending, 'held request exists'); const result = await pending; record({ heldResult: result });
    need(result.status === 401 || result.status === 403, 'current key revocation wins held valid observation');
    const current = await inspect(heldId); need(current.evidence.length === 1 && current.evidence[0].event_type === 'provider_requested' && !current.cached, 'only legitimate requested evidence; no fabricated result');
    need(equal(await books(), stable), 'held revocation posts nothing');
    return { heldRevocationQualified: true, current, providerTrace: trace() };
  }
  async function assertStable(page: Page) { owner = page; need(equal(await books(page), stable), 'exact books after restart/restore'); await sourceProof(page); }
  async function reopen(page: Page) {
    owner = page; await cookies(); await assertStable(page);
    invoke('mcp', 'book', keyed, true, 'refused');
    return { revokedReceiptRefused: true, noFinancialReplay: true, receiptId: keyedReceipt.idempotency.receiptId };
  }
  return { run, holdBegin, revoke, holdFinish, assertStable, reopen };
}
