import type { Page } from '@playwright/test';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { writeFileSync, readFileSync, unlinkSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

export function createJournalHistoryProof(page: Page, base: string, work: string, member: Page) {
  const prefix = '/api/companies/synthetic/', endpoint = prefix + 'journal/entries';
  const ownerFile = join(work, 'history-owner.cookie'), memberFile = join(work, 'history-member.cookie');
  const fields = ['id','entryNo','transactionDate','text','currency','amountForeign','amountDkk','fxRateToDkk','documentId','sourceBankTransactionId','status','reversalOfEntryId'];
  const snake = ['id','entry_no','transaction_date','text','currency','amount_foreign','amount_dkk','fx_rate_to_dkk','document_id','source_bank_transaction_id','status','reversal_of_entry_id'];
  const need = (v: unknown, message: string) => { if (!v) throw Error(message); };
  const hash = (v: unknown) => createHash('sha256').update(JSON.stringify(v)).digest('hex');
  let stable: any, saved: any[], today: string, firstDate: string, memberId: string, completed = false;
  async function get(path: string, actor = page) { const r = await actor.request.get(base + path); need(r.ok(), 'Journal history canonical read ' + path); return r.json(); }
  async function post(path: string, data: unknown) { const r = await page.request.post(base + path, { headers: { origin: base }, data }); const v = await r.json(); need(r.ok() && v.ok !== false, 'Journal history ordinary reviewed write ' + path); return v; }
  async function cookie(actor: Page, file: string) { const cookies = await actor.context().cookies(base); need(cookies.some(c => c.httpOnly), 'Normal HttpOnly MFA cookie'); writeFileSync(file, cookies.map(c => c.name + '=' + c.value).join('; '), { mode: 0o600 }); }
  const headers = (row: any) => Object.fromEntries(fields.map(k => [k, row[k]]));
  async function read(query: Record<string, string | number> = {}, actor = page) { return (await get(endpoint + '?' + new URLSearchParams(Object.entries(query).map(([k,v]) => [k,String(v)])), actor)).journalHistory; }
  function native(kind: 'cli' | 'mcp', query: Record<string, string | number> = {}, denied = false, file = ownerFile, company = 'synthetic', human = false) {
    const common = ['--server',base,'--cookie-file',file,'--allow-loopback-http'];
    const args = kind === 'cli' ? ['journal','list',...(human ? [] : ['--json']),...common,'--company',company] : common;
    const input = kind === 'mcp' ? [
      {jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-history131',version:'1'}}},
      {jsonrpc:'2.0',method:'notifications/initialized'},
      {jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'journal_list',arguments:{company,...query}}},
    ].map(v => JSON.stringify(v)).join('\n') + '\n' : '';
    const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), args, {input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
    const secret = readFileSync(file,'utf8').trim(); need(!r.error && !r.stdout.includes(secret) && !r.stderr.includes(secret),'Journal clients terminal credential privacy');
    if (kind === 'cli') { need(r.status === (denied ? 1 : 0),'Journal CLI exact success/refusal exit'); return denied ? null : human ? r.stdout : JSON.parse(r.stdout); }
    need(r.status === 0,'Journal MCP transport terminal'); const value = r.stdout.trim().split('\n').map(line => JSON.parse(line)).find(v => v.id === 2)?.result;
    need(value?.isError === denied && value.structuredContent?.ok === !denied,'Journal MCP exact business envelope'); return denied ? null : value.structuredContent.data;
  }
  async function books() {
    return {journal:await read({all:'true'}),trial:await get(prefix+'trial-balance'),bank:await get(prefix+'bank'),documents:await get(prefix+'documents'),audit:await get(prefix+'audit-log?limit=5000')};
  }
  async function unchanged() { need(isDeepStrictEqual(await books(),stable),'Journal history reads must preserve books, sources and company audit'); }
  async function audit() { const v = await get(prefix+'audit-verify'); need(v.audit?.available && v.audit.ok && v.audit.entries === 3 && v.audit.components.length === 5 && v.audit.components.every((c:any) => c.available && c.ok && c.errors.length === 0),'Complete three-entry accounting/source audit'); return v; }
  async function projections() {
    const cases: Record<string,string|number>[] = [{},{from:firstDate,to:firstDate},{status:'posted'},{status:'reversed'},{limit:1,offset:0},{limit:1,offset:1},{limit:1,offset:2},{limit:1,offset:3},{from:today,to:firstDate}];
    const out = [];
    for (const query of cases) { const canonical = await read(query); need(isDeepStrictEqual(native('mcp',query),canonical),'Exact MCP headers and original pagination'); out.push({query,page:canonical}); }
    const all = await read({all:'true'}); need(all.entries.length === 3 && all.total === 3 && all.count === 3 && all.limit === 3 && all.offset === 0 && !all.hasMore,'One complete all-years snapshot');
    need(all.entries[0].status === 'reversed' && all.entries[1].status === 'posted' && all.entries[2].status === 'posted','Stored correction status preserves original posted status');
    need(all.entries[2].transactionDate === firstDate && firstDate.slice(0,4) !== today.slice(0,4),'All-years history includes older native year');
    for (const row of all.entries) need(row.currency === 'DKK' && [row.amountForeign,row.amountDkk,row.fxRateToDkk,row.documentId,row.sourceBankTransactionId].every(v => v === null),'Exact nullable ordinary DKK/source header values');
    const expected = all.entries.map((row:any) => Object.fromEntries(fields.map((k,i) => [snake[i],row[k]])));
    const cli = native('cli'); need(Array.isArray(cli) && isDeepStrictEqual(cli,expected) && cli.every(row => Object.keys(row).length === 12),'Original CLI bare snake_case array, no envelope or truncation');
    const human = native('cli',{},false,ownerFile,'synthetic',true); need(human.includes('Finansposteringer (3)') && human.includes('7,25 kr.') && human.includes('10,02 kr.'),'Retained human journal labels and canonical debit fallback');
    need(out[1].page.total === 1 && out[2].page.total === 2 && out[3].page.total === 1 && out[7].page.count === 0 && out[8].page.total === 0,'Inclusive date/stored-status/empty and reversed-range contracts');
    need(new Set(out.slice(4,7).map(v => v.page.entries[0].id)).size === 3 && out[4].page.nextOffset === 1 && !out[6].page.hasMore,'Stable complete pagination without overlap');
    await unchanged(); return {cases:out,allFingerprint:hash(all),cliFingerprint:hash(cli),humanQualified:true};
  }
  return {
    async run() {
      need(!completed,'Journal fixture executes once'); await cookie(page,ownerFile); await cookie(member,memberFile);
      memberId = (await get('/api/me',member)).user.id;
      const portfolio = await get('/api/portfolio'); today = portfolio.portfolio.asOf; need(/^\d{4}-\d{2}-\d{2}$/.test(today),'Trusted dated portfolio'); firstDate = String(Number(today.slice(0,4))-1) + '-12-31';
      for (const [accountNo,type,normalBalance] of [['1731','asset','debit'],['8731','equity','credit']]) await post(prefix+'accounts',{accountNo,name:'Synthetic journal history '+accountNo,type,normalBalance});
      const initial = (await get(prefix+'journal')).journal; const version = initial.nativePosting.ruleVersion; const receipts = [];
      for (const [date,amount,key] of [[firstDate,'7.25','a'],[today,'10.02','b']]) receipts.push(await post(prefix+'manual-journal',{confirm:true,input:{format:'rentemester-manual-journal-post/1',expectedRuleVersion:version,idempotencyKey:'synthetic-history131-'+key,request:{transactionDate:date,text:'Synthetic history '+key,currency:'DKK',lines:[{accountNo:'1731',debitAmount:amount,creditAmount:'0.00'},{accountNo:'8731',debitAmount:'0.00',creditAmount:amount}]}}}));
      const reversal = await post(prefix+'manual-journal-reversal',{confirm:true,input:{format:'rentemester-manual-journal-reversal/1',expectedRuleVersion:version,idempotencyKey:'synthetic-history131-reversal',request:{entryId:String(receipts[0].entryId),transactionDate:today,reason:'Synthetic journal history reversal'}}});
      need(reversal.originalEntryId === receipts[0].entryId,'Exact original/reversal receipt');
      stable = await books(); need(stable.trial.trialBalance.balanced,'Canonical books balanced');
      const proof = await projections(); saved = proof.cases;
      need(isDeepStrictEqual(await read({},member),await read()),'Current ordinary invited reader gets exact headers');
      for (const raw of ['limit=0','limit=5001','offset=-1','from=2026-02-30','limit=1&limit=2','all=true&offset=0','source=archive','company=other','status=active']) { const r = await page.request.get(base+endpoint+'?'+raw); need(r.status() === 400,'Malformed/unknown selector refusal'); }
      need((await page.request.get(base+'/api/companies/missing-history131/journal/entries')).status() === 404,'Unknown company refused');
      await post('/api/workspace/members/company',{userId:memberId,action:'revoke',companySlug:'synthetic'});
      need((await member.request.get(base+endpoint)).status() === 403,'Current membership revocation refuses retained session');
      native('cli',{},true,memberFile); native('mcp',{},true,memberFile);
      await unchanged(); const fullAudit = await audit(); await unchanged(); completed = true;
      return {qualified:true,ordinaryMfa:true,readOnly:true,exactCliBareSnakeArray:true,exactMcpOriginalPage:true,crossYearHeaders:true,currentRevocationRefused:true,projections:proof,fullAudit,accountingFingerprint:hash(stable),receipts,reversal,sourceFreeRestoreQualified:false,installedNativeQualified:false};
    },
    async reopen() {
      need(completed,'Completed journal fixture required'); await cookie(page,ownerFile);
      need(isDeepStrictEqual(await books(),stable),'Exact admitted books and company audit after normal MFA restart');
      const proof = await projections(); need(isDeepStrictEqual(proof.cases,saved),'Exact dated/status/paged history after normal MFA restart'); const fullAudit = await audit(); await unchanged();
      unlinkSync(ownerFile); unlinkSync(memberFile);
      return {qualified:true,ordinaryMfa:true,exactNormalMfaRestartState:true,projections:proof,fullAudit,accountingFingerprint:hash(stable),sourceFreeRestoreQualified:false,installedNativeQualified:false};
    },
  };
}
