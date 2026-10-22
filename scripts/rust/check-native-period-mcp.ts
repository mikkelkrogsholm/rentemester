import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';
import { createOriginalPeriodProof } from './check-native-period-original';

/** Finite original named-tool journey; ordinary owner provisions every service grant.
 * The runner's explicitly synthetic operational exception is NOT a creation-UI proof.
 * Unknown writes abort here; there is no retry or financial replay. */
export function createOriginalPeriodMcpProof(page: Page, base: string, work: string, member: Page) {
  const normal = createOriginalPeriodProof(page, base, work, member);
  const keyFile = join(work, 'period-mcp162-service.key');
  const prefix = '/api/companies/synthetic/periods/original';
  const bounds = { company: 'synthetic', from: '2001-01-01', to: '2001-03-31' };
  const reason = 'Synthetic operational exception explicitly retained open for owner follow-up';
  const sourceEnvelopes: any[] = [], reviews: any[] = [];
  let subject = '', secret = '', stable: any, forced: any, normalEvidence: any, ran = false;
  const need = (v: unknown, label: string) => { if (!v) throw Error('Original period MCP: ' + label); };
  const fingerprint = (v: unknown) => createHash('sha256').update(JSON.stringify(v)).digest('hex');
  async function admin(path: string, data: any) {
    const r = await page.request.post(base + '/api/workspace/' + path, { headers: { origin: base }, data });
    need(r.ok(), 'normal owner provisioning ' + path); return r.json();
  }
  const grant = (role: string) => admin('members/company', { userId: subject, companySlug: 'synthetic', action: 'grant', role });
  async function get(path: string) { const r = await page.request.get(base + path); need(r.ok(), 'canonical ' + path); return r.json(); }
  async function state() {
    const historical = await Promise.all(reviews.map(r => get(prefix + '/status?reviewId=' + r.id)));
    return { historical, periods: await get(prefix + '/list'), journal: await get('/api/companies/synthetic/journal/entries?all=true'), trial: await get('/api/companies/synthetic/trial-balance'), audit: await get('/api/companies/synthetic/audit-log?limit=5000'), documents: await get('/api/companies/synthetic/documents') };
  }
  function call(name: string, args: any, denied = false) {
    const input = [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-original-period162', version: '1' } } },
      { jsonrpc: '2.0', method: 'notifications/initialized' },
      { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name, arguments: args } },
    ].map(v => JSON.stringify(v)).join('\n') + '\n';
    const r = spawnSync(join(work, 'rentemester-mcp'), ['--server', base, '--allow-loopback-http', '--service-key-file', keyFile], { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!r.error && r.status === 0 && secret && !r.stdout.includes(secret) && !r.stderr.includes(secret), 'one bounded invocation / secret privacy');
    const rpc = r.stdout.trim().split('\n').map(v => JSON.parse(v)).find(v => v.id === 2);
    const result = rpc?.result, v = result?.structuredContent;
    need(!rpc?.error && v?.ok === !denied && result.isError === denied && result.content.length === 1 && result.content[0].type === 'text' && equal(JSON.parse(result.content[0].text), v), 'exact source text/structuredContent/isError');
    need(Array.isArray(v.errors) && (denied ? v.errors.length > 0 && !('data' in v) && typeof v.code === 'string' && v.code.trim() : v.errors.length === 0 && v.data && !['ok', 'errors', 'appliedRules'].some(k => k in v.data)), 'source envelope fields');
    sourceEnvelopes.push({ name, arguments: args, result }); return v;
  }
  async function refused(name: string, args: any) {
    const before = await state(), v = call(name, args, true);
    need(equal(await state(), before), 'refusal changed canonical books/periods/audit/history'); return v;
  }
  function review() {
    const r = call('period_close_review', { ...bounds, confirm: true }).data.review;
    need(r.packet.version === 4 && r.packet.format === 'rentemester.native-period-close.v1' && r.reviewerPrincipal.kind === 'service-account' && r.reviewerPrincipal.subjectId === subject, 'native4/current service reviewer');
    need(equal(call('period_close_status', { company: 'synthetic', reviewId: r.id }).data, { review: r }), 'exact historical named-tool status');
    reviews.push(r); return r;
  }
  return {
    async run() {
      need(!ran, 'run once'); ran = true;
      // Retain the accepted normal close/locked-posting/reopen/report journey,
      // without reusing or relabelling its human cookie as service evidence.
      normalEvidence = await normal.run();
      const issued = await admin('service-principals', { displayName: 'Synthetic original period executor', operationId: randomUUID(), confirm: true });
      subject = issued.serviceAccountId; secret = issued.secret;
      writeFileSync(keyFile, secret + '\n', { mode: 0o600, flag: 'wx' });
      await admin('members/access', { userId: subject, action: 'set-role', workspaceRole: 'member' });
      await grant('reader');
      const packet = call('period_close_readiness', bounds).data.packet;
      need(packet.version === 4 && packet.sourceRevision && ['journal', 'journalCount', 'audit'].every(k => Array.isArray(packet.sourceRevision[k])), 'real native4 source revision');
      need(packet.items.some((i: any) => i.code === 'EXCEPTIONS_OPEN' && i.status === 'blocked' && i.waivable), 'real scoped operational exception blocker');
      need(packet.items.every((i: any) => !['blocked', 'unavailable'].includes(i.status) || i.waivable && i.status === 'blocked'), 'only explicit waivable blockers');
      const listed = call('period_list', { company: 'synthetic' }).data;
      need(listed.count === listed.periods.length && listed.periods.every((p: any) => Object.keys(p).sort().join(',') === 'createdAt,id,kind,periodEnd,periodStart,reference,status'), 'raw stored list DTO');
      need(equal(call('period_close_status', { company: 'synthetic', reviewId: 2147483647 }).data, { review: null }), 'missing status null');
      const confirmation = await refused('period_close_review', bounds);
      need(equal(confirmation, { ok: false, code: 'CONFIRM_REQUIRED', errors: ['confirm: true required for write tool period_close_review'] }), 'exact source confirmation gate');
      await refused('period_close_review', { ...bounds, confirm: true });
      await grant('bookkeeper'); await refused('period_close_review', { ...bounds, confirm: true });
      await grant('reviewer'); const r = review();
      const close = { ...bounds, packetHash: r.packet.hash, reviewId: r.id, confirm: true };
      await refused('period_close', close);
      await refused('period_close', { ...close, force: true, reason });
      await grant('owner');
      const current = review();
      forced = call('period_close', { ...bounds, packetHash: current.packet.hash, reviewId: current.id, force: true, reason, confirm: true }).data;
      need(Number.isSafeInteger(forced.periodId) && forced.status === 'closed', 'actual owner-service forced close');
      for (const saved of reviews) need(equal(call('period_close_status', { company: 'synthetic', reviewId: saved.id }).data.review, saved), 'immutable historical packet after force');
      await admin('members/company', { userId: subject, companySlug: 'synthetic', action: 'revoke' });
      await refused('period_close_status', { company: 'synthetic', reviewId: current.id });
      await refused('period_list', { company: 'synthetic' });
      await refused('period_close_readiness', bounds);
      await refused('period_close_review', { ...bounds, confirm: true });
      await refused('period_close', { ...close, force: true, reason });
      stable = await state();
      return { qualified: true, originalPeriodMcpQualified: true, ordinaryMfa: true, serviceSubjectId: subject, reason, bounds, forced, reviews, listed, sourceEnvelopes, normalEvidence, accountingFingerprint: fingerprint(stable), operationalExceptionFixtureOnly: true, exceptionCreationUiQualified: false, installedNativeQualified: false, sourceFreeRestoreQualified: false, externalFilingQualified: false };
    },
    async reopen() {
      need(stable, 'completed finite journey');
      // Historical SQL/HTTP state FIRST; no new readiness or review before equality.
      need(equal(await state(), stable), 'exact historical restart before intentional named-tool reads');
      rmSync(join(work, 'period-original-owner.cookie'));
      rmSync(join(work, 'period-original-member.cookie'));
      await grant('reader');
      for (const saved of reviews) need(equal(call('period_close_status', { company: 'synthetic', reviewId: saved.id }).data.review, saved), 'service historical restart');
      await admin('members/company', { userId: subject, companySlug: 'synthetic', action: 'revoke' });
      rmSync(keyFile);
      return { qualified: true, exactNormalMfaRestartState: true, originalPeriodMcpRestartQualified: true, accountingFingerprint: fingerprint(stable), reviews, installedNativeQualified: false, sourceFreeRestoreQualified: false, externalFilingQualified: false };
    },
  };
}
