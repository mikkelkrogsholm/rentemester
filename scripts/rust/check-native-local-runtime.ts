import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, readFileSync, renameSync, rmSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

type User = { email: string; password: string; codes: string[] };
/** One existing owner, normal MFA, one source-backed post through the local CLI. */
export function createLocalRuntimeProof(page: Page, base: string, work: string, member: Page, users: { owner: User; member: User }) {
  const prefix = '/api/companies/synthetic/', cli = join(work, 'rentemester-rust');
  const ownerFile = join(work, 'uds-owner.cookie'), memberFile = join(work, 'uds-member.cookie');
  const need = (v: unknown, message: string) => { if (!v) throw Error(message); };
  const hash = (v: Buffer | string) => createHash('sha256').update(v).digest('hex');
  let stable: any, documentId: number, posting: any, completed = false, logins = 0;
  const original = Buffer.from('Synthetic local CLI source 100 DKK\n');
  async function get(path: string, actor = page) { const r = await actor.request.get(base + path); need(r.ok(), 'Local runtime canonical read ' + path); return r.json(); }
  async function post(path: string, data: unknown) { const r = await page.request.post(base + path, { headers: { origin: base }, data }); const v = await r.json(); need(r.ok() && v.ok !== false, 'Local runtime setup ' + path); return v; }
  function invoke(args: string[], input = '', expected = 0) {
    const r = spawnSync(cli, args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!r.error && r.status === expected, 'Local CLI expected exit ' + expected + ': ' + r.stderr + r.stdout);
    for (const user of [users.owner, users.member]) need(!r.stdout.includes(user.password) && !r.stderr.includes(user.password), 'No password in CLI output');
    return JSON.parse(r.stdout);
  }
  function local(command: string, file = ownerFile, body?: any, expected = 0, extra: string[] = []) {
    return invoke(['remote', command, '--local-profile', work, '--cookie-file', file, '--company', 'synthetic', ...extra], body === undefined ? '' : JSON.stringify(body), expected);
  }
  function login(user: User, file: string) {
    const code = user.codes.shift(); need(code && !existsSync(file), 'Unused normally enrolled MFA code and fresh session path');
    const input = user.password + '\n' + code + '\n';
    const v = invoke(['remote', 'login', '--local-profile', work, '--email', user.email, '--factor', 'backup', '--session-out', file], input);
    need(v.ok && v.data.status === 'authenticated' && (statSync(file).mode & 0o777) === 0o600, 'Actual local password/MFA/private session');
    logins++; return v;
  }
  function descriptor() {
    const file = join(work, 'r.json'), socket = join(work, 'r.sock'), d = JSON.parse(readFileSync(file, 'utf8'));
    need(d.format === 'rentemester-local-runtime/1' && d.origin === base && d.socket === 'r.sock' && d.workspaceRoot === join(work, 'workspace') && typeof d.workspaceId === 'string' && d.workspaceId.length > 0, 'Exact private runtime descriptor');
    need((statSync(file).mode & 0o777) === 0o600 && (statSync(socket).mode & 0o777) === 0o600 && statSync(socket).isSocket(), 'Private record and actual socket');
    return { format: d.format, origin: d.origin, workspaceRoot: d.workspaceRoot, workspaceId: d.workspaceId, descriptorSha256: hash(readFileSync(file)) };
  }
  async function books() { return { journal: await get(prefix + 'journal/entries?all=true'), trial: await get(prefix + 'trial-balance'), documents: await get(prefix + 'documents'), bank: await get(prefix + 'bank') }; }
  async function unchanged() { need(isDeepStrictEqual(await books(), stable), 'Same source bookkeeping retained'); }
  async function projections() {
    const runtime = descriptor(), trial = local('trial-balance'), journal = local('journal');
    need(trial.ok && journal.ok && isDeepStrictEqual(trial.data.trialBalance, (await get(prefix + 'trial-balance')).trialBalance) && isDeepStrictEqual(journal.data.journal, (await get(prefix + 'journal')).journal), 'UDS CLI and browser/TCP exact canonical projections');
    const originalRead = await page.request.get(base + prefix + 'documents/' + documentId + '/file'); need(originalRead.ok() && (await originalRead.body()).equals(original), 'Retained exact original');
    const legacy = invoke(['journal', 'list', '--local-profile', work, '--cookie-file', ownerFile, '--company', 'synthetic', '--json']);
    need(Array.isArray(legacy) && legacy.length === 1 && legacy[0].status === 'posted', 'Global local binding covers original dedicated parser');
    await unchanged(); return { runtime, trial: trial.data.trialBalance, journal: journal.data.journal, originalSha256: hash(original), dedicatedJournal: legacy };
  }
  return {
    async run() {
      need(!completed, 'Local runtime fixture once'); const initialDescriptor = descriptor();
      const competing = spawnSync(join(work, 'rentemester-server'), ['--config', join(work, 'server.json'), '--web-dir', join(work, 'web'), '--listen', '127.0.0.1:0', '--local-runtime-dir', work], { encoding: 'utf8', timeout: 20000, maxBuffer: 1024 * 1024 });
      need(!competing.error && competing.status !== 0 && (competing.stdout + competing.stderr).includes('workspace_busy') && descriptor().descriptorSha256 === initialDescriptor.descriptorSha256, 'Competing server refused by same workspace lease without replacing endpoint');
      login(users.owner, ownerFile); login(users.member, memberFile);
      const today = (await get('/api/portfolio')).portfolio.asOf;
      for (const [accountNo, type, normalBalance] of [['1738', 'asset', 'debit'], ['3738', 'income', 'credit']]) await post(prefix + 'accounts', { accountNo, name: 'Synthetic local CLI ' + accountNo, type, normalBalance });
      const uploaded = await post(prefix + 'documents/ingest', { confirm: true, fileName: 'synthetic-local-cli.txt', fileBase64: original.toString('base64'), metadata: { source: 'Synthetic local CLI', documentType: 'cash_register_receipt', issueDate: today } }); documentId = uploaded.document.id;
      posting = { confirm: true, input: { format: 'rentemester-manual-journal-post/1', expectedRuleVersion: (await get(prefix + 'journal')).journal.nativePosting.ruleVersion, idempotencyKey: 'synthetic-local-runtime138', request: { transactionDate: today, text: 'Synthetic local CLI posting', currency: 'DKK', documentId: String(documentId), lines: [{ accountNo: '1738', debitAmount: '100.00', creditAmount: '0.00' }, { accountNo: '3738', debitAmount: '0.00', creditAmount: '100.00' }] } } };
      const posted = local('post-journal', ownerFile, posting, 0, ['--confirm']); need(posted.ok && posted.data.ok, 'One local source-backed posting');
      const replay = local('post-journal', ownerFile, posting, 0, ['--confirm']); need(replay.data.entryId === posted.data.entryId, 'Explicit replay reconciles same posting');
      stable = await books(); const proof = await projections();
      need(local('journal', memberFile).ok, 'Current invited reader uses local channel');
      const memberId = (await get('/api/me', member)).user.id; await post('/api/workspace/members/company', { userId: memberId, action: 'revoke', companySlug: 'synthetic' });
      need(local('journal', memberFile, undefined, 1).ok === false, 'Local retained-session revocation');
      need(local('post-journal', memberFile, { ...posting, input: { ...posting.input, idempotencyKey: 'synthetic-local-runtime138-refused' } }, 1, ['--confirm']).ok === false, 'Local revoked mutation refuses');
      invoke(['remote', 'health', '--local-profile', work, '--server', base], '', 2);
      const absent = join(work, 'missing-profile'); invoke(['remote', 'health', '--local-profile', absent], '', 2);
      invoke(['remote', '--help', '--local-profile', absent]);
      const record = join(work, 'r.json'); chmodSync(record, 0o644);
      try { invoke(['remote', 'health', '--local-profile', work], '', 2); } finally { chmodSync(record, 0o600); }
      const socket = join(work, 'r.sock'), held = join(work, 'r-held.sock'); renameSync(socket, held);
      try { invoke(['remote', 'health', '--local-profile', work], '', 2); need((await page.request.get(base + '/api/health')).ok(), 'TCP still available while UDS deliberately absent; no fallback'); } finally { renameSync(held, socket); }
      await unchanged(); const fullAudit = await get(prefix + 'audit-verify'); need(fullAudit.audit?.available && fullAudit.audit.ok && fullAudit.audit.entries === 1 && fullAudit.audit.components.length === 5 && fullAudit.audit.components.every((c: any) => c.available && c.ok), 'Complete accounting/source audit'); completed = true;
      return { qualified: true, ordinaryMfa: true, normalLocalCliLogins: logins, oneSharedOwner: true, competingOwnerRefused: true, sourceBackedLocalPost: true, replaySameEntry: true, currentRevocationRefused: true, unsafeBindingRefused: true, noTcpFallback: true, cliExitLeavesServerRunning: true, projections: proof, fullAudit, postingReceipt: posted.data, accountingFingerprint: hash(JSON.stringify(stable)), installedNativeQualified: false, sourceFreeRestoreQualified: false };
    },
    async reopen() {
      need(completed, 'Completed local CLI setup'); rmSync(ownerFile); login(users.owner, ownerFile); await unchanged(); const proof = await projections();
      const descriptorFilesPresent = existsSync(join(work, 'r.json')) && existsSync(join(work, 'r.sock')); need(descriptorFilesPresent, 'Normal restart republished local endpoint');
      rmSync(ownerFile); rmSync(memberFile); return { qualified: true, ordinaryMfa: true, normalLocalCliLogins: logins, exactNormalMfaRestartState: true, projections: proof, accountingFingerprint: hash(JSON.stringify(stable)), installedNativeQualified: false, sourceFreeRestoreQualified: false };
    },
  };
}
