import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Work-Issue #699. Parent runs actual only after integrated review. No setup retry.
 * readOnly/assertStable/reopen NEVER configure, register, replay or post. Exact known
 * committed intent replay is a separate, finite run-phase check under original identity.
 */
export function createMileagePostingProof(base: string, work: string, onStep: (name: string) => void) {
  const root = '/api/companies/synthetic/', api = root + 'mileage/';
  const cookie = join(work, 'mileage699.cookie'), observations: any[] = [];
  let owner: Page, stable: any, ran = false, source: Buffer, documentId: number, intent: any, preview: any, receipt: any;
  const need = (v: unknown, message: string) => { if (!v) throw Error('mileage_posting: ' + message); };
  const hash = (v: Buffer) => createHash('sha256').update(v).digest('hex');
  function record(v: any) { observations.push(v); writeFileSync(join(work, 'mileage699-observations.json'), JSON.stringify(observations), { mode: 0o600 }); }
  async function http(path: string, data?: any, status = 200, page = owner) {
    onStep((data === undefined ? 'GET:' : 'POST:') + path);
    const r = data === undefined ? await page.request.get(base + path) : await page.request.post(base + path, { headers: { origin: base }, data });
    const text = await r.text(); record({ path, data, status: r.status(), text });
    need(r.status() === status, path + ' HTTP' + r.status() + ' expected ' + status + '; stop, no resend');
    return JSON.parse(text);
  }
  async function cookies() { writeFileSync(cookie, (await owner.context().cookies(base)).map(c => c.name + '=' + c.value).join('; '), { mode: 0o600 }); }
  function client(kind: 'cli' | 'mcp', name: string, body?: any, confirmed = false) {
    onStep(kind + ':' + name);
    const common = ['--server', base, '--allow-loopback-http', '--cookie-file', cookie];
    let argv: string[], input: string | undefined;
    if (kind === 'cli') {
      const file = join(work, 'mileage699-body-' + randomUUID() + '.json');
      if (body !== undefined) writeFileSync(file, JSON.stringify(body), { mode: 0o600, flag: 'wx' });
      argv = [name, ...common, '--company', 'synthetic', ...(body === undefined ? [] : ['--body-file', file]), ...(confirmed ? ['--confirm'] : [])];
    } else {
      argv = common;
      input = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'mileage699', version: '1' } } },
        { jsonrpc: '2.0', method: 'notifications/initialized' }, { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: name.replaceAll('-', '_'), arguments: { company: 'synthetic', ...(body === undefined ? {} : { body: JSON.stringify(body) }), ...(confirmed ? { confirm: true } : {}) } } }].map(v => JSON.stringify(v)).join('\n') + '\n';
    }
    const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), argv, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    record({ kind, name, body, exit: r.status, stdout: r.stdout, stderr: r.stderr, terminalError: r.error?.message });
    need(!r.error && r.status === 0, 'client terminal failure; never retry unknown mutation');
    if (kind === 'cli') { const v = JSON.parse(r.stdout); need(v.ok === true && v.data, 'CLI exact remote envelope'); return v.data; }
    const v = r.stdout.trim().split('\n').map(s => JSON.parse(s)).find(v => v.id === 2);
    const result = v?.result, value = result?.structuredContent;
    need(!v?.error && !result?.isError && value?.ok === true && value.data && equal(JSON.parse(result.content[0].text), value), 'MCP exact structured/text source envelope');
    return value.data;
  }
  // A real minimal, cross-reference-correct PDF containing the actual synthetic claim.
  // Generated only during parent actual execution, never a renamed invoice or fake hash.
  function claimPdf(date: string) {
    const text = `Synthetic mileage claim #699 | ${date} | Synthetic driver | Synthetic private car | Synthetic origin to Synthetic destination | Purpose: synthetic client visit | 12.35 km | Explicit synthetic company rate 2.00 DKK/km | Claim 24.70 DKK | VAT 0 | Not statutory tax advice or payment`;
    const stream = `BT /F1 9 Tf 24 760 Td (${text}) Tj ET\n`;
    const objects = ['<< /Type /Catalog /Pages 2 0 R >>', '<< /Type /Pages /Kids [3 0 R] /Count 1 >>', '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1800 800] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>', '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>', `<< /Length ${Buffer.byteLength(stream)} >>\nstream\n${stream}endstream`];
    let pdf = '%PDF-1.4\n', offsets = [0];
    objects.forEach((o, i) => { offsets.push(Buffer.byteLength(pdf)); pdf += `${i + 1} 0 obj\n${o}\nendobj\n`; });
    const xref = Buffer.byteLength(pdf);
    pdf += `xref\n0 6\n0000000000 65535 f \n` + offsets.slice(1).map(n => String(n).padStart(10, '0') + ' 00000 n \n').join('') + `trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
    return Buffer.from(pdf);
  }
  async function physical() {
    const r = await owner.request.get(base + root + `documents/${documentId}/file`), bytes = await r.body();
    need(r.ok() && bytes.equals(source) && hash(bytes) === preview.documentHash, 'exact immutable actual mileage claim PDF bytes/hash');
  }
  async function snapshot() {
    const v: any = {};
    for (const path of ['documents', 'accounts', 'journal', 'trial-balance', 'vat', 'bank', 'payables?status=all', 'mileage', 'mileage/rate-versions', 'mileage/postings']) v[path] = await http(root + path);
    return v;
  }
  async function parity() {
    for (const kind of ['cli', 'mcp'] as const) {
      for (const [name, path] of [['mileage-rate-versions', 'rate-versions'], ['mileage-postings', 'postings']]) need(equal(client(kind, name), await http(api + path)), 'exact canonical client readback');
      need(equal(client(kind, 'preview-mileage-post', intent), { ok: true, mileagePosting: preview }), 'exact client preview parity');
    }
  }
  async function run(page: Page, reader: Page) {
    need(!ran, 'single bounded run, no replay of setup'); ran = true; owner = page; await cookies();
    const readerIdentity = await http('/api/me', undefined, 200, reader);
    await http('/api/workspace/members/company', { userId: readerIdentity.user.id, companySlug: 'synthetic', action: 'grant', role: 'reader' });
    const mileage = (await http(root + 'mileage')).mileage, date = mileage.periodStart;
    need(/^\d{4}-\d{2}-\d{2}$/.test(date), 'canonical fiscal date');
    for (const [accountNo, type, normalBalance] of [['3699', 'expense', 'debit'], ['7399', 'liability', 'credit']]) await http(root + 'accounts', { accountNo, name: 'Synthetic mileage699 ' + accountNo, type, normalBalance });
    source = claimPdf(date); writeFileSync(join(work, 'synthetic-mileage699-claim.pdf'), source, { mode: 0o600, flag: 'wx' });
    const doc = await http(root + 'documents/ingest', { confirm: true, fileName: 'synthetic-mileage699-claim.pdf', fileBase64: source.toString('base64'), metadata: { source: 'Synthetic mileage claim699', documentType: 'cash_register_receipt', issueDate: date, amountIncVat: 24.70, vatAmount: 0, currency: 'DKK', deliveryDescription: 'Synthetic mileage claim, not supplier invoice: 12.35 km at explicit company rate 2.00 DKK/km' } });
    documentId = doc.document.id; need(Number.isSafeInteger(documentId) && documentId > 0, 'admitted real immutable claim');
    const config = { version: 'synthetic-mileage699-v1', validFrom: date, validTo: mileage.periodEnd, vehicle: 'Synthetic private car', ratePerKm: '2.00', basis: 'Explicit synthetic company reimbursement configuration, not statutory eligibility', source: 'Synthetic mileage claim #699; company decision', expenseAccountNo: '3699', liabilityAccountNo: '7399', confirm: true };
    await http(api + 'rate-versions', config, 403, reader);
    const configured = (await http(api + 'rate-versions', config)).mileagePosting;
    need(configured.replayed === false && configured.rateVersion.version === config.version, 'first configured version receipt');
    const trip = { tripDate: date, purpose: 'Synthetic client visit', fromLocation: 'Synthetic origin', toLocation: 'Synthetic destination', kilometers: 12.35, vehicle: config.vehicle, driver: 'Synthetic driver', ratePerKm: 2, rateBasis: config.basis, rateSource: config.source, notes: 'Synthetic mileage claim #699', confirm: true };
    const registered = (await http(root + 'mileage', trip)).mileage;
    need(registered.amountBasis === 24.70 && Number.isSafeInteger(registered.mileageEntryId), 'exact original log receipt');
    need((await http(root + 'journal')).journal.entries.length === 0, 'registering STILL never posts');
    intent = { mileageEntryId: registered.mileageEntryId, rateVersion: config.version, documentId };
    const before = await snapshot();
    await http(api + 'post-preview', { ...intent, documentId: 2147483647 }, 422);
    preview = (await http(api + 'post-preview', intent)).mileagePosting;
    need(preview.amount === 24.70 && preview.kilometers === 12.35 && preview.ratePerKm === '2.00' && preview.tripDate === date && preview.documentHash === hash(source) && /^[a-f0-9]{64}$/.test(preview.previewHash) && equal(preview.appliedRules, ['NATIVE-MILEAGE-POST-1']), 'exact canonical source-bound preview');
    const invalidPost = { ...intent, expectedPreviewHash: '0'.repeat(64), operationKey: 'synthetic-mileage699-stale', confirm: true };
    await http(api + 'post', invalidPost, 422);
    await http(api + 'post', { ...invalidPost, expectedPreviewHash: preview.previewHash }, 403, reader);
    await parity(); need(equal(await snapshot(), before), 'preview/client/refusals cause no business effects');
    await page.goto(base + '/companies/synthetic/koersel');
    const panel = page.getByRole('region', { name: 'Kørselsbogføring', exact: true }); await panel.waitFor();
    // These human controls must be reviewed against the integrated panel, never bypassed with HTTP posting.
    await panel.getByLabel('Kørsel', { exact: true }).selectOption(String(intent.mileageEntryId));
    await panel.getByLabel('Takstversion', { exact: true }).selectOption(config.version);
    await panel.getByLabel('Bilags-id', { exact: true }).fill(String(documentId));
    const previewResponse = page.waitForResponse(r => r.url() === base + api + 'post-preview' && r.request().method() === 'POST');
    await panel.getByRole('button', { name: 'Forhåndsvis bogføring', exact: true }).click();
    const uiPreview = await previewResponse; record({ uiPreview: await uiPreview.text(), status: uiPreview.status() });
    need(uiPreview.ok() && equal((await uiPreview.json()).mileagePosting, preview), 'shared UI exact server preview');
    need((await panel.innerText()).includes('3699') && (await panel.innerText()).includes('7399') && /24[,.]70/.test(await panel.innerText()), 'human amount and account review');
    await panel.getByRole('button', { name: 'Bogfør kørsel', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Bogfør kørsel', exact: true }); await dialog.waitFor();
    const pending = page.waitForResponse(r => r.url() === base + api + 'post' && r.request().method() === 'POST');
    await dialog.getByRole('button', { name: 'Bekræft bogføring', exact: true }).click();
    const posted = await pending, body = posted.request().postDataJSON(), text = await posted.text();
    record({ uiPost: { body, status: posted.status(), text } });
    need(posted.ok(), 'one explicit UI post failed; preserve profile, read canonical postings, NEVER resend');
    receipt = JSON.parse(text).mileagePosting;
    need(equal(body, { ...intent, expectedPreviewHash: preview.previewHash, operationKey: body.operationKey, confirm: true }) && typeof body.operationKey === 'string' && body.operationKey.length > 0, 'exact confirmed UI original intent');
    need(equal(receipt, { ...preview, journalEntryId: receipt.journalEntryId, journalEntryNo: receipt.journalEntryNo, replayed: false }) && Number.isSafeInteger(receipt.journalEntryId) && receipt.journalEntryId > 0 && !!receipt.journalEntryNo, 'exact durable first result');
    stable = await snapshot();
    const entries = stable.journal.journal.entries;
    need(entries.length === 1 && entries[0].id === receipt.journalEntryId && entries[0].documentId === documentId && equal(entries[0].lines.map((l: any) => [l.accountNo, l.debit, l.credit]).sort(), [['3699', 24.70, 0], ['7399', 0, 24.70]]), 'one journal/two exact expense liability lines');
    need(stable.vat.vat.inputVat === 0 && stable.vat.vat.payable === 0 && equal(stable['mileage/postings'], { ok: true, mileagePosting: { postings: [receipt] } }), 'no VAT and exact canonical receipt list');
    // Only known-committed exact intent; finite once per client, never an unknown-outcome retry.
    for (const kind of ['cli', 'mcp'] as const) need(equal(client(kind, 'post-mileage', body, true), { ok: true, mileagePosting: { ...receipt, replayed: true } }), 'explicit exact original-principal receipt replay');
    await http(api + 'post', { ...body, operationKey: 'different-intent699' }, 422);
    await http(api + 'post', body, 403, reader);
    await dialog.waitFor({ state: 'hidden' });
    await panel.getByText(receipt.journalEntryNo, { exact: false }).waitFor();
    await page.screenshot({ path: process.env.RM_BATCH_SCREENSHOT, fullPage: true });
    await readOnly();
    return { config, intent, preview, receipt, originalSha256: hash(source), noAutomaticWriteRetry: true, journals: 1, lines: 2, amount: 24.70, actualLimitations: ['SDK is exercised transitively by native CLI/MCP, no independent SDK executable', 'closed NEW-post refusal and fault/concurrency matrix require targeted component evidence; not qualified by this one-trip actual', 'retained original-accrual169 and mileage171 acceptance unchanged, not rerun'] };
  }
  async function readOnly() { const before = await snapshot(); need(equal(before, stable), 'exact immutable mileage/books/receipt snapshot'); await parity(); await physical(); need(equal(await snapshot(), before), 'read-only callbacks never write or replay'); return { readOnly: true, noMutationReplay: true }; }
  async function assertStable(page: Page) { owner = page; need(equal(await snapshot(), stable), 'exact restart/source-free restore snapshot under current identity'); await physical(); }
  async function reopen(page: Page) { await assertStable(page); await cookies(); await readOnly(); return { normalMfaReadback: true, noMutationReplay: true }; }
  return { run, readOnly, assertStable, reopen };
}
