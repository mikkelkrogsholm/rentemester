import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Work-Issue #717. Exactly three append-only writes, never retried or replayed.
 * The parent owns actual execution and the shared signed source-free recovery.
 * File failure/prepared-audit and overlimit boundaries require targeted SDK/server tests.
 */
export function createOriginalMileageProof(base: string, work: string, onStep: (name: string) => void) {
  const root = '/api/companies/synthetic/', api = root + 'original/mileage/';
  const cookie = join(work, 'mileage171.cookie'), key = join(work, 'mileage171.key');
  const observations: any[] = [];
  let owner: Page, service: any, stable: any, ran = false;
  const need = (v: unknown, message: string) => { if (!v) throw Error('original_mileage: ' + message); };
  function record(value: any) { observations.push(value); writeFileSync(join(work, 'mileage171-observations.json'), JSON.stringify(observations), { mode: 0o600 }); }
  async function http(path: string, data?: any, expected = 200, page = owner) {
    onStep((data === undefined ? 'GET:' : 'POST:') + path);
    const r = data === undefined ? await page.request.get(base + path) : await page.request.post(base + path, { headers: { origin: base }, data });
    need(r.status() === expected, path + ' HTTP' + r.status());
    const value = await r.json();
    record({ path, status: r.status(), value: path === '/api/workspace/service-principals' ? { ...value, secret: '[redacted]' } : value });
    return value;
  }
  const admin = (path: string, data: any) => http('/api/workspace/' + path, data, path === 'service-principals' ? 201 : 200);
  async function cookies() { writeFileSync(cookie, (await owner.context().cookies(base)).map(c => c.name + '=' + c.value).join('; '), { mode: 0o600 }); }
  function invoke(kind: 'cli' | 'mcp', op: 'log' | 'list' | 'report' | 'export', args: any = {}, asService = false, mode: 'ok' | 'refused' | 'invalid' = 'ok') {
    onStep(kind + ':' + op + ':' + (asService ? 'service' : 'human'));
    const common = ['--server', base, '--allow-loopback-http', asService ? '--service-key-file' : '--cookie-file', asService ? key : cookie];
    let argv: string[], input: string | undefined;
    if (kind === 'cli') {
      const names: Record<string, string> = { tripDate: 'date', fromLocation: 'from', toLocation: 'to', kilometers: 'km', ratePerKm: 'rate-per-km', rateBasis: 'rate-basis', rateSource: 'rate-source', outputDir: 'out' };
      argv = ['mileage', op, ...common, '--company', 'synthetic', '--json', ...Object.entries(args).filter(([k]) => k !== 'confirm').flatMap(([k,v]) => ['--' + (names[k] ?? k), String(v)])];
    } else {
      argv = common;
      input = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'original-mileage171', version: '1' } } },
        { jsonrpc: '2.0', method: 'notifications/initialized' }, { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'mileage_' + op.replaceAll('-', '_'), arguments: { company: 'synthetic', ...(op === 'log' ? { input: Object.fromEntries(Object.entries(args).filter(([k]) => k !== 'confirm')), ...(args.confirm === undefined ? {} : { confirm: args.confirm }) } : args) } } }].map(v => JSON.stringify(v)).join('\n') + '\n';
    }
    const r = spawnSync(join(work, kind === 'cli' ? 'rentemester-rust' : 'rentemester-mcp'), argv, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    for (const file of [cookie, key]) { const secret = readFileSync(file, 'utf8').trim(); need(!r.stdout.includes(secret) && !r.stderr.includes(secret), 'credential privacy'); }
    record({ kind, op, args, asService, exit: r.status, stdout: r.stdout, stderr: r.stderr, terminalError: r.error?.name });
    need(!r.error && r.status === (kind === 'cli' && mode !== 'ok' ? (mode === 'invalid' ? 2 : 1) : 0), 'terminal status; never resend unknown write');
    if (kind === 'cli') {
      if (mode === 'invalid') return;
      const value = JSON.parse(r.stdout); need(value.ok === (mode === 'ok'), 'CLI flat outcome'); return value;
    }
    const response = r.stdout.trim().split('\n').map(s => JSON.parse(s)).find(v => v.id === 2);
    if (mode === 'invalid') { need(response?.error?.code === -32602, 'RPC strict arguments'); return; }
    const result = response?.result, value = result?.structuredContent;
    need(!response?.error && result.content.length === 1 && equal(JSON.parse(result.content[0].text), value), 'MCP text/structured equality');
    need(value?.ok === (mode === 'ok') && result.isError === (mode !== 'ok') && Array.isArray(value.errors), 'MCP source envelope');
    if (mode !== 'ok') { need(!Object.hasOwn(value, 'data') && value.errors.length > 0, 'failure has no success data'); return value; }
    return { ok: true, ...value.data, errors: value.errors, ...(value.appliedRules ? { appliedRules: value.appliedRules } : {}) };
  }
  const range = { from: '2025-12-31', to: '2026-01-02' };
  async function snapshot() {
    const state: any = {};
    for (const path of ['journal', 'vat', 'trial-balance', 'documents', 'bank']) state[path] = await http(root + path);
    state.list = await http(api + 'list');
    state.report = await http(api + 'report?' + new URLSearchParams(range));
    return state;
  }
  async function parity() {
    for (const kind of ['cli', 'mcp'] as const) {
      need(equal(invoke(kind, 'list'), stable.list), 'all-year list source parity');
      need(equal(invoke(kind, 'report', range), stable.report), 'dated source report parity');
    }
  }
  const rowKeys = ['id','entryNo','tripDate','purpose','fromLocation','toLocation','kilometers','vehicle','driver','ratePerKm','amountBasis','rateBasis','rateSource','notes','createdAt'];
  function exports(outputDir: string) {
    const report = stable.report;
    const ordered = report.entries.map((r: any) => Object.fromEntries(rowKeys.map(k => [k, r[k]])));
    const expectedJson = JSON.stringify({ appliedRules: ['DK-MILEAGE-LOG-001'], ...range, entryCount: 3, totalKilometers: 15.85, totalAmountBasis: 71.63, note: "Mileage log only. Rentemester records the trips; tax treatment is the user/advisor's responsibility.", entries: ordered }, null, 2) + '\n';
    const cell = (v: any) => { const s = v == null ? '' : String(v); return /[",\r\n]/.test(s) ? '"' + s.replaceAll('"', '""') + '"' : s; };
    const columns = ['entryNo','tripDate','purpose','fromLocation','toLocation','kilometers','vehicle','driver','ratePerKm','amountBasis','rateBasis','rateSource'];
    const expectedCsv = 'entry_no,trip_date,purpose,from_location,to_location,kilometers,vehicle,driver,rate_per_km,amount_basis,rate_basis,rate_source\n' + ordered.map((r: any) => columns.map(k => cell(r[k])).join(',')).join('\n') + '\n';
    need(readFileSync(join(outputDir, 'mileage-log.json'), 'utf8') === expectedJson, 'exact source JSON bytes/order/newline');
    need(readFileSync(join(outputDir, 'mileage-log.csv'), 'utf8') === expectedCsv, 'exact source CSV bytes/escaping/newline');
    need(readFileSync(join(outputDir, 'unrelated.txt'), 'utf8') === 'preserve me\n', 'unrelated file preserved');
    need(equal(readdirSync(outputDir).sort(), ['mileage-log.csv','mileage-log.json','unrelated.txt']), 'only fixed targets, no stage leftovers');
  }
  async function run(page: Page, reader: Page) {
    need(!ran, 'one bounded run; never replay log writes'); ran = true; owner = page; await cookies();
    const who = await http('/api/me');
    service = await admin('service-principals', { displayName: 'Synthetic mileage171', operationId: randomUUID(), confirm: true });
    writeFileSync(key, service.secret + '\n', { mode: 0o600, flag: 'wx' });
    await admin('members/access', { userId: service.serviceAccountId, action: 'set-role', workspaceRole: 'member' });
    await admin('members/company', { userId: service.serviceAccountId, companySlug: 'synthetic', action: 'grant', role: 'bookkeeper' });
    const trip = { tripDate: range.from, purpose: 'Synthetic, "mileage" æ\nvisit', fromLocation: 'Synthetic origin', toLocation: 'Synthetic destination', kilometers: 12.35, vehicle: 'Synthetic private car', driver: 'Synthetic driver', ratePerKm: 3.791, rateBasis: 'Explicit caller chosen documentation basis; no tax inference', rateSource: ' https://example.invalid/rate ', notes: ' Synthetic notes ' };
    const before = await snapshot();
    for (const confirm of [undefined, false]) need(invoke('mcp','log',{...trip,...(confirm === undefined ? {} : {confirm})},false,'refused').code === 'CONFIRM_REQUIRED', 'local confirmation fence');
    const invalid = invoke('cli','log',{},false,'refused');
    need(equal(invalid, { ok:false, appliedRules:['DK-MILEAGE-LOG-001'], warnings:[], errors:['tripDate is required','purpose is required','fromLocation is required','toLocation is required','kilometers must be a positive number','vehicle is required','driver is required','ratePerKm must be a positive number','rateBasis is required (user-supplied / source-backed)'] }), 'missing CLI fields reach ordered source validation');
    for (const kilometers of [0, -1]) need(invoke('cli','log',{...trip,kilometers},false,'refused').errors[0] === 'kilometers must be a positive number', 'semantic numeric source refusal');
    const badRange = invoke('cli','report',{},false,'refused');
    need(equal(badRange,{ok:false,from:'',to:'',entryCount:0,totalKilometers:0,totalAmountBasis:0,entries:[],appliedRules:['DK-MILEAGE-LOG-001'],errors:['from must be a valid YYYY-MM-DD date','to must be a valid YYYY-MM-DD date']}),'exact source invalid range');
    await http(api+'log',{...trip,confirm:true,expectedActor:'user:forged'},403);
    need(equal(await snapshot(),before),'source/confirmation/actor refusals no business effects');
    const receipts = [invoke('cli','log',trip), invoke('mcp','log',{...trip,tripDate:range.to,kilometers:2.5,ratePerKm:9.125,notes:' ',confirm:true},true), invoke('mcp','log',{...trip,tripDate:range.to,kilometers:1,ratePerKm:2,confirm:true})];
    need(equal(receipts.map(r=>[r.entryNo,r.amountBasis]),[['MIL-2025-000001',46.82],['MIL-2026-000001',22.81],['MIL-2026-000002',2]]),'exact yearly sequence and hundredth amount');
    need(receipts[1].warnings.length===1 && receipts[1].warnings[0].includes('9,125'),'warning only preserves chosen rate');
    stable = await snapshot();
    const entries = stable.report.entries;
    need(stable.report.entryCount===3 && stable.report.totalKilometers===15.85 && stable.report.totalAmountBasis===71.63,'exact totals');
    need(equal(stable.list.rows,[...entries].reverse()),'all-year DESC date/id versus report ASC date/id');
    for(const e of entries) need(equal(Object.keys(e).sort(),[...rowKeys].sort()),'exact source row fields');
    need(equal(entries.map((e:any)=>[e.kilometers,e.ratePerKm,e.amountBasis,e.rateSource,e.notes]),[[12.35,3.791,46.82,trip.rateSource.trim(),trip.notes.trim()],[2.5,9.125,22.81,trip.rateSource.trim(),null],[1,2,2,trip.rateSource.trim(),trip.notes.trim()]]),'stored source trip facts trimmed/null');
    for(const date of [range.from,range.to]) { const r=invoke('cli','report',{from:date,to:date}); need(equal(r.entries,entries.filter((e:any)=>e.tripDate===date)),'inclusive boundary report'); }
    await parity();
    need(stable.journal.journal.entries.length===0 && stable.vat.vat.inputVat===0 && stable.vat.vat.payable===0,'zero journals/VAT');
    const outputDir=join(work,'mileage171-export'); mkdirSync(outputDir,{mode:0o700});
    writeFileSync(join(outputDir,'unrelated.txt'),'preserve me\n');
    for(const name of ['mileage-log.json','mileage-log.csv'])writeFileSync(join(outputDir,name),'replace stale fixed target\n');
    for(const kind of ['cli','mcp'] as const) {
      const result=invoke(kind,'export',{...range,outputDir,...(kind==='mcp'?{confirm:true}:{})},kind==='mcp');
      need(equal(result,{ok:true,jsonPath:join(outputDir,'mileage-log.json'),csvPath:join(outputDir,'mileage-log.csv'),...range,entryCount:3,totalKilometers:15.85,totalAmountBasis:71.63,appliedRules:['DK-MILEAGE-LOG-001'],errors:[]}),'flat local success only after both files'); exports(outputDir);
    }
    await http(api+'log',{...trip,confirm:true},403,reader);
    await admin('members/company',{userId:service.serviceAccountId,companySlug:'synthetic',action:'grant',role:'reader'});
    invoke('mcp','log',{...trip,confirm:true},true,'refused');
    invoke('mcp','export',{...range,outputDir,confirm:true},true,'refused');
    await admin('service-principals/revoke',{serviceAccountId:service.serviceAccountId,credentialId:service.credentialId,operationId:randomUUID(),confirm:true});
    invoke('mcp','log',{...trip,confirm:true},true,'refused');
    invoke('mcp','export',{...range,outputDir,confirm:true},true,'refused');
    need(equal(await snapshot(),stable),'role/key refusal no entries/journals/VAT effects');
    return {receipts,report:stable.report,humanActor:'user:'+who.user.id,serviceActor:'service_account:'+service.serviceAccountId,noLogReplay:true,realExports:2,targetedTestsRequired:['local-file-failure prepared audit boundary','16MiB packet refusal before preparation audit']};
  }
  async function assertStable(page: Page) { owner=page; need(equal(await snapshot(),stable),'exact mileage and zero accounting through restart/restore/reopen'); }
  async function reopen(page: Page) { await assertStable(page); await cookies(); await parity(); invoke('mcp','list',{},true,'refused'); }
  return {run,assertStable,reopen};
}
