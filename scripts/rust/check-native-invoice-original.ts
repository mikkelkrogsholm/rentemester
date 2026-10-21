import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

type Operation = 'list' | 'find' | 'status' | 'overdue';
type Invoice = { id: number; number: string; currency: 'DKK' | 'EUR' };
const need = (v: unknown, label: string) => { if (!v) throw Error(label); };
/** Read-only original public adapters against the existing owned invoice fixture. */
export function originalInvoiceProof(page: Page, base: string, work: string, ownerCookie: string) {
  function client(kind: 'cli' | 'mcp', operation: Operation, query: Record<string, any>, cookieFile: string, denied: boolean) {
    const common = ['--server',base,'--allow-loopback-http','--cookie-file',cookieFile];
    const flags: Record<string,string> = {documentId:'document-id',invoiceNumber:'invoice-number',customerCvr:'customer-cvr',asOf:'as-of',minDays:'min-days',minAmount:'min-amount',maxAmount:'max-amount'};
    let args: string[], input: string | undefined;
    if (kind === 'cli') {
      const q={...query}; const words=operation==='find'&&q.query!==undefined?[String(q.query)]:[];delete q.query;
      // Source CLI find has exact --amount only; SDK/MCP range precedence is
      // covered separately by their focused contract checks.
      if(operation==='find'){delete q.minAmount;delete q.maxAmount;}
      args=['invoice',operation,...words,...common,'--company','synthetic','--json',...Object.entries(q).flatMap(([k,v])=>['--'+(flags[k]??k),String(v)])];
    } else {
      args=common;input=[{jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'synthetic-invoice-original147',version:'1'}}},{jsonrpc:'2.0',method:'notifications/initialized'},{jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'invoice_'+operation,arguments:{company:'synthetic',...query}}}].map(v=>JSON.stringify(v)).join('\n')+'\n';
    }
    const r=spawnSync(join(work,kind==='cli'?'rentemester-rust':'rentemester-mcp'),args,{input,encoding:'utf8',timeout:45000,maxBuffer:16*1024*1024});
    const secret=readFileSync(cookieFile,'utf8').trim();need(!r.error&&!r.stdout.includes(secret)&&!r.stderr.includes(secret)&&r.status===(kind==='cli'&&denied?1:0),'Original invoice client terminal/privacy');
    if(kind==='cli')return JSON.parse(r.stdout);
    const reply=r.stdout.trim().split('\n').map(v=>JSON.parse(v)).find(v=>v.id===2);need(!reply?.error&&reply.result.isError===denied&&isDeepStrictEqual(JSON.parse(reply.result.content[0].text),reply.result.structuredContent),'Original invoice MCP text/structured agreement');
    const v=reply.result.structuredContent;return{ok:v.ok,...(v.data??{}),errors:v.errors,...(v.appliedRules?{appliedRules:v.appliedRules}:{}),...(v.code?{code:v.code}:{})};
  }
  async function read(operation: Operation, query: Record<string,any>, who=page, cookieFile=ownerCookie, denied=false) {
    const response=await who.request.get(base+'/api/companies/synthetic/invoices/original/'+operation+'?'+new URLSearchParams(Object.entries(query).map(([k,v])=>[k,String(v)])));
    const value=await response.json();
    if(denied){need(response.status()===403&&value.ok===false,'Original invoice current revoked read denied');for(const kind of ['cli','mcp'] as const)need(client(kind,operation,query,cookieFile,true).ok===false,'Original revoked client denied');return{refused:true,status:403};}
    need(response.ok()&&value.ok===true&&value.errors.length===0,'Original invoice canonical read');
    const flat={ok:true,...value.data,errors:[]};for(const kind of ['cli','mcp'] as const)need(isDeepStrictEqual(client(kind,operation,query,cookieFile,false),flat),'Original invoice HTTP/CLI/MCP parity');
    return value;
  }
  async function stage(invoices: Invoice[], today: string, issueDate: string, expected: {open:number[]; paid:number[]; written:number[]; claim:number; label:string}) {
    const [dkk,eur]=invoices,records:any[]=[];
    const list=await read('list',{});need(list.data.count===2,'Original list includes both native currencies');
    const fields=['documentId','invoiceNumber','invoiceDate','customerName','customerCvr','buyerEanNumber','buyerPublicRecipient','peppolStatus','grossAmount','currency','openBalance','claimOpenBalance','status','effectiveDueDate','isOverdue','overdueDays'].sort();
    for(let n=0;n<invoices.length;n++){
      const invoice=invoices[n],row=list.data.rows.find((r:any)=>r.documentId===invoice.id);need(row&&isDeepStrictEqual(Object.keys(row).sort(),fields)&&row.currency===invoice.currency&&row.openBalance===expected.open[n]&&row.status===(expected.open[n]>0?'open':'written_off'),'Original invoice exact row/current native principal/status');
      need(row.claimOpenBalance===(invoice.currency==='EUR'?null:expected.open[n]+expected.claim),'Original separate claim currency');
      const status=await read('status',{documentId:invoice.id,asOf:issueDate}),byNumber=await read('status',{invoiceNumber:invoice.number,asOf:issueDate});need(isDeepStrictEqual(status,byNumber),'Original status ID and number resolve exact same invoice');
      need(status.data.openBalance===expected.open[n]&&status.data.paidAmount===expected.paid[n]&&status.data.totalBadDebtWrittenOff===expected.written[n]&&status.data.invoiceNumber===invoice.number&&status.data.asOfDate===issueDate,'Original asOf retains current full payment/writeoff history');
      need(Array.isArray(status.data.payments)&&Array.isArray(status.data.creditNotes)&&Array.isArray(status.data.refunds)&&Array.isArray(status.data.claimPayments)&&Array.isArray(status.data.compensationClaims)&&Array.isArray(status.data.interestClaims),'Original status exposes concrete histories');
      records.push({invoice,status});
    }
    const cases:[Operation,Record<string,any>,number][]=[['list',{customer:' commercial buyer EUR '},1],['list',{customerCvr:' dk12345678 '},2],['list',{invoiceNumber:dkk.number},1],['list',{invoiceNumber:'absent-original147'},0],['list',{minAmount:125,maxAmount:125},1],['list',{from:today,to:issueDate},0],['find',{query:'commercial buyer',amount:125,minAmount:9999,maxAmount:1,asOf:today},1],['overdue',{asOf:today,minDays:0},expected.open.some(v=>v>0)?2:0]];
    for(const[operation,query,count]of cases){const value=await read(operation,query);need(value.data.count===count,'Original invoice source filter/count');records.push({operation,query,value});}
    return{qualified:true,label:expected.label,defaultList:list,records};
  }
  return{read,stage};
}
