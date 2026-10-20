import { request, type APIRequestContext, type Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

/** Normal-owner issued service delivery proof; root controls actual SMTP/crash/restart. */
export async function createServiceMailProof(page: Page, base: string, work: string, invoiceId: number) {
  const prefix = '/api/companies/synthetic/';
  const keyFile = join(work, 'invoice-mail-service.key');
  let key = '', oldKey = '', serviceId = '', credentialId = '';
  let http: APIRequestContext | undefined;
  let pending: Promise<{ status?: number; networkUnknown?: boolean }> | undefined;
  const need = (value: unknown, label: string) => { if (!value) { const error = Error(label); error.name = 'ServiceMailAssertion'; throw error; } };
  const service = () => { if (!http) throw Error('Service mail context missing'); return http; };
  async function admin(resource: string, body: unknown) {
    const r = await page.request.post(base + '/api/workspace/' + resource, { headers: { origin: base }, data: body });
    need(r.ok(), `Owner mail service ${resource} HTTP${r.status()}`); return r.json();
  }
  async function bind(secret: string) {
    key = secret;
    writeFileSync(keyFile, key + '\n', { mode: 0o600 });
    await http?.dispose();
    http = await request.newContext({ extraHTTPHeaders: { origin: base, 'x-rentemester-service-key': key } });
  }
  const input = (recipient: string) => ({ invoiceDocumentId: invoiceId, to: recipient, kind: 'invoice', confirm: true });
  async function call(resource: string, body?: unknown, status = 200) {
    const r = body === undefined ? await service().get(base + prefix + resource)
      : await service().post(base + prefix + resource, { data: body });
    need(r.status() === status, `Service mail ${resource} HTTP${r.status()} expected${status}`); return r.json();
  }
  function client(kind: 'cli' | 'mcp', command: string, body?: unknown, accepted = true) {
    const common = ['--server', base, '--allow-loopback-http', '--service-key-file', keyFile];
    const args = kind === 'cli' ? ['remote', command.replaceAll('_', '-'), ...common, '--company', 'synthetic', ...(body === undefined ? [] : ['--confirm'])] : common;
    const stdin = kind === 'cli' ? (body === undefined ? '' : JSON.stringify(body)) : [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-service-mail', version: '1' } } },
      { jsonrpc: '2.0', method: 'notifications/initialized' },
      { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: command, arguments: { company: 'synthetic', ...(body === undefined ? {} : { body: JSON.stringify(body), confirm: true }) } } },
    ].map(v => JSON.stringify(v)).join('\n') + '\n';
    const out = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), args, { input: stdin, encoding: 'utf8', timeout: 30000, maxBuffer: 16 * 1024 * 1024 });
    need(!out.error && ![key, oldKey].filter(Boolean).some(secret => out.stdout.includes(secret) || out.stderr.includes(secret)), 'Mail client transport or private secret disclosure');
    need(out.status === (kind === 'cli' && !accepted ? 1 : 0), `Service mail ${kind} ${command} exit${out.status}`);
    const value = kind === 'cli' ? JSON.parse(out.stdout) : out.stdout.trim().split('\n').map(l => JSON.parse(l)).find(v => v.id === 2)?.result?.structuredContent;
    need(value?.ok === accepted && value.data, `Service mail ${kind} ${command} canonical outcome`);
    return value.data;
  }
  async function current() { return call('invoices/email-deliveries'); }
  async function selected(recipient: string) {
    const rows = (await current()).invoiceEmail.rows.filter((r: any) => r.recipient === recipient && r.invoiceDocumentId === invoiceId);
    need(rows.length === 1, 'Exact service delivery identity'); return rows[0];
  }
  async function parity() {
    const v = await current();
    for (const kind of ['cli', 'mcp'] as const) {
      need(isDeepStrictEqual(client(kind, 'invoice_email_status'), v), `${kind} full service email parity`);
    }
    return v;
  }
  async function setup() {
    const issued = await admin('service-principals', { displayName: 'Synthetic invoice delivery executor', operationId: randomUUID(), confirm: true });
    need(issued.secret && issued.serviceAccountId && issued.credentialId, 'Ordinary owner service issuance');
    serviceId = issued.serviceAccountId; credentialId = issued.credentialId;
    await bind(issued.secret);
    await admin('members/access', { userId: serviceId, action: 'set-role', workspaceRole: 'member' });
    for (const role of ['reader', 'bookkeeper']) {
      await admin('members/company', { userId: serviceId, action: 'grant', companySlug: 'synthetic', role });
      const before = await current();
      const denied = await call('invoices/send-email', input('accepted@example.invalid'), 403);
      need(denied.usage === 'not_committed' && denied.business === 'not_attempted', 'Unauthorized role refuses before usage and delivery');
      need(isDeepStrictEqual((await current()).invoiceEmail, before.invoiceEmail), 'Unauthorized send creates no reservation');
    }
    await admin('members/company', { userId: serviceId, action: 'grant', companySlug: 'synthetic', role: 'owner' });
    return { serviceId, credentialId, normalOwnerProvisioning: true, readerAndBookkeeperDenied: true, explicitCompanyOwnerGrant: true };
  }
  async function accepted() {
    const receipt = client('cli', 'invoice_email_send', input('accepted@example.invalid'));
    need(receipt.delivery.state === 'accepted' && receipt.usage === 'committed', 'Service CLI accepted current configured SMTP');
    return { row: await selected('accepted@example.invalid') };
  }
  async function acceptedDuplicate() {
    const row = await selected('accepted@example.invalid');
    const duplicate = client('mcp', 'invoice_email_send', input('accepted@example.invalid'));
    need(duplicate.delivery.duplicate && duplicate.delivery.messageId === row.messageId && duplicate.delivery.state === 'accepted', 'MCP exact replay existing delivery');
    return { row, canonical: await parity() };
  }
  async function refusalRetry() {
    const failure = client('cli', 'invoice_email_send', input('retry@example.invalid'), false);
    const refused = failure.delivery;
    need(failure.code === 'delivery_refused' && refused.state === 'refused', 'Definite service SMTP refusal');
    const receipt = client('mcp', 'invoice_email_retry', { messageId: refused.messageId, expectedStateHash: refused.stateHash, reason: 'Synthetic definite SMTP refusal reviewed against exact queue', confirm: true });
    need(receipt.delivery.state === 'accepted' && receipt.delivery.messageId === refused.messageId, 'Separate explicitly reviewed retry accepted');
    return { refused, row: await selected('retry@example.invalid'), canonical: await parity() };
  }
  async function rotate() {
    oldKey = key;
    const rotated = await admin('service-principals/rotate', { serviceAccountId: serviceId, credentialId, operationId: randomUUID(), confirm: true });
    need(rotated.secret && rotated.credentialId !== credentialId, 'Ordinary rotated service credential');
    credentialId = rotated.credentialId; await bind(rotated.secret);
    const stale = await request.newContext({ extraHTTPHeaders: { origin: base, 'x-rentemester-service-key': oldKey } });
    try {
      need((await stale.get(base + prefix + 'invoices/email-deliveries')).status() === 401, 'Rotated key cannot inspect delivery history');
      need((await stale.post(base + prefix + 'invoices/send-email', { data: input('accepted@example.invalid') })).status() === 401, 'Rotated key cannot replay delivery');
    } finally { await stale.dispose(); }
    return { oldCredentialDeniedForReadAndReplay: true, serviceId, credentialId };
  }
  async function startUnknown() {
    need(!pending, 'Unknown service send begins once');
    pending = service().post(base + prefix + 'invoices/send-email', { data: input('unknown@example.invalid') })
      .then(r => ({ status: r.status() })).catch(() => ({ networkUnknown: true }));
    return { requestStarted: true };
  }
  async function killed() {
    need(pending, 'Service send pending at intentional crash');
    const result = await pending;
    need(result?.networkUnknown || (result?.status ?? 0) >= 500, 'Killed service request cannot claim delivery success');
    return { serviceUnknownOutcome: true };
  }
  async function reopened(evidence: string) {
    const before = await selected('unknown@example.invalid');
    need(before.state === 'unknown', 'Restart preserves uncertain SMTP outcome');
    const duplicate = client('mcp', 'invoice_email_send', input('unknown@example.invalid'), false);
    need(duplicate.code === 'delivery_unknown' && duplicate.delivery.state === 'unknown' && duplicate.delivery.messageId === before.messageId, 'Unknown MCP replay cannot resend');
    const reconciled = client('cli', 'invoice_email_reconcile', { messageId: before.messageId, expectedStateHash: before.stateHash, reason: 'Synthetic exact durable SMTP queue confirms DATA acceptance', resolution: 'accepted', evidence, confirm: true });
    need(reconciled.delivery.state === 'accepted', 'Explicit service reconciliation records external evidence without send');
    const canonical = await parity();
    need(canonical.invoiceEmail.rows.length === 3 && canonical.invoiceEmail.rows.every((r: any) => r.state === 'accepted' && r.history.every((h: any) => h.actor === 'service_account:' + serviceId)), 'Three complete service-attributed delivery histories');
    return { before, row: reconciled.delivery, canonical, serviceId, unchangedBooksAndOriginals: true };
  }
  return { setup, accepted, acceptedDuplicate, refusalRetry, rotate, startUnknown, killed, reopened, dispose: async () => http?.dispose() };
}
