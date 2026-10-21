import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import type { Page } from "@playwright/test";
import { readTar } from "../../src/core/tar";

const saved = "/tmp/rentemester-rust-reports/native-saft.json";
const profile = "rentemester-dk-saft-v3-ledger-sales-purchases-masterfiles";
function unpack(bytes: Buffer) {
  const entries = readTar(bytes), files = new Map(entries.map(e => [e.path.split("/").slice(1).join("/"), Buffer.from(e.content)]));
  if (files.size !== 3 || !files.has("README.txt") || !files.has("manifest.json") || !files.has("saft.xml")) throw new Error("SAF-T package changed file scope");
  const manifest = JSON.parse(files.get("manifest.json")!.toString("utf8"));
  for (const o of manifest.outputs as Array<{path:string;sha256:string;sizeBytes:number}>) {
    const file=files.get(o.path); if (!file || file.length!==o.sizeBytes || createHash("sha256").update(file).digest("hex")!==o.sha256) throw new Error("SAF-T manifest hash mismatch");
  }
  if (manifest.profileId!==profile || manifest.manifestVersion!==2 || manifest.generatedAtSource!=="trusted_runtime_clock" || manifest.provenance!==null || manifest.sourceContext.journalHashChain.ok!==true || !manifest.outOfScope.includes("official_schema_submission")) throw new Error("SAF-T profile/source/provenance contract mismatch");
  const xml=files.get("saft.xml")!.toString("utf8");
  return {manifest,xml,stableXml:xml.replace(/<DateCreated>[^<]*<\/DateCreated>/,"<DateCreated>trusted-runtime-clock</DateCreated>")};
}
/** Standalone acceptance: existing normal MFA browser session; call initial/restart, no harness edits. */
export async function checkNativeSaft(page:Page,base:string,restarted=false){
 const url=`${base}/api/companies/synthetic`,headers={origin:base,"content-type":"application/json"};
 const overview=(await(await page.request.get(`${url}/overview`)).json()).overview;
 const {periodStart,periodEnd,selectedYear}=overview;
 const get=async(path:string)=>{const r=await page.request.get(`${url}/${path}`);if(!r.ok())throw new Error(`SAF-T canonical read refused ${path}`);return r.json();};
 const before=(await get(`journal?year=${encodeURIComponent(selectedYear)}`)).journal;
 const tb=(await get(`trial-balance?year=${encodeURIComponent(selectedYear)}`)).trialBalance;
 const accounts=(await get("accounts")).accounts;
 for(const data of [{periodStart:"2026-02-30",periodEnd,confirm:true},{periodStart,periodEnd,confirm:false},{periodStart,periodEnd,confirm:true,generatedAt:"2000-01-01T00:00:00Z"}]){
  const r=await page.request.post(`${url}/saft-export`,{headers,data});if(![400,422].includes(r.status()))throw new Error("SAF-T invalid dates/confirmation/caller clock accepted");
 }
 const wrong=await page.request.post(`${base}/api/companies/nonexistent/saft-export`,{headers,data:{periodStart,periodEnd,confirm:true}});if(wrong.status()!==404)throw new Error("SAF-T company boundary failed");
 await page.goto(`${base}/companies/synthetic`);
 const card=page.locator(".accountant-export");await card.getByLabel("Fra",{exact:true}).fill(periodStart);await card.getByLabel("Til",{exact:true}).fill(periodEnd);
 const [download,response]=await Promise.all([page.waitForEvent("download"),page.waitForResponse(r=>r.url().endsWith("/saft-export")&&r.request().method()==="POST"),card.getByRole("button",{name:"SAF-T v3 — generér og download",exact:true}).click()]);
 if(!response.ok()||response.headers()["content-type"]!=="application/x-tar"||response.headers()["cache-control"]!=="private, no-store")throw new Error("Native SAF-T browser download failed");
 const temp=mkdtempSync("/tmp/rm-native-saft-");
 try{
  const path=join(temp,"package.tar");await download.saveAs(path);const browser=unpack(readFileSync(path));
  const parsed=await page.evaluate(xml=>{const doc=new DOMParser().parseFromString(xml,"application/xml");if(doc.querySelector("parsererror"))throw new Error("Malformed native SAF-T XML");const text=(tag:string)=>Array.from(doc.getElementsByTagName(tag),n=>n.textContent);return {accounts:text("AccountID"),transactions:text("TransactionID"),sources:text("SourceDocumentID"),profile:text("ProfileID"),quantities:text("Quantity"),sales:doc.getElementsByTagName("SalesInvoices")[0]?.children.length,purchases:doc.getElementsByTagName("PurchaseInvoices")[0]?.children.length};},browser.xml);
  if(parsed.profile[0]!==profile||parsed.transactions.length!==before.entries.length||browser.manifest.counts.journalEntries!==before.entries.length)throw new Error("SAF-T omitted selected ledger entries");
  const chart=Array.isArray(accounts)?accounts:accounts.accounts;
  for(const a of chart)if(!parsed.accounts.includes(a.accountNo))throw new Error("SAF-T omitted chart account");
  const source=browser.manifest.sourceContext;
  if(source.trialBalance.totalDebit!==String(tb.totalDebit.toFixed(2))||source.trialBalance.totalCredit!==String(tb.totalCredit.toFixed(2)))throw new Error("SAF-T differs from shared trial balance");
  for(const d of source.documents){const r=await page.request.get(`${url}/documents/${d.id}/file`);if(!r.ok()||createHash("sha256").update(await r.body()).digest("hex")!==d.sha256)throw new Error("SAF-T source-original evidence mismatch");}
  for(const row of source.journalEntries){if(!row.registrationDatetime||!row.createdBy||!row.ruleVersion||!row.hash||!Array.isArray(row.lines))throw new Error("SAF-T omitted original registration/audit/rule/hash context");if(row.documentId&&!parsed.sources.includes(row.documentNo))throw new Error("SAF-T missing source document reference");}
  // Each export has its own authoritative clock and audit event. Compare the XML accounting projection, never forge deterministic runtime timestamps.
  const cookieFile=join(temp,"session.cookie");writeFileSync(cookieFile,(await page.context().cookies(base)).map(c=>`${c.name}=${c.value}`).join("; "),{mode:0o600,flag:"wx"});
  const common=["--server",base,"--allow-loopback-http","--cookie-file",cookieFile],body=JSON.stringify({periodStart,periodEnd,confirm:true});
  const cli=spawnSync(process.env.RM_NATIVE_CLI_BIN??resolve("target/debug/rentemester-rust"),["remote","saft-export",...common,"--company","synthetic","--confirm"],{input:body,encoding:"utf8",maxBuffer:128*1024*1024});if(cli.error||cli.status!==0)throw new Error("SAF-T actual CLI failed");
  const input=[{jsonrpc:"2.0",id:1,method:"initialize",params:{protocolVersion:"2025-11-25",capabilities:{},clientInfo:{name:"synthetic-saft",version:"1"}}},{jsonrpc:"2.0",method:"notifications/initialized"},{jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"saft_export",arguments:{company:"synthetic",body,confirm:true}}}].map(v=>JSON.stringify(v)).join("\n")+"\n";
  const mcp=spawnSync(process.env.RM_NATIVE_MCP_BIN??resolve("target/debug/rentemester-mcp"),common,{input,encoding:"utf8",maxBuffer:128*1024*1024});if(mcp.error||mcp.status!==0)throw new Error("SAF-T actual MCP failed");const call=mcp.stdout.trim().split("\n").map(l=>JSON.parse(l)).find(v=>v.id===2);if(call?.error||call?.result?.isError)throw new Error("SAF-T MCP tool refused");
  for(const value of [JSON.parse(cli.stdout).data,call.result.structuredContent.data]){if(value.export.contentType!=="application/x-tar")throw new Error("SAF-T adapter MIME mismatch");const candidate=unpack(Buffer.from(value.export.bytes));if(candidate.stableXml!==browser.stableXml)throw new Error("SAF-T HTTP/browser/CLI/MCP accounting projection differs");}
  const after=(await get(`journal?year=${encodeURIComponent(selectedYear)}`)).journal;if(JSON.stringify(after)!==JSON.stringify(before))throw new Error("SAF-T export changed ledger");
  mkdirSync("/tmp/rentemester-rust-reports",{recursive:true});
  const evidence={periodStart,periodEnd,stableXml:browser.stableXml,journal:source.journalEntries,documents:source.documents,accounts:browser.manifest.counts.accounts};
  if(!restarted)writeFileSync(saved,JSON.stringify(evidence),{mode:0o600});else if(JSON.stringify(JSON.parse(readFileSync(saved,"utf8")))!==JSON.stringify(evidence))throw new Error("SAF-T source projection changed after restart");
  console.log(`PASS: native SAF-T v3 browser/CLI/MCP tar, exact XML/manifest/source originals/shared trial balance; ledger unchanged${restarted?" after restart":""}; no official validation/submission claim`);
 }finally{rmSync(temp,{recursive:true,force:true});}
}

