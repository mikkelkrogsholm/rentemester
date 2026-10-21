import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, readdirSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

type Kind = 'cli' | 'mcp';
type Invoice = { id: number; number: string; currency: 'DKK' | 'EUR' };
type Operation = 'calculate' | 'register' | 'post' | 'claims' | 'settle' | 'audit' | 'original' | 'pdf';
const operations: Record<Operation, [string, string]> = {
  calculate: ['calculate-invoice-compensation', 'invoice_compensation_calc'],
  register: ['register-invoice-compensation', 'invoice_claim_compensation'],
  post: ['post-invoice-compensation', 'invoice_post_compensation'],
  claims: ['invoice-reminder-claims', 'invoice_reminder_claims'],
  settle: ['settle-invoice-claims', 'settle_invoice_claims'],
  audit: ['audit-verify', 'audit_verify'],
  original: ['document-original', 'document_original'],
  pdf: ['invoice-pdf', 'invoice_pdf'],
};

/** Actual public adapters only. This helper never seeds or reads business/auth SQL. */
export function createInvoiceCompensationProof(page: Page, base: string, work: string, memberPage: Page | undefined, serviceRunResult: any) {
  const prefix = '/api/companies/synthetic/';
  const need = (value: unknown, message: string) => { if (!value) throw Error(message); };
  const hash = (bytes: Buffer | string) => createHash('sha256').update(bytes).digest('hex');
  const fingerprint = (value: unknown) => hash(JSON.stringify(value));
  const ownerCookie = join(work, 'compensation-owner.cookie'), memberCookie = join(work, 'compensation-member.cookie');
  const account = { debtor: '1210', claim: '1230', revenue: '3210', compensation: '3230', outputVat: '4210', inputVat: '4220', creditor: '2210', gain: '3220', loss: '6220', bank: '1711' };
  let today = '', issueDate = '', dueDate = '', previousDate = '', completed = false, started = false;
  let stable: any, reports: any, terminalCalcs: any, terminalClaims: any;
  const invoices: Invoice[] = [], originalBytes = new Map<string, Buffer>(), bankBytes = new Map<string, Buffer>();
  const steps: Array<{ name: string; fingerprint: string }> = [];
  const refusals: Array<{ label: string; status: number; code: string; canonicalFingerprint: string }> = [];
  const endpoint = (action: string) => base + prefix + 'invoices/compensation/' + action;
  async function cookies(who: Page, file: string, fresh: boolean) {
    const all = await who.context().cookies(base); need(all.some(c => c.httpOnly), 'Normal MFA HttpOnly cookie required');
    writeFileSync(file, all.map(c => `${c.name}=${c.value}`).join('; ') + '\n', { mode: 0o600, flag: fresh ? 'wx' : 'w' });
  }
  async function get(resource: string, who = page) {
    const r = await who.request.get(base + prefix + resource); need(r.ok(), `Compensation canonical ${resource} HTTP${r.status()}`); return r.json();
  }
  async function post(resource: string, body: unknown, who = page, method = 'POST') {
    const r = await who.request.fetch(base + prefix + resource, { method, headers: { origin: base }, data: body });
    const value = await r.json(); need(r.ok() && value.ok === true, `Compensation ${resource} HTTP${r.status()}: ${JSON.stringify(value)}`); return value;
  }
  function client(kind: Kind, op: Operation, body?: unknown, options: { id?: number; member?: boolean; success?: boolean; confirmed?: boolean; usage?: boolean } = {}) {
    const write = ['register', 'post', 'settle'].includes(op), confirmed = options.confirmed ?? write;
    const common = ['--server', base, '--cookie-file', options.member ? memberCookie : ownerCookie, '--allow-loopback-http'];
    const selected = options.id === undefined ? {} : { id: options.id };
    const args = kind === 'cli' ? ['remote', operations[op][0], ...common, '--company', 'synthetic', ...(options.id === undefined ? [] : ['--id', String(options.id)]), ...(confirmed ? ['--confirm'] : [])] : common;
    const input = kind === 'cli' ? body === undefined ? '' : JSON.stringify(body) : [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-compensation-proof120', version: '1' } } },
      { jsonrpc: '2.0', method: 'notifications/initialized' },
      { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: operations[op][1], arguments: { company: 'synthetic', ...selected, ...(body === undefined ? {} : { body: JSON.stringify(body) }), ...(write ? { confirm: confirmed } : {}) } } },
    ].map(v => JSON.stringify(v)).join('\n') + '\n';
    const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!r.error, `Compensation ${kind} subprocess failed`);
    for (const file of [ownerCookie, memberCookie]) if (existsSync(file)) need(!r.stdout.includes(readFileSync(file, 'utf8').trim()) && !r.stderr.includes(readFileSync(file, 'utf8').trim()), 'Private cookie escaped client output');
    if (kind === 'cli') {
      const value = JSON.parse(r.stdout);
      if (options.success === false) { need(r.status === (options.usage ? 2 : 1) && value.ok === false && (options.usage || value.code === 'server_refused'), `Compensation CLI refusal contract ${op}: ${r.status}`); return { exit: r.status, code: value.code }; }
      need(r.status === 0 && value.ok === true && value.data?.ok === true, `Compensation CLI success envelope ${op}`); return value.data;
    }
    need(r.status === 0, 'Compensation MCP terminal status');
    const reply = r.stdout.trim().split('\n').map(line => JSON.parse(line)).find(v => v.id === 2);
    if (options.success === false) { need(!reply?.error && reply?.result?.isError === true && (options.usage || JSON.stringify(reply.result).includes('server_refused')), 'Compensation MCP expected refusal'); return { isError: true }; }
    need(!reply?.error && reply?.result?.isError === false && reply.result.structuredContent?.data?.ok === true, `Compensation MCP success envelope ${op}`); return reply.result.structuredContent.data;
  }
  const calcBody = (invoice: Invoice) => ({ invoiceDocumentId: invoice.id, asOfDate: today });
  const registerBody = (invoice: Invoice) => ({ ...calcBody(invoice), compensationAmountDkk: 310, note: 'Synthetic separately reviewed compensation', confirm: true });
  const postingBody = (invoice: Invoice) => ({ invoiceDocumentId: invoice.id, transactionDate: today, receivableAccountNo: account.claim, compensationIncomeAccountNo: account.compensation, confirm: true });
  async function calculate(invoice: Invoice, who = page) {
    const r = await who.request.post(endpoint('calculate'), { headers: { origin: base }, data: calcBody(invoice) }); need(r.ok(), 'Canonical compensation calculation refused'); return r.json();
  }
  async function parity(invoice: Invoice) {
    const before = await snapshot(), value = await calculate(invoice);
    for (const kind of ['cli', 'mcp'] as const) need(isDeepStrictEqual(client(kind, 'calculate', calcBody(invoice)), value), `${kind} exact compensation calculation differs`);
    const claims = await get(`invoices/${invoice.id}/reminder-claims`);
    for (const kind of ['cli', 'mcp'] as const) need(isDeepStrictEqual(client(kind, 'claims', undefined, { id: invoice.id }), claims), `${kind} exact compensation/receipt status differs`);
    const c = value.compensation;
    need(c.invoiceDocumentId === invoice.id && c.invoiceNumber === invoice.number && c.principalCurrency === invoice.currency && c.compensationAmountDkk === (c.eligible ? 310 : 0) && (c.claimId === null || c.registeredAmountDkk === 310) && ['claimId', 'claimDate', 'registeredAmountDkk', 'postedJournalEntryId', 'postedJournalEntryNo'].every(key => key in c), 'Complete compensation identification/currency/readback required');
    need(claims.reminderClaims.principalCurrency === invoice.currency && claims.reminderClaims.claimCurrency === 'DKK' && (invoice.currency === 'DKK' || claims.reminderClaims.claimOpenBalance === null), 'Principal and DKK claim units must remain separate');
    await unchanged(before, 'Read-only cross-client calculation/status'); return { calculation: value, claims };
  }
  async function snapshot() {
    return { documents: await get('documents'), invoices: await get('invoices'), journal: await get('journal'), bank: await get('bank'), trial: await get('trial-balance'), balance: await get('balance'), income: await get('income-statement'), vat: await get('vat'), audit: await get('audit-log?limit=5000') };
  }
  async function unchanged(before: any, label: string) { need(isDeepStrictEqual(await snapshot(), before), label + ' changed canonical documents/bank/books/company audit'); }
  async function denied(action: string, body: unknown, before: any, label: string, who = page, code?: string) {
    const r = await who.request.post(endpoint(action), { headers: { origin: base }, data: body }); const value = await r.json();
    need([400, 403, 409, 422, 423].includes(r.status()) && value.ok === false && typeof value.code === 'string' && (!code || value.code === code), `${label} must be a definite server refusal: HTTP${r.status()}`);
    await unchanged(before, label); refusals.push({ label, status: r.status(), code: value.code, canonicalFingerprint: fingerprint(before) }); return { status: r.status(), code: value.code };
  }
  async function originals() {
    for (const invoice of invoices) for (const [kind, route] of [['original', `documents/${invoice.id}/file`], ['pdf', `invoices/${invoice.id}/pdf`]] as const) {
      const r = await page.request.get(base + prefix + route); need(r.ok(), 'Immutable invoice original/PDF read required'); const bytes = await r.body(), key = `${invoice.id}:${kind}`;
      need(bytes.length > 0 && (kind !== 'pdf' || bytes.subarray(0, 5).toString() === '%PDF-'), 'Actual nonempty issued JSON/PDF required');
      if (originalBytes.has(key)) need(originalBytes.get(key)!.equals(bytes), 'Invoice original/PDF changed'); else originalBytes.set(key, bytes);
      for (const clientKind of ['cli', 'mcp'] as const) {
        const value = client(clientKind, kind, undefined, { id: invoice.id }); const binary = value[kind === 'pdf' ? 'pdf' : 'original'];
        need(binary && Buffer.from(binary.bytes).equals(bytes), 'Exact original bytes differ through native adapter');
      }
    }
    const directory = join(work, 'workspace', 'synthetic', 'bank', 'processed'); need(!lstatSync(directory).isSymbolicLink(), 'Owned bank-original directory required');
    const names = readdirSync(directory).sort();
    if (bankBytes.size) need(isDeepStrictEqual(names, [...bankBytes.keys()].sort()), 'Bank-original inventory changed');
    for (const name of names) { const file = join(directory, name), info = lstatSync(file); need(info.isFile() && !info.isSymbolicLink() && info.nlink === 1, 'Owned regular bank original required'); const bytes = readFileSync(file); if (bankBytes.has(name)) need(bankBytes.get(name)!.equals(bytes), 'Bank source bytes changed'); else bankBytes.set(name, bytes); }
  }
  async function audit() {
    const before = await snapshot(), value = await get('audit-verify');
    need(value.ok === true && value.audit?.format === 'rentemester-native-audit-verification/1' && value.audit.available === true && value.audit.ok === true && value.audit.errors.length === 0 && value.audit.entries === 9, 'Actual DKK+EUR compensation ledger must pass full audit119: ' + JSON.stringify(value));
    need(isDeepStrictEqual(value.audit.components.map((c: any) => c.name), ['ledger', 'bank', 'invoices', 'originals', 'physical_schema']) && value.audit.components.every((c: any) => c.available && c.ok && c.errors.length === 0), 'All five full-audit components required');
    for (const kind of ['cli', 'mcp'] as const) need(isDeepStrictEqual(client(kind, 'audit'), value), 'Full audit HTTP/CLI/MCP parity');
    await unchanged(before, 'Full audit inspection'); return value;
  }
  async function openClaims(invoice: Invoice) {
    await page.goto(base + '/companies/synthetic/fakturaer');
    const row = page.getByRole('row').filter({ hasText: invoice.number });
    await row.getByRole('button', { name: 'Rykkergebyrer og krav', exact: true }).click();
    const card = page.getByRole('region', { name: 'Rykkergebyrer og krav', exact: true });
    await card.getByRole('region', { name: 'Kompensationskrav', exact: true }).waitFor(); return card;
  }
  async function uiCalculate(invoice: Invoice) {
    const card = await openClaims(invoice), compensation = card.getByRole('region', { name: 'Kompensationskrav', exact: true });
    await compensation.getByLabel('Opgørelsesdato for kompensation', { exact: true }).fill(today);
    await compensation.getByLabel('Kompensation i DKK', { exact: true }).fill('310');
    const pending = page.waitForResponse(r => r.url() === endpoint('calculate') && r.request().method() === 'POST');
    await compensation.getByRole('button', { name: 'Beregn kompensation', exact: true }).click(); const response = await pending;
    need(response.ok() && isDeepStrictEqual(await response.json(), await calculate(invoice)), 'Actual web calculation exact HTTP parity');
    await compensation.getByText(/hovedstol/).filter({ hasText: invoice.currency === 'EUR' ? /EUR/ : /kr\./ }).waitFor(); return { card, compensation };
  }
  async function uiRegister(invoice: Invoice) {
    const { compensation } = await uiCalculate(invoice);
    await compensation.getByRole('button', { name: 'Gennemgå registrering af kompensation', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Registrér kompensationskrav', exact: true });
    const pending = page.waitForResponse(r => r.url() === endpoint('register') && r.request().method() === 'POST');
    await dialog.getByRole('button', { name: 'Registrér kompensation', exact: true }).click(); const response = await pending;
    need(response.ok() && (await response.json()).compensation.claimId > 0 && response.request().postDataJSON().confirm === true, 'Actual DKK UI separate claim registration');
    await dialog.waitFor({ state: 'hidden' });
  }
  async function uiPost(invoice: Invoice) {
    const { compensation } = await uiCalculate(invoice);
    await compensation.getByLabel('Bogføringsdato for kompensation', { exact: true }).fill(today);
    await compensation.getByLabel('Konto til kompensationstilgodehavende', { exact: true }).selectOption(account.claim);
    await compensation.getByLabel('Konto til kompensationsindtægt', { exact: true }).selectOption(account.compensation);
    await compensation.getByRole('button', { name: 'Gennemgå bogføring af kompensation', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Bogfør kompensationskrav', exact: true });
    const pending = page.waitForResponse(r => r.url() === endpoint('post') && r.request().method() === 'POST');
    await dialog.getByRole('button', { name: 'Bogfør kompensation', exact: true }).click(); const response = await pending;
    need(response.ok() && (await response.json()).compensation.postedJournalEntryId > 0 && isDeepStrictEqual(response.request().postDataJSON(), postingBody(invoice)), 'Actual DKK UI explicit-account compensation posting');
    await dialog.waitFor({ state: 'hidden' });
  }
  async function uiPrincipal(invoice: Invoice, reference: string) {
    await page.goto(base + '/companies/synthetic/fakturaer'); await page.getByRole('row').filter({ hasText: invoice.number }).getByRole('button', { name: 'Afstem', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Afstem faktura mod bankbetaling', exact: true }); await dialog.getByLabel('Bankreference', { exact: true }).fill(reference);
    const pending = page.waitForResponse(r => r.url() === base + prefix + 'invoices/settle' && r.request().method() === 'POST');
    await dialog.getByRole('button', { name: 'Afstem faktura', exact: true }).click(); need((await pending).ok(), 'Actual normal DKK principal settlement UI'); await dialog.waitFor({ state: 'hidden' });
  }
  async function uiClaimReceipt(invoice: Invoice, bankId: number) {
    const card = await openClaims(invoice); await card.getByLabel('Faktisk indbetaling på gebyrkrav', { exact: true }).selectOption(String(bankId));
    await card.getByRole('button', { name: 'Gennemgå indbetaling på gebyrkrav', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Afstem indbetaling på gebyrkrav', exact: true });
    const pending = page.waitForResponse(r => r.url() === base + prefix + 'invoices/claims/settle' && r.request().method() === 'POST');
    await dialog.getByRole('button', { name: 'Afstem gebyrindbetaling', exact: true }).click(); const response = await pending;
    need(response.ok() && isDeepStrictEqual(response.request().postDataJSON(), { invoiceDocumentId: invoice.id, bankTransactionId: bankId, confirm: true }), 'Actual DKK claim receipt UI uses one exact real bank source'); await dialog.waitFor({ state: 'hidden' });
  }
  async function setup() {
    today = (await get('scheduled-recurring')).scheduledRecurring.asOfDate;
    need(/^\d{4}-\d{2}-\d{2}$/.test(today), 'Trusted server date required');
    const ago = (n: number) => { const d = new Date(today + 'T12:00:00Z'); d.setUTCDate(d.getUTCDate() - n); return d.toISOString().slice(0, 10); };
    issueDate = ago(6); dueDate = ago(3); previousDate = ago(1);
    need(issueDate.slice(0, 4) === today.slice(0, 4), 'Synthetic proof requires one current fiscal year');
    await post('company', { cvr: '87654321', address: 'Synthetic Compensation Street 120', postalCode: '1000', city: 'Synthetic City', vatPeriodType: 'quarter', paymentTermsDays: 1 }, page, 'PATCH');
    for (const [accountNo, name, type, normalBalance] of [
      [account.debtor, 'Synthetic invoice receivable', 'asset', 'debit'], [account.claim, 'Synthetic compensation receivable', 'asset', 'debit'],
      [account.revenue, 'Synthetic service revenue', 'income', 'credit'], [account.compensation, 'Synthetic compensation income', 'income', 'credit'],
      [account.outputVat, 'Synthetic output VAT', 'vat', 'credit'], [account.inputVat, 'Synthetic input VAT', 'vat', 'debit'],
      [account.creditor, 'Synthetic creditors', 'liability', 'credit'], [account.gain, 'Synthetic FX gain', 'income', 'credit'], [account.loss, 'Synthetic FX loss', 'expense', 'debit'],
    ]) await post('accounts', { accountNo, name, type, normalBalance });
    for (const [role, accountNo] of [['debtors', account.debtor], ['bank', account.bank], ['creditors', account.creditor], ['output_vat', account.outputVat], ['input_vat', account.inputVat], ['fx_gain', account.gain], ['fx_loss', account.loss]]) await post('account-roles', { role, accountNo, confirm: true });
    await post('bank-accounts', { name: 'Synthetic compensation EUR bank', slug: 'synthetic-compensation-eur', currency: 'EUR', ledgerAccountNo: account.bank });
    for (const currency of ['DKK', 'EUR'] as const) {
      const input = { issueDate, deliveryDate: issueDate, dueDate, currency, ...(currency === 'EUR' ? { fxRateToDkk: 7.45 } : {}), vatRatePercent: 25,
        seller: { name: 'Synthetic Document Scanner Company', address: 'Synthetic Compensation Street 120', vatOrCvr: 'DK87654321' }, buyer: { name: 'Synthetic Commercial Buyer ' + currency, address: 'Synthetic Buyer Street 120', vatOrCvr: 'DK12345678' },
        lines: [{ description: 'Synthetic compensation service ' + currency, quantity: 1, unitPriceExVat: 80 }] };
      const invoice = (await post('invoices/issue', input)).invoice;
      need(invoice.documentId > 0 && invoice.grossAmount === 100 && (currency === 'DKK' || invoice.grossAmountDkk === 745), 'Issued real commercial gross100 invoice with frozen DKK valuation');
      const row = { id: invoice.documentId, number: invoice.invoiceNumber, currency }; invoices.push(row);
      await post('invoices/post', { invoiceDocumentId: row.id, revenueAccountNo: account.revenue, confirm: true });
    }
    const dkk = `date;text;amount;currency;reference\n${today};Synthetic DKK principal;100;DKK;SYN120-DKK-PRINCIPAL\n${today};Synthetic DKK compensation receipt;310;DKK;SYN120-DKK-CLAIM\n${today};Synthetic EUR invoice DKK compensation receipt;310;DKK;SYN120-EUR-CLAIM\n`;
    const eur = `date;text;amount;currency;reference;amount_dkk;fx_rate_to_dkk\n${today};Synthetic EUR principal;100;EUR;SYN120-EUR-PRINCIPAL;745;7.45\n`;
    for (const [csvContent, bankAccount, count] of [[dkk, 'synthetic-backup-lock', 3], [eur, 'synthetic-compensation-eur', 1]] as const) {
      const imported = (await post('bank/import', { account: bankAccount, csvContent, confirm: true })).import;
      need(imported.imported === count && imported.sourceFileHash === hash(csvContent), 'Exact synthetic cash sources imported through public API');
    }
    need((await get('journal')).journal.entries.length === 3 && (await get('bank')).bank.transactions.length === 5, 'Exactly baseline plus two invoice journals and four real bank rows');
  }
  async function permissions(invoice: Invoice) {
    need(memberPage, 'Existing independent normal-MFA member required');
    const me = await memberPage!.request.get(base + '/api/me'); need(me.ok(), 'Current independent MFA member identity required'); const id = (await me.json()).user.id;
    const members = await page.request.get(base + '/api/workspace/members'); need(members.ok(), 'Owner current membership inventory required');
    const previous = (await members.json()).members.find((m: any) => m.userId === id)?.memberships.find((m: any) => m.companySlug === 'synthetic')?.role;
    const change = async (action: string, role?: string) => { const r = await page.request.post(base + '/api/workspace/members/company', { headers: { origin: base }, data: { userId: id, companySlug: 'synthetic', action, ...(role ? { role } : {}) } }); need(r.ok(), 'Current owner membership change refused'); };
    const before = await snapshot();
    try {
      await change('grant', 'reader'); need(isDeepStrictEqual(await calculate(invoice, memberPage), await calculate(invoice)), 'Current reader grant must admit calculation');
      await denied('register', registerBody(invoice), before, 'Current reader cannot register', memberPage!, 'access_denied');
      for (const kind of ['cli', 'mcp'] as const) { client(kind, 'register', registerBody(invoice), { member: true, success: false }); await unchanged(before, 'Reader client registration refusal'); }
      await change('revoke'); await denied('calculate', calcBody(invoice), before, 'Revoked calculation', memberPage!, 'access_denied');
      for (const kind of ['cli', 'mcp'] as const) { client(kind, 'calculate', calcBody(invoice), { member: true, success: false }); await unchanged(before, 'Revoked client calculation'); }
    } finally { await change(previous ? 'grant' : 'revoke', previous); }
    return { currentReaderGrant: true, draftWriteDeniedAcrossHttpCliMcp: true, currentRevocationAcrossHttpCliMcp: true, noCompanyAuditOrFinancialEffects: true };
  }
  async function backupNegative(invoice: Invoice) {
    const sha = serviceRunResult?.artifact?.sha256; need(/^[a-f0-9]{64}$/.test(sha) && serviceRunResult.artifact.independentlyVerified === true, 'Prior owned qualified backup artifact required');
    const directory = join(work, 'workspace', 'native-backups'); need(!lstatSync(directory).isSymbolicLink(), 'Owned retained backup directory required');
    const file = join(directory, sha + '.native-backup.json'), parked = file + '.compensation120-held', info = lstatSync(file), bytes = readFileSync(file);
    need(info.isFile() && !info.isSymbolicLink() && info.nlink === 1 && info.uid === process.getuid!() && (info.mode & 0o777) === 0o600 && hash(bytes) === sha && bytes.equals(readFileSync(join(work, 'backup-lock-ui.native-backup.json'))) && !existsSync(parked), 'Exact owned retained prior artifact required, no caller path');
    const before = await snapshot(); let refusal: { status: number; code: string } | undefined;
    try {
      renameSync(file, parked); const status = await get('backup-status'); need(status.lock.enforced === true && status.lock.locked === true, 'Missing current retained backup must lock actual compensation posting');
      refusal = await denied('post', postingBody(invoice), before, 'Current backup lock on compensation', page, 'backup_overdue');
    } finally { need(!existsSync(file) && existsSync(parked), 'Never overwrite another retained artifact'); renameSync(parked, file); }
    need(readFileSync(file).equals(bytes) && (await get('backup-status')).lock.locked === false, 'Exact backup bytes restored and qualified before one real post'); await unchanged(before, 'Backup file park/refusal/restore');
    return { refusal, exactArtifactSha256: sha, parkedOnlyOwnedRetainedArtifact: true, exactBytesRestoredBeforePosting: true };
  }
  async function stage(invoice: Invoice, name: string) { const value = await parity(invoice); steps.push({ name: `${invoice.currency}:${name}`, fingerprint: fingerprint(value) }); return value; }
  return {
    async run() {
      need(!started, 'Compensation proof executes once'); started = true;
      await cookies(page, ownerCookie, true); need(memberPage, 'Normal MFA member page required'); await cookies(memberPage!, memberCookie, true);
      const baseline = await snapshot(); need(baseline.journal.journal.entries.length === 1 && baseline.bank.bank.transactions.length === 1, 'Accepted backup-lock single-post baseline required');
      await setup(); const [dkk, eur] = invoices;
      for (const invoice of invoices) { const first = await stage(invoice, 'eligible'); need(first.calculation.compensation.eligible === true && first.calculation.compensation.claimId === null && first.claims.reminderClaims.principalOpenBalance === 100, 'Both commercial overdue principals eligible without a claim'); }
      const rights = await permissions(dkk), before = await snapshot();
      const notDue = await page.request.post(endpoint('calculate'), { headers: { origin: base }, data: { invoiceDocumentId: dkk.id, asOfDate: dueDate } }); need(notDue.ok() && (await notDue.json()).compensation.eligible === false, 'Due-date itself is not overdue');
      await denied('register', { ...registerBody(dkk), compensationAmountDkk: 310.01 }, before, 'Overmaximum registration');
      await denied('register', { ...registerBody(dkk), confirm: false }, before, 'Missing inner confirmation');
      for (const kind of ['cli', 'mcp'] as const) { client(kind, 'register', registerBody(dkk), { success: false, confirmed: false, usage: true }); await unchanged(before, 'Missing outer confirmation'); }
      await unchanged(before, 'Not-due pure calculation');
      await uiRegister(dkk); const registered = await stage(dkk, 'registered-not-posted');
      need(registered.claims.reminderClaims.pendingUnpostedCompensation === true && registered.calculation.compensation.postedJournalEntryId === null && (await get('journal')).journal.entries.length === 3, 'Registration must create no journal');
      const registeredBefore = await snapshot();
      await denied('register', registerBody(dkk), registeredBefore, 'Duplicate registration');
      for (const kind of ['cli', 'mcp'] as const) { client(kind, 'register', registerBody(dkk), { success: false }); await unchanged(registeredBefore, 'Duplicate client registration'); }
      await denied('post', { ...postingBody(dkk), receivableAccountNo: '8711' }, registeredBefore, 'Incompatible compensation account');
      await denied('post', { ...postingBody(dkk), transactionDate: previousDate }, registeredBefore, 'Compensation posting before claim date');
      const backup = await backupNegative(dkk); await uiPost(dkk); const dkkPosted = await stage(dkk, 'posted');
      need(dkkPosted.calculation.compensation.postedJournalEntryId > 0 && dkkPosted.claims.reminderClaims.ledgerBackedClaimBalance === 310 && dkkPosted.claims.reminderClaims.principalOpenBalance === 100, 'DKK post must add only exact separate claim');
      client('cli', 'register', registerBody(eur)); const eurRegistered = await stage(eur, 'registered-not-posted'); need(eurRegistered.calculation.compensation.claimId > 0 && eurRegistered.calculation.compensation.postedJournalEntryId === null, 'Actual EUR CLI registration must stay separate');
      client('mcp', 'post', postingBody(eur)); const eurPosted = await stage(eur, 'posted'); need(eurPosted.calculation.compensation.postedJournalEntryId > 0 && eurPosted.claims.reminderClaims.ledgerBackedClaimBalance === 310 && eurPosted.claims.reminderClaims.principalOpenBalance === 100, 'Actual EUR MCP post remains DKK310 versus EUR100');
      const bank = (await get('bank')).bank.transactions, bankId = (reference: string) => { const row = bank.find((b: any) => b.reference === reference); need(row?.id > 0, 'Actual referenced bank source required'); return row.id as number; };
      await uiPrincipal(dkk, 'SYN120-DKK-PRINCIPAL');
      const principal = (await post('invoices/settle', { invoiceDocumentId: eur.id, bankTransactionReference: 'SYN120-EUR-PRINCIPAL', paymentDate: today, confirm: true })).settlement;
      need(principal.principalAmountDkk === 745 && principal.bankAmountDkk === 745, 'EUR100 principal clears at frozen/actual DKK745 without claim mixing');
      for (const invoice of invoices) { const paid = await stage(invoice, 'principal-paid-claim-open'); need(paid.claims.reminderClaims.principalOpenBalance === 0 && paid.claims.reminderClaims.ledgerBackedClaimBalance === 310 && paid.claims.reminderClaims.settlementAvailable === true && paid.calculation.compensation.eligible === false, 'Principal paid independently; DKK claim remains collectible'); }
      await uiClaimReceipt(dkk, bankId('SYN120-DKK-CLAIM'));
      client('mcp', 'settle', { invoiceDocumentId: eur.id, bankTransactionId: bankId('SYN120-EUR-CLAIM'), confirm: true });
      terminalCalcs = {}; terminalClaims = {};
      for (const invoice of invoices) {
        const final = await stage(invoice, 'principal-and-claim-paid'), claims = final.claims.reminderClaims;
        need(claims.principalOpenBalance === 0 && claims.ledgerBackedClaimBalance === 0 && claims.registeredClaimBalanceDkk === 0 && claims.totalCompensationDkk === 310 && claims.totalClaimPayments === 310 && claims.totalReminderFees === 0 && claims.pendingUnpostedCompensation === false && claims.compensations.length === 1 && claims.claimPayments.length === 1, 'One exact posted and settled DKK compensation per invoice');
        need(claims.claimPayments[0].bankTransactionId === bankId(invoice.currency === 'DKK' ? 'SYN120-DKK-CLAIM' : 'SYN120-EUR-CLAIM') && claims.claimPayments[0].amount === 310 && claims.claimPayments[0].journalEntryId > 0 && claims.compensations[0].claimId === final.calculation.compensation.claimId && claims.compensations[0].journalEntryId === final.calculation.compensation.postedJournalEntryId, 'Exact requested bank receipt and canonical compensation/journal identities must agree');
        terminalCalcs[invoice.id] = final.calculation; terminalClaims[invoice.id] = final.claims;
      }
      const finalBefore = await snapshot();
      for (const invoice of invoices) { await denied('post', postingBody(invoice), finalBefore, 'Duplicate compensation posting'); await denied('register', registerBody(invoice), finalBefore, 'Paid invoice cannot create another claim'); }
      await denied('post', { ...postingBody(dkk), confirm: false }, finalBefore, 'Missing posting confirmation');
      // No unknown write is retried; every accepted financial action occurs once.
      stable = await snapshot(); need(stable.journal.journal.entries.length === 9 && stable.bank.bank.transactions.length === 5 && stable.trial.trialBalance.balanced === true, 'Exactly nine journal effects and five real bank rows');
      const cents = (value: number | string) => { const text = String(value); need(/^-?\d+(\.\d{1,2})?$/.test(text), 'Exact cent wire amount required'); const [whole, fraction = ''] = text.replace('-', '').split('.'); return (BigInt(whole) * 100n + BigInt(fraction.padEnd(2, '0'))) * (text.startsWith('-') ? -1n : 1n); };
      const total = (no: string) => stable.journal.journal.entries.flatMap((e: any) => e.lines).filter((l: any) => l.accountNo === no).reduce((sum: bigint, l: any) => sum + cents(l.debit) - cents(l.credit), 0n);
      need(total(account.debtor) === 0n && total(account.claim) === 0n && total(account.revenue) === -67600n && total(account.compensation) === -62000n && total(account.outputVat) === -16900n && total(account.gain) === 0n && total(account.loss) === 0n && total(account.bank) === 147225n, 'Exact synthetic terminal account movements');
      await originals(); reports = await audit(); await unchanged(stable, 'Final originals/full-audit reads');
      await page.goto(base + '/companies/synthetic/integritet'); const card = page.getByRole('region', { name: 'Regnskabskontrol', exact: true }); const response = page.waitForResponse(r => r.url() === base + prefix + 'audit-verify'); await card.getByRole('button', { name: 'Kontrollér regnskabet', exact: true }).click(); need(isDeepStrictEqual(await (await response).json(), reports), 'Actual web final full-audit parity'); await card.getByRole('status').filter({ hasText: 'Kontrollen fandt ingen afvigelser.' }).waitFor(); await page.screenshot({ path: process.env.RM_SCANNER_SCREENSHOT!, fullPage: true });
      const exported = await page.request.post(base + '/api/workspace/backup', { headers: { origin: base }, data: { confirm: true } });
      need(exported.ok() && exported.headers()['content-type']?.split(';')[0] === 'application/vnd.rentemester.native-backup+json', 'Actual signed compensation workspace export required');
      const artifact = await exported.body(); need(artifact.length > 0 && artifact.length <= 96 * 1024 * 1024, 'Bounded exact signed compensation backup');
      writeFileSync(join(work, 'compensation-source.native-backup.json'), artifact, { mode: 0o600, flag: 'wx' });
      await unchanged(stable, 'Signed compensation backup export');
      for (const invoice of invoices) need(isDeepStrictEqual(await calculate(invoice), terminalCalcs[invoice.id]) && isDeepStrictEqual(await get(`invoices/${invoice.id}/reminder-claims`), terminalClaims[invoice.id]), 'Signed export changed exact compensation status');
      completed = true;
      return { qualified: true, signedBackup: { sha256: hash(artifact), bytes: artifact.length, independentlyVerified: false }, invoices, stages: steps, refusals, permissions: rights, backup, terminalCalcs, terminalClaims, fullAudit: reports, accountingFingerprint: fingerprint(stable), originalSha256: Object.fromEntries([...originalBytes].map(([key, bytes]) => [key, hash(bytes)])), bankOriginalSha256: Object.fromEntries([...bankBytes].map(([key, bytes]) => [key, hash(bytes)])), exactHttpCliMcpCalculationAndStatusParity: true, dkkActualWebCalculateRegisterPostPrincipalAndClaimReceipt: true, eurCliRegisterMcpPostAndDkkClaimReceipt: true, noDuplicateOrRefusedFinancialAuditEffects: true, originalCurrencyPrincipalAndDkkClaimsKeptSeparate: true, sourceFreeRestoreQualified: false, privateBuyerActualQualified: false, realMailOrProviderEffectsQualified: false };
    },
    async reopen() {
      need(completed, 'Completed actual compensation flow required before restart'); await cookies(page, ownerCookie, false);
      for (const invoice of invoices) { const current = await parity(invoice); need(isDeepStrictEqual(current.calculation, terminalCalcs[invoice.id]) && isDeepStrictEqual(current.claims, terminalClaims[invoice.id]), 'Normal MFA restart changed exact compensation/receipt state'); }
      need(isDeepStrictEqual(await snapshot(), stable) && isDeepStrictEqual(await audit(), reports), 'Normal restart changed books/audit119'); await originals();
      // Only private cookie files owned by this helper are removed. The runner
      // owns server/browser/profile cleanup, including all synthetic data.
      for (const file of [ownerCookie, memberCookie]) { const info = lstatSync(file); need(info.isFile() && !info.isSymbolicLink(), 'Owned proof cookie cleanup required'); unlinkSync(file); }
      return { qualified: true, exactNormalMfaRestartState: true, exactOriginalBytesAfterRestart: true, fullAudit119GreenAfterRestart: true, accountingFingerprint: fingerprint(stable), sourceFreeRestoreQualified: false };
    },
  };
}
