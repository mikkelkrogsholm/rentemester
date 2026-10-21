import { request, type Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, readdirSync, lstatSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

/** Owned normal-MFA fixture; no business/auth SQL seeds or financial writes. */
export function createAuditVerifyProof(page: Page, base: string, work: string, nonAdminPage?: Page) {
  const slugs = ['synthetic', 'synthetic-backup-inbox-target'];
  const cookieFile = join(work, 'verification-owner.cookie'), serviceFile = join(work, 'backup-lock-service.key');
  const need = (value: unknown, message: string) => { if (!value) throw Error(message); };
  const hash = (value: unknown) => createHash('sha256').update(JSON.stringify(value)).digest('hex');
  const path = (slug: string, resource: string) => base + '/api/companies/' + slug + '/' + resource;
  let stable: any, reports: any, completed = false;
  async function cookies() { writeFileSync(cookieFile, (await page.context().cookies(base)).map(cookie => `${cookie.name}=${cookie.value}`).join('; ') + '\n', { mode: 0o600 }); }
  async function get(slug: string, resource: string) { const r = await page.request.get(path(slug, resource)); need(r.ok(), 'Verification fixture read refused: ' + resource); return r.json(); }
  async function snapshot() {
    return Promise.all(slugs.map(async slug => ({ slug, journal: await get(slug, 'journal'), bank: await get(slug, 'bank'), trial: await get(slug, 'trial-balance'), balance: await get(slug, 'balance'), history: await get(slug, 'audit-log?limit=5000') })));
  }
  function client(kind: 'cli' | 'mcp', slug: string, service: boolean) {
    const common = ['--server', base, '--allow-loopback-http', service ? '--service-key-file' : '--cookie-file', service ? serviceFile : cookieFile];
    const args = kind === 'cli' ? ['remote', 'audit-verify', ...common, '--company', slug] : common;
    const input = kind === 'cli' ? '' : [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-verification-proof', version: '1' } } },
      { jsonrpc: '2.0', method: 'notifications/initialized' },
      { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'audit_verify', arguments: { company: slug } } },
    ].map(value => JSON.stringify(value)).join('\n') + '\n';
    const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    const secret = readFileSync(serviceFile, 'utf8').trim();
    need(!r.error && r.status === 0 && !r.stdout.includes(secret) && !r.stderr.includes(secret), 'Verification client/credential safety refused');
    if (kind === 'cli') { const value = JSON.parse(r.stdout); need(value.ok === true, 'Verification CLI envelope refused'); return value.data; }
    const value = r.stdout.trim().split('\n').map(line => JSON.parse(line)).find(value => value.id === 2);
    need(value?.result?.isError === false, 'Verification MCP envelope refused'); return value.result.structuredContent.data;
  }
  async function parity(slug: string, service = false) {
    const context = service ? await request.newContext({ extraHTTPHeaders: { 'x-rentemester-service-key': readFileSync(serviceFile, 'utf8').trim() } }) : page.request;
    try {
      const response = await context.get(path(slug, 'audit-verify')); need(response.ok(), 'Verification HTTP read refused');
      const value = await response.json();
      for (const kind of ['cli', 'mcp'] as const) need(isDeepStrictEqual(client(kind, slug, service), value), 'Verification HTTP/CLI/MCP findings differ');
      return value;
    } finally { if (service) await context.dispose(); }
  }
  function clean(value: any, slug: string) {
    need(value.ok === true && value.audit?.format === 'rentemester-native-audit-verification/1' && value.audit.slug === slug && value.audit.available === true && value.audit.ok === true && value.audit.errors.length === 0,
      'Expected populated synthetic accounting to verify clean: ' + JSON.stringify(value));
    need(isDeepStrictEqual(value.audit.components.map((c: any) => c.name), ['ledger', 'bank', 'invoices', 'originals', 'physical_schema']) && value.audit.components.every((c: any) => c.available && c.ok && c.errors.length === 0), 'All five required components must be available and clean');
  }
  async function ui(slug: string, expected: boolean) {
    await page.goto(base + '/companies/' + slug + '/integritet');
    const card = page.getByRole('region', { name: 'Regnskabskontrol', exact: true });
    const response = page.waitForResponse(r => r.url() === path(slug, 'audit-verify') && r.request().method() === 'GET');
    await card.getByRole('button', { name: 'Kontrollér regnskabet', exact: true }).click();
    const value = await (await response).json(); need(value.audit.ok === expected, 'Actual UI control result differs');
    await card.getByRole('status').filter({ hasText: expected ? 'Kontrollen fandt ingen afvigelser.' : 'Kontrollen fandt afvigelser.' }).waitFor();
    return value;
  }
  return {
    async run() {
      need(!completed, 'Verification proof executes once'); await cookies(); stable = await snapshot(); reports = {};
      for (const slug of slugs) { reports[slug] = await parity(slug); clean(reports[slug], slug); }
      need(isDeepStrictEqual(await parity(slugs[0], true), reports[slugs[0]]), 'Current scoped service full verification differs');
      await ui(slugs[0], true);
      for (const query of ['?unknown=x', '?actor=forged']) { const r = await page.request.get(path(slugs[0], 'audit-verify') + query); need(r.status() === 400, 'Verification accepts no caller query'); }
      need(nonAdminPage, 'Existing normal MFA second user required');
      const identity = await nonAdminPage!.request.get(base + '/api/me'); need(identity.ok(), 'Second current human required'); const readerId = (await identity.json()).user.id;
      const members = await page.request.get(base + '/api/workspace/members'); need(members.ok(), 'Owner membership read required');
      const previous = (await members.json()).members.find((m: any) => m.userId === readerId)?.memberships.find((m: any) => m.companySlug === slugs[0])?.role;
      const change = async (action: string, role?: string) => { const r = await page.request.post(base + '/api/workspace/members/company', { headers: { origin: base }, data: { userId: readerId, companySlug: slugs[0], action, ...(role ? { role } : {}) } }); need(r.ok(), 'Normal current reader change refused'); };
      try {
        await change('grant', 'reader'); const allowed = await nonAdminPage!.request.get(path(slugs[0], 'audit-verify')); need(allowed.ok() && isDeepStrictEqual(await allowed.json(), reports[slugs[0]]), 'Current reader full verification differs');
        await change('revoke'); const denied = await nonAdminPage!.request.get(path(slugs[0], 'audit-verify')); need(denied.status() === 403, 'Revoked reader still admitted');
      } finally { await change(previous ? 'grant' : 'revoke', previous); }
      const directory = join(work, 'workspace', slugs[1], 'documents', 'originals');
      need(!lstatSync(directory).isSymbolicLink(), 'Owned originals directory required');
      const names = readdirSync(directory); need(names.length === 1, 'Exactly one owned unposted document original required');
      const original = join(directory, names[0]); need(lstatSync(original).isFile() && !lstatSync(original).isSymbolicLink(), 'Owned regular original required');
      const bytes = readFileSync(original); let failed: any;
      try {
        writeFileSync(original, Buffer.from('Synthetic verification119 deliberately changed original\n'));
        failed = await parity(slugs[1]);
        need(failed.ok === true && failed.audit.available === true && failed.audit.ok === false && failed.audit.components.find((c: any) => c.name === 'originals')?.ok === false && failed.audit.errors.length > 0, 'Changed unposted original must produce truthful failed accounting observation');
        need(isDeepStrictEqual(await ui(slugs[1], false), failed), 'UI must show exact current failed observation');
      } finally { writeFileSync(original, bytes); }
      need(isDeepStrictEqual(await parity(slugs[1]), reports[slugs[1]]), 'Restored original must restore exact clean observation');
      await ui(slugs[1], true);
      const bankDirectory = join(work, 'workspace', slugs[1], 'bank', 'processed');
      need(!lstatSync(bankDirectory).isSymbolicLink(), 'Owned bank originals directory required');
      const bankNames = readdirSync(bankDirectory); need(bankNames.length === 1, 'One owned unmatched bank source required');
      const bankOriginal = join(bankDirectory, bankNames[0]); need(lstatSync(bankOriginal).isFile() && !lstatSync(bankOriginal).isSymbolicLink(), 'Owned regular bank original required');
      const bankBytes = readFileSync(bankOriginal); let failedBank: any;
      try {
        writeFileSync(bankOriginal, Buffer.from('Synthetic verification119 deliberately changed bank original\n'));
        failedBank = await parity(slugs[1]);
        need(failedBank.ok === true && failedBank.audit.available === true && failedBank.audit.ok === false && failedBank.audit.components.find((c: any) => c.name === 'originals')?.errors.some((finding: any) => Number.isSafeInteger(finding.bankTransactionId)), 'Unmatched bank original must also be physically checked');
      } finally { writeFileSync(bankOriginal, bankBytes); }
      need(isDeepStrictEqual(await parity(slugs[1]), reports[slugs[1]]), 'Restored bank original must restore exact clean observation');
      need(isDeepStrictEqual(await snapshot(), stable), 'Read-only verification changed accounting or company audit'); completed = true;
      return { qualified: true, reports, failedOriginal: failed, failedBankOriginal: failedBank, unmatchedBankOriginalTamperDetectedAndRestored: true, accountingFingerprint: hash(stable), originalSha256: createHash('sha256').update(bytes).digest('hex'), completeHumanCliMcpParity: true, serviceParity: true, currentReaderRevocation: true, webCleanAndFailed: true, unpostedOriginalTamperDetectedAndRestored: true, accountingAndAuditUnchanged: true };
    },
    async reopen() {
      need(completed, 'Completed verification required before restart'); await cookies();
      for (const slug of slugs) need(isDeepStrictEqual(await parity(slug), reports[slug]), 'Normal MFA restart changed exact verification');
      need(isDeepStrictEqual(await parity(slugs[0], true), reports[slugs[0]]), 'Service restart verification differs');
      await ui(slugs[0], true); need(isDeepStrictEqual(await snapshot(), stable), 'Verification restart changed accounting/audit');
      return { exactReportsAfterNormalMfaRestart: true, completeHumanCliMcpAndServiceParity: true, accountingAndAuditUnchanged: true, accountingFingerprint: hash(stable) };
    },
  };
}
