import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';
import { createProof } from './check-native-period-original';

/** Only two accepted financial intents. Auth/server/period bootstrap is shared with 160. */
export function createOriginalJournalMcpProof(page: Page, base: string, work: string, member: Page) {
  const envelopes: any[] = [];
  const keys = { post: 'synthetic-original-mcp161-post', reverse: 'synthetic-original-mcp161-reverse', closed: 'synthetic-original-mcp161-closed', duplicate: 'synthetic-original-mcp161-duplicate' };
  let payload: any, reverseArgs: any, posted: any, reversed: any;
  const need = (v: unknown, label: string) => { if (!v) throw Error('Original journal MCP: ' + label); };
  function call(name: string, args: any, file: string, denied = false) {
    const wire = [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-original-journal161', version: '1' } } },
      { jsonrpc: '2.0', method: 'notifications/initialized' },
      { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name, arguments: { company: 'synthetic', ...args } } },
    ].map(v => JSON.stringify(v)).join('\n') + '\n';
    // One invocation; unknown writes are never automatically retried.
    const r = spawnSync(join(work, 'rentemester-mcp'), ['--server', base, '--allow-loopback-http', '--cookie-file', file], { input: wire, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    const secret = readFileSync(file, 'utf8').trim();
    need(!r.error && r.status === 0 && secret && !r.stdout.includes(secret) && !r.stderr.includes(secret), 'stdio/session privacy');
    const rpc = r.stdout.trim().split('\n').map(v => JSON.parse(v)).find(v => v.id === 2);
    const result = rpc?.result, v = result?.structuredContent;
    need(!rpc?.error && result.isError === denied && v?.ok === !denied && Array.isArray(v.errors) && result.content.length === 1 && result.content[0].type === 'text' && equal(JSON.parse(result.content[0].text), v), 'source text/structured/isError envelope');
    need(denied ? v.errors.length > 0 && v.data === undefined : v.errors.length === 0 && v.data && !('ok' in v.data) && !('errors' in v.data) && !('appliedRules' in v.data), 'flat core fields inside source data');
    envelopes.push({ name, arguments: args, result });
    return v;
  }
  function flat(v: any) { return v.ok ? { ok: true, ...v.data, errors: v.errors, appliedRules: v.appliedRules } : v; }
  function receipt(v: any, replayed: boolean) {
    const r = v.idempotency;
    need(r && Object.keys(r).sort().join(',') === 'createdAt,expiresAt,receiptId,replayed' && r.replayed === replayed && Number.isSafeInteger(r.receiptId) && r.receiptId > 0 && typeof r.createdAt === 'string' && Number.isFinite(Date.parse(r.createdAt)) && typeof r.expiresAt === 'string' && Date.parse(r.expiresAt) > Date.parse(r.createdAt), 'exact native receipt metadata');
  }
  function intended(state: any) {
    // Native replay/conflict audit is diagnostic, not a financial mutation.
    const { audit, ...books } = state.books;
    const { audit: revisionAudit, ...sourceRevision } = state.sourceRevision;
    return { books, sourceRevision };
  }
  async function observed(action: () => any, state: () => Promise<any>) {
    const before = await state(), result = action(), after = await state();
    need(equal(intended(before), intended(after)), 'replay/conflict journal/lines/sequence projection unchanged');
    return { result, before, after };
  }
  async function replay(operation: 'post' | 'reverse', file: string, state: () => Promise<any>) {
    const original = operation === 'post' ? posted : reversed;
    const args = operation === 'post' ? { payload, confirm: true, idempotencyKey: keys.post } : reverseArgs;
    const observation = await observed(() => flat(call('journal_' + operation, args, file)), state);
    receipt(observation.result, true);
    need(equal(observation.result, { ...original, idempotency: { ...original.idempotency, replayed: true } }), 'same complete outcome, receipt identity and timestamps');
    return observation;
  }
  const motor = createProof(page, base, work, member, true, {
    journal(operation: string, flags: any, denied: boolean, file: string) {
      let args: any;
      if (operation === 'dry-run' || operation === 'post') {
        const input = JSON.parse(readFileSync(flags.input, 'utf8'));
        args = { payload: input, ...(operation === 'post' ? { confirm: true, idempotencyKey: denied ? keys.closed : keys.post } : {}) };
      } else args = { entryId: flags['entry-id'], entryNo: flags['entry-no'], date: flags.date, reason: flags.reason, confirm: true, idempotencyKey: denied ? keys.duplicate : keys.reverse };
      const v = flat(call(operation === 'dry-run' ? 'journal_dry_run' : 'journal_' + operation, args, file, denied));
      if (!denied) {
        need(Number.isSafeInteger(v.entryId) && v.entryId > 0 && typeof v.entryNo === 'string' && /^[a-f0-9]{64}$/.test(v.entryHash) && Array.isArray(v.appliedRules), 'original numeric identity/rules');
        if (operation !== 'dry-run') receipt(v, false);
        if (operation === 'reverse') { reverseArgs = args; reversed = v; }
      } else need(typeof v.code === 'string' && Array.isArray(v.appliedRules), 'authoritative business refusal code/rules');
      return v;
    },
    async afterPost(input: any, result: any, file: string, state: () => Promise<any>) {
      payload = input; posted = result;
      const exact = await replay('post', file, state);
      const conflict = await observed(() => call('journal_post', { payload: { ...payload, text: payload.text + ' changed' }, confirm: true, idempotencyKey: keys.post }, file, true), state);
      need(/conflict/i.test(conflict.result.code), 'changed intent conflicts');
      return { exact, conflict };
    },
    async afterReverse(_result: any, file: string, state: () => Promise<any>) { return replay('reverse', file, state); },
    async revoked(file: string, state: () => Promise<any>) {
      const before = await state();
      call('journal_post', { payload, confirm: true, idempotencyKey: keys.post }, file, true);
      call('journal_reverse', reverseArgs, file, true);
      need(equal(before, await state()), 'current revoked named tools denied before replay');
    },
    async restart(file: string, state: () => Promise<any>) {
      return { post: await replay('post', file, state), reverse: await replay('reverse', file, state), sameReceiptOutcomes: true };
    },
  });
  return {
    async run() { return { ...await motor.run(), originalJournalMcpQualified: true, explicitKeys: keys, sourceEnvelopes: envelopes }; },
    async reopen() { return { ...await motor.reopen(), originalJournalMcpRestartQualified: true, sourceEnvelopes: envelopes }; },
  };
}
