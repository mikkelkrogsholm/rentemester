import { originalInvoiceProof } from './check-native-invoice-original';
import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, readdirSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

type Kind = 'cli' | 'mcp';
type Invoice = { id: number; number: string; currency: 'DKK' | 'EUR' };
type Operation = 'writeoff' | 'register' | 'post' | 'claims' | 'settle' | 'audit' | 'original' | 'pdf';
const operations: Record<Operation, [string, string]> = {
  writeoff: ['write-off-invoice-bad-debt', 'invoice_write_off_bad_debt'],
  register: ['register-invoice-compensation', 'invoice_claim_compensation'],
  post: ['post-invoice-compensation', 'invoice_post_compensation'],
  claims: ['invoice-reminder-claims', 'invoice_reminder_claims'],
  settle: ['settle-invoice-claims', 'settle_invoice_claims'],
  audit: ['audit-verify', 'audit_verify'],
  original: ['document-original', 'document_original'],
  pdf: ['invoice-pdf', 'invoice_pdf'],
};

/** Actual public adapters only. This helper never seeds or reads business/auth SQL. */
export function createInvoiceBadDebtProof(page: Page, base: string, work: string, memberPage: Page | undefined, serviceRunResult: any) {
  const prefix = '/api/companies/synthetic/';
  const need = (value: unknown, message: string) => { if (!value) throw Error(message); };
  const hash = (bytes: Buffer | string) => createHash('sha256').update(bytes).digest('hex');
  const fingerprint = (value: unknown) => hash(JSON.stringify(value));
  const ownerCookie = join(work, 'bad-debt-owner.cookie'), memberCookie = join(work, 'bad-debt-member.cookie');
  const account = { debtor: '1210', claim: '1230', revenue: '3210', compensation: '3230', outputVat: '4210', inputVat: '4220', creditor: '2210', gain: '3220', loss: '6220', expense: '6230', bank: '1711' };
  let today = '', issueDate = '', dueDate = '', previousDate = '', completed = false, started = false;
  let stable: any, reports: any, terminalClaims: any;
  let projectionSequence = 0;
  let finalProjections: any;
  const originalQueries=originalInvoiceProof(page,base,work,ownerCookie), originalStages:any[]=[];
  let originalTerminal:any;
  const invoices: Invoice[] = [], originalBytes = new Map<string, Buffer>(), bankBytes = new Map<string, Buffer>();
  const steps: Array<{ name: string; fingerprint: string }> = [];
  const refusals: Array<{ label: string; status: number; code: string; canonicalFingerprint: string }> = [];
  const endpoint = () => base + prefix + 'invoices/bad-debt/write-off';
  async function cookies(who: Page, file: string, fresh: boolean) {
    const all = await who.context().cookies(base); need(all.some(c => c.httpOnly), 'Normal MFA HttpOnly cookie required');
    writeFileSync(file, all.map(c => `${c.name}=${c.value}`).join('; ') + '\n', { mode: 0o600, flag: fresh ? 'wx' : 'w' });
  }
  async function get(resource: string, who = page) {
    const r = await who.request.get(base + prefix + resource); need(r.ok(), `Bad-debt canonical ${resource} HTTP${r.status()}`); return r.json();
  }
  async function post(resource: string, body: unknown, who = page, method = 'POST') {
    const r = await who.request.fetch(base + prefix + resource, { method, headers: { origin: base }, data: body });
    const value = await r.json(); need(r.ok() && value.ok === true, `Bad-debt ${resource} HTTP${r.status()}: ${JSON.stringify(value)}`); return value;
  }
  function client(kind: Kind, op: Operation, body?: unknown, options: { id?: number; member?: boolean; success?: boolean; confirmed?: boolean; usage?: boolean } = {}) {
    const write = ['writeoff', 'register', 'post', 'settle'].includes(op), confirmed = options.confirmed ?? write;
    const common = ['--server', base, '--cookie-file', options.member ? memberCookie : ownerCookie, '--allow-loopback-http'];
    const selected = options.id === undefined ? {} : { id: options.id };
    const args = kind === 'cli' ? ['remote', operations[op][0], ...common, '--company', 'synthetic', ...(options.id === undefined ? [] : ['--id', String(options.id)]), ...(confirmed ? ['--confirm'] : [])] : common;
    const input = kind === 'cli' ? body === undefined ? '' : JSON.stringify(body) : [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-bad-debt-proof122', version: '1' } } },
      { jsonrpc: '2.0', method: 'notifications/initialized' },
      { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: operations[op][1], arguments: { company: 'synthetic', ...selected, ...(body === undefined ? {} : { body: JSON.stringify(body) }), ...(write ? { confirm: confirmed } : {}) } } },
    ].map(v => JSON.stringify(v)).join('\n') + '\n';
    const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!r.error, `Bad-debt ${kind} subprocess failed`);
    for (const file of [ownerCookie, memberCookie]) if (existsSync(file)) need(!r.stdout.includes(readFileSync(file, 'utf8').trim()) && !r.stderr.includes(readFileSync(file, 'utf8').trim()), 'Private cookie escaped client output');
    if (kind === 'cli') {
      const value = JSON.parse(r.stdout);
      if (options.success === false) { need(r.status === (options.usage ? 2 : 1) && value.ok === false && (options.usage || value.code === 'server_refused'), `Bad-debt CLI refusal contract ${op}: ${r.status}`); return { exit: r.status, code: value.code }; }
      need(r.status === 0 && value.ok === true && value.data?.ok === true, `Bad-debt CLI success envelope ${op}`); return value.data;
    }
    need(r.status === 0, 'Bad-debt MCP terminal status');
    const reply = r.stdout.trim().split('\n').map(line => JSON.parse(line)).find(v => v.id === 2);
    if (options.success === false) { need(!reply?.error && reply?.result?.isError === true && (options.usage || JSON.stringify(reply.result).includes('server_refused')), 'Bad-debt MCP expected refusal'); return { isError: true }; }
    need(!reply?.error && reply?.result?.isError === false && reply.result.structuredContent?.data?.ok === true, `Bad-debt MCP success envelope ${op}`); return reply.result.structuredContent.data;
  }
  const writeBody = (invoice: Invoice, grossAmount?: number, note = 'Synthetic documented partial loss 122') => ({ invoiceDocumentId: invoice.id, writeOffDate: today, expenseAccountNo: account.expense, ...(grossAmount === undefined ? {} : { grossAmount }), note, confirm: true });
  async function parity(invoice: Invoice) {
    const before = await snapshot(), claims = await get(`invoices/${invoice.id}/reminder-claims`);
    for (const kind of ['cli','mcp'] as const) need(isDeepStrictEqual(client(kind,'claims',undefined,{id:invoice.id}),claims), `${kind} exact bad-debt/principal/claim status`);
    const c=claims.reminderClaims;
    need(c.invoiceDocumentId === invoice.id && c.invoiceNumber === invoice.number && c.principalCurrency === invoice.currency && c.claimCurrency === 'DKK' && Array.isArray(c.badDebtWriteOffs), 'Canonical bad-debt identity and currencies');
    await unchanged(before,'Read-only bad-debt status parity'); return claims;
  }
  async function snapshot() {
    return { documents: await get('documents'), invoices: await get('invoices'), journal: await get('journal'), bank: await get('bank'), trial: await get('trial-balance'), balance: await get('balance'), income: await get('income-statement'), vat: await get('vat'), audit: await get('audit-log?limit=5000') };
  }
  async function unchanged(before: any, label: string) { need(isDeepStrictEqual(await snapshot(), before), label + ' changed canonical documents/bank/books/company audit'); }
  async function denied(action: string, body: unknown, before: any, label: string, who = page, code?: string) {
    const r = await who.request.post(endpoint(), { headers: { origin: base }, data: body }); const value = await r.json();
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
    need(value.ok === true && value.audit?.format === 'rentemester-native-audit-verification/1' && value.audit.available === true && value.audit.ok === true && value.audit.errors.length === 0 && value.audit.entries === 10, 'Actual DKK+EUR bad-debt ledger must pass full audit119: ' + JSON.stringify(value));
    need(isDeepStrictEqual(value.audit.components.map((c: any) => c.name), ['ledger', 'bank', 'invoices', 'originals', 'physical_schema']) && value.audit.components.every((c: any) => c.available && c.ok && c.errors.length === 0), 'All five full-audit components required');
    for (const kind of ['cli', 'mcp'] as const) need(isDeepStrictEqual(client(kind, 'audit'), value), 'Full audit HTTP/CLI/MCP parity');
    await unchanged(before, 'Full audit inspection'); return value;
  }
  async function openLoss(invoice: Invoice) {
    await page.goto(base+'/companies/synthetic/fakturaer');
    await page.getByRole('row').filter({hasText:invoice.number}).getByRole('button',{name:'Rykkergebyrer og krav',exact:true}).click();
    const card=page.getByRole('region',{name:'Rykkergebyrer og krav',exact:true});
    const loss=card.getByRole('region',{name:'Konstateret tab',exact:true});await loss.waitFor();return {card,loss};
  }
  async function uiWriteOff(invoice: Invoice) {
    const {loss}=await openLoss(invoice);
    await loss.getByLabel('Dato for tabsafskrivning',{exact:true}).fill(today);
    await loss.getByLabel('Bruttotab i fakturaens valuta (valgfrit)',{exact:true}).fill('312.50');
    await loss.getByLabel('Konto til tab på debitorer',{exact:true}).selectOption(account.expense);
    await loss.getByLabel('Note til tabsafskrivning',{exact:true}).fill(writeBody(invoice).note);
    const reviewed=loss.getByRole('button',{name:'Gennemgå tabsafskrivning',exact:true});
    need(await reviewed.isDisabled(),'Loss documentation confirmation must precede review');
    await loss.getByLabel('Jeg bekræfter, at tabet er konstateret og dokumenteret.',{exact:true}).check();
    await reviewed.click();const dialog=page.getByRole('dialog',{name:'Bogfør konstateret tab',exact:true});
    let posts=0, accepted:any;
    let finished!:()=>void, failed!:(error:unknown)=>void;const acceptedOnce=new Promise<void>((resolve,reject)=>{finished=resolve;failed=reject;});
    const intercept=async(route:any)=>{
      try { posts++; const r=await route.fetch({maxRetries:0});accepted=await r.json();
      need(r.ok()&&accepted.ok===true&&isDeepStrictEqual(route.request().postDataJSON(),writeBody(invoice,312.5)),'Exact accepted UI partial loss before lost reply');
      await route.abort('failed');finished(); } catch(error) {failed(error);await route.abort('failed').catch(()=>{});}
    };
    await page.route(endpoint(),intercept);
    try {
      await dialog.getByRole('button',{name:'Bogfør tab',exact:true}).click();await acceptedOnce;
      await dialog.getByText(/En ny skrivning er blokeret/).waitFor();
      need(await dialog.getByRole('button',{name:'Bogfør tab',exact:true}).isDisabled(),'Unknown result blocks repeat mutation');
      const read=page.waitForResponse(r=>r.url()===base+prefix+`invoices/${invoice.id}/reminder-claims`&&r.request().method()==='GET');
      await dialog.getByRole('button',{name:'Kontrollér status',exact:true}).click();need((await read).ok(),'Manual canonical read after lost reply');
      need(posts===1&&(await get(`invoices/${invoice.id}/reminder-claims`)).reminderClaims.badDebtWriteOffs.length===1&&await dialog.getByRole('button',{name:'Bogfør tab',exact:true}).isDisabled(),'Exactly one accepted write; manual read never silently unlocks retry');
    } finally {await page.unroute(endpoint(),intercept);}
    return accepted;
  }

  async function setup() {
    today=(await get('scheduled-recurring')).scheduledRecurring.asOfDate;need(/^\d{4}-\d{2}-\d{2}$/.test(today),'Trusted server date');
    const ago=(n:number)=>{const d=new Date(today+'T12:00:00Z');d.setUTCDate(d.getUTCDate()-n);return d.toISOString().slice(0,10);};
    issueDate=ago(6);dueDate=ago(3);previousDate=ago(1);
    need(issueDate.slice(0,4)===today.slice(0,4)&&issueDate.slice(0,7)===today.slice(0,7),'Synthetic acceptance requires current month/fiscal year');
    await post('company',{cvr:'87654321',address:'Synthetic Bad Debt Street 122',postalCode:'1000',city:'Synthetic City',vatPeriodType:'quarter',paymentTermsDays:1},page,'PATCH');
    for(const [accountNo,name,type,normalBalance] of [[account.debtor,'Synthetic debtors','asset','debit'],[account.claim,'Synthetic claim receivable','asset','debit'],[account.revenue,'Synthetic revenue','income','credit'],[account.compensation,'Synthetic compensation','income','credit'],[account.outputVat,'Synthetic output VAT','vat','credit'],[account.inputVat,'Synthetic input VAT','vat','debit'],[account.creditor,'Synthetic creditor','liability','credit'],[account.gain,'Synthetic FX gain','income','credit'],[account.loss,'Synthetic FX loss','expense','debit'],[account.expense,'Synthetic established bad debt expense','expense','debit']])await post('accounts',{accountNo,name,type,normalBalance});
    for(const [role,accountNo] of [['debtors',account.debtor],['bank',account.bank],['creditors',account.creditor],['output_vat',account.outputVat],['input_vat',account.inputVat],['fx_gain',account.gain],['fx_loss',account.loss]])await post('account-roles',{role,accountNo,confirm:true});
    await post('bank-accounts',{name:'Synthetic bad debt EUR bank',slug:'synthetic-bad-debt-eur',currency:'EUR',ledgerAccountNo:account.bank});
    for(const currency of ['DKK','EUR'] as const){
      const issued=(await post('invoices/issue',{issueDate,deliveryDate:issueDate,dueDate,currency,...(currency==='EUR'?{fxRateToDkk:7.43208}:{}),vatRatePercent:25,seller:{name:'Synthetic Document Scanner Company',address:'Synthetic Bad Debt Street 122',vatOrCvr:'DK87654321'},buyer:{name:'Synthetic Commercial Buyer '+currency,address:'Synthetic Buyer Street 122',vatOrCvr:'DK12345678'},lines:[{description:'Synthetic bad-debt service '+currency,quantity:1,unitPriceExVat:currency==='DKK'?1000:100}]})).invoice;
      need(issued.documentId>0&&issued.grossAmount===(currency==='DKK'?1250:125)&&(currency==='DKK'||issued.grossAmountDkk===929.01),'Actual frozen currency invoice');
      invoices.push({id:issued.documentId,number:issued.invoiceNumber,currency});await post('invoices/post',{invoiceDocumentId:issued.documentId,revenueAccountNo:account.revenue,confirm:true});
    }
    const [dkk]=invoices;
    client('cli','register',{invoiceDocumentId:dkk.id,asOfDate:today,compensationAmountDkk:310,confirm:true});
    client('mcp','post',{invoiceDocumentId:dkk.id,transactionDate:today,receivableAccountNo:account.claim,compensationIncomeAccountNo:account.compensation,confirm:true});
    const dkkCsv=`date;text;amount;currency;reference\n${today};Synthetic surviving DKK principal;625;DKK;SYN122-DKK-PRINCIPAL\n${today};Synthetic separate compensation;310;DKK;SYN122-DKK-CLAIM\n`;
    const eurCsv=`date;text;amount;currency;reference;amount_dkk;fx_rate_to_dkk\n${previousDate};Synthetic partial EUR principal;50.05;EUR;SYN122-EUR-PRINCIPAL;375.38;7.5\n`;
    for(const [csvContent,bankAccount,count]of [[dkkCsv,'synthetic-backup-lock',2],[eurCsv,'synthetic-bad-debt-eur',1]]as const){const imported=(await post('bank/import',{account:bankAccount,csvContent,confirm:true})).import;need(imported.imported===count&&imported.sourceFileHash===hash(csvContent),'Exact imported bank original');}
    need((await get('journal')).journal.entries.length===4&&(await get('bank')).bank.transactions.length===4,'Baseline plus invoice/compensation journals and three real bank rows');
  }
  async function permissions(invoice: Invoice) {
    need(memberPage,'Independent normal-MFA member required');const me=await memberPage!.request.get(base+'/api/me');need(me.ok(),'Current member identity');const id=(await me.json()).user.id;
    const members=await page.request.get(base+'/api/workspace/members');need(members.ok(),'Current memberships');const previous=(await members.json()).members.find((m:any)=>m.userId===id)?.memberships.find((m:any)=>m.companySlug==='synthetic')?.role;
    const change=async(action:string,role?:string)=>{const r=await page.request.post(base+'/api/workspace/members/company',{headers:{origin:base},data:{userId:id,companySlug:'synthetic',action,...(role?{role}:{})}});need(r.ok(),'Owner membership transition');};
    const before=await snapshot();
    try{
      await change('grant','reader');need(isDeepStrictEqual(await get(`invoices/${invoice.id}/reminder-claims`,memberPage),await get(`invoices/${invoice.id}/reminder-claims`)),'Reader can inspect current debt');
      await denied('writeoff',writeBody(invoice,312.5),before,'Reader cannot write off',memberPage!,'access_denied');
      for(const kind of ['cli','mcp']as const){client(kind,'writeoff',writeBody(invoice,312.5),{member:true,success:false});await unchanged(before,'Reader client refusal');}
      await change('revoke');const r=await memberPage!.request.get(base+prefix+`invoices/${invoice.id}/reminder-claims`);need(r.status()===403&&(await r.json()).code==='access_denied','Current revoked reader cannot read');
      await originalQueries.read('status',{documentId:invoice.id},memberPage!,memberCookie,true);
      await denied('writeoff',writeBody(invoice,312.5),before,'Revoked write',memberPage!,'access_denied');
      for(const kind of ['cli','mcp']as const){client(kind,'claims',undefined,{id:invoice.id,member:true,success:false});client(kind,'writeoff',writeBody(invoice,312.5),{member:true,success:false});await unchanged(before,'Current revoked client refusal');}
    }finally{await change(previous?'grant':'revoke',previous);}
    return {currentReaderWriteDenialAcrossHttpCliMcp:true,currentRevocationAcrossHttpCliMcp:true,noFinancialEffects:true};
  }
  async function backupNegative(invoice: Invoice) {
    const sha=serviceRunResult?.artifact?.sha256;need(/^[a-f0-9]{64}$/.test(sha)&&serviceRunResult.artifact.independentlyVerified===true,'Prior owned qualified backup required');
    const directory=join(work,'workspace','native-backups');need(!lstatSync(directory).isSymbolicLink(),'Owned retained directory');
    const file=join(directory,sha+'.native-backup.json'),parked=file+'.bad-debt122-held',info=lstatSync(file),bytes=readFileSync(file);
    need(info.isFile()&&!info.isSymbolicLink()&&info.nlink===1&&info.uid===process.getuid!()&&(info.mode&0o777)===0o600&&hash(bytes)===sha&&bytes.equals(readFileSync(join(work,'backup-lock-ui.native-backup.json')))&&!existsSync(parked),'Exact owned retained artifact');
    const before=await snapshot();let refusal: {status:number;code:string}|undefined;
    try{renameSync(file,parked);need((await get('backup-status')).lock.locked===true,'Missing current artifact must lock writeoff');refusal=await denied('writeoff',writeBody(invoice,312.5),before,'Current backup lock',page,'backup_overdue');}
    finally{need(!existsSync(file)&&existsSync(parked),'Never overwrite backup');renameSync(parked,file);}
    need(readFileSync(file).equals(bytes)&&(await get('backup-status')).lock.locked===false,'Exact qualified bytes restored');await unchanged(before,'Backup refusal/restore');return{refusal,exactArtifactSha256:sha,onlyOwnedArtifactParked:true,exactBytesRestored:true};
  }
  async function uiClaimReceipt(invoice: Invoice,bankId:number){
    const {card}=await openLoss(invoice);await card.getByLabel('Faktisk indbetaling på gebyrkrav',{exact:true}).selectOption(String(bankId));
    await card.getByRole('button',{name:'Gennemgå indbetaling på gebyrkrav',exact:true}).click();const dialog=page.getByRole('dialog',{name:'Afstem indbetaling på gebyrkrav',exact:true});
    const pending=page.waitForResponse(r=>r.url()===base+prefix+'invoices/claims/settle'&&r.request().method()==='POST');
    await dialog.getByRole('button',{name:'Afstem gebyrindbetaling',exact:true}).click();const r=await pending;need(r.ok()&&isDeepStrictEqual(r.request().postDataJSON(),{invoiceDocumentId:invoice.id,bankTransactionId:bankId,confirm:true}),'Separate actual compensation receipt');await dialog.waitFor({state:'hidden'});
  }
  function verifiedWrite(value:any,invoice:Invoice,expected:{gross:number;net:number;vat:number;grossDkk:number;netDkk:number;vatDkk:number;remaining:number;remainingDkk:number}){
    const v=value.writeOff;need(value.ok===true&&v.writeOffId>0&&v.journalEntryId>0&&typeof v.journalEntryNo==='string'&&v.invoiceDocumentId===invoice.id&&v.invoiceNumber===invoice.number&&v.principalCurrency===invoice.currency&&v.claimCurrency==='DKK'&&v.remainingClaimDkk===(invoice.currency==='DKK'?310:0),'Canonical admitted writeoff identity');
    need(v.grossAmount===expected.gross&&v.netAmount===expected.net&&v.vatAmount===expected.vat&&v.grossAmountDkk===expected.grossDkk&&v.netAmountDkk===expected.netDkk&&v.vatAmountDkk===expected.vatDkk&&v.remainingPrincipal===expected.remaining&&v.remainingPrincipalDkk===expected.remainingDkk,'Exact original and DKK loss allocation: '+JSON.stringify(v));return v;
  }
  async function stage(invoice:Invoice,name:string){const value=await parity(invoice);steps.push({name:invoice.currency+':'+name,fingerprint:fingerprint(value)});return value.reminderClaims;}
  async function projections(label: string, cases: Array<{ date: string; count: number; total: number; eur?: number; eurDkk?: number }>, clients = false) {
    const before = await snapshot(), result: any[] = [];
    const common = ['--server', base, '--cookie-file', ownerCookie, '--allow-loopback-http'];
    const normalized = (html: string) => {
      need((html.match(/<footer><p>Genereret: [^<]+<\/p>/g) || []).length === 1, 'One explicit HTML generation time');
      return html.replace(/(<footer><p>Genereret: )[^<]+(<\/p>)/, '$1[generatedAt]$2');
    };
    function readClient(kind: Kind, resource: 'portfolio' | 'dashboard', date: string) {
      const out = join(work, `dated-report-${++projectionSequence}.html`);
      const args = kind === 'cli' ? [resource, ...common, '--as-of', date, ...(resource === 'dashboard' ? ['--company', 'synthetic', '--out', out] : [])] : common;
      if (kind === 'cli' && resource === 'portfolio') args.unshift('remote');
      const input = kind === 'mcp' ? [
        { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-dated-fx130', version: '1' } } },
        { jsonrpc: '2.0', method: 'notifications/initialized' },
        { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: resource === 'dashboard' ? 'dashboard_export' : 'portfolio', arguments: { asOf: date, ...(resource === 'dashboard' ? { company: 'synthetic' } : {}) } } },
      ].map(v => JSON.stringify(v)).join('\n') + '\n' : '';
      const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
      need(!r.error && r.status === 0 && !r.stdout.includes(readFileSync(ownerCookie, 'utf8').trim()) && !r.stderr.includes(readFileSync(ownerCookie, 'utf8').trim()), `Dated ${kind} ${resource} terminal/privacy contract`);
      if (kind === 'cli') {
        const value = JSON.parse(r.stdout); need(value.ok === true, 'Dated CLI success');
        if (resource === 'portfolio') return value.data;
        const bytes = readFileSync(out); need(bytes.length === value.bytes && value.asOfDate === date && value.opened === false, 'Atomic dated HTML receipt');
        return bytes.toString('utf8');
      }
      const envelope = r.stdout.trim().split('\n').map(v => JSON.parse(v)).find(v => v.id === 2)?.result;
      need(envelope?.isError === false && envelope.structuredContent?.ok === true, 'Dated MCP success');
      const value = envelope.structuredContent.data;
      if (resource === 'portfolio') return value;
      need(value.ok === true && value.contentType === 'text/html; charset=utf-8' && value.asOfDate === date, 'Dated MCP exact HTML metadata');
      return Buffer.from(value.bytes).toString('utf8');
    }
    for (const expected of cases) {
      const response = await page.request.get(base + '/api/portfolio?asOf=' + expected.date);
      need(response.ok(), 'Actual dated FX/loss portfolio HTTP');
      const canonical = await response.json(), company = canonical.portfolio.companies.find((c: any) => c.slug === 'synthetic');
      need(company?.openInvoiceCount === expected.count && company.openInvoiceTotal === expected.total, `Dated FX/loss native principal ${label}/${expected.date}`);
      need(canonical.portfolio.totals.openInvoiceTotal === expected.total, 'Aggregate uses DKK carrying without claim principal or mixed currencies');
      let htmlSha256: string | undefined;
      if (clients) {
        const download = await page.request.get(base + prefix + 'dashboard/export?format=html&asOf=' + expected.date);
        need(download.ok() && download.headers()['content-type'] === 'text/html; charset=utf-8', 'Actual dated FX/loss HTML download');
        const html = await download.text(); htmlSha256 = hash(normalized(html));
        const rows = await page.evaluate(({ html, invoiceNumber }) => {
          const doc = new DOMParser().parseFromString(html, 'text/html');
          const section = doc.querySelector('section[data-section="invoices"]');
          return [...(section?.querySelectorAll('tbody tr') || [])].map(r => [...r.querySelectorAll('td')].map(c => c.textContent)).find(c => c[1] === invoiceNumber);
        }, { html, invoiceNumber: invoices.find(i => i.currency === 'EUR')!.number });
        if (expected.eur !== undefined) {
          need(rows?.[3] === expected.eur.toFixed(2).replace('.', ',') + ' EUR' && rows[4] === expected.eurDkk!.toFixed(2).replace('.', ',') + ' kr.', 'Original EUR and actual DKK carrying shown in distinct columns');
        }
        if (expected.date === previousDate) {
          await page.goto(base + '/');
          await page.getByRole('heading', { name: 'Portefølje', exact: true }).waitFor();
          await page.getByLabel('Pr. dato', { exact: true }).fill(expected.date);
          const pending = page.waitForResponse(r => { const u = new URL(r.url()); return u.pathname === '/api/portfolio' && u.searchParams.get('asOf') === expected.date; });
          await page.getByRole('button', { name: 'Hent portefølje igen', exact: true }).click();
          const shown = await pending;
          need(shown.ok() && isDeepStrictEqual(await shown.json(), canonical), 'Shared React portfolio uses exact dated FX/loss projection');
          const card = page.getByRole('article', { name: company.name, exact: true });
          await card.getByText('Åbne fakturaer', { exact: true }).locator('..').locator('dd').filter({ hasText: new RegExp('^' + expected.count + '$') }).waitFor();
          // Existing portfolio card displays invoice count; exact amounts remain checked in the API and HTML report.
        }
        for (const kind of ['cli', 'mcp'] as const) {
          need(isDeepStrictEqual(readClient(kind, 'portfolio', expected.date), canonical), 'Actual dated portfolio HTTP/CLI/MCP exact parity');
          need(hash(normalized(readClient(kind, 'dashboard', expected.date))) === htmlSha256, 'Actual dated HTML HTTP/CLI/MCP equality except explicit generation time');
        }
      }
      result.push({ ...expected, canonicalFingerprint: fingerprint(canonical), ...(htmlSha256 ? { htmlSha256 } : {}) });
    }
    await unchanged(before, 'Dated FX/loss portfolio and HTML read-only projection');
    return { qualified: true, label, readOnly: true, clientsQualified: clients, cases: result };
  }
  return {
    async run(){
      need(!started,'Bad-debt proof executes once');started=true;await cookies(page,ownerCookie,true);need(memberPage,'Normal MFA member required');await cookies(memberPage!,memberCookie,true);
      const baseline=await snapshot();need(baseline.journal.journal.entries.length===1&&baseline.bank.bank.transactions.length===1&&new Set(baseline.documents.documents.documents.map((v:any)=>v.id)).size===0,'Accepted backup-lock baseline');
      await setup();const[dkk,eur]=invoices;
      const originalBefore=await snapshot();originalStages.push(await originalQueries.stage(invoices,today,issueDate,{label:'issued-with-separate-compensation',open:[1250,125],paid:[0,0],written:[0,0],claim:310}));await unchanged(originalBefore,'Original issued query family');
      await projections('issued', [{ date: issueDate, count: 2, total: 2179.01 }]);
      const first=await stage(dkk,'separate-compensation-posted');need(first.principalOpenBalance===1250&&first.ledgerBackedClaimBalance===310&&first.badDebtWriteOffs.length===0,'Separate initial principal and compensation');
      const rights=await permissions(dkk),before=await snapshot();
      await denied('writeoff',{...writeBody(dkk,312.5),confirm:false},before,'Missing inner confirmation');
      await denied('writeoff',{...writeBody(dkk,312.5),actor:'agent:untrusted'},before,'Unknown caller authority field');
      await denied('writeoff',writeBody(dkk,1250.01),before,'Amount above open principal');
      await denied('writeoff',{...writeBody(dkk,312.5),expenseAccountNo:account.revenue},before,'Incompatible expense account');
      for(const kind of ['cli','mcp']as const){client(kind,'writeoff',writeBody(dkk,312.5),{success:false,confirmed:false,usage:true});await unchanged(before,'Missing outer confirmation');}
      const backup=await backupNegative(dkk);
      // The first operation is accepted once but its response is deliberately
      // dropped. The UI remains blocked; only explicit canonical reads follow.
      const ui=verifiedWrite(await uiWriteOff(dkk),dkk,{gross:312.5,net:250,vat:62.5,grossDkk:312.5,netDkk:250,vatDkk:62.5,remaining:937.5,remainingDkk:937.5});
      const one=await stage(dkk,'first-ui-partial-loss-unknown-reconciled');need(one.badDebtWriteOffs.length===1&&one.totalBadDebtWrittenOff===312.5&&one.ledgerBackedClaimBalance===310,'Lost reply caused exactly one loss and preserved compensation');
      // A second, separately authorized intent, deliberately not an idempotent
      // replay. This tests the source's legal partial-loss operation twice.
      const cli=verifiedWrite(client('cli','writeoff',writeBody(dkk,312.5,'Second deliberately reviewed partial loss 122')),dkk,{gross:312.5,net:250,vat:62.5,grossDkk:312.5,netDkk:250,vatDkk:62.5,remaining:625,remainingDkk:625});
      need(cli.writeOffId!==ui.writeOffId&&cli.journalEntryId!==ui.journalEntryId,'Two distinct deliberate loss receipts');
      const two=await stage(dkk,'two-deliberate-partial-losses');need(two.badDebtWriteOffs.length===2&&two.totalBadDebtWrittenOff===625&&two.totalBadDebtWrittenOffDkk===625&&two.principalOpenBalance===625&&two.ledgerBackedClaimBalance===310,'Two DKK losses preserve remaining principal/claim');
      const bank=(await get('bank')).bank.transactions;const bankId=(reference:string)=>{const row=bank.find((b:any)=>b.reference===reference);need(row?.id>0,'Actual referenced bank source');return row.id as number;};
      const foreign=(await post('invoices/settle',{invoiceDocumentId:eur.id,bankTransactionId:bankId('SYN122-EUR-PRINCIPAL'),paymentDate:previousDate,confirm:true})).settlement;
      need(foreign.principalAmountDkk===371.98&&foreign.bankAmountDkk===375.38,'Actual EUR partial cash and carrying relief differ by exact FXgain3.40');
      const eurOpen=await stage(eur,'foreign-principal-part-paid');need(eurOpen.principalOpenBalance===74.95,'Original currency principal before final loss');
      await projections('foreign-paid-before-loss', [{ date: previousDate, count: 2, total: 1807.03 }, { date: today, count: 2, total: 1182.03 }]);
      const partialReadBefore=await snapshot();originalStages.push(await originalQueries.stage(invoices,today,issueDate,{label:'partial-payment-and-loss',open:[625,74.95],paid:[0,50.05],written:[625,0],claim:310}));await unchanged(partialReadBefore,'Original current histories despite earlier asOf');
      const mcp=verifiedWrite(client('mcp','writeoff',writeBody(eur,undefined,'Synthetic reviewed remaining foreign loss 122')),eur,{gross:74.95,net:59.96,vat:14.99,grossDkk:557.03,netDkk:445.62,vatDkk:111.41,remaining:0,remainingDkk:0});
      const eurLost=await stage(eur,'foreign-remainder-written-off');need(eurLost.badDebtWriteOffs.length===1&&eurLost.totalBadDebtWrittenOff===74.95&&eurLost.totalBadDebtWrittenOffDkk===557.03&&eurLost.principalOpenBalance===0,'Foreign original/DKK loss units preserved');
      await post('invoices/settle',{invoiceDocumentId:dkk.id,bankTransactionId:bankId('SYN122-DKK-PRINCIPAL'),paymentDate:today,confirm:true});
      const paid=await stage(dkk,'surviving-principal-paid');need(paid.principalOpenBalance===0&&paid.ledgerBackedClaimBalance===310&&paid.settlementAvailable===true,'Bad-debt relief does not cancel a separate compensation claim');
      await projections('principal-zero-claim-still-open', [{ date: today, count: 0, total: 0 }]);
      await uiClaimReceipt(dkk,bankId('SYN122-DKK-CLAIM'));
      terminalClaims={};for(const invoice of invoices){const v=await parity(invoice);need(v.reminderClaims.principalOpenBalance===0&&v.reminderClaims.ledgerBackedClaimBalance===0,'Terminal principal and claim zero');terminalClaims[invoice.id]=v;}
      const dc=terminalClaims[dkk.id].reminderClaims,ec=terminalClaims[eur.id].reminderClaims;
      need(dc.totalCompensationDkk===310&&dc.totalClaimPayments===310&&dc.claimPayments.length===1&&dc.claimPayments[0].bankTransactionId===bankId('SYN122-DKK-CLAIM')&&dc.totalBadDebtWrittenOff===625&&ec.totalBadDebtWrittenOff===74.95,'Exact compensation and original-currency bad-debt counters');
      for(const [row,receipt]of [[dc.badDebtWriteOffs[0],ui],[dc.badDebtWriteOffs[1],cli],[ec.badDebtWriteOffs[0],mcp]])need(row.writeOffId===receipt.writeOffId&&row.journalEntryId===receipt.journalEntryId&&row.journalEntryNo===receipt.journalEntryNo&&row.writeOffDate===today&&row.grossAmountDkk===receipt.grossAmountDkk&&row.vatAmountDkk===receipt.vatAmountDkk,'Exact persisted loss receipt/journal history');
      const finalBefore=await snapshot();for(const invoice of invoices){await denied('writeoff',writeBody(invoice),finalBefore,'Paid/written-off principal cannot be written off twice');for(const kind of ['cli','mcp']as const){client(kind,'writeoff',writeBody(invoice),{success:false});await unchanged(finalBefore,'Exhausted-principal adapter refusal');}}
      stable=await snapshot();need(stable.journal.journal.entries.length===10&&stable.bank.bank.transactions.length===4&&new Set(stable.documents.documents.documents.map((v:any)=>v.id)).size===4&&stable.trial.trialBalance.balanced===true,'Exactly ten journals four bank rows four distinct original IDs');
      const cents=(value:number|string)=>{const text=String(value);need(/^-?\d+(\.\d{1,2})?$/.test(text),'Exact cent wire amount');const[w,f='']=text.replace('-','').split('.');return(BigInt(w)*100n+BigInt(f.padEnd(2,'0')))*(text.startsWith('-')?-1n:1n);};
      const total=(no:string)=>stable.journal.journal.entries.flatMap((e:any)=>e.lines).filter((l:any)=>l.accountNo===no).reduce((sum:bigint,l:any)=>sum+cents(l.debit)-cents(l.credit),0n);
      need(total(account.debtor)===0n&&total(account.claim)===0n&&total(account.revenue)===-174321n&&total(account.compensation)===-31000n&&total(account.outputVat)===-19939n&&total(account.gain)===-340n&&total(account.expense)===94562n&&total(account.loss)===0n&&total(account.bank)===131763n,'Exact final DKK revenue/VAT/loss/FX/bank accounts');
      const vat=stable.vat.vat;need(vat.vatRegistered===true&&vat.outputVat===435.8&&vat.outputVatAdjustment===-236.41&&vat.inputVat===0&&vat.payable===199.39&&vat.vatReportErrors.length===0,'Current VAT report separates original output435.80 and bad-debt adjustment236.41');
      originalTerminal=await originalQueries.stage(invoices,today,issueDate,{label:'terminal-written-off',open:[0,0],paid:[625,50.05],written:[625,74.95],claim:0});originalStages.push(originalTerminal);await unchanged(stable,'Original terminal query family');
      await originals();reports=await audit();await unchanged(stable,'Originals and full-audit inspection');
      await page.goto(base+'/companies/synthetic/integritet');const card=page.getByRole('region',{name:'Regnskabskontrol',exact:true});const response=page.waitForResponse(r=>r.url()===base+prefix+'audit-verify');await card.getByRole('button',{name:'Kontrollér regnskabet',exact:true}).click();need(isDeepStrictEqual(await(await response).json(),reports),'Web exact full-audit parity');await card.getByRole('status').filter({hasText:'Kontrollen fandt ingen afvigelser.'}).waitFor();await page.screenshot({path:process.env.RM_SCANNER_SCREENSHOT!,fullPage:true});
      const exported=await page.request.post(base+'/api/workspace/backup',{headers:{origin:base},data:{confirm:true}});need(exported.ok()&&exported.headers()['content-type']?.split(';')[0]==='application/vnd.rentemester.native-backup+json','Actual signed bad-debt export');const artifact=await exported.body();need(artifact.length>0&&artifact.length<=96*1024*1024,'Bounded signed backup');writeFileSync(join(work,'bad-debt-source.native-backup.json'),artifact,{mode:0o600,flag:'wx'});await unchanged(stable,'Signed export');finalProjections=await projections('terminal-after-signed-backup',[{date:issueDate,count:2,total:2179.01,eur:125,eurDkk:929.01},{date:previousDate,count:2,total:1807.03,eur:74.95,eurDkk:557.03},{date:today,count:0,total:0}],true);completed=true;
      return{qualified:true,originalInvoiceQueriesQualified:true,originalStages,datedInvoiceReports130:finalProjections,signedBackup:{sha256:hash(artifact),bytes:artifact.length,independentlyVerified:false},invoices,writeOffReceipts:[ui,cli,mcp],stages:steps,refusals,permissions:rights,backup,terminalClaims,fullAudit:reports,vatReport:vat,accountingFingerprint:fingerprint(stable),originalSha256:Object.fromEntries([...originalBytes].map(([key,bytes])=>[key,hash(bytes)])),bankOriginalSha256:Object.fromEntries([...bankBytes].map(([key,bytes])=>[key,hash(bytes)])),allThreeWriteChannelsActualQualified:true,twoPartialLossesWereDistinctReviewedIntents:true,lostAcceptedReplyBlockedNoAutomaticRetry:true,exactHttpCliMcpStatusAndOriginals:true,foreignActualCarryingAndVatQualified:true,legalEstablishmentIndependentlyProven:false,recoveryFeatureQualified:false,foreignInterestQualified:false,sourceFreeRestoreQualified:false,installedNativeQualified:false,realMailOrProviderEffectsQualified:false};
    },
    async reopen(){need(completed,'Completed bad-debt flow required');await cookies(page,ownerCookie,false);const originalReopen=await originalQueries.stage(invoices,today,issueDate,{label:'normal-restart',open:[0,0],paid:[625,50.05],written:[625,74.95],claim:0});need(isDeepStrictEqual(originalReopen.defaultList,originalTerminal.defaultList)&&isDeepStrictEqual(originalReopen.records,originalTerminal.records),'Exact original invoice query restart');for(const invoice of invoices)need(isDeepStrictEqual(await parity(invoice),terminalClaims[invoice.id]),'Normal MFA exact loss/claim restart');need(isDeepStrictEqual(await snapshot(),stable)&&isDeepStrictEqual(await audit(),reports),'Exact books VAT and full-audit after restart');await originals();const reopenedProjections=await projections('normal-restart',[{date:issueDate,count:2,total:2179.01,eur:125,eurDkk:929.01},{date:previousDate,count:2,total:1807.03,eur:74.95,eurDkk:557.03},{date:today,count:0,total:0}],true);need(isDeepStrictEqual(reopenedProjections.cases,finalProjections.cases),'Exact dated FX/loss portfolio and normalized HTML after normal MFA restart');for(const file of [ownerCookie,memberCookie]){const info=lstatSync(file);need(info.isFile()&&!info.isSymbolicLink(),'Owned cookie cleanup');unlinkSync(file);}return{qualified:true,originalInvoiceQueriesQualified:true,originalReopen,datedInvoiceReports130:reopenedProjections,exactNormalMfaRestartState:true,exactOriginalBytesAfterRestart:true,fullAudit119GreenAfterRestart:true,accountingFingerprint:fingerprint(stable),sourceFreeRestoreQualified:false,installedNativeQualified:false};},
  };
}
