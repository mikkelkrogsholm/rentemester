import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, readFileSync, renameSync, rmSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

type User = { email: string; password: string; codes: string[] };
/** One existing owner, normal MFA, one source-backed post through a selected local adapter. */
export function createLocalRuntimeProof(page: Page, base: string, work: string, member: Page, users: { owner: User; member: User }, adapter: 'cli' | 'mcp' = 'cli') {
  const candidate = process.env.RM_NATIVE_CLI_CANDIDATE === '1';
  const program = (name: string) => join(work, ...(candidate ? ['candidate', 'bin', name === 'rentemester-rust' ? 'rentemester' : name] : [name]));
  // Only product children receive this environment; the Bun/Playwright harness retains its PATH.
  const childEnv = candidate ? Object.fromEntries(Object.entries(process.env).filter(([key]) => !/BUN|NODE|FALLBACK|LEGACY|RENTEMESTER|^RM_/.test(key))) : process.env;
  if (candidate) childEnv.PATH = join(work, 'empty-path');
  const prefix = '/api/companies/synthetic/', cli = program('rentemester-rust');
  let retainedVendors: any;

  const ownerFile = join(work, 'uds-owner.cookie'), memberFile = join(work, 'uds-member.cookie');
  let proofStep = 'prelude';
  const need = (v: unknown, message: string) => { if (!v) { const error = Error(message); error.name = 'LocalRuntime_' + proofStep + '_' + message.split(':')[0].replace(/[^A-Za-z0-9_]/g, '_').slice(0, 70); throw error; } };
  const hash = (v: Buffer | string) => createHash('sha256').update(v).digest('hex');
  let stable: any, documentId: number, posting: any, completed = false, logins = 0;
  const original = Buffer.from('Synthetic local CLI source 100 DKK\n');
  async function get(path: string, actor = page) { const r = await actor.request.get(base + path); need(r.ok(), 'Local runtime canonical read ' + path); return r.json(); }
  async function post(path: string, data: unknown) { const r = await page.request.post(base + path, { headers: { origin: base }, data }); const v = await r.json(); need(r.ok() && v.ok !== false, 'Local runtime setup ' + path); return v; }
  function invoke(args: string[], input = '', expected = 0) {
    const r = spawnSync(cli, args, { env: childEnv, input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    if (r.error || r.status !== expected) { const error = Error('Local CLI result differs'); error.name = 'LocalRuntime_' + proofStep + '_cli_exit_' + String(r.status ?? 'spawn') + '_expected_' + expected; throw error; }
    for (const user of [users.owner, users.member]) need(!r.stdout.includes(user.password) && !r.stderr.includes(user.password), 'No password in CLI output');
    if (candidate && expected === 2 && !r.stdout) { need(!!r.stderr.trim(), 'Usage stderr only'); return; }
    return JSON.parse(r.stdout);
  }
  function invokeMcp(args: string[], input = '', expected = 0) {
    const r = spawnSync(program('rentemester-mcp'), args, { env: childEnv, input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!r.error && r.status === expected, 'MCP process expected exit ' + expected);
    for (const user of [users.owner, users.member]) need(!r.stdout.includes(user.password) && !r.stderr.includes(user.password), 'No password in MCP output');
    for (const file of [ownerFile, memberFile]) if (existsSync(file)) {
      const secret = readFileSync(file, 'utf8').trim();
      need(secret.length > 0 && !r.stdout.includes(secret) && !r.stderr.includes(secret), 'No session in MCP output');
    }
    return r;
  }
  function local(command: string, file = ownerFile, body?: any, expected = 0) {
    if (adapter === 'cli') return invoke(['remote', command, '--local-profile', work, '--cookie-file', file, '--company', 'synthetic'], body === undefined ? '' : JSON.stringify(body), expected);
    const names: Record<string, string> = { journal: 'journal', 'trial-balance': 'trial_balance', 'post-journal': 'post_journal' };
    need(names[command], 'Existing MCP tool mapping');
    const wire = [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-local-runtime139', version: '1' } } },
      { jsonrpc: '2.0', method: 'notifications/initialized' },
      { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: names[command], arguments: { company: 'synthetic', ...(body === undefined ? {} : { envelope: JSON.stringify(body) }) } } },
    ].map(v => JSON.stringify(v)).join('\n') + '\n';
    const r = invokeMcp(['--local-profile', work, '--cookie-file', file], wire);
    const responses = r.stdout.trim().split('\n').map(v => JSON.parse(v));
    need(responses.find(v => v.id === 1)?.result?.protocolVersion === '2025-11-25', 'MCP initialized negotiated protocol');
    const call = responses.find(v => v.id === 2), value = call?.result?.structuredContent;
    need(!call?.error && value && call.result.isError === (expected !== 0) && value.ok === (expected === 0), 'MCP transport and business result distinction');
    need(isDeepStrictEqual(JSON.parse(call.result.content[0].text), value), 'MCP text and structured content agree');
    if (expected !== 0) need(typeof value.error?.code === 'string' && typeof value.error?.message === 'string', 'MCP refusal retains typed error');
    return value;
  }
  function vendorCli(operation: string, transport: 'remote' | 'local', flags: string[] = [], file = ownerFile, expected = 0, company = 'synthetic') {
    return invoke(['vendor', operation, ...(transport === 'remote' ? ['--server', base, '--allow-loopback-http'] : ['--local-profile', work]), '--cookie-file', file, '--company', company, '--json', ...flags], '', expected);
  }
  function vendorMcp() {
    const wire = [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-candidate181', version: '1' } } },
      { jsonrpc: '2.0', method: 'notifications/initialized' },
      { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'vendor_list', arguments: { company: 'synthetic' } } },
    ].map(v => JSON.stringify(v)).join('\n') + '\n';
    const r = invokeMcp(['--local-profile', work, '--cookie-file', ownerFile], wire);
    const responses = r.stdout.trim().split('\n').map(v => JSON.parse(v));
    need(responses.find(v => v.id === 1)?.result?.protocolVersion === '2025-11-25', 'Vendor MCP initialization');
    const call = responses.find(v => v.id === 2), value = call?.result?.structuredContent;
    need(!call?.error && value?.ok && !call.result.isError && isDeepStrictEqual(JSON.parse(call.result.content[0].text), value), 'Vendor MCP source envelope');
    return value.data;
  }
  async function vendors() {
    const canonical = await get(prefix + 'master-data/original/vendors?archived=false');
    need(canonical.ok && canonical.count === 2 && isDeepStrictEqual(canonical, vendorCli('list', 'remote')) && isDeepStrictEqual(canonical, vendorCli('list', 'local')), 'Vendor source remote/local exact projection');
    need(canonical.rows.every((r: any) => r.name === 'Synthetic Shared Supplier' && r.countryCode === 'DK' && r.identifierKind === 'dk_cvr') && canonical.rows[0].id !== canonical.rows[1].id && isDeepStrictEqual(canonical.rows.map((r: any) => r.vatOrCvr), ['DK12345678', 'DK87654321']), 'Same name distinct legal identities');
    need(isDeepStrictEqual(vendorMcp().rows, canonical.rows), 'Same candidate MCP exact vendor rows');
    if (retainedVendors) need(isDeepStrictEqual(canonical, retainedVendors), 'Exact entire vendor rows retained');
    return canonical;
  }
  function login(user: User, file: string) {
    proofStep = user === users.owner ? 'owner_login' : 'member_login';
    const code = user.codes.shift(); need(code && !existsSync(file), 'Unused normally enrolled MFA code and fresh session path');
    const input = user.password + '\n' + code + '\n';
    const v = invoke(['remote', 'login', '--local-profile', work, '--email', user.email, '--factor', 'backup', '--session-out', file], input);
    need(v.ok && v.data.status === 'authenticated' && (statSync(file).mode & 0o777) === 0o600, 'Actual local password/MFA/private session');
    logins++; return v;
  }
  function descriptor() {
    need(existsSync(join(work, 'r.json')) && existsSync(join(work, 'r.sock')), 'Published private endpoint files present');
    const file = join(work, 'r.json'), socket = join(work, 'r.sock'), d = JSON.parse(readFileSync(file, 'utf8'));
    need(d.format === 'rentemester-local-runtime/1' && d.origin === base && d.socket === 'r.sock' && d.workspaceRoot === join(work, 'workspace') && typeof d.workspaceId === 'string' && d.workspaceId.length > 0, 'Exact private runtime descriptor');
    need((statSync(file).mode & 0o777) === 0o600 && (statSync(socket).mode & 0o777) === 0o600 && statSync(socket).isSocket(), 'Private record and actual socket');
    return { format: d.format, origin: d.origin, workspaceRoot: d.workspaceRoot, workspaceId: d.workspaceId, descriptorSha256: hash(readFileSync(file)) };
  }
  async function books() { return { journal: await get(prefix + 'journal/entries?all=true'), trial: await get(prefix + 'trial-balance'), documents: await get(prefix + 'documents'), bank: await get(prefix + 'bank') }; }
  async function unchanged() { need(isDeepStrictEqual(await books(), stable), 'Same source bookkeeping retained'); }
  async function projections() {
    proofStep = 'projection';
    const runtime = descriptor(), trial = local('trial-balance'), journal = local('journal');
    need(trial.ok && journal.ok && isDeepStrictEqual(trial.data.trialBalance, (await get(prefix + 'trial-balance')).trialBalance) && isDeepStrictEqual(journal.data.journal, (await get(prefix + 'journal')).journal), 'Selected UDS adapter and browser/TCP exact canonical projections');
    const originalRead = await page.request.get(base + prefix + 'documents/' + documentId + '/file'); need(originalRead.ok() && (await originalRead.body()).equals(original), 'Retained exact original');
    const legacy = adapter === 'mcp' ? null : invoke(['journal', 'list', '--local-profile', work, '--cookie-file', ownerFile, '--company', 'synthetic', '--json']);
    need(adapter === 'mcp' || (Array.isArray(legacy) && legacy.length === 1 && legacy[0].status === 'posted'), 'Global local binding covers original dedicated parser');
    await unchanged(); return { runtime, trial: trial.data.trialBalance, journal: journal.data.journal, originalSha256: hash(original), dedicatedJournal: legacy };
  }
  return {
    async run() {
      need(!completed, 'Local runtime fixture once'); const initialDescriptor = descriptor(); proofStep = 'competing_owner';
      const competing = spawnSync(program('rentemester-server'), ['--config', join(work, 'server.json'), '--web-dir', join(work, ...(candidate ? ['candidate', 'web'] : ['web'])), '--listen', '127.0.0.1:0', '--local-runtime-dir', work], { env: childEnv, encoding: 'utf8', timeout: 20000, maxBuffer: 1024 * 1024 });
      need(!competing.error && competing.status !== 0 && (competing.stdout + competing.stderr).includes('workspace_busy') && descriptor().descriptorSha256 === initialDescriptor.descriptorSha256, 'Competing server refused by same workspace lease without replacing endpoint');
      login(users.owner, ownerFile); login(users.member, memberFile);
      if (candidate) {
        proofStep = 'vendor_collision';
        const create = (transport: 'remote' | 'local', cvr: string) => vendorCli('create', transport, ['--name', 'Synthetic Shared Supplier', '--cvr', cvr, '--country', 'DK', '--identifier-kind', 'dk_cvr']);
        const first = create('remote', 'DK12345678');
        need(first.ok && Number.isSafeInteger(first.vendorId), 'First source vendor receipt');
        const firstRow = (await get(prefix + 'master-data/original/vendors')).rows[0];
        const second = create('local', 'DK87654321');
        need(second.ok && Number.isSafeInteger(second.vendorId) && first.vendorId !== second.vendorId, 'Second source vendor receipt distinct');
        retainedVendors = await vendors();
        need(isDeepStrictEqual(retainedVendors.rows[0], firstRow), 'Second creation never overwrites entire first row');
        vendorCli('list', 'local', ['--bogus'], ownerFile, 2);
        need(vendorCli('list', 'remote', [], ownerFile, 1, 'ungranted-synthetic').ok === false, 'Wrong company refused');
        await vendors();
      }
      proofStep = 'source_setup';
      const today = (await get('/api/portfolio')).portfolio.asOf;
      for (const [accountNo, type, normalBalance] of [['1738', 'asset', 'debit'], ['3738', 'income', 'credit']]) await post(prefix + 'accounts', { accountNo, name: 'Synthetic local CLI ' + accountNo, type, normalBalance });
      const uploaded = await post(prefix + 'documents/ingest', { confirm: true, fileName: 'synthetic-local-cli.txt', fileBase64: original.toString('base64'), metadata: { source: 'Synthetic local CLI', documentType: 'cash_register_receipt', issueDate: today } }); documentId = uploaded.document.id;
      posting = { confirm: true, input: { format: 'rentemester-manual-journal-post/1', expectedRuleVersion: (await get(prefix + 'journal')).journal.nativePosting.ruleVersion, idempotencyKey: 'synthetic-local-runtime-' + adapter, request: { transactionDate: today, text: 'Synthetic local CLI posting', currency: 'DKK', documentId: String(documentId), lines: [{ accountNo: '1738', debitAmount: '100.00', creditAmount: '0.00' }, { accountNo: '3738', debitAmount: '0.00', creditAmount: '100.00' }] } } };
      proofStep = 'posting';
      const posted = local('post-journal', ownerFile, posting, 0); need(posted.ok && posted.data.ok, 'One local source-backed posting');
      proofStep = 'posting_replay';
      const replay = local('post-journal', ownerFile, posting, 0); need(replay.data.entryId === posted.data.entryId, 'Explicit replay reconciles same posting');
      stable = await books(); const proof = await projections();
      need(local('journal', memberFile).ok, 'Current invited reader uses local channel');
      proofStep = 'revocation';
      const memberId = (await get('/api/me', member)).user.id; await post('/api/workspace/members/company', { userId: memberId, action: 'revoke', companySlug: 'synthetic' });
      need(local('journal', memberFile, undefined, 1).ok === false, 'Local retained-session revocation');
      need(local('post-journal', memberFile, { ...posting, input: { ...posting.input, idempotencyKey: 'synthetic-local-runtime-' + adapter + '-refused' } }, 1).ok === false, 'Local revoked mutation refuses');
      if (candidate) {
        for (const transport of ['remote', 'local'] as const) need(vendorCli('list', transport, [], memberFile, 1).ok === false, 'Revoked vendor read refused');
        await vendors();
      }
      proofStep = 'negative_binding';
      const binding = (profile: string, expected = 2, extra: string[] = []) => adapter === 'mcp'
        ? invokeMcp(['--local-profile', profile, '--cookie-file', ownerFile, ...extra], '', expected)
        : invoke(['remote', 'health', '--local-profile', profile, ...extra], '', expected);
      binding(work, 2, ['--server', base]);
      const absent = join(work, 'missing-profile'); binding(absent);
      if (adapter === 'mcp') {
        need(invokeMcp(['--help']).stdout.includes('--local-profile'), 'Pure local MCP help without credentials');
        invokeMcp(['--help', '--local-profile', absent], '', 2);
      } else invoke(['remote', '--help', '--local-profile', absent]);
      const record = join(work, 'r.json'); chmodSync(record, 0o644);
      try { binding(work); } finally { chmodSync(record, 0o600); }
      const socket = join(work, 'r.sock'), held = join(work, 'r-held.sock'); renameSync(socket, held);
      try { binding(work); need((await page.request.get(base + '/api/health')).ok(), 'TCP still available while UDS deliberately absent; no fallback'); } finally { renameSync(held, socket); }
      await unchanged(); const fullAudit = await get(prefix + 'audit-verify'); need(fullAudit.audit?.available && fullAudit.audit.ok && fullAudit.audit.entries === 1 && fullAudit.audit.components.length === 5 && fullAudit.audit.components.every((c: any) => c.available && c.ok), 'Complete accounting/source audit'); completed = true;
      return { adapter, ...(candidate ? { vendors: retainedVendors, packagedPrograms: ['rentemester', 'rentemester-mcp', 'rentemester-server'], emptyProductPath: childEnv.PATH, noLegacyFallback: true } : {}), qualified: true, rpcTextStructuredAgreement: adapter === 'mcp', ordinaryMfa: true, normalLocalCliLogins: logins, oneSharedOwner: true, competingOwnerRefused: true, sourceBackedLocalPost: true, replaySameEntry: true, currentRevocationRefused: true, unsafeBindingRefused: true, noTcpFallback: true, adapterExitLeavesServerRunning: true, cliExitLeavesServerRunning: adapter === 'cli', projections: proof, fullAudit, postingReceipt: posted.data, accountingFingerprint: hash(JSON.stringify(stable)), installedNativeQualified: false, sourceFreeRestoreQualified: false };
    },
    async reopen() {
      need(completed, 'Completed local CLI setup'); rmSync(ownerFile); login(users.owner, ownerFile); await unchanged(); const proof = await projections();
      const descriptorFilesPresent = existsSync(join(work, 'r.json')) && existsSync(join(work, 'r.sock')); need(descriptorFilesPresent, 'Normal restart republished local endpoint');
      if (candidate) await vendors();
      rmSync(ownerFile); rmSync(memberFile); return { adapter, ...(candidate ? { vendors: retainedVendors, packagedPrograms: ['rentemester', 'rentemester-mcp', 'rentemester-server'], emptyProductPath: childEnv.PATH, noLegacyFallback: true } : {}), qualified: true, rpcTextStructuredAgreement: adapter === 'mcp', ordinaryMfa: true, normalLocalCliLogins: logins, exactNormalMfaRestartState: true, projections: proof, accountingFingerprint: hash(JSON.stringify(stable)), installedNativeQualified: false, sourceFreeRestoreQualified: false };
    },
  };
}

/** Reuse the same accounting fixture with the existing MCP RPC tools. */
export function createLocalRuntimeMcpProof(page: Page, base: string, work: string, member: Page, users: { owner: User; member: User }) {
  return createLocalRuntimeProof(page, base, work, member, users, 'mcp');
}
