import type { Page } from '@playwright/test';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, unlinkSync } from 'node:fs';
import { join } from 'node:path';

/** Reuse the ordinary-MFA R32 books; this proof creates no new accounting fixture. */
export async function checkNativeDashboardArtifact(owner: Page, member: Page, base: string, work: string, phase: string) {
  const need = (value: unknown, message: string) => { if (!value) throw Error(message); };
  const hash = (value: string | Buffer) => createHash('sha256').update(value).digest('hex');
  const asOf = '2025-10-12', company = 'synthetic';
  const path = `/api/companies/${company}/dashboard/export?format=html&asOf=${asOf}`;
  const response = await owner.request.get(base + path);
  need(response.ok(), `Dashboard HTML HTTP${response.status()}`);
  const html = await response.text(), headers = response.headers();
  need(headers['content-type'] === 'text/html; charset=utf-8' && headers['cache-control'] === 'private, no-store', 'HTML download MIME and private cache contract');
  need(headers['content-disposition'] === `attachment; filename="dashboard-${company}-${asOf}.html"`, 'Server-owned safe dashboard filename');
  need(headers['x-rentemester-as-of'] === asOf && Number.isFinite(Number(headers['x-rentemester-render-ms'])), 'Explicit dated download metadata');
  // Each request has a real generation time; all other bytes must agree.
  const stableHtml = (value: string) => {
    const matches = value.match(/<footer><p>Genereret: [^<]+<\/p>/g);
    need(matches?.length === 1, 'One explicit generatedAt rendering input');
    return value.replace(/(<footer><p>Genereret: )[^<]+(<\/p>)/, '$1[generatedAt]$2');
  };
  const stable = stableHtml(html);
  const view = await owner.context().newPage();
  try {
    await view.setContent(html);
    need(await view.locator('section[data-section]').count() === 12, 'All original dashboard sections rendered');
    for (const id of ['metrics', 'exceptions', 'vat', 'invoices', 'payables', 'accruals', 'budget', 'liquidity', 'tax', 'eu', 'activity', 'system']) {
      need(await view.locator(`section[data-section="${id}"]`).count() === 1, `Required dashboard section ${id}`);
    }
    const metrics = view.locator('section[data-section="metrics"]');
    need(await metrics.getByText('Åben saldo for fakturaer udstedt i Rentemester', { exact: true }).locator('..').locator('dd').textContent() === '500,00 kr.', 'Real dated native invoice total reconciles to R32 fixture');
    need(await metrics.getByText('Åbne fakturaer udstedt i Rentemester', { exact: true }).locator('..').locator('dd').textContent() === '2', 'Real dated native invoice count');
    need((await view.locator('section[data-section="liquidity"]').textContent())?.includes('kalendermåneder'), 'Calendar month forecast basis visible');
    need((await view.locator('section[data-section="tax"]').textContent())?.includes('ikke tilgængelig'), 'Missing tax calculation never represented as zero or submitted');
    need((await view.locator('section[data-section="system"]').textContent())?.includes('Seneste verificerede lokale backup'), 'Current owner sees privileged retained backup subsection');
  } finally { await view.close(); }
  for (const query of ['asOf=2025-02-29', 'asOf=2025-10-12&asOf=2025-10-12', 'format=pdf', 'out=/tmp/not-server-authority.html', 'confirm=true']) {
    need((await owner.request.get(base + `/api/companies/${company}/dashboard/export?${query}`)).status() === 400, 'Invalid or caller-path selector refused');
  }
  need((await owner.request.get(base + '/api/companies/unselected/dashboard/export')).status() === 404, 'Unselected company refuses HTML export');
  need((await member.request.get(base + '/api/companies/synthetic-portfolio-shifted/dashboard/export')).status() === 403, 'Current ordinary reviewer cannot export another company');
  const cookie = (await owner.context().cookies(base)).map(c => `${c.name}=${c.value}`).join('; ');
  need(cookie.length > 0, 'Ordinary MFA cookie exists');
  const cookieFile = join(work, `dashboard-${phase}.cookie`), out = join(work, `dashboard-${phase}.html`);
  writeFileSync(cookieFile, cookie, { mode: 0o600, flag: 'wx' });
  const common = ['--server', base, '--cookie-file', cookieFile, '--allow-loopback-http'];
  try {
    const cli = spawnSync(join(work, 'rentemester-rust'), ['dashboard', ...common, '--company', company, '--as-of', asOf, '--out', out], { encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!cli.error && cli.status === 0 && !cli.stdout.includes(cookie) && !cli.stderr.includes(cookie), 'Actual CLI dashboard terminal/privacy contract');
    const cliResult = JSON.parse(cli.stdout), cliBytes = readFileSync(out);
    need(cliResult.ok === true && cliResult.bytes === cliBytes.length && cliResult.asOfDate === asOf && cliResult.opened === false, 'Atomic local HTML output and server date receipt');
    need(stableHtml(cliBytes.toString('utf8')) === stable, 'HTTP and actual CLI artifact bytes agree except explicit generation time');
    const input = [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-dashboard129', version: '1' } } },
      { jsonrpc: '2.0', method: 'notifications/initialized' },
      { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'dashboard_export', arguments: { company, asOf } } },
    ].map(v => JSON.stringify(v)).join('\n') + '\n';
    const rpc = spawnSync(join(work, 'rentemester-mcp'), common, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!rpc.error && rpc.status === 0 && !rpc.stdout.includes(cookie) && !rpc.stderr.includes(cookie), 'Actual MCP dashboard terminal/privacy contract');
    const envelope = rpc.stdout.trim().split('\n').map(v => JSON.parse(v)).find(v => v.id === 2)?.result;
    need(envelope?.isError === false && envelope.structuredContent?.ok === true, 'MCP dashboard success');
    const artifact = envelope.structuredContent.data;
    need(artifact.ok === true && artifact.contentType === headers['content-type'] && artifact.filename === `dashboard-${company}-${asOf}.html` && artifact.asOfDate === asOf, 'MCP exact bounded artifact metadata');
    need(stableHtml(Buffer.from(artifact.bytes).toString('utf8')) === stable, 'HTTP and actual MCP artifact bytes agree except explicit generation time');
    return { qualified: true, ordinaryMfa: true, asOf, company, sectionCount: 12, nativeInvoiceCount: 2, nativeInvoiceTotal: 500, exactHttpCliMcpExceptGeneratedAt: true, normalizedHtmlSha256: hash(stable), rawHttpSha256: hash(html), cliFileSha256: hash(cliBytes), currentCrossCompanyRefused: true, taxUnavailableTruthful: true, installedDesktopQualified: false };
  } finally { unlinkSync(cookieFile); }
}
