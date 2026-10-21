import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, unlinkSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import type { Page } from '@playwright/test';

/** Focused real source import → native invoice → portfolio; prior import refusal/replay evidence is retained separately. */
export function createImportedPortfolioProof(page: Page, base: string, work: string, prior: any) {
  const company = prior?.inboxUnlocked?.target;
  const need = (v: unknown, m: string) => { if (!v) throw Error(m); };
  need(company === 'synthetic-backup-inbox-target', 'Accepted fresh imported company required');
  const prefix = '/api/companies/' + company + '/';
  const reason = 'native_invoices_only_imported_receivables_separate';
  const early = '2025-03-21', late = '2025-04-20';
  const hash = (v: Buffer | string) => createHash('sha256').update(v).digest('hex');
  const fingerprint = (v: unknown) => hash(JSON.stringify(v));
  let started = false, stable: any, report: any, issued: any, sourceReceipt: any;
  const zip = readFileSync(new URL('../../crates/rentemester-storage/src/dinero_import/fixtures/source126-deflated.zip', import.meta.url));
  need(hash(zip) === 'aadd8ceb22d3daf40928d1e2930d3e54a53119677a7060fdca6b31ab8c13e265', 'Exact accepted synthetic ZIP source');
  const originals = new Map<string, string>();
  async function get(path: string) {
    const r = await page.request.get(base + path);
    need(r.ok(), `Imported portfolio GET ${path} HTTP${r.status()}`);
    return r.json();
  }
  async function post(resource: string, data: any, method = 'POST') {
    const r = await page.request.fetch(base + prefix + resource, { method, headers: { origin: base }, data });
    const value = await r.json();
    need(r.ok() && value.ok === true, `Imported portfolio POST ${resource} HTTP${r.status()}: ${JSON.stringify(value)}`);
    return value;
  }
  async function snapshot() {
    return {
      documents: await get(prefix + 'documents'),
      journal: await get(prefix + 'journal?year=2025&source=live'),
      bank: await get(prefix + 'bank'),
      trial: await get(prefix + 'trial-balance?year=2025&source=live'),
      imported: await get(prefix + 'invoices/imported-receivables?asOf=' + late),
      audit: await get(prefix + 'audit-log?limit=5000'),
    };
  }
  async function client(kind: 'cli' | 'mcp', asOf: string, canonical: any) {
    const file = join(work, 'imported-portfolio.cookie');
    const cookies = await page.context().cookies(base);
    need(cookies.some(c => c.httpOnly), 'Normal MFA session required');
    const secret = cookies.map(c => `${c.name}=${c.value}`).join('; ');
    writeFileSync(file, secret, { mode: 0o600, flag: 'wx' });
    try {
      const common = ['--server', base, '--cookie-file', file, '--allow-loopback-http'];
      const args = kind === 'cli' ? ['remote', 'portfolio', ...common, '--as-of', asOf] : common;
      const input = kind === 'mcp' ? [
        { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-imported-portfolio128', version: '1' } } },
        { jsonrpc: '2.0', method: 'notifications/initialized' },
        { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'portfolio', arguments: { asOf } } },
      ].map(v => JSON.stringify(v)).join('\n') + '\n' : '';
      const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
      need(!r.error && r.status === 0 && !r.stdout.includes(secret) && !r.stderr.includes(secret), `Portfolio ${kind} terminal/private contract`);
      const envelope = kind === 'cli' ? JSON.parse(r.stdout) : JSON.parse(r.stdout.trim().split('\n').find(l => JSON.parse(l).id === 2)!).result;
      if (kind === 'mcp') need(envelope.isError === false, 'Portfolio MCP success');
      const value = kind === 'cli' ? envelope : envelope.structuredContent;
      need(value.ok === true && isDeepStrictEqual(value.data, canonical), `Exact imported portfolio ${kind} parity`);
    } finally { unlinkSync(file); }
  }
  async function projection(asOf: string, count: number, total: number, overdue: number) {
    const canonical = await get('/api/portfolio?asOf=' + asOf);
    const p = canonical.portfolio, c = p.companies.find((v: any) => v.slug === company);
    need(c && c.openInvoiceCount === count && c.openInvoiceTotal === total && c.overdueInvoiceCount === overdue, 'Exact native subset at ' + asOf + ': ' + JSON.stringify(c));
    need(c.availability.invoices.status === 'partial' && c.availability.invoices.reason === reason, 'Explicit partial native invoice boundary');
    need(c.limitations.some((v: string) => /importerede/i.test(v)) && p.limitations.some((v: string) => /importerede/i.test(v)), 'Company and aggregate invoice boundary explanation');
    need(p.totals.openInvoiceTotal === total, 'Imported receivables never added to native invoice total');
    for (const kind of ['cli', 'mcp'] as const) await client(kind, asOf, canonical);
    return canonical;
  }
  async function ui(canonical: any) {
    await page.goto(base + '/');
    await page.getByRole('heading', { name: 'Portefølje', exact: true }).waitFor();
    await page.getByLabel('Pr. dato', { exact: true }).fill(late);
    const pending = page.waitForResponse(r => { const u = new URL(r.url()); return u.pathname === '/api/portfolio' && u.searchParams.get('asOf') === late; });
    await page.getByRole('button', { name: 'Hent portefølje igen', exact: true }).click();
    const response = await pending;
    need(response.ok() && isDeepStrictEqual(await response.json(), canonical), 'Actual web renders same dated portfolio');
    const c = canonical.portfolio.companies.find((v: any) => v.slug === company);
    const card = page.getByRole('article', { name: c.name, exact: true });
    await card.getByRole('heading', { name: c.name, exact: true }).waitFor();
    await card.getByText('Åbne fakturaer', { exact: true }).locator('..').locator('dd').filter({ hasText: '1' }).waitFor();
    await card.getByText(/importerede tilgodehavender/i).first().waitFor();
    await page.screenshot({ path: process.env.RM_SCANNER_SCREENSHOT!, fullPage: true });
  }
  async function sourceOriginal() {
    const r = await page.request.get(base + prefix + 'imports/dinero/' + sourceReceipt.attemptId + '/original');
    need(r.ok() && r.headers()['content-type']?.split(';')[0] === 'application/zip' && (await r.body()).equals(zip), 'Exact retained source ZIP before/after restart');
  }
  async function originalFiles() {
    for (const path of ['documents/' + issued.documentId + '/file', 'invoices/' + issued.documentId + '/pdf']) {
      const r = await page.request.get(base + prefix + path);
      need(r.ok(), 'Native issued original available: ' + path);
      const bytes = await r.body(), digest = hash(bytes);
      need(bytes.length > 0, 'Nonempty native original');
      if (originals.has(path)) need(originals.get(path) === digest, 'Exact original bytes preserved');
      else originals.set(path, digest);
    }
  }
  async function audit() {
    const v = await get(prefix + 'audit-verify');
    need(v.audit.available === true && v.audit.ok === true && v.audit.entries === 5 && v.audit.components.length === 5 && v.audit.components.every((c: any) => c.available && c.ok && c.errors.length === 0), 'Full imported and native audit green: ' + JSON.stringify(v));
    return v;
  }
  return {
    async run() {
      need(!started, 'Imported portfolio executes once'); started = true;
      const fresh = await snapshot();
      need(fresh.journal.journal.entries.length === 0 && fresh.documents.documents.documents.length === 1, 'Fresh source target before real import');
      const source = { fileName: 'synthetic-imported-portfolio128.zip', contentBase64: zip.toString('base64') };
      const plan = (await post('imports/dinero/plan', source)).dineroImport;
      need(plan.status === 'ready' && plan.counts.historicalVouchers === 3 && plan.counts.importedInvoices === 2 && plan.controls.length === 1 && plan.controls[0].reconciles === true, 'Reviewed real source plan and debtor control');
      need(isDeepStrictEqual(await snapshot(), fresh), 'Source planning has no effects');
      sourceReceipt = (await post('imports/dinero/apply', { ...source, planHash: plan.planHash, idempotencyKey: 'synthetic-imported-portfolio128-source', confirm: true })).dineroImport;
      need(Number.isSafeInteger(sourceReceipt.attemptId) && sourceReceipt.attemptId > 0, 'Real import accepted');
      await sourceOriginal();
      const importedBefore = (await snapshot()).imported;
      need(importedBefore.importedReceivables.totalOpen === 24500 && importedBefore.importedReceivables.count === 2, 'Exact historical schedule before native issuance');
      const before = await snapshot();
      await projection(late, 0, 0, 0);
      need(isDeepStrictEqual(await snapshot(), before), 'Imported-only report reads have no ledger effects');
      await post('company', { cvr: '87654321', address: 'Synthetic Native Portfolio Street', postalCode: '1000', city: 'Synthetic City', vatPeriodType: 'quarter', paymentTermsDays: 14 }, 'PATCH');
      await post('accounts', { accountNo: '4200', name: 'Synthetic native output VAT', type: 'vat', normalBalance: 'credit' });
      for (const [role, accountNo] of [['debtors', '5520'], ['output_vat', '4200']]) await post('account-roles', { role, accountNo, confirm: true });
      const customer = (await post('customers', { name: 'Synthetic Native Portfolio Buyer', address: 'Synthetic Buyer Street', vatOrCvr: 'DK11223344', paymentTermsDays: 14, defaultCurrency: 'DKK' })).customer;
      const result = await post('invoices/issue', { issueDate: '2025-04-01', customerId: customer.id, vatTreatment: 'standard', lines: [{ description: 'Synthetic native service', quantity: 1, unitPriceExVat: 100, taxClassification: 'taxable', vatRate: .25 }] });
      issued = result.invoice;
      need(Number.isSafeInteger(issued.documentId) && issued.grossAmount === 125, 'Ordinary native gross125 invoice');
      await post('invoices/post', { invoiceDocumentId: issued.documentId, revenueAccountNo: '1000', confirm: true });
      stable = await snapshot();
      need(isDeepStrictEqual(stable.imported, importedBefore), 'Native invoice cannot alter historical imported schedule');
      const zero = await projection(early, 0, 0, 0), current = await projection(late, 1, 125, 1);
      await ui(current); await originalFiles(); await sourceOriginal(); const fullAudit = await audit();
      need(isDeepStrictEqual(await snapshot(), stable), 'All portfolio and original/audit reads preserve company books');
      const r = await page.request.post(base + '/api/workspace/backup', { headers: { origin: base }, data: { confirm: true } });
      need(r.ok() && r.headers()['content-type']?.split(';')[0] === 'application/vnd.rentemester.native-backup+json', 'Signed combined import/native backup');
      const bytes = await r.body(); writeFileSync(join(work, 'imported-portfolio.native-backup.json'), bytes, { mode: 0o600, flag: 'wx' });
      report = { qualified: true, company, sourceReceipt, rawZipSha256: hash(zip), accountingFingerprint: fingerprint(stable), historicalTotal: 24500, nativeTotal: 125, zero, current, fullAudit, originals: Object.fromEntries(originals), signedBackup: { sha256: hash(bytes), bytes: bytes.length }, installedNativeQualified: false, sourceFreeRestoreQualified: false };
      return report;
    },
    async reopen() {
      need(report, 'Completed native portfolio flow');
      need(isDeepStrictEqual(await snapshot(), stable), 'Exact normal MFA restart state');
      need(isDeepStrictEqual(await projection(early, 0, 0, 0), report.zero) && isDeepStrictEqual(await projection(late, 1, 125, 1), report.current), 'Exact dated projections after restart');
      await originalFiles(); await sourceOriginal(); await audit();
      need(isDeepStrictEqual(await snapshot(), stable), 'Restart reports preserve financial state');
      return { qualified: true, exactNormalMfaRestartState: true, accountingFingerprint: fingerprint(stable), exactOriginalsAfterRestart: true };
    },
  };
}
