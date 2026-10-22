import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Work-Issue #717. Ordinary runner credentials only; no SQL writes or bookkeeping. */
export function createDocumentPartyOriginalProof(base: string, work: string, onStep: (name: string) => void) {
  const root = '/api/companies/synthetic/', api = root + 'documents/original/party-links';
  const cookie = join(work, 'document-party-original.cookie'), key = join(work, 'document-party-original.key');
  const transcript: any[] = [];
  let owner: Page, service: any, stable: any, linkedDocumentId: number, ran = false;
  const need = (v: unknown, label: string) => { if (!v) throw Error('document_party_original: ' + label); };
  const hash = (v: Buffer | string) => createHash('sha256').update(v).digest('hex');
  async function http(path: string, data?: any, method = 'POST') {
    onStep(method + ':' + path);
    const r = data === undefined ? await owner.request.get(base + path) : await owner.request.fetch(base + path, { method, headers: { origin: base }, data });
    const value = await r.json(); need(r.ok() && value.ok !== false, method + ':' + path + ':HTTP' + r.status()); return value;
  }
  const admin = (path: string, data: any) => http('/api/workspace/' + path, data);
  function execute(binary: string, args: string[], input?: string) {
    const r = spawnSync(join(work, binary), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!r.error && !r.stdout.includes(service.secret) && !r.stderr.includes(service.secret), 'bounded private transport'); return r;
  }
  function cli(op: string, input: any = {}, exit = 0) {
    onStep('cli:' + op);
    const flags = Object.entries(input).flatMap(([k, v]) => ['--' + k.replace(/[A-Z]/g, c => '-' + c.toLowerCase()), String(v)]);
    const r = execute('rentemester-rust', ['documents', op, '--company', 'synthetic', '--server', base, '--allow-loopback-http', '--cookie-file', cookie, '--json', ...flags]);
    need(r.status === exit, 'CLI ' + op + ' exit ' + r.status + ' expected ' + exit);
    if (exit === 2) { need(!r.stdout && !!r.stderr.trim(), 'usage stderr only'); return; }
    const value = JSON.parse(r.stdout); transcript.push({ client: 'cli', op, exit, value }); return value;
  }
  function rpc(op: string, args: any, mode = 'ok') {
    onStep('mcp:' + op);
    const input = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-document-party-original166', version: '1' } } }, { jsonrpc: '2.0', method: 'notifications/initialized' }, { jsonrpc: '2.0', id: 2, method: op === 'tools/list' ? op : 'tools/call', params: op === 'tools/list' ? {} : { name: 'documents_' + op.replaceAll('-', '_'), arguments: { company: 'synthetic', ...args } } }].map(v => JSON.stringify(v)).join('\n') + '\n';
    const r = execute('rentemester-mcp', ['--server', base, '--allow-loopback-http', '--service-key-file', key], input);
    need(r.status === 0, 'stdio terminal'); const response = r.stdout.trim().split('\n').map(s => JSON.parse(s)).find(v => v.id === 2);
    if (mode === 'invalid') { need(response?.error?.code === -32602, 'RPC invalid args'); return; }
    if (op === 'tools/list') return response.result;
    const result = response?.result, value = result?.structuredContent;
    need(value?.ok === (mode === 'ok') && result.isError === (mode !== 'ok') && result.content.length === 1 && result.content[0].type === 'text' && equal(JSON.parse(result.content[0].text), value), 'MCP envelope/text parity');
    need(mode === 'ok' ? value.data && value.errors.length === 0 : value.errors.length > 0 && !('data' in value), 'failure has no success data');
    transcript.push({ client: 'mcp', op, mode, value }); return value;
  }
  async function snapshot() {
    const docs = await http(root + 'documents');
    const bytes: any[] = [];
    for (const doc of docs.documents.documents) {
      const r = await owner.request.get(base + root + `documents/${doc.id}/file`);
      need(r.ok(), 'authorized original'); bytes.push({ id: doc.id, sha256: hash(await r.body()) });
    }
    return { docs, bytes, journal: await http(root + 'journal'), trial: await http(root + 'trial-balance'), vat: await http(root + 'vat'), list: await http(api) };
  }
  const inspect = (documentId: number) => http(api + '/' + documentId);
  const state = async (id: number, expected: string) => need((await http(api)).links.find((r: any) => r.id === id)?.resolution_state === expected, expected + ' current state');
  async function parity(id: number) {
    const links = await inspect(id), list = await http(api);
    need(equal(cli('party-link-inspect', { documentId: id }), links) && equal(rpc('party-link-inspect', { documentId: id }).data, { links: links.links }), 'inspect CLI/MCP parity');
    need(equal(cli('party-link-list'), list) && equal(rpc('party-link-list', {}).data, { links: list.links }), 'list CLI/MCP parity');
    const columns = 'id,party_id,party_role,event_type,evidence_kind,evidence_json,document_sha256,document_payload_sha256,party_snapshot_json,plan_hash,reason,actor,principal,created_at'.split(',').sort();
    for (const row of links.links) need(equal(Object.keys(row).sort(), columns) && typeof row.evidence_json === 'string' && typeof row.party_snapshot_json === 'string', 'exact history schema');
    for (const row of list.links) need(equal(Object.keys(row).sort(), 'id,document_no,sha256_hash,linked,resolution_state'.split(',').sort()) && [0, 1].includes(row.linked), 'exact list schema');
  }
  return {
    async run(page: Page, originals: any[], today: string) {
      need(!ran && originals.length === 2, 'one run on real batch originals'); ran = true; owner = page;
      writeFileSync(cookie, (await owner.context().cookies(base)).map(c => c.name + '=' + c.value).join('; '), { mode: 0o600, flag: 'wx' });
      service = await admin('service-principals', { displayName: 'Synthetic document-party166', operationId: randomUUID(), confirm: true });
      writeFileSync(key, service.secret + '\n', { mode: 0o600, flag: 'wx' });
      await admin('members/access', { userId: service.serviceAccountId, action: 'set-role', workspaceRole: 'member' });
      await admin('members/company', { userId: service.serviceAccountId, companySlug: 'synthetic', action: 'grant', role: 'bookkeeper' });
      const party = await http(root + 'workspace-parties', { kind: 'organization', name: 'Synthetic supplier', identifiers: [{ country: 'DK', identifierKind: 'dk_cvr', identifier: '12345678' }], source: 'Synthetic retained originals', observedAt: today, reviewAssertion: 'Owner reviewed exact source identifier', role: 'vendor', confirm: true });
      const partyId = party.party.partyId, documentId = originals[0].id; linkedDocumentId = documentId;
      const input = { documentId, partyId, role: 'vendor', jurisdiction: 'DK', identifierKind: 'dk_cvr', identifier: '12345678' };
      const tools = rpc('tools/list', {}).tools;
      const ops = ['party-link-plan', 'party-link-apply', 'party-link-inspect', 'party-link-list', 'party-link-supersede', 'internal-no-external-party', 'internal-no-external-party-supersede'];
      for (const op of ops) {
        const t = tools.find((t: any) => t.name === 'documents_' + op.replaceAll('-', '_'));
        need(t?.outputSchema && equal(t.annotations, { readOnlyHint: ['party-link-plan', 'party-link-inspect', 'party-link-list'].includes(op), destructiveHint: false, idempotentHint: true, openWorldHint: false }), 'schemas/annotations ' + op);
      }
      const plan = cli('party-link-plan', input).plan;
      need(plan.documentSha256 === originals[0].sha256 && plan.partyId === partyId && /^[a-f0-9]{64}$/.test(plan.planHash), 'source-bound plan');
      need(equal(rpc('party-link-plan', input).data, { ok: true, plan }), 'plan inner ok retained');
      const apply = { ...input, planHash: plan.planHash, idempotencyKey: 'synthetic-party166-first', confirm: true };
      rpc('party-link-apply', { ...apply, confirm: false }, 'refused');
      rpc('party-link-apply', { ...apply, actor: 'user:forged' }, 'invalid');
      rpc('party-link-apply', { ...apply, planHash: '0'.repeat(64) }, 'refused');
      rpc('party-link-plan', { ...input, identifier: '99999999' }, 'refused');
      const before = await snapshot();
      cli('party-link-apply', { ...apply, confirm: 'yes', actor: 'user:forged' }, 1);
      need(equal(await snapshot(), before), 'assertion refusal has no effect');
      const receipt = rpc('party-link-apply', apply).data; need(receipt.id > 0 && receipt.idempotent === false, 'service apply');
      need(rpc('party-link-apply', apply).data.idempotent === true, 'current replay'); await parity(documentId);
      cli('party-link-supersede', { documentId, role: 'vendor', planHash: plan.planHash, reason: 'Explicit new review required', confirm: 'yes' });
      await state(documentId, 'unresolved'); rpc('party-link-apply', apply, 'refused');
      // Source snapshots bind canonical identity and the selected company role;
      // appended descriptive assertions intentionally do not rename that identity.
      await http(root + `workspace-parties/${encodeURIComponent(partyId)}/role`, { role: 'issuer', confirm: true });
      const nextInput = { ...input, role: 'issuer' };
      const next = cli('party-link-plan', nextInput).plan; need(next.planHash !== plan.planHash, 'explicit reviewed current role changes plan');
      rpc('party-link-apply', { ...apply, ...nextInput, planHash: next.planHash }, 'refused');
      const relink = { ...apply, ...nextInput, planHash: next.planHash, idempotencyKey: 'synthetic-party166-relink' };
      rpc('party-link-apply', relink);
      rpc('party-link-apply', apply, 'refused'); await parity(documentId);
      const linkedSnapshot = await snapshot();
      for (const k of ['docs', 'bytes', 'journal', 'trial', 'vat']) need(equal(linkedSnapshot[k as keyof typeof linkedSnapshot], before[k as keyof typeof before]), 'links financially/source inert ' + k);
      // An internal voucher must be admitted with real archived bank evidence, not a SQL fixture.
      const csv = `dato;tekst;beløb;valuta;saldo\n${today};Synthetic internal166;-10,00;DKK;-385,00\n`;
      await http(root + 'bank/import', { confirm: true, csvContent: csv, account: 'synthetic-batch-bank' });
      const bank = (await http(root + 'bank')).bank.transactions.find((r: any) => r.text === 'Synthetic internal166'); need(bank?.id > 0, 'real internal bank source');
      const bytes = Buffer.from('Synthetic internal voucher166 bank evidence\n');
      await http(root + 'documents/ingest', { confirm: true, fileName: 'synthetic-party166-internal.txt', fileBase64: bytes.toString('base64'), metadata: { source: 'Synthetic internal166', documentType: 'internal_voucher', issueDate: today, deliveryDescription: 'Synthetic internal bank charge', amountIncVat: 10, vatAmount: 0, currency: 'DKK', sourceBankTransactionId: bank.id, accountingRationale: 'Synthetic retained bank evidence with no external party' } });
      const internal = (await http(root + 'documents')).documents.documents.find((d: any) => d.filename === 'synthetic-party166-internal.txt'); need(internal?.id > 0, 'admitted internal voucher');
      const decision = { documentId: internal.id, reason: 'Explicitly reviewed internal bank charge', idempotencyKey: 'synthetic-internal166', confirm: true };
      const financialBaseline = await snapshot();
      const decided = cli('internal-no-external-party', { ...decision, confirm: 'yes' }); need(decided.id > 0 && !decided.idempotent, 'internal decision');
      need(rpc('internal-no-external-party', decision).data.idempotent, 'internal repeat');
      rpc('internal-no-external-party', { ...decision, reason: 'Changed reason' }, 'refused');
      rpc('internal-no-external-party', { ...decision, documentId }, 'refused');
      rpc('party-link-apply', { ...apply, documentId: internal.id, planHash: next.planHash, idempotencyKey: 'synthetic-internal166-link' }, 'refused');
      await state(internal.id, 'internal_no_external_party');
      rpc('internal-no-external-party-supersede', { documentId: internal.id, decisionHash: decided.decisionHash, reason: 'Explicitly withdraw decision', confirm: true });
      await state(internal.id, 'unresolved'); rpc('internal-no-external-party', decision, 'refused');
      // Retain a second live decision as well as the superseded nonempty internal history.
      rpc('internal-no-external-party', { ...decision, reason: 'Fresh review of internal bank evidence', idempotencyKey: 'synthetic-internal166-current' });
      const after = await snapshot();
      for (const k of ['docs', 'bytes', 'journal', 'trial', 'vat']) need(equal(after[k as keyof typeof after], financialBaseline[k as keyof typeof financialBaseline]), 'resolution financially/source inert ' + k);
      need(after.journal.journal.entries.length === 0, 'zero financial postings');
      await admin('members/company', { userId: service.serviceAccountId, companySlug: 'synthetic', action: 'revoke' });
      rpc('party-link-inspect', { documentId }, 'refused'); rpc('party-link-apply', relink, 'refused');
      stable = { snapshot: await snapshot(), history: await inspect(documentId) };
      return { documentId, internalDocumentId: internal.id, plan, nextPlan: next, receipt, internalDecision: decided, stable, transcript, reviewedLegacyActualPositiveQualified: false, noFinancialPosting: true };
    },
    async assertStable(page: Page) {
      owner = page;
      need(equal(await snapshot(), stable.snapshot) && equal(await inspect(linkedDocumentId), stable.history), 'exact restart/restore source and histories');
      return { exact: true, zeroJournals: true };
    },
  };
}
