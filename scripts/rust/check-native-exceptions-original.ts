import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Two explicitly synthetic undated facts only. No accounting/waiver or creation-UI claim. */
export function createOriginalExceptionsProof(page: Page, base: string, work: string, member: Page) {
  const cookieFile = join(work, 'exceptions-original-owner.cookie'), memberFile = join(work, 'exceptions-original-member.cookie'), keyFile = join(work, 'exceptions-original-service.key');
  const path = '/api/companies/synthetic/exceptions/original';
  const note = 'Synthetic human reviewed source evidence; accounting remains separate.';
  const transcript: any[] = [];
  let subject = '', ownerId = '', stable: any, original: any, ran = false;
  const need = (v: unknown, label: string) => { if (!v) throw Error('Original exceptions: ' + label); };
  const fingerprint = (v: unknown) => createHash('sha256').update(JSON.stringify(v)).digest('hex');
  async function cookie(who: Page, file: string) { const cookies = await who.context().cookies(base); need(cookies.some(c => c.httpOnly), 'ordinary MFA session'); writeFileSync(file, cookies.map(c => c.name + '=' + c.value).join('; '), { mode: 0o600 }); }
  async function get(p: string) { const r = await page.request.get(base + p); need(r.ok(), 'canonical ' + p); return r.json(); }
  async function admin(p: string, data: any) { const r = await page.request.post(base + '/api/workspace/' + p, { headers: { origin: base }, data }); need(r.ok(), 'ordinary owner ' + p); return r.json(); }
  const grant = (role: string) => admin('members/company', { userId: subject, companySlug: 'synthetic', action: 'grant', role });
  async function state() { return { exceptions: await get(path + '?status=all&includeArchived=true'), audit: await get('/api/companies/synthetic/audit-log?limit=5000'), journal: await get('/api/companies/synthetic/journal/entries?all=true'), documents: await get('/api/companies/synthetic/documents') }; }
  function execute(binary: string, args: string[], input?: string, secretFile = cookieFile) {
    const r = spawnSync(join(work, binary), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    const secret = readFileSync(secretFile, 'utf8').trim();
    need(!r.error && secret && !r.stdout.includes(secret) && !r.stderr.includes(secret), 'bounded single invocation/privacy'); return r;
  }
  function cli(operation: string, flags: string[] = [], exit = 0, human = false, file = cookieFile) {
    const r = execute('rentemester-rust', ['exceptions', operation, '--server', base, '--allow-loopback-http', '--cookie-file', file, '--company', 'synthetic', ...(human ? [] : ['--json']), ...flags], undefined, file);
    need(r.status === exit, 'CLI exact exit ' + operation + ': ' + r.stdout.slice(0, 1000));
    if (exit === 2) { need(!r.stdout && !!r.stderr.trim(), 'usage stderr only'); return; }
    if (human) { need(!!r.stdout.trim() && !r.stderr.trim(), 'human report'); transcript.push({ operation, human: r.stdout }); return r.stdout; }
    const v = JSON.parse(r.stdout); need(v.ok === (exit === 0) && Array.isArray(v.errors), 'CLI flat envelope'); transcript.push({ operation, flags, result: v }); return v;
  }
  function mcp(name: string, args: any, mode: 'ok' | 'refused' | 'invalid' = 'ok', offline = false) {
    const input = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-original-exceptions163', version: '1' } } }, { jsonrpc: '2.0', method: 'notifications/initialized' }, { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name, arguments: args } }].map(v => JSON.stringify(v)).join('\n') + '\n';
    // An unreachable transport makes a source-local gate distinguishable from HTTP refusal.
    const r = execute('rentemester-mcp', ['--server', offline ? 'http://127.0.0.1:1' : base, '--allow-loopback-http', '--service-key-file', keyFile], input, keyFile);
    need(r.status === 0, 'stdio terminal'); const rpc = r.stdout.trim().split('\n').map(v => JSON.parse(v)).find(v => v.id === 2);
    if (mode === 'invalid') { need(rpc?.error?.code === -32602, 'RPC invalid args before transport'); transcript.push({ name, args, error: rpc.error, unreachableTransport: offline }); return; }
    const result = rpc?.result, v = result?.structuredContent;
    need(!rpc?.error && v?.ok === (mode === 'ok') && result.isError === (mode !== 'ok') && result.content.length === 1 && result.content[0].type === 'text' && equal(JSON.parse(result.content[0].text), v), 'source text/structured/isError');
    need(Array.isArray(v.errors) && (mode === 'ok' ? v.errors.length === 0 && v.data && !['ok', 'errors', 'appliedRules'].some(k => k in v.data) : v.errors.length > 0 && !('data' in v) && typeof v.code === 'string' && !!v.code.trim()), 'source envelope');
    transcript.push({ name, args, result, unreachableTransport: offline }); return v;
  }
  async function unchanged(before: any, label: string) { need(equal(await state(), before), label + ' changed source/books/audit'); }
  async function lists(status: string | undefined, expected: number[]) {
    const flags = status ? ['--status', status] : [], args = { company: 'synthetic', ...(status ? { status } : {}) };
    const native = await get(path + (status ? '?status=' + status : ''));
    need(native.ok && native.count === expected.length && equal(native.rows.map((r: any) => r.id), expected), 'list exact IDs DESC');
    need(equal(cli('list', flags), native), 'CLI source parity'); const v = mcp('exceptions_list', args);
    need(equal(v.data, { count: native.count, rows: native.rows }), 'MCP source parity'); return native;
  }
  return {
    async run() {
      need(!ran, 'run once'); ran = true; await cookie(page, cookieFile); await cookie(member, memberFile);
      ownerId = (await get('/api/me')).user.id;
      const issued = await admin('service-principals', { displayName: 'Synthetic original exception reviewer', operationId: randomUUID(), confirm: true }); subject = issued.serviceAccountId;
      writeFileSync(keyFile, issued.secret + '\n', { mode: 0o600, flag: 'wx' });
      await admin('members/access', { userId: subject, action: 'set-role', workspaceRole: 'member' }); await grant('reader');
      original = await lists(undefined, [2, 1]); await lists('all', [2, 1]); await lists('resolved', []); cli('list', [], 0, true);
      need(equal(cli('list', ['--include-archived']), original), 'standalone archive flag');
      const fields = 'id,type,severity,status,relatedBankTransactionId,relatedDocumentId,message,requiredAction,sourceEvidence,postingPreview,createdAt,resolvedAt,resolvedBy,resolutionNote,archived'.split(',').sort();
      for (const row of original.rows) need(equal(Object.keys(row).sort(), fields) && row.relatedBankTransactionId === null && row.relatedDocumentId === null && row.archived === false && row.resolvedAt === null && row.resolvedBy === null && row.resolutionNote === null, 'exact 15-field undated source DTO');
      const before = await state();
      const invalid = cli('list', ['--status', 'invalid'], 1); need(invalid.count === 0 && equal(invalid.rows, []) && equal(invalid.errors, ['status must be open, resolved, or all when present']), 'bounded status business failure');
      cli('resolve', [], 2); cli('resolve', ['--id', '0'], 2);
      for (const confirm of [undefined, false]) { const v = mcp('exception_resolve', { company: 'synthetic', id: 2, ...(confirm === undefined ? {} : { confirm }) }, 'refused', true); need(equal(v, { ok: false, code: 'CONFIRM_REQUIRED', errors: ['confirm: true required for write tool exception_resolve'] }), 'exact preHTTP confirmation'); }
      for (const extra of [{ note: null }, { note: 123 }, { confirm: 'true' }, { actor: 'user:forged' }]) mcp('exception_resolve', { company: 'synthetic', id: 2, confirm: true, ...extra }, 'invalid', true);
      await unchanged(before, 'source local refusals');
      need(equal(cli('resolve', ['--id', '1', '--note', '  ' + note + '  ']), { ok: true, resolved: true, errors: [] }), 'first CLI trimmed review');
      const first = await state(); need(equal(cli('resolve', ['--id', '1', '--note', 'must not overwrite']), { ok: true, resolved: false, errors: [] }), 'CLI repeat no-op'); await unchanged(first, 'CLI no-op');
      // Human reviewer session independently loses review authority, then regains it.
      const memberId = (await (await member.request.get(base + '/api/me')).json()).user.id;
      for (const role of ['reader', 'bookkeeper']) {
        await admin('members/company', { userId: memberId, companySlug: 'synthetic', action: 'grant', role });
        const snapshot = await state(); cli('resolve', ['--id', '2'], 1, false, memberFile); await unchanged(snapshot, 'human ' + role);
        await grant(role); const current = await state(); mcp('exception_resolve', { company: 'synthetic', id: 2, confirm: true }, 'refused'); await unchanged(current, 'service ' + role);
      }
      await admin('members/company', { userId: memberId, companySlug: 'synthetic', action: 'grant', role: 'reviewer' });
      await grant('reviewer'); need(equal(mcp('exception_resolve', { company: 'synthetic', id: 2, note: '   ', confirm: true }).data, { resolved: true }), 'Reviewer blank note review');
      const reviewed = await state();
      const rows = reviewed.exceptions.rows; need(rows[0].resolvedBy === 'user:' + subject && rows[0].resolutionNote === null && rows[1].resolvedBy === 'user:' + ownerId && rows[1].resolutionNote === note && rows.every((r: any) => !!r.resolvedAt), 'current authenticated actors/time/first notes');
      for (const row of rows) for (const k of fields.filter(k => !['status', 'resolvedAt', 'resolvedBy', 'resolutionNote'].includes(k))) need(equal(row[k], original.rows.find((r: any) => r.id === row.id)[k]), 'preserved source fact ' + k);
      const missing = cli('resolve', ['--id', '2147483647'], 1); need(missing.errors.includes('exception 2147483647 does not exist'), 'missing ID business phrase');
      need(mcp('exception_resolve', { company: 'synthetic', id: 2147483647, confirm: true }, 'refused').errors.includes('exception 2147483647 does not exist'), 'MCP missing ID phrase'); await unchanged(reviewed, 'missing IDs');
      await lists(undefined, []); await lists('all', [2, 1]); await lists('resolved', [2, 1]); cli('list', ['--status', 'resolved'], 0, true);
      await admin('members/company', { userId: subject, companySlug: 'synthetic', action: 'revoke' }); const revoked = await state();
      mcp('exceptions_list', { company: 'synthetic' }, 'refused'); mcp('exception_resolve', { company: 'synthetic', id: 2, confirm: true }, 'refused'); await unchanged(revoked, 'current revoked before no-op');
      stable = await state(); return { qualified: true, originalExceptionsQualified: true, ordinaryMfa: true, ownerId, serviceSubjectId: subject, note, original, stable, transcript, accountingFingerprint: fingerprint(stable), waiverObligationJourneyQualified: false, exceptionCreationUiQualified: false, installedNativeQualified: false, sourceFreeRestoreQualified: false, externalFilingQualified: false };
    },
    async reopen() {
      need(stable, 'completed finite journey');
      // FIRST: exact state before any grant, history read through an adapter or no-op.
      await unchanged(stable, 'exact normal MFA restart'); await cookie(page, cookieFile);
      need(equal(cli('list', ['--status', 'resolved']), stable.exceptions), 'CLI historical restart');
      need(equal(cli('resolve', ['--id', '1', '--note', 'restart must not overwrite']), { ok: true, resolved: false, errors: [] }), 'restart CLI no-op'); await unchanged(stable, 'restart CLI no-op');
      await grant('reviewer'); const admitted = await state(); need(equal(mcp('exceptions_list', { company: 'synthetic', status: 'resolved' }).data, { count: 2, rows: stable.exceptions.rows }), 'service historical restart');
      need(equal(mcp('exception_resolve', { company: 'synthetic', id: 2, note: 'restart must not overwrite', confirm: true }).data, { resolved: false }), 'restart MCP no-op'); await unchanged(admitted, 'restart service no-op');
      await admin('members/company', { userId: subject, companySlug: 'synthetic', action: 'revoke' });
      rmSync(cookieFile); rmSync(memberFile); rmSync(keyFile);
      return { qualified: true, exactNormalMfaRestartState: true, originalExceptionsRestartQualified: true, accountingFingerprint: fingerprint(stable) };
    },
  };
}
