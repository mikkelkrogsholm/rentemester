import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

type Credential = { file: string; service?: boolean };
type Operation = 'validate' | 'write-off-bad-debt' | 'imported-receivables';
const need = (value: unknown, label: string) => { if (!value) throw Error(label); };

/** Real original CLI/MCP calls. Single submissions; no ledger writes or credential creation. */
export function invoiceFinancialProof(base: string, work: string, owner: Credential) {
  let sequence = 0;
  function invoke(kind: 'cli' | 'mcp', operation: Operation, body: Record<string, unknown>, credential = owner, refused = false) {
    const pure = operation === 'validate', mutation = operation === 'write-off-bad-debt';
    const common = pure ? [] : ['--server', base, '--allow-loopback-http', credential.service ? '--service-key-file' : '--cookie-file', credential.file];
    let args: string[], input: string | undefined;
    if (kind === 'cli') {
      args = ['invoice', operation, ...common, ...(pure ? [] : ['--company', 'synthetic']), '--json'];
      if (body.payload !== undefined) {
        const file = join(work, `invoice-financial152-input-${++sequence}.json`);
        writeFileSync(file, JSON.stringify(body.payload), { mode: 0o600, flag: 'wx' });
        args.push('--input', file);
      }
      if (body.asOf !== undefined) args.push('--as-of', String(body.asOf));
    } else {
      // MCP's process can be connected, but pure validation itself uses no company or server call.
      args = ['--server', base, '--allow-loopback-http', credential.service ? '--service-key-file' : '--cookie-file', credential.file];
      input = [
        { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-invoice-financial152', version: '1' } } },
        { jsonrpc: '2.0', method: 'notifications/initialized' },
        { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'invoice_' + operation.replaceAll('-', '_'), arguments: { ...(pure ? {} : { company: 'synthetic' }), ...body, ...(mutation ? { confirm: true } : {}) } } },
      ].map(value => JSON.stringify(value)).join('\n') + '\n';
    }
    const result = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    const secret = readFileSync(credential.file, 'utf8').trim();
    need(!result.error && !result.stdout.includes(secret) && !result.stderr.includes(secret), 'Financial client terminal/privacy');
    if (kind === 'cli') {
      need(result.status === (refused ? 1 : 0), `Financial CLI ${operation} exit`);
      const value = JSON.parse(result.stdout); need(value.ok === !refused, `Financial CLI ${operation} outcome`); return value;
    }
    need(result.status === 0, 'Financial MCP terminal exit');
    const reply = result.stdout.trim().split('\n').map(line => JSON.parse(line)).find(value => value.id === 2);
    need(!reply?.error && reply.result.isError === refused && isDeepStrictEqual(JSON.parse(reply.result.content[0].text), reply.result.structuredContent), 'Financial MCP text/structured agreement');
    return reply.result.structuredContent;
  }
  return { invoke };
}
