import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

type Operation = 'remind' | 'post-reminder' | 'compensation' | 'claim-compensation' | 'post-compensation' | 'interest' | 'claim-interest' | 'post-interest' | 'interest-correction' | 'post-interest-correction' | 'settle-claim-bank';
type Credential = { file: string; service?: boolean };
const tools: Record<Operation, string> = {
  remind: 'invoice_remind', 'post-reminder': 'invoice_post_reminder',
  compensation: 'invoice_compensation_calc', 'claim-compensation': 'invoice_claim_compensation', 'post-compensation': 'invoice_post_compensation',
  interest: 'invoice_interest_calc', 'claim-interest': 'invoice_claim_interest', 'post-interest': 'invoice_post_interest',
  'interest-correction': 'invoice_interest_correction_calc', 'post-interest-correction': 'invoice_post_interest_correction',
  'settle-claim-bank': 'invoice_settle_claim_bank',
};
const readonly = new Set<Operation>(['compensation', 'interest', 'interest-correction']);
const flags = {
  documentId: 'document-id', invoiceNumber: 'invoice-number', date: 'date', fee: 'fee', note: 'note', reminderId: 'reminder-id',
  asOf: 'as-of', amountDkk: 'amount-dkk', referenceRate: 'reference-rate', claimId: 'claim-id', reason: 'reason',
  receivableAccountNo: 'receivable-account', reminderIncomeAccountNo: 'reminder-income-account',
  compensationIncomeAccountNo: 'compensation-income-account', interestIncomeAccountNo: 'interest-income-account',
};
const need = (v: unknown, label: string) => { if (!v) throw Error(label); };

/** Single public submissions; amounts/rates, accounts and allocation stay server-owned. */
export function originalClaimsProof(base: string, work: string, owner: Credential) {
  let sequence = 0;
  function invoke(kind: 'cli' | 'mcp', operation: Operation, body: Record<string, unknown>, credential = owner, refused = false) {
    const common = ['--server', base, '--allow-loopback-http', credential.service ? '--service-key-file' : '--cookie-file', credential.file];
    let args: string[], input: string | undefined;
    if (kind === 'cli') {
      args = ['invoice', operation, ...common, '--company', 'synthetic', '--json'];
      if (operation === 'settle-claim-bank') {
        const file = join(work, `original-claims151-input-${++sequence}.json`);
        writeFileSync(file, JSON.stringify(body.payload), { mode: 0o600, flag: 'wx' }); args.push('--input', file);
      } else {
        for (const [key, value] of Object.entries(body)) {
          need(key in flags, 'Known original claim CLI field');
          args.push('--' + flags[key as keyof typeof flags], String(value));
        }
      }
    } else {
      args = common;
      input = [
        { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-original-claims151', version: '1' } } },
        { jsonrpc: '2.0', method: 'notifications/initialized' },
        { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: tools[operation], arguments: { company: 'synthetic', ...body, ...(readonly.has(operation) ? {} : { confirm: true }) } } },
      ].map(v => JSON.stringify(v)).join('\n') + '\n';
    }
    const result = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    const secret = readFileSync(credential.file, 'utf8').trim();
    need(!result.error && !result.stdout.includes(secret) && !result.stderr.includes(secret), 'Original claims terminal/privacy');
    if (kind === 'cli') {
      need(result.status === (refused ? 1 : 0), `Original claims CLI ${operation} exit`);
      const value = JSON.parse(result.stdout); need(value.ok === !refused, `Original claims CLI ${operation} outcome`); return value;
    }
    need(result.status === 0, 'Original claims MCP terminal');
    const reply = result.stdout.trim().split('\n').map(line => JSON.parse(line)).find(v => v.id === 2);
    need(!reply?.error && reply.result.isError === refused && isDeepStrictEqual(JSON.parse(reply.result.content[0].text), reply.result.structuredContent), 'Original claims MCP text/structured equality');
    const value = reply.result.structuredContent; need(value.ok === !refused, `Original claims MCP ${operation} outcome`);
    return { ok: value.ok, ...(value.data ?? {}), errors: value.errors, ...(value.appliedRules ? { appliedRules: value.appliedRules } : {}) };
  }
  return { invoke };
}
