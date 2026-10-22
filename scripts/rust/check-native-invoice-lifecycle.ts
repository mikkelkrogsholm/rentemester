import type { APIRequestContext } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { originalInvoiceProof } from './check-native-invoice-original';

type Operation = 'issue' | 'render' | 'post' | 'settle-bank' | 'credit-note' | 'refund-bank' | 'apply-payment';
type Credential = { file: string; service?: boolean };
const need = (value: unknown, label: string) => { if (!value) throw Error(label); };

/** Original public clients, single submission per write; never seeds auth or ledger SQL. */
export function invoiceLifecycleProof(http: APIRequestContext, base: string, work: string, owner: Credential, run = '148') {
  let sequence = 0;
  function client(kind: 'cli' | 'mcp', operation: Operation, body: Record<string, any>, credential = owner, refused = false) {
    const common = ['--server', base, '--allow-loopback-http', credential.service ? '--service-key-file' : '--cookie-file', credential.file];
    let args: string[], input: string | undefined;
    if (kind === 'cli') {
      args = ['invoice', operation, ...common, '--company', 'synthetic', '--json'];
      if (body.payload !== undefined) {
        const file = join(work, `invoice-lifecycle${run}-input-${++sequence}.json`);
        writeFileSync(file, JSON.stringify(body.payload), { mode: 0o600, flag: 'wx' });
        args.push('--input', file);
      }
      for (const [key, flag] of [['customerId', 'customer-id'], ['documentId', 'document-id'], ['invoiceNumber', 'invoice-number'], ['revenueAccountNo', 'revenue-account']]) {
        if (body[key] !== undefined) args.push('--' + flag, String(body[key]));
      }
    } else {
      args = common;
      input = [
        { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-invoice-lifecycle148', version: '1' } } },
        { jsonrpc: '2.0', method: 'notifications/initialized' },
        { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'invoice_' + operation.replaceAll('-', '_'), arguments: { company: 'synthetic', ...body, confirm: true } } },
      ].map(value => JSON.stringify(value)).join('\n') + '\n';
    }
    const result = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    const secret = readFileSync(credential.file, 'utf8').trim();
    need(!result.error && !result.stdout.includes(secret) && !result.stderr.includes(secret), 'Lifecycle client terminal/privacy');
    if (kind === 'cli') {
      need(result.status === (refused ? 1 : 0), `Lifecycle CLI ${operation} exit`);
      const value = JSON.parse(result.stdout); need(value.ok === !refused, `Lifecycle CLI ${operation} outcome`); return value;
    }
    need(result.status === 0, 'Lifecycle MCP terminal exit');
    const reply = result.stdout.trim().split('\n').map(line => JSON.parse(line)).find(value => value.id === 2);
    need(!reply?.error && reply.result.isError === refused && isDeepStrictEqual(JSON.parse(reply.result.content[0].text), reply.result.structuredContent), 'Lifecycle MCP text/structured agreement');
    const value = reply.result.structuredContent;
    need(value.ok === !refused, `Lifecycle MCP ${operation} outcome`);
    return { ok: value.ok, ...(value.data ?? {}), errors: value.errors, ...(value.appliedRules ? { appliedRules: value.appliedRules } : {}) };
  }
  const prefix = base + '/api/companies/synthetic/';
  async function get(resource: string) {
    const response = await http.get(prefix + resource); need(response.ok(), 'Lifecycle canonical read ' + resource); return response.json();
  }
  async function snapshot() {
    return { documents: await get('documents'), journal: await get('journal'), bank: await get('bank'), invoices: await get('invoices'), audit: await get('audit-log?limit=5000') };
  }
  async function unchanged(before: unknown, label: string) { need(isDeepStrictEqual(await snapshot(), before), label); }
  const queries = originalInvoiceProof({ request: http } as any, base, work, owner.file);
  async function status(documentId: number) { return (await queries.read('status', { documentId })).data; }
  async function render(documentId: number) {
    const before = await snapshot();
    const response = await http.post(prefix + 'invoices/original/render', { data: { documentId, confirm: true } });
    const value = await response.json(); need(response.ok() && value.ok, 'Original immutable render');
    const flat = { ok: true, ...value.data, errors: value.errors, ...(value.appliedRules ? { appliedRules: value.appliedRules } : {}) };
    for (const kind of ['cli', 'mcp'] as const) need(isDeepStrictEqual(client(kind, 'render', { documentId }), flat), 'Original render exact HTTP/CLI/MCP parity');
    const pdf = await http.get(prefix + `invoices/${documentId}/pdf`);
    need(pdf.ok() && (await pdf.body()).subarray(0, 5).toString() === '%PDF-', 'Existing immutable PDF bytes');
    await unchanged(before, 'Render must not change books or audit');
    return { render: value, pdfBase64: (await pdf.body()).toString('base64') };
  }
  return { client, snapshot, unchanged, status, render, get };
}