/** Original CLI packaging and explicit export timestamps through the shared server. */
export function createSaftArtifactProof(page: Page, base: string, work: string, member: Page) {
  const prefix = '/api/companies/synthetic/', ownerFile = join(work, 'saft-owner.cookie'), memberFile = join(work, 'saft-member.cookie');
  const need = (v: unknown, message: string) => { if (!v) throw Error(message); };
  const sha = (v: Buffer | string) => createHash('sha256').update(v).digest('hex');
  const original = Buffer.from('Synthetic SAF-T source 100 DKK\n');
  let input: any, stable: any, savedProjection: any, documentId: number, originalPath: string, exports = 0, sequence = 0, completed = false;
  const explicit = '2001-02-03T04:05:06.789123+02:00', explicitUtc = '2001-02-03T02:05:06.789Z';
  async function get(path: string, actor = page) { const r = await actor.request.get(base + path); need(r.ok(), 'SAF-T canonical read ' + path); return r.json(); }
  async function post(path: string, data: unknown) { const r = await page.request.post(base + path, { headers: { origin: base }, data }); const v = await r.json(); need(r.ok() && v.ok !== false, 'SAF-T setup ' + path + ' ' + JSON.stringify(v)); return v; }
  async function cookie(actor: Page, file: string) { const c = await actor.context().cookies(base); need(c.some(c => c.httpOnly), 'Normal MFA export cookie'); writeFileSync(file, c.map(c => c.name + '=' + c.value).join('; '), { mode: 0o600 }); }
  async function books() { return { journal: await get(prefix + 'journal/entries?all=true'), trial: await get(prefix + 'trial-balance'), bank: await get(prefix + 'bank'), documents: await get(prefix + 'documents') }; }
  async function unchanged() { need(JSON.stringify(await books()) === JSON.stringify(stable), 'SAF-T leaves source bookkeeping unchanged'); }
  async function auditCount() { const rows = (await get(prefix + 'audit-log?limit=5000')).rows.filter((r: any) => r.eventType === 'saft_export'); need(rows.length === exports, 'Exactly one audit per accepted SAF-T and none for refusals'); }
  function verify(files: Map<string, Buffer>, mode: string) {
    need(files.size === 3 && ['manifest.json', 'README.txt', 'saft.xml'].every(name => files.has(name)), 'Finite SAF-T package');
    const m = JSON.parse(files.get('manifest.json')!.toString('utf8')), xml = files.get('saft.xml')!.toString('utf8');
    need(m.manifestVersion === 2 && m.packageType === 'saft_export' && m.profileId === profile && m.provenance === null && m.limitations.includes('native_release_provenance_unavailable'), 'Retained bounded profile and honest provenance');
    need(m.generatedAtSource === mode && m.generatedAtExplicit === (mode === 'caller_supplied'), 'Honest timestamp source');
    if (mode === 'period_end') need(m.generatedAt === input.periodEnd + 'T23:59:59.000Z', 'Original default period-end timestamp');
    else if (mode === 'caller_supplied') need(m.generatedAt === explicitUtc, 'Caller timezone/fraction normalized by Rust server');
    else need(Math.abs(Date.parse(m.generatedAt) - Date.now()) < 300000 && files.get('README.txt')!.toString('utf8').includes('trusted runtime clock'), 'Native omitted mode retains actual trusted runtime');
    need(xml.includes('<DateCreated>' + m.generatedAt + '</DateCreated>'), 'XML and manifest creation time agree');
    need(m.periodStart === input.periodStart && m.periodEnd === input.periodEnd && m.counts.journalEntries === 1 && m.counts.salesInvoices === 0 && m.counts.purchaseInvoices === 0 && m.counts.documentReferences === 1, 'Whole selected accounting scope');
    need(m.outputs.length === 2, 'Every nonmanifest output covered');
    for (const o of m.outputs) { const b = files.get(o.path); need(b && b.length === o.sizeBytes && sha(b) === o.sha256, 'Output hash and size'); }
    const source = m.sourceContext, d = source.documents.find((d: any) => d.id === documentId);
    need(d && d.sha256 === sha(original) && source.journalEntries.length === 1 && source.journalEntries[0].lines.length === 2 && source.journalHashChain.ok === true, 'Whole journal, source original and chain');
    originalPath = d.storedPath;
    const accounting = { stableXml: xml.replace(/<DateCreated>[^<]*<\/DateCreated>/, '<DateCreated>export-metadata</DateCreated>'), journal: source.journalEntries, documents: source.documents, trialBalance: source.trialBalance, chain: source.journalHashChain, counts: m.counts };
    return { accounting, timestamp: { generatedAt: m.generatedAt, source: m.generatedAtSource, explicit: m.generatedAtExplicit } };
  }
  function archive(bytes: Buffer, mode: string) { const entries = readTar(bytes), roots = new Set(entries.map(e => e.path.split('/')[0])); need(roots.size === 1 && [...roots][0].startsWith('saft-export-'), 'One portable SAF-T root'); return verify(new Map(entries.map(e => [e.path.split('/').slice(1).join('/'), Buffer.from(e.content)])), mode); }
  async function download(mode: string, actor = page) { const data = mode === 'trusted_runtime_clock' ? input : { ...input, generationMode: mode, ...(mode === 'caller_supplied' ? { generatedAt: explicit } : {}) }; const r = await actor.request.post(base + prefix + 'saft-export', { headers: { origin: base }, data }); need(r.ok() && r.headers()['content-type'] === 'application/x-tar' && r.headers()['cache-control'] === 'private, no-store', 'Private SAF-T download ' + mode); const bytes = await r.body(); exports++; const projection = archive(bytes, mode); await auditCount(); return { projection, tarBase64: bytes.toString('base64') }; }
  function localCli(mode: string, file = ownerFile, denied = false) {
    const out = join(work, 'saft-cli-' + (++sequence));
    const args = ['system', 'export-saft', '--server', base, '--cookie-file', file, '--allow-loopback-http', '--company', 'synthetic', '--from', input.periodStart, '--to', input.periodEnd, '--out', out, '--json'];
    if (mode === 'caller_supplied') args.push('--generated-at', explicit);
    const r = spawnSync(join(work, 'rentemester-rust'), args, { encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 }); need(!r.error && r.status === (denied ? 1 : 0), 'Original SAF-T CLI business result ' + r.stderr + r.stdout);
    if (denied) { need(!existsSync(out), 'Refused CLI no local output'); return null; }
    const v = JSON.parse(r.stdout); need(v.ok && v.exportDir.startsWith(out + '/') && v.manifestPath === join(v.exportDir, 'manifest.json') && v.saftXmlPath === join(v.exportDir, 'saft.xml') && v.journalEntryCount === 1 && v.salesInvoiceCount === 0 && v.purchaseInvoiceCount === 0 && Array.isArray(v.errors) && v.errors.length === 0, 'Original flat metadata');
    const files = new Map(['manifest.json', 'README.txt', 'saft.xml'].map(name => [name, readFileSync(join(v.exportDir, name))]));
    const result = verify(files, mode); need(v.generatedAt === result.timestamp.generatedAt, 'Flat timestamp from verified manifest'); exports++; rmSync(out, { recursive: true }); return result;
  }
  function native(kind: 'cli' | 'mcp') {
    const common = ['--server', base, '--cookie-file', ownerFile, '--allow-loopback-http'], body = JSON.stringify(input);
    const wire = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-saft137', version: '1' } } }, { jsonrpc: '2.0', method: 'notifications/initialized' }, { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'saft_export', arguments: { company: 'synthetic', body, confirm: true } } }].map(v => JSON.stringify(v)).join('\n') + '\n';
    const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), kind === 'cli' ? ['remote', 'saft-export', ...common, '--company', 'synthetic', '--confirm'] : common, { input: kind === 'cli' ? body : wire, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 }); need(!r.error && r.status === 0, 'Native SAF-T ' + kind);
    let v: any; if (kind === 'cli') v = JSON.parse(r.stdout).data; else { const call = r.stdout.trim().split('\n').map(s => JSON.parse(s)).find(v => v.id === 2); need(!call?.error && !call?.result?.isError, 'Native SAF-T MCP envelope'); v = call.result.structuredContent.data; }
    need(v.export.contentType === 'application/x-tar', 'Native SAF-T MIME'); exports++; return archive(Buffer.from(v.export.bytes), 'trusted_runtime_clock');
  }
  async function projections() {
    const period = await download('period_end'), supplied = await download('caller_supplied'), nativeHttp = await download('trusted_runtime_clock');
    const results = [supplied.projection, nativeHttp.projection, localCli('period_end'), localCli('caller_supplied'), native('cli'), native('mcp')];
    for (const result of results) need(JSON.stringify(result.accounting) === JSON.stringify(period.projection.accounting), 'All timestamp modes/native/original clients share accounting projection');
    await auditCount(); await unchanged(); return { period, supplied, nativeHttp };
  }
  return {
    async run() {
      need(!completed, 'SAF-T fixture once'); await cookie(page, ownerFile); await cookie(member, memberFile);
      const today = (await get('/api/portfolio')).portfolio.asOf; input = { periodStart: today.slice(0, 4) + '-01-01', periodEnd: today, confirm: true };
      if (!(await get(prefix + 'company')).company.cvr) { const r = await page.request.patch(base + prefix + 'company', { headers: { origin: base }, data: { cvr: 'DK12345678' } }); need(r.ok(), 'Explicit synthetic CVR'); }
      for (const [accountNo, type, normalBalance] of [['1737', 'asset', 'debit'], ['3737', 'income', 'credit']]) await post(prefix + 'accounts', { accountNo, name: 'Synthetic SAF-T ' + accountNo, type, normalBalance });
      const uploaded = await post(prefix + 'documents/ingest', { confirm: true, fileName: 'synthetic-saft.txt', fileBase64: original.toString('base64'), metadata: { source: 'Synthetic SAF-T', documentType: 'cash_register_receipt', issueDate: today } }); documentId = uploaded.document.id;
      await post(prefix + 'manual-journal', { confirm: true, input: { format: 'rentemester-manual-journal-post/1', expectedRuleVersion: (await get(prefix + 'journal')).journal.nativePosting.ruleVersion, idempotencyKey: 'synthetic-saft137', request: { transactionDate: today, text: 'Synthetic SAF-T source', currency: 'DKK', documentId: String(documentId), lines: [{ accountNo: '1737', debitAmount: '100.00', creditAmount: '0.00' }, { accountNo: '3737', debitAmount: '0.00', creditAmount: '100.00' }] } } });
      stable = await books(); savedProjection = await projections();
      for (const data of [{ ...input, confirm: false }, { ...input, generatedAt: explicit }, { ...input, generationMode: 'unknown' }, { ...input, generationMode: 'caller_supplied' }, { ...input, generationMode: 'period_end', generatedAt: explicit }, { ...input, generationMode: 'caller_supplied', generatedAt: '2026-02-30T12:00:00Z' }]) { const r = await page.request.post(base + prefix + 'saft-export', { headers: { origin: base }, data }); need(!r.ok(), 'Unconfirmed/malformed timestamp refuses'); }
      await auditCount(); need(originalPath && !originalPath.startsWith('/') && !originalPath.split('/').includes('..'), 'Registered relative original path');
      const path = join(work, 'workspace/synthetic', originalPath); need(readFileSync(path).equals(original), 'Actual original selected'); writeFileSync(path, Buffer.from('altered synthetic SAF-T bytes'));
      try { const r = await page.request.post(base + prefix + 'saft-export', { headers: { origin: base }, data: input }); need(!r.ok(), 'Altered registered original refuses SAF-T'); } finally { writeFileSync(path, original); }
      await auditCount(); const memberId = (await get('/api/me', member)).user.id;
      await post('/api/workspace/members/company', { userId: memberId, action: 'grant', companySlug: 'synthetic', role: 'reader' }); await download('period_end', member);
      await post('/api/workspace/members/company', { userId: memberId, action: 'revoke', companySlug: 'synthetic' }); const r = await member.request.post(base + prefix + 'saft-export', { headers: { origin: base }, data: input }); need(r.status() === 403, 'Current reader revocation'); localCli('period_end', memberFile, true); await auditCount(); await unchanged();
      const fullAudit = await get(prefix + 'audit-verify'); need(fullAudit.audit?.available && fullAudit.audit.ok && fullAudit.audit.entries === 1 && fullAudit.audit.components.length === 5 && fullAudit.audit.components.every((c: any) => c.available && c.ok), 'Full source accounting audit'); completed = true;
      return { qualified: true, ordinaryMfa: true, bookkeepingUnchanged: true, originalCliLocalPackage: true, nativeTrustedClockRetained: true, originalTamperRefused: true, currentRevocationRefused: true, invalidTimingBeforeEffects: true, projections: savedProjection, fullAudit, exportAuditCount: exports, accountingFingerprint: sha(JSON.stringify(stable)), installedNativeQualified: false, sourceFreeRestoreQualified: false };
    },
    async reopen() { need(completed, 'Completed SAF-T setup'); await cookie(page, ownerFile); await unchanged(); const current = await projections(); need(JSON.stringify(current.period.projection) === JSON.stringify(savedProjection.period.projection) && JSON.stringify(current.supplied.projection) === JSON.stringify(savedProjection.supplied.projection), 'Historical metadata and accounting survive normal MFA restart'); rmSync(ownerFile); rmSync(memberFile); return { qualified: true, ordinaryMfa: true, exactNormalMfaRestartState: true, projections: current, exportAuditCount: exports, accountingFingerprint: sha(JSON.stringify(stable)), installedNativeQualified: false, sourceFreeRestoreQualified: false }; },
  };
}
