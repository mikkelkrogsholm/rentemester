import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Finite, synthetic, run-once proof. No SQL writes, accounting effects or retry. */
export function createMasterDataOriginalProof(owner: Page, member: Page, base: string, work: string) {
  const root = '/api/companies/synthetic/', original = root + 'master-data/original';
  const cookie = join(work, 'master-original-owner.cookie'), memberCookie = join(work, 'master-original-member.cookie'), key = join(work, 'master-original-service.key');
  const transcript: any[] = [];
  let service: any, memberId: string, stable: any, ran = false;
  const need = (v: unknown, message: string) => { if (!v) throw Error('Original master data: ' + message); };
  async function saveCookie(page: Page, file: string) { const cookies = await page.context().cookies(base); need(cookies.some(c => c.httpOnly), 'ordinary MFA cookie'); writeFileSync(file, cookies.map(c => c.name + '=' + c.value).join('; '), { mode: 0o600 }); }
  async function get(path: string) { const r = await owner.request.get(base + path); need(r.ok(), 'canonical read ' + path); return r.json(); }
  async function admin(path: string, data: any) { const r = await owner.request.post(base + '/api/workspace/' + path, { headers: { origin: base }, data }); need(r.ok(), 'ordinary owner administration ' + path); return r.json(); }
  const grant = (userId: string, role: string) => admin('members/company', { userId, companySlug: 'synthetic', action: 'grant', role });
  async function state() { return { customers: await get(original + '/customers?archived=true'), vendors: await get(original + '/vendors?archived=true'), audit: await get(root + 'audit-log?limit=5000'), journal: await get(root + 'journal'), bank: await get(root + 'bank'), documents: await get(root + 'documents') }; }
  function execute(binary: string, args: string[], input?: string) { const r = spawnSync(join(work, binary), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 }); need(!r.error, 'bounded binary invocation'); for (const file of [cookie, memberCookie, key]) { const secret = readFileSync(file, 'utf8').trim(); need(secret && !r.stdout.includes(secret) && !r.stderr.includes(secret), 'no credential disclosure'); } return r; }
  function cli(kind: string, operation: string, flags: string[] = [], exit = 0, human = false, file = cookie) {
    const r = execute('rentemester-rust', [kind, operation, '--server', base, '--allow-loopback-http', '--cookie-file', file, '--company', 'synthetic', ...(human ? ['--format', 'human'] : ['--json']), ...flags]);
    need(r.status === exit, `CLI ${kind} ${operation} expected exit ${exit}: ${r.stdout.slice(0, 1000)} ${r.stderr.slice(0, 300)}`);
    if (exit === 2) { need(!r.stdout && !!r.stderr.trim(), 'usage stderr only'); return; }
    if (human) { need(!!r.stdout.trim() && !r.stderr.trim(), 'explicit human report'); transcript.push({ kind, operation, human: r.stdout }); return r.stdout; }
    const v = JSON.parse(r.stdout); need(v.ok === (exit === 0) && Array.isArray(v.errors), 'flat CLI envelope'); transcript.push({ kind, operation, flags, result: v }); return v;
  }
  function rpc(name: string, args: any, mode: 'ok' | 'refused' | 'invalid' = 'ok', offline = false) {
    const input = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-master-original164', version: '1' } } }, { jsonrpc: '2.0', method: 'notifications/initialized' }, { jsonrpc: '2.0', id: 2, method: name === 'tools/list' ? name : 'tools/call', params: name === 'tools/list' ? {} : { name, arguments: args } }].map(v => JSON.stringify(v)).join('\n') + '\n';
    const r = execute('rentemester-mcp', ['--server', offline ? 'http://127.0.0.1:1' : base, '--allow-loopback-http', '--service-key-file', key], input);
    need(r.status === 0, 'stdio process terminal'); const response = r.stdout.trim().split('\n').map(v => JSON.parse(v)).find(v => v.id === 2);
    if (mode === 'invalid') { need(response?.error?.code === -32602, 'schema rejects before HTTP'); return; }
    if (name === 'tools/list') return response.result;
    const result = response?.result, v = result?.structuredContent;
    need(!response?.error && v?.ok === (mode === 'ok') && result.isError === (mode !== 'ok') && result.content.length === 1 && result.content[0].type === 'text' && equal(JSON.parse(result.content[0].text), v), 'MCP text/structured/isError');
    need(Array.isArray(v.errors) && (mode === 'ok' ? v.errors.length === 0 && v.data && !['ok', 'errors', 'appliedRules'].some(k => k in v.data) : v.errors.length > 0 && !('data' in v) && typeof v.code === 'string' && v.code.trim()), 'source MCP envelope');
    transcript.push({ name, args, result, offline }); return v;
  }
  async function unchanged(before: any, label: string) { need(equal(await state(), before), label); }
  async function lists() {
    for (const [kind, plural, fields, heading] of [
      ['customer', 'customers', 'id,name,address,vatOrCvr,email,phone,website,eanNumber,paymentTermsDays,defaultCurrency,notes,archived,createdAt', 'Kunder'],
      ['vendor', 'vendors', 'id,name,address,vatOrCvr,countryCode,identifierKind,identityStatus,email,phone,website,defaultExpenseAccount,defaultVatTreatment,notes,archived,createdAt', 'Leverandører'],
    ]) {
      const full = await get(original + '/' + plural + '?archived=false'); need(equal(cli(kind, 'list'), full), 'unpaged CLI source list'); need(full.count >= 2, 'two pages fixture');
      for (const row of full.rows) need(equal(Object.keys(row).sort(), fields.split(',').sort()) && row.archived === false, 'exact source fields/null preserving');
      const all = await get(original + '/' + plural + '?archived=true'); need(equal(cli(kind, 'list', ['--archived']), all), 'standalone archived means all');
      const a = rpc(kind + '_list', { company: 'synthetic', limit: 1, offset: 0 }).data, b = rpc(kind + '_list', { company: 'synthetic', limit: 5000, offset: 1 }).data;
      need(equal([...a.rows, ...b.rows], full.rows) && a.total === full.count && a.count === 1 && a.limit === 1 && a.offset === 0 && a.hasMore && a.nextOffset === 1 && b.hasMore === false && !('nextOffset' in b), 'source deterministic two pages');
      need(cli(kind, 'list', [], 0, true).includes(heading + ' (' + full.count + ')'), 'source human heading');
    }
  }
  return {
    async run() {
      need(!ran, 'run once'); ran = true; await saveCookie(owner, cookie); await saveCookie(member, memberCookie);
      memberId = (await (await member.request.get(base + '/api/me')).json()).user.id;
      service = await admin('service-principals', { displayName: 'Synthetic original master data', operationId: randomUUID(), confirm: true }); writeFileSync(key, service.secret + '\n', { mode: 0o600, flag: 'wx' });
      await admin('members/access', { userId: service.serviceAccountId, action: 'set-role', workspaceRole: 'member' }); await grant(service.serviceAccountId, 'bookkeeper');
      const before = await state();
      const tools = rpc('tools/list', {}).tools;
      for (const name of ['customer_list', 'vendor_list', 'customer_create', 'vendor_create', 'customer_validate_vat']) {
        const tool = tools.find((t: any) => t.name === name); need(tool?.outputSchema && tool.inputSchema, 'advertised source schemas');
        const read = name.endsWith('_list'), vat = name === 'customer_validate_vat'; need(equal(tool.annotations, { readOnlyHint: read, destructiveHint: false, idempotentHint: read || vat, openWorldHint: vat }), 'source annotations');
      }
      for (const name of ['customer_create', 'vendor_create', 'customer_validate_vat']) {
        const args = { company: 'synthetic', ...(name === 'customer_validate_vat' ? { cvr: 'DK12345678' } : { input: {} }) };
        for (const confirm of [undefined, false]) need(equal(rpc(name, { ...args, ...(confirm === undefined ? {} : { confirm }) }, 'refused', true), { ok: false, code: 'CONFIRM_REQUIRED', errors: ['confirm: true required for write tool ' + name] }), 'exact preHTTP confirm');
        rpc(name, { ...args, confirm: 'true' }, 'invalid', true); rpc(name, { ...args, confirm: true, actor: 'user:forged' }, 'invalid', true);
      }
      for (const args of [{ limit: 0 }, { offset: -1 }, { archived: 'true' }, { note: 'unknown' }]) rpc('customer_list', { company: 'synthetic', ...args }, 'invalid', true);
      rpc('customer_create', { company: 'synthetic', input: { paymentTermsDays: 1.5 }, confirm: true }, 'invalid', true);
      cli('customer', 'validate-vat', [], 2); cli('customer', 'cvr-lookup', [], 2); cli('vendor', 'list', ['--bogus'], 2);
      await unchanged(before, 'local gates have no domain/audit effects');
      const created = cli('customer', 'create', ['--from-cvr', '12345678', '--name', '  zeta Manual Customer164  ', '--email', '  manual164@example.invalid  ', '--currency', ' eur ', '--notes', '  reviewed164  ']);
      need(equal(created.appliedRules, ['DK-MASTER-DATA-CUSTOMER-001']) && Number.isInteger(created.customerId), 'source create receipt');
      cli('customer', 'create', ['--name', 'Alpha Customer164', '--from-cvr', '']);
      cli('vendor', 'create', ['--name', 'Alpha Vendor164', '--country', 'US', '--identifier-kind', 'non_eu']);
      rpc('vendor_create', { company: 'synthetic', input: { name: 'zeta Vendor164', countryCode: 'DK', identifierKind: 'dk_cvr', notes: 'reviewed vendor164' }, fromCvr: '87654321', confirm: true });
      const customer = (await get(original + '/customers')).rows.find((r: any) => r.id === created.customerId);
      need(customer.name === 'zeta Manual Customer164' && customer.email === 'manual164@example.invalid' && customer.paymentTermsDays === null && customer.defaultCurrency === 'EUR' && customer.notes === 'reviewed164' && customer.vatOrCvr === 'DK12345678' && customer.phone === '12345678' && customer.website === 'https://example.invalid' && customer.address === 'Synthetic CVR Road85 10, 2100 Synthetic CVR City85', 'CVR six-field merge, explicit wins, NULL inheritance');
      need(cli('customer', 'cvr-lookup', ['--cvr', '12345678']).company.cvr === '12345678', 'source CVR flat company');
      await lists();
      for (const cvr of ['DK12345678', 'DE123456789', 'DE000000000', 'DE123456789']) {
        const result = cli('customer', 'validate-vat', ['--cvr', cvr]); need(result.validation.normalized === cvr && result.validation.valid === (cvr !== 'DE000000000') && equal(result.appliedRules, ['DK-VAT-REVERSE-CHARGE-001']), 'source current VAT observation including authoritative invalid');
      }
      rpc('customer_validate_vat', { company: 'synthetic', cvr: 'DE123456789', confirm: true });
      for (const cvr of ['DE111111111', 'DE333333333']) cli('customer', 'validate-vat', ['--cvr', cvr], 1);
      const failed = await state(); for (const cvr of ['55555555', '44444444']) cli('customer', 'create', ['--from-cvr', cvr], 1); await unchanged(failed, 'provider failure/mismatch no contact/audit');
      for (const role of ['reader', 'bookkeeper']) {
        await grant(memberId, role); await grant(service.serviceAccountId, role);
        cli('customer', 'list', [], 0, false, memberCookie); rpc('vendor_list', { company: 'synthetic' });
        if (role === 'reader') { const denied = await state(); cli('customer', 'create', ['--from-cvr', '55555555'], 1, false, memberCookie); rpc('vendor_create', { company: 'synthetic', input: {}, fromCvr: '55555555', confirm: true }, 'refused'); cli('customer', 'validate-vat', ['--cvr', 'DE333333333'], 1, false, memberCookie); await unchanged(denied, 'reader denied before provider'); }
        else { cli('customer', 'create', ['--name', 'Bookkeeper Customer164'], 0, false, memberCookie); rpc('vendor_create', { company: 'synthetic', input: { name: 'Bookkeeper Vendor164', countryCode: 'US', identifierKind: 'non_eu' }, confirm: true }); }
      }
      for (const userId of [memberId, service.serviceAccountId]) await admin('members/company', { userId, companySlug: 'synthetic', action: 'revoke' });
      const revoked = await state(); cli('customer', 'list', [], 1, false, memberCookie); rpc('vendor_list', { company: 'synthetic' }, 'refused'); cli('customer', 'create', ['--from-cvr', '55555555'], 1, false, memberCookie); rpc('customer_validate_vat', { company: 'synthetic', cvr: 'DE333333333', confirm: true }, 'refused'); await unchanged(revoked, 'revoked current human/service no effects');
      await grant(service.serviceAccountId, 'reader'); await admin('service-principals/revoke', { serviceAccountId: service.serviceAccountId, credentialId: service.credentialId, operationId: randomUUID(), confirm: true }); rpc('vendor_list', { company: 'synthetic' }, 'refused');
      stable = await state(); need(equal(stable.journal, before.journal) && equal(stable.bank, before.bank) && equal(stable.documents, before.documents), 'no accounting/bank/document effects');
      return { qualified: true, stable, transcript, archivedBoundaryActualQualified: false, delayedProviderRevokeActualQualified: false, installedNativeQualified: false, sourceFreeRestoreQualified: false };
    },
    async reopen() {
      // FIRST domain observation: no grant changes, provider calls or history adapters first.
      await unchanged(stable, 'exact normal MFA restart before mutations/history'); await saveCookie(owner, cookie);
      need(equal(cli('customer', 'list', ['--archived']), stable.customers) && equal(cli('vendor', 'list', ['--archived']), stable.vendors), 'exact restart source projection');
      rmSync(cookie); rmSync(memberCookie); rmSync(key); return { qualified: true, exactNormalMfaRestartState: true };
    },
  };
}
