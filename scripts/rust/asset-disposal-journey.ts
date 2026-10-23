import { Database } from 'bun:sqlite';
import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Work-Issue #698. New actual disposal flow only, not a replay of original170.
 * Called by the existing normal-MFA owned-server lifecycle. Every write is sent
 * once. Only explicit verified receipt reconciliation permits same-key replay.
 */
export function createAssetDisposalProof(base: string, work: string, onStep: (name: string) => void) {
  const root = '/api/companies/synthetic/', original = root + 'asset/original/';
  const cookie = join(work, 'disposal698.cookie'), serviceKey = join(work, 'disposal698.service');
  const observations: any[] = [], cases: any[] = [], checks: Record<string, boolean> = {};
  const operationCounts: Record<string, number> = {};
  let owner: Page, today: string, originals: any[], stable: any, ownerId: string, adopted = false, ran = false;
  const need = (v: unknown, message: string) => { if (!v) throw Error('asset_disposal698: ' + message); };
  const hash = (bytes: Buffer) => createHash('sha256').update(bytes).digest('hex');
  function record(value: any) {
    observations.push(value);
    writeFileSync(join(work, 'disposal698-observations.json'), JSON.stringify(observations, null, 2), { mode: 0o600 });
  }
  async function cookies() {
    writeFileSync(cookie, (await owner.context().cookies(base)).map(c => c.name + '=' + c.value).join('; '), { mode: 0o600 });
  }
  async function http(path: string, data?: any, expected = 200, page = owner) {
    onStep((data === undefined ? 'GET:' : 'POST:') + path);
    const r = data === undefined ? await page.request.get(base + path) : await page.request.post(base + path, { headers: { origin: base }, data });
    const text = await r.text();
    let value: any; try { value = JSON.parse(text); } catch { record({path, status:r.status(), text}); throw Error('asset_disposal698: non-JSON; outcome unknown, never retry'); }
    record({ path, data, status: r.status(), value: path.endsWith('/service-principals') ? { ...value, secret: '[redacted]' } : value });
    need(r.status() === expected, `${path} expected HTTP${expected}, captured HTTP${r.status()}`);
    need(value.ok === (expected === 200 || expected === 201), 'explicit canonical outcome ' + path);
    return value;
  }
  async function refused(path: string, data: any, pattern: RegExp, page = owner) {
    onStep('refusal:' + path);
    const r = await page.request.post(base + path, { headers: { origin: base }, data });
    const text = await r.text(); record({path, data, status:r.status(), text});
    need([403,404,409,422].includes(r.status()), 'business refusal HTTP, not parse/server error');
    const value = JSON.parse(text);
    need(value.ok === false && pattern.test(text), 'specific refusal evidence: ' + pattern);
    return value;
  }
  function invoke(kind: 'cli' | 'mcp', group: string, op: string, args: any = {}, service = false, failure = false) {
    onStep(kind + ':' + group + ':' + op);
    operationCounts[kind + ':' + group + ':' + op] = (operationCounts[kind + ':' + group + ':' + op] ?? 0) + 1;
    const common = ['--server',base,'--allow-loopback-http', service ? '--service-key-file' : '--cookie-file', service ? serviceKey : cookie];
    let argv: string[], input: string | undefined;
    if (kind === 'cli') {
      argv = [group,op,...common,'--company','synthetic','--json',...Object.entries(args).flatMap(([k,v]) => ['--' + k.replace(/[A-Z]/g,c=>'-'+c.toLowerCase()), v === true ? 'yes' : String(v)])];
    } else {
      argv = common;
      input = [{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'asset-disposal698',version:'1'}}},
        {jsonrpc:'2.0',method:'notifications/initialized'},
        {jsonrpc:'2.0',id:2,method:'tools/call',params:{name:group+'_'+op.replaceAll('-','_'),arguments:{company:'synthetic',...args}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';
    }
    const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), argv, {input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
    const secret = readFileSync(service ? serviceKey : cookie,'utf8').trim();
    need(!r.stdout.includes(secret) && !r.stderr.includes(secret), 'credential privacy');
    record({kind,group,op,args,service,exit:r.status,stdout:r.stdout,stderr:r.stderr,error:r.error?.message});
    need(!r.error && r.status === (kind === 'cli' && failure ? 1 : 0), 'client terminal; unknown write never retried');
    let value: any;
    if (kind === 'cli') value = JSON.parse(r.stdout);
    else {
      const result = r.stdout.trim().split('\n').map(l=>JSON.parse(l)).find(v=>v.id===2)?.result;
      need(result && result.isError === failure && equal(JSON.parse(result.content[0].text), result.structuredContent), 'MCP exact text/structured outcome');
      value = result.structuredContent;
      if (!failure && value.data && typeof value.data === "object") value = {ok:true,...value.data,errors:value.errors,...(value.appliedRules ? {appliedRules:value.appliedRules} : {})};
    }
    need(value.ok === !failure, 'explicit client outcome'); return value;
  }
  const path = (id: number, suffix: string) => root + `assets/${id}/` + suffix;
  async function snapshot(page = owner) {
    const v: any = {};
    for (const p of ['journal','assets','trial-balance','documents', 'asset/original/register-report']) v[p] = await http(root+p,undefined,200,page);
    return v;
  }
  async function sources(page = owner) {
    for (const d of originals) {
      const r = await page.request.get(base+root+`documents/${d.id}/file`);
      need(r.ok() && hash(await r.body()) === d.sha256, 'retained physical original exact bytes');
    }
  }
  function receipt(value: any, c: any, replayed: boolean) {
    const r = value.disposal;
    need(value.ok === true && r?.assetId === c.id && r.cost === c.cost && r.accumulatedDepreciation === c.depreciation && r.carryingValueWrittenOff === c.cost-c.depreciation && r.remainingCarryingValue === 0 && r.replayed === replayed && typeof r.journalEntryNo === 'string' && /^[a-f0-9]{64}$/.test(r.journalEntryHash), 'exact frozen disposal receipt');
    if (c.receipt) need(equal({...r,replayed:false}, {...c.receipt,replayed:false}), 'same original disposal and journal identity');
    return r;
  }
  function businessInventory() {
    const root = join(work, existsSync(join(work,'workspace')) ? 'workspace' : 'restored');
    const file = join(root,'synthetic/data/ledger.sqlite');
    need(!lstatSync(root).isSymbolicLink() && !lstatSync(file).isSymbolicLink(), 'owned regular ledger inspection');
    const db = new Database(file,{readonly:true});
    try {
      const state: any = {};
      for (const table of ['documents','journal_entries','journal_lines','audit_log','native_asset_disposals','mutation_idempotency_tombstones','mutation_idempotency_outcomes']) state[table] = db.query('SELECT * FROM '+table).all();
      return state;
    } finally { db.close(); }
  }
  async function preview(c: any) {
    const before = await snapshot(), raw = businessInventory();
    const p = (await http(path(c.id,'disposal/preview'),c.input)).disposalPreview;
    need(equal(await snapshot(),before) && equal(businessInventory(),raw), 'preview creates no journal/audit/receipt/document writes');
    need(p.assetId === c.id && p.cost === c.cost && p.accumulatedDepreciation === c.depreciation && p.carryingValue === c.cost-c.depreciation && /^[a-f0-9]{64}$/.test(p.planHash), 'server exact preview amounts/hash');
    need(p.lossAccountNo === (c.depreciation === c.cost ? null : '3000'), 'unused full-depreciation account explicit null');
    const lines = [['5800',0,c.cost],...(c.depreciation ? [['5810',c.depreciation,0]] : []),...(c.cost-c.depreciation ? [['3000',c.cost-c.depreciation,0]] : [])];
    need(equal(p.lines.map((l:any)=>[l.accountNo,l.debit,l.credit]).sort(),lines.sort()),'exact zero-omitting clearing lines');
    c.lines = lines; c.preview = p; c.body = {...c.input,expectedPlanHash:p.planHash,idempotencyKey:'synthetic-disposal698-'+c.name,confirm:true};
    need(equal(invoke('cli','asset','disposal-preview',{assetId:c.id,...c.input}).disposalPreview,p),'CLI uses same server preview');
    need(equal(businessInventory(),raw),'CLI preview also inert');
    return p;
  }
  async function journals() {
    const entries = (await http(root+'journal')).journal.entries;
    for (const c of cases) {
      const entry = entries.find((e:any)=>e.id === c.receipt.journalEntryId);
      need(entry?.documentId === c.input.documentId && equal(entry.lines.map((l:any)=>[l.accountNo,l.debit,l.credit]).sort(),c.lines), 'actual durable journal with exact source and lines');
    }
    need(entries.length === 10 && entries.reduce((n:number,e:any)=>n+e.lines.length,0) === 21, 'ten actual journals / twenty-one lines, no duplicate');
    const report = await http(original+'register-report');
    need(report.assets.length === 3 && report.assets.every((a:any)=>a.status === 'disposed' && a.netBookValue === 0) && report.totals.cost === 125 && report.totals.accumulatedDepreciation === 70 && report.totals.netBookValue === 0, 'retained source reports reconcile historical cost/depreciation');
    need(equal(invoke('cli','asset','register-report'),report), 'existing CLI report matches disposed server report exactly');
    need(equal(invoke('mcp','asset','register-report'),report), 'existing MCP report matches disposed server report exactly');
    for (const c of cases) {
      const next=(await http(path(c.id,'next-depreciation'))).nextDepreciation;
      need(next.nextPeriodIndex === null && next.remainingPeriods === 0, 'disposed next schedule cannot promise another posting');
    }
    return entries;
  }
  async function run(page: Page, supplied: any[], date: string, reader: Page) {
    need(!ran && supplied.length === 4, 'one new bounded synthetic run'); ran = true;
    owner = page; originals = supplied; today = date;
    ownerId = (await http('/api/me')).user.id; await cookies();
    for (const [accountNo,type,normalBalance] of [['5800','asset','debit'],['5810','asset','credit'],['5820','expense','debit']]) await http(root+'accounts',{accountNo,name:'Synthetic disposal698 '+accountNo,type,normalBalance});
    const fixtures: any[] = [];
    for (const [name,cost,periods] of [['partial',30,1],['fullyDepreciated',60,3],['noDepreciation',35,0]] as const) {
      const purchase = originals[fixtures.length];
      const version = (await http(root+'journal')).journal.nativePosting.ruleVersion;
      await http(root+'manual-journal',{confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:version,idempotencyKey:'synthetic-disposal698-acquire-'+name,request:{transactionDate:today,text:'Synthetic disposal698 '+name,currency:'DKK',documentId:String(purchase.id),lines:[{accountNo:'5800',debitAmount:cost.toFixed(2),creditAmount:'0.00'},{accountNo:'1800',debitAmount:'0.00',creditAmount:cost.toFixed(2)}]}}});
      const registered = invoke('cli','asset','register',{name:'Synthetic disposal698 '+name,category:'equipment',acquisitionDate:today,cost,usefulLifeMonths:3,documentId:purchase.id});
      const c: any = {id:registered.assetId,name,cost,depreciation:0,input:{transactionDate:today,documentId:originals[3].id,reason:'Synthetic retirement '+name,lossAccountNo:'3000'}};
      need(Number.isSafeInteger(c.id) && c.id>0,'canonical asset ID');
      fixtures.push({...c,periods});
    }
    // All three allocations are proven before any derecognition clears cost.
    // Shared purchase evidence remains exactly 125, no duplicate allocation.
    for (const c of fixtures) {
      const {name,periods} = c;
      if (name === 'partial') {
        const old = await preview(c);
        invoke('mcp','asset','depreciate',{assetId:c.id,period:1,date:today,confirm:true}); c.depreciation=10;
        await refused(path(c.id,'dispose'),{...c.body,expectedPlanHash:old.planHash},/accounting|stale/i);
        checks.stalePlanRefused=true;
      } else for (let period=1;period<=periods;period++) { invoke('mcp','asset','depreciate',{assetId:c.id,period,date:today,confirm:true}); c.depreciation+=20; }
      await preview(c);
      if (name === 'noDepreciation') {
        const bounds = {from:today,to:today};
        const packet = invoke('cli','period','readiness',bounds).packet;
        need(packet.blockers === 0, 'ordinary period readiness, no bypass');
        const review = invoke('cli','period','review',{...bounds,packetHash:packet.hash,confirm:true});
        invoke('cli','period','close',{...bounds,reviewId:review.id,packetHash:packet.hash,kind:'custom',confirm:true});
        const before = await snapshot();
        await refused(path(c.id,'dispose'),c.body,/period|accounting/i);
        need(equal(await snapshot(),before),'closed period refusal leaves books exact');
        invoke('cli','period','reopen',{...bounds,kind:'custom',reason:'Synthetic disposal698 resume reviewed fixture'});
        checks.closedPeriodRefused = true;
        await preview(c);
      }
      if (name === 'partial') {
        const beforeEvidence = await snapshot();
        await refused(path(c.id,'disposal/preview'),{...c.input,documentId:99999999},/document|evidence|original|accounting/i); need(equal(await snapshot(),beforeEvidence),'missing evidence inert'); checks.evidenceRefused=true;
        await refused(path(c.id,'dispose'),c.body,/permission|forbidden|denied|refused|access_denied/i,reader);
        // Qualify actual revoked credential separately from an insufficient role.
        const svc = await http('/api/workspace/service-principals',{displayName:'Synthetic disposal698',operationId:randomUUID(),confirm:true},201);
        writeFileSync(serviceKey,svc.secret+'\n',{mode:0o600,flag:'wx'});
        await http('/api/workspace/members/access',{userId:svc.serviceAccountId,action:'set-role',workspaceRole:'member'});
        await http('/api/workspace/members/company',{userId:svc.serviceAccountId,companySlug:'synthetic',action:'grant',role:'bookkeeper'});
        invoke('mcp','asset','disposal-preview',{assetId:c.id,...c.input},true);
        await http('/api/workspace/service-principals/revoke',{serviceAccountId:svc.serviceAccountId,credentialId:svc.credentialId,operationId:randomUUID(),confirm:true});
        const denied=invoke('cli','asset','dispose',{assetId:c.id,...c.body},true,true); need(/401|403|invalid_credentials|revoked/i.test(JSON.stringify(denied)), 'actual revoked credential refusal, not unrelated client failure'); checks.revokedRefused=true;
        // Real React review, explicit ConfirmDialog, capture its generated key.
        await owner.goto(base+'/companies/synthetic/anlaeg');
        await owner.getByRole('button',{name:'Afhænd Synthetic disposal698 partial',exact:true}).click();
        const dialog = owner.getByRole('dialog',{name:'Afhænd anlæg: Synthetic disposal698 partial',exact:true});
        await dialog.getByLabel('Bogføringsdato',{exact:true}).fill(today);
        await dialog.getByLabel('Bilag for afhændelse',{exact:true}).selectOption(String(originals[3].id));
        await dialog.getByLabel('Tabskonto (påkrævet ved positiv restværdi)',{exact:true}).selectOption('3000');
        await dialog.getByLabel('Begrundelse',{exact:true}).fill(c.input.reason);
        await dialog.getByRole('button',{name:'Forhåndsvis afhændelse',exact:true}).click();
        await dialog.getByText(c.preview.planHash,{exact:true}).waitFor();
        await dialog.getByRole('button',{name:'Gennemgå og bekræft',exact:true}).click();
        const confirmed = owner.waitForResponse(r=>new URL(r.url()).pathname===path(c.id,'dispose') && r.request().method()==='POST');
        await owner.getByRole('dialog',{name:'Bekræft afhændelse: '+c.id,exact:true}).getByRole('button',{name:'Bogfør afhændelse',exact:true}).click();
        const response = await confirmed, text = await response.text(); record({step:'React-confirm',status:response.status(),text});
        need(response.status()===200,'React explicit apply outcome; unknown is never resent');
        c.body=response.request().postDataJSON(); c.receipt=receipt(JSON.parse(text),c,false);
        await owner.screenshot({path:process.env.RM_BATCH_SCREENSHOT,fullPage:true});
        await owner.reload(); await owner.getByRole('button',{name:'Hent afhændelseskvittering',exact:true}).first().waitFor();
      } else c.receipt=receipt(invoke('mcp','asset','dispose',{assetId:c.id,...c.body}),c,false);
      cases.push(c); checks[name]=true;
      receipt(await http(path(c.id,'disposal-receipt')+'?key='+encodeURIComponent(c.body.idempotencyKey)),c,true);
      receipt(invoke('cli','asset','dispose',{assetId:c.id,...c.body}),c,true);
      await refused(path(c.id,'dispose'),{...c.body,reason:c.input.reason+' changed'},/intent|key|payload|conflict|refused/i);
      await refused(original+c.id+'/depreciate',{period:2,date:today,confirm:true},/disposed|disposal|refused|accounting/i);
      await refused(root+'journal/original/reverse',{entryId:c.receipt.journalEntryId,transactionDate:today,reason:'Synthetic forbidden correction',confirm:true},/asset|reverse|reversal|refused/i);
    }
    checks.sameKeyReplay=true; checks.changedIntentRefused=true; checks.laterDepreciationRefused=true; checks.correctionRefused=true;
    checks.cliMcpSharedWeb=true;
    const entries=await journals(); await sources(); stable=await snapshot();
    return {checks,operationCounts,journals:entries,cases,originals};
  }
  async function assertStable(page: Page) {
    need(equal(await snapshot(page),stable),'exact books/history/originals after restart/restore'); await sources(page);
    return {exact:true};
  }
  async function reopen(page: Page) {
    owner=page; await assertStable(page); await cookies();
    for (const c of cases) receipt(invoke('cli','asset','disposal-receipt',{assetId:c.id,key:c.body.idempotencyKey}),c,true);
    return {exact:true};
  }
  async function reconcile(page: Page) {
    need(!adopted, 'one explicit receipt reconciliation'); owner = page; await cookies();
    const me = (await http('/api/me')).user.id;
    need(me !== ownerId, 'fresh restore identity, no automatic historical mapping');
    const inventory = await http(root+'restore/receipts');
    const rows = inventory.restoredReceipts.rows.filter((r:any)=>r.operation === 'asset_dispose');
    need(rows.length === 3 && rows.every((r:any)=>r.mapping === null && r.resultAvailable), 'three retained unmapped disposal receipts');
    for (const c of cases) {
      await http(path(c.id,'disposal-receipt')+'?key='+encodeURIComponent(c.body.idempotencyKey),undefined,409);
      const row = rows.find((r:any)=>r.result?.disposal?.disposalId === c.receipt.disposalId);
      need(row && row.result.disposal.journalEntryHash === c.receipt.journalEntryHash, 'review exact retained journal receipt');
      const mapped = await http(root+'restore/receipts/reconcile',{receiptId:row.receiptId,expectedStateHash:row.stateHash,reason:'Synthetic owner reviewed retained disposal and original journal',confirm:true});
      need(mapped.restoredReceipts.rows.find((r:any)=>r.receiptId===row.receiptId)?.mapping?.targetUserId === me, 'explicit current human mapping');
      receipt(invoke('cli','asset','disposal-receipt',{assetId:c.id,key:c.body.idempotencyKey}),c,true);
      receipt(invoke('mcp','asset','dispose',{assetId:c.id,...c.body}),c,true);
    }
    adopted=true; await assertStable(page); await journals();
    return {mappedOriginalKeyReplay:true};
  }
  return {run,assertStable,reopen,reconcile};
}
