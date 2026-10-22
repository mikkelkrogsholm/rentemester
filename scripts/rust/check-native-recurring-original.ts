import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

type Credential = { file: string; service?: boolean };
type Operation = 'create' | 'generate' | 'list';
const need = (value: unknown, label: string) => { if (!value) throw Error(label); };

/** Original recurring clients; single explicit submissions, no scheduler, credentials or SQL seeding. */
export function originalRecurringProof(base: string, work: string, owner: Credential) {
  let sequence = 0;
  function invoke(kind: 'cli' | 'mcp', operation: Operation, body: Record<string, unknown>, credential = owner, refused = false) {
    const common = ['--server', base, '--allow-loopback-http', credential.service ? '--service-key-file' : '--cookie-file', credential.file];
    let args: string[], input: string | undefined;
    if (kind === 'cli') {
      args = ['recurring-invoice', operation, ...common, '--company', 'synthetic', '--json'];
      if (operation === 'create') {
        const file = join(work, `recurring-original154-input-${++sequence}.json`);
        writeFileSync(file, JSON.stringify(body), { mode: 0o600, flag: 'wx' });
        args.push('--input', file);
      } else if (operation === 'generate') {
        args.push('--template-id', String(body.templateId), '--as-of', String(body.asOfDate));
      } else if (body.includeInactive === true) args.push('--include-inactive');
    } else {
      args = common;
      input = [
        { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-recurring-original154', version: '1' } } },
        { jsonrpc: '2.0', method: 'notifications/initialized' },
        { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'recurring_invoice_' + operation, arguments: { company: 'synthetic', ...body, ...(operation === 'list' ? {} : { confirm: true }) } } },
      ].map(value => JSON.stringify(value)).join('\n') + '\n';
    }
    const result = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    const secret = readFileSync(credential.file, 'utf8').trim();
    need(!result.error && !result.stdout.includes(secret) && !result.stderr.includes(secret), 'Recurring client terminal/privacy');
    if (kind === 'cli') {
      need(result.status === (refused ? 1 : 0), `Recurring CLI ${operation} exit`);
      const value = JSON.parse(result.stdout); need(value.ok === !refused, `Recurring CLI ${operation} outcome`); return value;
    }
    need(result.status === 0, 'Recurring MCP terminal exit');
    const reply = result.stdout.trim().split('\n').map(line => JSON.parse(line)).find(value => value.id === 2);
    need(!reply?.error && reply.result.isError === refused && isDeepStrictEqual(JSON.parse(reply.result.content[0].text), reply.result.structuredContent), 'Recurring MCP text/structured agreement');
    return reply.result.structuredContent;
  }
  return { invoke };
}
