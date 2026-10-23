import type { Page } from '@playwright/test';
import { request } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { createLocalRuntimeProof } from './check-native-local-runtime';

/** Full #641 on the existing normal-MFA/local-owner fixture; no authority seeded in SQL. */
export function createKnowledgeScopeProof(owner: Page, base: string, work: string, member: Page, users: Parameters<typeof createLocalRuntimeProof>[4]) {
  const local = createLocalRuntimeProof(owner, base, work, member, users);
  const root = join(work, 'workspace', 'synthetic'), company = 'synthetic', other = 'synthetic-knowledge-other';
  const cookieFile = join(work, 'knowledge-owner.cookie');
  const keyFile = join(work, 'knowledge-a.key'), otherFile = join(work, 'knowledge-b.key');
  const at = '2099-06-01';
  let baseline: unknown, canonical: any, firstId: string, replacementId: string, serviceA: any, serviceB: any;
  const need = (ok: unknown, label: string) => { if (!ok) throw Error('Knowledge705: ' + label); };
  async function admin(path: string, body: unknown) {
    const r = await owner.request.post(base + path, { headers: { origin: base }, data: body });
    need(r.ok(), path + ': HTTP' + r.status()); return r.json();
  }
  async function get(path: string) {
    const r = await owner.request.get(base + path); need(r.ok(), path + ': HTTP' + r.status()); return r.json();
  }
  const knowledge = () => get(`/api/companies/${company}/knowledge?asOf=${at}&includeProposed=true`);
  const books = async () => ({ a: await get(`/api/companies/${company}/journal/entries?all=true`), b: await get(`/api/companies/${other}/journal/entries?all=true`) });
  const grant = (service: any, slug: string, action = 'grant', role = 'bookkeeper') => admin('/api/workspace/members/company', { userId: service.serviceAccountId, companySlug: slug, action, ...(action === 'grant' ? { role } : {}) });
  async function refreshCookie() { writeFileSync(cookieFile, (await owner.context().cookies(base)).map(c => `${c.name}=${c.value}`).join('; '), { mode: 0o600 }); }
  function call(kind: 'cli' | 'mcp', operation: string, selected = company, body?: any, credential = keyFile, success = true) {
    const auth = [credential === cookieFile ? '--cookie-file' : '--service-key-file', credential];
    const common = ['--local-profile', work, ...auth];
    const read = operation === 'company_knowledge';
    const args = kind === 'cli' ? ['remote', operation.replaceAll('_', '-'), ...common, '--company', selected, ...(read ? ['--as-of', at, '--include-proposed', 'true'] : ['--confirm'])] : common;
    const input = kind === 'cli' ? (read ? '' : JSON.stringify(body)) : [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-knowledge705', version: '1' } } },
      { jsonrpc: '2.0', method: 'notifications/initialized' },
      { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: operation, arguments: { company: selected, ...(read ? { asOf: at, includeProposed: true } : { body: JSON.stringify(body), confirm: true }) } } },
    ].map(v => JSON.stringify(v)).join('\n') + '\n';
    const out = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 8 * 1024 * 1024 });
    need(!out.error, `${kind} transport ${operation}`);
    for (const f of [keyFile, otherFile, cookieFile]) { const secret = readFileSync(f, 'utf8').trim(); need(secret && !out.stdout.includes(secret) && !out.stderr.includes(secret), 'No credential disclosure'); }
    if (kind === 'cli') { need(out.status === (success ? 0 : 1), `${kind} ${operation} exit${out.status}: ${out.stdout.slice(0, 600)}`); return success ? JSON.parse(out.stdout).data : null; }
    need(out.status === 0, `MCP process exit${out.status}`);
    const result = out.stdout.trim().split('\n').map(s => JSON.parse(s)).find(v => v.id === 2);
    need(success ? result?.result?.isError === false : result?.result?.isError === true || result?.error, `${kind} ${operation} expected success=${success}: ${JSON.stringify(result).slice(0,600)}`);
    if (!success) return null;
    need(isDeepStrictEqual(JSON.parse(result.result.content[0].text), result.result.structuredContent), 'MCP text/structured agreement');
    return result.result.structuredContent.data;
  }
  async function parity() {
    const expected = await knowledge();
    for (const adapter of ['cli', 'mcp'] as const) for (const selected of [company, root]) {
      need(isDeepStrictEqual(call(adapter, 'company_knowledge', selected), expected), `${adapter} ${selected === company ? 'slug' : 'verified root'} exact canonical JSON`);
    }
    need(expected.history.every((a: any) => a.companySlug === company), 'All assertions use one canonical slug');
    return expected;
  }
  return {
    async run() {
      const inherited = await local.run();
      await refreshCookie();
      await admin('/api/companies', { name: 'Synthetic knowledge other', slug: other });
      for (const [name, file, slug] of [['A', keyFile, company], ['B', otherFile, other]]) {
        const issued = await admin('/api/workspace/service-principals', { displayName: 'Synthetic knowledge service ' + name, operationId: crypto.randomUUID(), confirm: true });
        await admin('/api/workspace/members/access', { userId: issued.serviceAccountId, action: 'set-role', workspaceRole: 'member' });
        await grant(issued, slug); writeFileSync(file, issued.secret + '\n', { mode: 0o600, flag: 'wx' });
        if (name === 'A') serviceA = issued; else serviceB = issued;
      }
      baseline = await books();
      const proposal = { predicate: 'operating_status', value: 'Synthetic canonical service-backed context', source: { kind: 'user', ref: 'synthetic-knowledge705-source-1' }, validFrom: '2000-01-01', certainty: 'confirmed', confirm: true };
      const proposed = call('mcp', 'propose_company_knowledge', root, proposal);
      firstId = proposed.assertion.assertionId;
      need(proposed.assertion.companySlug === company && proposed.assertion.principal.kind === 'service_account' && proposed.assertion.principal.id === serviceA.serviceAccountId && proposed.assertion.actor === `service_account:${serviceA.serviceAccountId}`, 'Canonical typed service attribution');
      const dup = call('cli', 'propose_company_knowledge', company, proposal);
      need(isDeepStrictEqual(dup.assertion, proposed.assertion), 'Same source replay preserves exact proposed history');
      const review = { assertionId: firstId, decision: 'approved', reason: 'Synthetic source reviewed by current human', confirm: true };
      const reviewed = call('cli', 'review_company_knowledge', root, review, cookieFile);
      need(reviewed.assertion.reviewState === 'approved' && reviewed.assertion.history.at(-1).principal_kind === 'user', 'Supported human review retains attribution');
      const reviewedAgain = call('mcp', 'review_company_knowledge', company, review, cookieFile);
      need(isDeepStrictEqual(reviewedAgain.assertion, reviewed.assertion), 'Same review is exact no-op');
      const { confirm: _, ...replacement } = { ...proposal, value: 'Synthetic canonical replacement', source: { kind: 'user', ref: 'synthetic-knowledge705-source-2' } };
      call('mcp', 'supersede_company_knowledge', root, { assertionId: firstId, replacement, confirm: true });
      const state = await knowledge();
      const next = state.history.find((a: any) => a.value === replacement.value); need(next?.reviewState === 'proposed' && state.history.find((a: any) => a.assertionId === firstId)?.reviewState === 'superseded', 'Append-only supersession and separate review');
      replacementId = next.assertionId;
      call('mcp', 'review_company_knowledge', root, { ...review, assertionId: replacementId }, cookieFile);
      canonical = await parity(); need(canonical.history.length === 2, 'Slug/root operations share exactly two intended assertions');
      for (const adapter of ['cli', 'mcp'] as const) {
        call(adapter, 'company_knowledge', company, undefined, otherFile, false);
        call(adapter, 'review_company_knowledge', root, { ...review, assertionId: replacementId }, otherFile, false);
        call(adapter, 'supersede_company_knowledge', company, { assertionId: replacementId, replacement, confirm: true }, otherFile, false);
      }
      const http = await request.newContext({ baseURL: base, extraHTTPHeaders: { origin: base, 'x-rentemester-service-key': serviceA.secret } });
      try {
        const response = await http.get(`/api/companies/${company}/knowledge?asOf=${at}&includeProposed=true`);
        need(response.ok() && isDeepStrictEqual(await response.json(), canonical), 'Service HTTP same full canonical context');
        for (const forged of [{ actor: 'user:forged' }, { principal: { kind: 'user', id: 'forged' } }, { companySlug: other }]) {
          const r = await http.post(`/api/companies/${company}/knowledge/propose`, { data: { ...proposal, ...forged } }); need(r.status() === 400, 'Forged authority selector refused');
        }
        await grant(serviceA, company, 'revoke');
        need((await http.get(`/api/companies/${company}/knowledge`)).status() === 403, 'Current HTTP service revocation');
        for (const adapter of ['cli', 'mcp'] as const) {
          call(adapter, 'company_knowledge', root, undefined, keyFile, false);
          call(adapter, 'propose_company_knowledge', company, proposal, keyFile, false);
        }
        await grant(serviceA, company, 'grant', 'reader');
        need((await http.post(`/api/companies/${company}/knowledge/propose`, { data: proposal })).status() === 403, 'Reader service cannot manage knowledge');
      } finally { await http.dispose(); }
      need(isDeepStrictEqual(await parity(), canonical) && isDeepStrictEqual(await books(), baseline), 'All refusals preserve exact history and both books');
      return { ...inherited, knowledge705: { qualified: true, firstId, replacementId, serviceSubjectId: serviceA.serviceAccountId, canonical, serviceMcpProposal: true, humanReview: true, typedAttribution: true, exactSourceDedup: true, reviewNoop: true, separateReplacementReview: true, cliMcpSlugRootParity: true, httpParity: true, otherCompanyRefused: true, forgedPrincipalRefused: true, currentRevocationRefused: true, readerManageRefused: true, accountingUnchanged: true } };
    },
    async reopen() {
      const inherited = await local.reopen(); await refreshCookie();
      need(isDeepStrictEqual(await parity(), canonical) && isDeepStrictEqual(await books(), baseline), 'Exact normal-MFA restart history, root scope and books');
      return { ...inherited, knowledge705: { qualified: true, exactNormalMfaRestartState: true, canonical, firstId, replacementId, cliMcpSlugRootParity: true, accountingUnchanged: true } };
    },
  };
}
