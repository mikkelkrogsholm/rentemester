import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, readdirSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

type Kind = 'cli' | 'mcp';
type Invoice = { id: number; number: string; currency: 'DKK' };
type Operation = 'calculate' | 'register' | 'post' | 'correction' | 'post-correction' | 'claims' | 'settle' | 'audit' | 'original' | 'pdf';
const operations: Record<Operation, [string, string]> = {
  calculate: ['calculate-invoice-interest', 'invoice_interest_calc'],
  register: ['register-invoice-interest', 'invoice_claim_interest'],
  post: ['post-invoice-interest', 'invoice_post_interest'],
  correction: ['calculate-invoice-interest-correction', 'invoice_interest_correction_calc'],
  'post-correction': ['post-invoice-interest-correction', 'invoice_post_interest_correction'],
  claims: ['invoice-reminder-claims', 'invoice_reminder_claims'],
  settle: ['settle-invoice-claims', 'settle_invoice_claims'],
  audit: ['audit-verify', 'audit_verify'],
  original: ['document-original', 'document_original'],
  pdf: ['invoice-pdf', 'invoice_pdf'],
};

/** Actual public adapters only. This helper never seeds or reads business/auth SQL. */
export function createInvoiceInterestProof(page: Page, base: string, work: string, memberPage: Page | undefined, serviceRunResult: any) {
  const prefix = '/api/companies/synthetic/';
  const need = (value: unknown, message: string) => { if (!value) throw Error(message); };
  const hash = (bytes: Buffer | string) => createHash('sha256').update(bytes).digest('hex');
  const fingerprint = (value: unknown) => hash(JSON.stringify(value));
  const ownerCookie = join(work, 'interest-owner.cookie'), memberCookie = join(work, 'interest-member.cookie');
  const account = { debtor: '1210', claim: '1230', revenue: '3210', interest: '3230', outputVat: '4210', inputVat: '4220', creditor: '2210', gain: '3220', loss: '6220', bank: '1711' };
  let today = '', issueDate = '', dueDate = '', previousDate = '', partialDate = '', completed = false, started = false;
  let stable: any, reports: any, terminalCalcs: any, terminalClaims: any;
  const invoices: Invoice[] = [], originalBytes = new Map<string, Buffer>(), bankBytes = new Map<string, Buffer>();
  const steps: Array<{ name: string; fingerprint: string }> = [];
  const refusals: Array<{ label: string; status: number; code: string; canonicalFingerprint: string }> = [];
  const endpoint = (action: string) => base + prefix + 'invoices/interest/' + action;
  async function cookies(who: Page, file: string, fresh: boolean) {
    const all = await who.context().cookies(base); need(all.some(c => c.httpOnly), 'Normal MFA HttpOnly cookie required');
    writeFileSync(file, all.map(c => `${c.name}=${c.value}`).join('; ') + '\n', { mode: 0o600, flag: fresh ? 'wx' : 'w' });
  }
  async function get(resource: string, who = page) {
    const r = await who.request.get(base + prefix + resource); need(r.ok(), `Interest canonical ${resource} HTTP${r.status()}`); return r.json();
  }
  async function post(resource: string, body: unknown, who = page, method = 'POST') {
    const r = await who.request.fetch(base + prefix + resource, { method, headers: { origin: base }, data: body });
    const value = await r.json(); need(r.ok() && value.ok === true, `Interest ${resource} HTTP${r.status()}: ${JSON.stringify(value)}`); return value;
  }
  function client(kind: Kind, op: Operation, body?: unknown, options: { id?: number; member?: boolean; success?: boolean; confirmed?: boolean; usage?: boolean } = {}) {
    const write = ['register', 'post', 'post-correction', 'settle'].includes(op), confirmed = options.confirmed ?? write;
    const common = ['--server', base, '--cookie-file', options.member ? memberCookie : ownerCookie, '--allow-loopback-http'];
    const selected = options.id === undefined ? {} : { id: options.id };
    const args = kind === 'cli' ? ['remote', operations[op][0], ...common, '--company', 'synthetic', ...(options.id === undefined ? [] : ['--id', String(options.id)]), ...(confirmed ? ['--confirm'] : [])] : common;
    const input = kind === 'cli' ? body === undefined ? '' : JSON.stringify(body) : [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-interest-proof121', version: '1' } } },
      { jsonrpc: '2.0', method: 'notifications/initialized' },
      { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: operations[op][1], arguments: { company: 'synthetic', ...selected, ...(body === undefined ? {} : { body: JSON.stringify(body) }), ...(write ? { confirm: confirmed } : {}) } } },
    ].map(v => JSON.stringify(v)).join('\n') + '\n';
    const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!r.error, `Interest ${kind} subprocess failed`);
    for (const file of [ownerCookie, memberCookie]) if (existsSync(file)) need(!r.stdout.includes(readFileSync(file, 'utf8').trim()) && !r.stderr.includes(readFileSync(file, 'utf8').trim()), 'Private cookie escaped client output');
    if (kind === 'cli') {
      const value = JSON.parse(r.stdout);
      if (options.success === false) { need(r.status === (options.usage ? 2 : 1) && value.ok === false && (options.usage || value.code === 'server_refused'), `Interest CLI refusal contract ${op}: ${r.status}`); return { exit: r.status, code: value.code }; }
      need(r.status === 0 && value.ok === true && value.data?.ok === true, `Interest CLI success envelope ${op}`); return value.data;
    }
    need(r.status === 0, 'Interest MCP terminal status');
    const reply = r.stdout.trim().split('\n').map(line => JSON.parse(line)).find(v => v.id === 2);
    if (options.success === false) { need(!reply?.error && reply?.result?.isError === true && (options.usage || JSON.stringify(reply.result).includes('server_refused')), 'Interest MCP expected refusal'); return { isError: true }; }
    need(!reply?.error && reply?.result?.isError === false && reply.result.structuredContent?.data?.ok === true, `Interest MCP success envelope ${op}`); return reply.result.structuredContent.data;
  }
  const calcBody = (invoice: Invoice) => ({ invoiceDocumentId: invoice.id, asOfDate: previousDate, referenceRatePercent: 2 });
  const registerBody = (invoice: Invoice) => ({ ...calcBody(invoice), note: 'Synthetic reviewed interest', confirm: true });
  const postingBody = (invoice: Invoice) => ({ invoiceDocumentId: invoice.id, transactionDate: previousDate, receivableAccountNo: account.claim, interestIncomeAccountNo: account.interest, confirm: true });
  const correctionBody = (invoice: Invoice) => ({ invoiceDocumentId: invoice.id });
  const correctionPostBody = (invoice: Invoice) => ({ invoiceDocumentId: invoice.id, transactionDate: today, reason: 'Synthetic effective dated principal payment', confirm: true });
  async function calculate(invoice: Invoice, who = page) {
    const r = await who.request.post(endpoint('calculate'), { headers: { origin: base }, data: calcBody(invoice) }); need(r.ok(), 'Canonical interest calculation refused'); return r.json();
  }
  async function correction(invoice: Invoice) { return post('invoices/interest/correction', correctionBody(invoice)); }
  async function parity(invoice: Invoice) {
    const before = await snapshot(), value = await calculate(invoice), corrected = await correction(invoice);
    for (const kind of ['cli', 'mcp'] as const) {
      need(isDeepStrictEqual(client(kind, 'calculate', calcBody(invoice)), value), `${kind} exact interest calculation differs`);
      need(isDeepStrictEqual(client(kind, 'correction', correctionBody(invoice)), corrected), `${kind} exact interest correction differs`);
    }
    const claims = await get(`invoices/${invoice.id}/reminder-claims`);
    for (const kind of ['cli', 'mcp'] as const) need(isDeepStrictEqual(client(kind, 'claims', undefined, { id: invoice.id }), claims), `${kind} exact interest/receipt status differs`);
    need(value.interest.invoiceDocumentId === invoice.id && value.interest.invoiceNumber === invoice.number && value.interest.principalCurrency === 'DKK' && Array.isArray(value.interest.claims), 'Canonical interest identity and claim history required');
    need(claims.reminderClaims.principalCurrency === 'DKK' && claims.reminderClaims.claimCurrency === 'DKK', 'Principal and interest units explicit');
    await unchanged(before, 'Read-only cross-client interest/correction/status'); return { calculation: value, correction: corrected, claims };
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
    need(value.ok === true && value.audit?.format === 'rentemester-native-audit-verification/1' && value.audit.available === true && value.audit.ok === true && value.audit.errors.length === 0 && value.audit.entries === 7, 'Actual DKK interest ledger must pass full audit119: ' + JSON.stringify(value));
    need(isDeepStrictEqual(value.audit.components.map((c: any) => c.name), ['ledger', 'bank', 'invoices', 'originals', 'physical_schema']) && value.audit.components.every((c: any) => c.available && c.ok && c.errors.length === 0), 'All five full-audit components required');
    for (const kind of ['cli', 'mcp'] as const) need(isDeepStrictEqual(client(kind, 'audit'), value), 'Full audit HTTP/CLI/MCP parity');
    await unchanged(before, 'Full audit inspection'); return value;
  }
  async function uiCalculate(invoice: Invoice) {
    await page.goto(base + '/companies/synthetic/fakturaer');
    await page.getByRole('row').filter({ hasText: invoice.number }).getByRole('button', { name: 'Rykkergebyrer og krav', exact: true }).click();
    const card = page.getByRole('region', { name: 'Rykkergebyrer og krav', exact: true }), interest = card.getByRole('region', { name: 'Renter', exact: true });
    await interest.getByLabel('Opgørelsesdato for renter', { exact: true }).fill(previousDate);
    await interest.getByLabel('Referencerente i procent (valgfri)', { exact: true }).fill('2');
    const pending = page.waitForResponse(r => r.url() === endpoint('calculate') && r.request().method() === 'POST');
    await interest.getByRole('button', { name: 'Beregn renter', exact: true }).click();
    need(isDeepStrictEqual(await (await pending).json(), await calculate(invoice)), 'Actual web calculation exact HTTP parity'); return { card, interest };
  }
  async function reviewed(button: string, dialogName: string, confirm: string, action: string, body: unknown, region: any) {
    await region.getByRole('button', { name: button, exact: true }).click();
    const dialog = page.getByRole('dialog', { name: dialogName, exact: true });
    const pending = page.waitForResponse(r => r.url() === endpoint(action) && r.request().method() === 'POST');
    await dialog.getByRole('button', { name: confirm, exact: true }).click(); const r = await pending, result = await r.json();
    need(r.ok() && result.ok === true && isDeepStrictEqual(r.request().postDataJSON(), body), 'One reviewed exact interest ' + action + ': ' + JSON.stringify(result)); await dialog.waitFor({ state: 'hidden' }); return result;
  }
  async function uiRegister(invoice: Invoice) {
    const { interest } = await uiCalculate(invoice);
    // UI note is optional; assert the exact economic fields without inventing it.
    const body = { ...calcBody(invoice), confirm: true };
    return reviewed('Gennemgå registrering af renter', 'Registrér rentekrav', 'Registrér renter', 'register', body, interest);
  }
  async function uiPost(invoice: Invoice) {
    const { interest } = await uiCalculate(invoice);
    await interest.getByLabel('Registreret rentekrav', { exact: true }).selectOption(String(claimId()));
    await interest.getByLabel('Bogføringsdato for renter', { exact: true }).fill(previousDate);
    await interest.getByLabel('Konto til rentetilgodehavende', { exact: true }).selectOption(account.claim);
    await interest.getByLabel('Konto til renteindtægt', { exact: true }).selectOption(account.interest);
    return reviewed('Gennemgå bogføring af renter', 'Bogfør rentekrav', 'Bogfør renter', 'post', { ...postingBody(invoice), claimId: claimId() }, interest);
  }
  function claimId() { const id = terminalCalcs?.interest?.claims?.[0]?.id; need(id > 0, 'Canonical actual interest claim id required'); return id; }
  async function uiCorrection(invoice: Invoice) {
    const { interest } = await uiCalculate(invoice);
    const pending = page.waitForResponse(r => r.url() === endpoint('correction') && r.request().method() === 'POST');
    await interest.getByRole('button', { name: 'Beregn rentekorrektion', exact: true }).click(); need(isDeepStrictEqual(await (await pending).json(), await correction(invoice)), 'Actual web correction proposal exact parity');
    await interest.getByLabel('Bogføringsdato for rentekorrektion', { exact: true }).fill(today);
    await interest.getByLabel('Begrundelse for rentekorrektion', { exact: true }).fill(correctionPostBody(invoice).reason);
    return reviewed('Gennemgå rentekorrektion', 'Bogfør rentekorrektion', 'Bogfør korrektion', 'post-correction', correctionPostBody(invoice), interest);
  }
  async function uiClaimReceipt(invoice: Invoice, bankId: number) {
    const { card } = await uiCalculate(invoice); await card.getByLabel('Faktisk indbetaling på gebyrkrav', { exact: true }).selectOption(String(bankId));
    await card.getByRole('button', { name: 'Gennemgå indbetaling på gebyrkrav', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Afstem indbetaling på gebyrkrav', exact: true });
    const pending = page.waitForResponse(r => r.url() === base + prefix + 'invoices/claims/settle' && r.request().method() === 'POST');
    await dialog.getByRole('button', { name: 'Afstem gebyrindbetaling', exact: true }).click(); const r = await pending;
    need(r.ok() && isDeepStrictEqual(r.request().postDataJSON(), { invoiceDocumentId: invoice.id, bankTransactionId: bankId, confirm: true }), 'Actual reviewed claim receipt exact bank source'); await dialog.waitFor({ state: 'hidden' });
  }
  async function setup() {
    today = (await get('scheduled-recurring')).scheduledRecurring.asOfDate; need(/^\d{4}-\d{2}-\d{2}$/.test(today), 'Trusted server date required');
    const ago = (n: number) => { const d = new Date(today + 'T12:00:00Z'); d.setUTCDate(d.getUTCDate() - n); return d.toISOString().slice(0,10); };
    issueDate = ago(12); dueDate = ago(10); previousDate = ago(1); partialDate = ago(6);
    need(issueDate.slice(0,4) === today.slice(0,4), 'Synthetic proof requires one current fiscal year');
    await post('company', { cvr: '87654321', address: 'Synthetic Interest Street 121', postalCode: '1000', city: 'Synthetic City', vatPeriodType: 'quarter', paymentTermsDays: 1 }, page, 'PATCH');
    for (const [accountNo,name,type,normalBalance] of [[account.debtor,'Synthetic invoice receivable','asset','debit'],[account.claim,'Synthetic interest receivable','asset','debit'],[account.revenue,'Synthetic revenue','income','credit'],[account.interest,'Synthetic interest income','income','credit'],[account.outputVat,'Synthetic VAT','vat','credit']]) await post('accounts',{accountNo,name,type,normalBalance});
    for (const [role,accountNo] of [['debtors',account.debtor],['bank',account.bank],['output_vat',account.outputVat]]) await post('account-roles',{role,accountNo,confirm:true});
    const issued = (await post('invoices/issue',{issueDate,deliveryDate:issueDate,dueDate,currency:'DKK',vatRatePercent:25,seller:{name:'Synthetic Document Scanner Company',address:'Synthetic Interest Street 121',vatOrCvr:'DK87654321'},buyer:{name:'Synthetic Interest Buyer',address:'Synthetic Buyer Street',vatOrCvr:'DK12345678'},lines:[{description:'Synthetic interest service',quantity:1,unitPriceExVat:800}]})).invoice;
    need(issued.documentId > 0 && issued.grossAmount === 1000, 'Actual gross1000 invoice required');
    invoices.push({id:issued.documentId,number:issued.invoiceNumber,currency:'DKK'});
    await post('invoices/post',{invoiceDocumentId:issued.documentId,revenueAccountNo:account.revenue,confirm:true});
    const csvContent=`date;text;amount;currency;reference\n${partialDate};Synthetic partial principal;500;DKK;SYN121-PARTIAL\n${today};Synthetic remaining principal;500;DKK;SYN121-PRINCIPAL\n${today};Synthetic lawful interest;1.78;DKK;SYN121-INTEREST\n`;
    const imported=(await post('bank/import',{account:'synthetic-backup-lock',csvContent,confirm:true})).import;
    need(imported.imported===3 && imported.sourceFileHash===hash(csvContent),'Exact public imported dated bank source');
    need((await get('journal')).journal.entries.length===2 && (await get('bank')).bank.transactions.length===4,'One invoice journal and three imported bank rows');
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
    const file = join(directory, sha + '.native-backup.json'), parked = file + '.interest121-held', info = lstatSync(file), bytes = readFileSync(file);
    need(info.isFile() && !info.isSymbolicLink() && info.nlink === 1 && info.uid === process.getuid!() && (info.mode & 0o777) === 0o600 && hash(bytes) === sha && bytes.equals(readFileSync(join(work, 'backup-lock-ui.native-backup.json'))) && !existsSync(parked), 'Exact owned retained prior artifact required, no caller path');
    const before = await snapshot(); let refusal: { status: number; code: string } | undefined;
    try {
      renameSync(file, parked); const status = await get('backup-status'); need(status.lock.enforced === true && status.lock.locked === true, 'Missing current retained backup must lock actual interest posting');
      refusal = await denied('post', postingBody(invoice), before, 'Current backup lock on interest', page, 'backup_overdue');
    } finally { need(!existsSync(file) && existsSync(parked), 'Never overwrite another retained artifact'); renameSync(parked, file); }
    need(readFileSync(file).equals(bytes) && (await get('backup-status')).lock.locked === false, 'Exact backup bytes restored and qualified before one real post'); await unchanged(before, 'Backup file park/refusal/restore');
    return { refusal, exactArtifactSha256: sha, parkedOnlyOwnedRetainedArtifact: true, exactBytesRestoredBeforePosting: true };
  }
  async function stage(invoice: Invoice, name: string) { const value = await parity(invoice); steps.push({ name, fingerprint: fingerprint(value) }); return value; }
  return {
    async run() {
      need(!started, 'Interest proof executes once'); started = true;
      await cookies(page, ownerCookie, true); need(memberPage, 'Independent normal MFA member required'); await cookies(memberPage!, memberCookie, true);
      const baseline = await snapshot(); need(baseline.journal.journal.entries.length === 1 && baseline.bank.bank.transactions.length === 1 && baseline.documents.documents.documents.length === 0, 'Exact accepted backup-lock baseline required');
      await setup(); const invoice = invoices[0];
      const initial = await stage(invoice, 'calculated');
      need(initial.calculation.interest.accruedInterestAmount === 2.47 && initial.calculation.interest.annualInterestRatePercent === 10 && initial.calculation.interest.referenceRateSource === 'manual-override' && initial.calculation.interest.claimableDays === 9 && initial.claims.reminderClaims.principalOpenBalance === 1000, 'Exact nine-day DKK manual interest calculation');
      const before = await snapshot();
      const table = await post('invoices/interest/calculate', { invoiceDocumentId: invoice.id, asOfDate: previousDate });
      need(table.interest.referenceRateSource === 'statutory-table' && table.interest.referenceRatePercent >= 0 && table.interest.annualInterestRatePercent === table.interest.referenceRatePercent + 8, 'Omitted rate uses server statutory table');
      await unchanged(before, 'Statutory default read');
      const rights = await permissions(invoice);
      await denied('register', { ...registerBody(invoice), confirm: false }, before, 'Missing inner registration confirmation');
      for (const kind of ['cli', 'mcp'] as const) { client(kind, 'register', registerBody(invoice), { success: false, confirmed: false, usage: true }); await unchanged(before, 'Missing outer confirmation'); }
      await uiRegister(invoice); const registered = await stage(invoice, 'registered-without-journal'); terminalCalcs = registered.calculation;
      need(registered.claims.reminderClaims.pendingUnpostedInterest === true && registered.calculation.interest.claims.length === 1 && registered.calculation.interest.claims[0].postedJournalEntryId === null && (await get('journal')).journal.entries.length === 2, 'Interest registration creates no journal');
      const registeredBefore = await snapshot(); await denied('register', registerBody(invoice), registeredBefore, 'Duplicate interest registration');
      for (const kind of ['cli', 'mcp'] as const) { client(kind, 'register', registerBody(invoice), { success: false }); await unchanged(registeredBefore, 'Duplicate client registration'); }
      await denied('post', { ...postingBody(invoice), confirm: false }, registeredBefore, 'Missing interest posting confirmation');
      const backup = await backupNegative(invoice);
      await uiPost(invoice); const posted = await stage(invoice, 'interest-posted');
      need(posted.calculation.interest.claims[0].postedJournalEntryId > 0 && posted.claims.reminderClaims.ledgerBackedClaimBalance === 2.47 && posted.claims.reminderClaims.principalOpenBalance === 1000 && posted.claims.reminderClaims.pendingUnpostedInterest === false, 'Separate interest receivable posted exactly once');
      const bank = (await get('bank')).bank.transactions;
      const bankId = (reference: string) => { const row = bank.find((b: any) => b.reference === reference); need(row?.id > 0, 'Actual referenced bank source required'); return row.id as number; };
      // This actual posting is deliberately later than the interest claim, but
      // the payment's effective date reduces five days of that earlier window.
      await post('invoices/settle', { invoiceDocumentId: invoice.id, bankTransactionId: bankId('SYN121-PARTIAL'), paymentDate: partialDate, confirm: true });
      const adjusted = await stage(invoice, 'effective-dated-partial-payment');
      const proposal = adjusted.correction.interestCorrection;
      need(adjusted.claims.reminderClaims.principalOpenBalance === 500 && proposal.lawfulInterest === 1.78 && proposal.postedInterest === 2.47 && proposal.overClaimedAmount === 0.69 && proposal.canPost === true && proposal.requiresRefund === false && proposal.proposedLines.length === 2, 'Actual partial principal yields exact safe causal correction');
      need(isDeepStrictEqual(proposal.proposedLines, [{ accountNo: account.claim, debitAmount: 0, creditAmount: 0.69 }, { accountNo: account.interest, debitAmount: 0.69, creditAmount: 0 }]) || isDeepStrictEqual(proposal.proposedLines, [{ accountNo: account.interest, debitAmount: 0.69, creditAmount: 0 }, { accountNo: account.claim, debitAmount: 0, creditAmount: 0.69 }]), 'Correction only reverses original claim/income accounts');
      const correctionBefore = await snapshot();
      await denied('post-correction', { ...correctionPostBody(invoice), confirm: false }, correctionBefore, 'Missing correction confirmation');
      await uiCorrection(invoice); const corrected = await stage(invoice, 'correction-posted');
      need(corrected.correction.interestCorrection.overClaimedAmount === 0 && corrected.correction.interestCorrection.alreadyCorrected === 0.69 && corrected.claims.reminderClaims.ledgerBackedClaimBalance === 1.78 && corrected.claims.reminderClaims.totalInterestCorrectionsDkk === 0.69, 'Exact causal correction readback');
      const correctedBefore = await snapshot();
      for (const [operation, body] of [['post', { ...postingBody(invoice), claimId: claimId() }], ['post-correction', correctionPostBody(invoice)]] as const) {
        await denied(operation, body, correctedBefore, 'Duplicate ' + operation);
        for (const kind of ['cli', 'mcp'] as const) { client(kind, operation, body, { success: false }); await unchanged(correctedBefore, 'Duplicate adapter ' + operation); }
      }
      await post('invoices/settle', { invoiceDocumentId: invoice.id, bankTransactionId: bankId('SYN121-PRINCIPAL'), paymentDate: today, confirm: true });
      const principalPaid = await stage(invoice, 'principal-paid-interest-open');
      need(principalPaid.claims.reminderClaims.principalOpenBalance === 0 && principalPaid.claims.reminderClaims.ledgerBackedClaimBalance === 1.78 && principalPaid.claims.reminderClaims.settlementAvailable === true, 'Principal and interest remain separate before claim receipt');
      await uiClaimReceipt(invoice, bankId('SYN121-INTEREST'));
      const final = await stage(invoice, 'principal-and-interest-paid'), claims = final.claims.reminderClaims;
      need(claims.principalOpenBalance === 0 && claims.ledgerBackedClaimBalance === 0 && claims.registeredClaimBalanceDkk === 0 && claims.totalInterestDkk === 2.47 && claims.totalInterestCorrectionsDkk === 0.69 && claims.totalClaimPayments === 1.78 && claims.pendingUnpostedInterest === false && claims.interestClaims.length === 1 && claims.interestCorrections.length === 1 && claims.claimPayments.length === 1, 'Exact one claim/correction/receipt terminal state');
      need(claims.interestClaims[0].claimId === final.calculation.interest.claims[0].id && claims.interestClaims[0].claimDate === previousDate && claims.interestClaims[0].amountDkk === 2.47 && claims.interestClaims[0].journalEntryId === final.calculation.interest.claims[0].postedJournalEntryId && claims.interestCorrections[0].correctionId > 0 && claims.interestCorrections[0].correctionDate === today && claims.interestCorrections[0].amountDkk === 0.69 && claims.interestCorrections[0].journalEntryId > 0, 'Exact persisted claim/correction identities and journals');
      need(claims.claimPayments[0].bankTransactionId === bankId('SYN121-INTEREST') && claims.claimPayments[0].amount === 1.78 && claims.claimPayments[0].journalEntryId > 0, 'Actual interest bank receipt identity');
      terminalCalcs = final.calculation; terminalClaims = { claims: final.claims, correction: final.correction };
      stable = await snapshot(); need(stable.journal.journal.entries.length === 7 && stable.bank.bank.transactions.length === 4 && new Set(stable.documents.documents.documents.map((document: any) => document.id)).size === 2 && stable.trial.trialBalance.balanced === true, 'Exactly seven journals four bank rows and JSON/PDF inventory');
      const cents = (value: number | string) => { const text = String(value); need(/^-?\d+(\.\d{1,2})?$/.test(text), 'Exact cent wire amount required'); const [whole, fraction = ''] = text.replace('-', '').split('.'); return (BigInt(whole) * 100n + BigInt(fraction.padEnd(2,'0'))) * (text.startsWith('-') ? -1n : 1n); };
      const total = (no: string) => stable.journal.journal.entries.flatMap((e: any) => e.lines).filter((l: any) => l.accountNo === no).reduce((sum: bigint,l: any) => sum+cents(l.debit)-cents(l.credit),0n);
      need(total(account.debtor) === 0n && total(account.claim) === 0n && total(account.revenue) === -80000n && total(account.interest) === -178n && total(account.outputVat) === -20000n && total(account.bank) === 100903n, 'Exact terminal principal/interest/VAT/account movements');
      await originals(); reports = await audit(); await unchanged(stable, 'Exact final original/audit readback');
      await page.goto(base + '/companies/synthetic/integritet'); const card = page.getByRole('region', { name: 'Regnskabskontrol', exact: true }); const response = page.waitForResponse(r => r.url() === base + prefix + 'audit-verify');
      await card.getByRole('button', { name: 'Kontrollér regnskabet', exact: true }).click(); need(isDeepStrictEqual(await (await response).json(), reports), 'Actual web full audit parity'); await card.getByRole('status').filter({hasText:'Kontrollen fandt ingen afvigelser.'}).waitFor();
      await page.screenshot({ path: process.env.RM_SCANNER_SCREENSHOT!, fullPage: true });
      const exported = await page.request.post(base + '/api/workspace/backup', { headers: { origin: base }, data: { confirm: true } });
      need(exported.ok() && exported.headers()['content-type']?.split(';')[0] === 'application/vnd.rentemester.native-backup+json', 'Actual signed interest backup required'); const artifact = await exported.body();
      need(artifact.length > 0 && artifact.length <= 96*1024*1024, 'Bounded signed backup'); writeFileSync(join(work,'interest-source.native-backup.json'),artifact,{mode:0o600,flag:'wx'}); await unchanged(stable,'Signed export'); completed = true;
      return { qualified:true,signedBackup:{sha256:hash(artifact),bytes:artifact.length,independentlyVerified:false},invoice,stages:steps,refusals,permissions:rights,backup,terminalCalcs,terminalClaims,fullAudit:reports,expectedFinalJournalCount:7,accountingFingerprint:fingerprint(stable),originalSha256:Object.fromEntries([...originalBytes].map(([key,bytes])=>[key,hash(bytes)])),bankOriginalSha256:Object.fromEntries([...bankBytes].map(([key,bytes])=>[key,hash(bytes)])),exactHttpCliMcpReadsAndWriteRefusals:true,actualWebRegistrationPostingCorrectionAndClaimReceipt:true,actualBackdatedPartialPayment:true,paidOrAmbiguousNegativeActualQualified:false,foreignInterestActualQualified:false,fullForeignInterestRequirementRemoved:false,sourceFreeRestoreQualified:false,installedNativeQualified:false,realMailOrProviderEffectsQualified:false };
    },
    async reopen() {
      need(completed,'Completed actual interest flow required'); await cookies(page,ownerCookie,false);
      const current = await parity(invoices[0]); need(isDeepStrictEqual(current.calculation,terminalCalcs) && isDeepStrictEqual(current.claims,terminalClaims.claims) && isDeepStrictEqual(current.correction,terminalClaims.correction),'Fresh normal MFA restart exact interest state');
      need(isDeepStrictEqual(await snapshot(),stable) && isDeepStrictEqual(await audit(),reports),'Restart exact books and audit'); await originals();
      for (const file of [ownerCookie,memberCookie]) { const info=lstatSync(file); need(info.isFile()&&!info.isSymbolicLink(),'Owned cookie cleanup'); unlinkSync(file); }
      return {qualified:true,exactNormalMfaRestartState:true,exactOriginalBytesAfterRestart:true,fullAudit119GreenAfterRestart:true,accountingFingerprint:fingerprint(stable),foreignInterestActualQualified:false,sourceFreeRestoreQualified:false,installedNativeQualified:false};
    },
  };
}
