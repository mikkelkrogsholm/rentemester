import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { createHash } from "node:crypto";
import { isDeepStrictEqual } from "node:util";

/** Call after checkNativeAnnualReport, initially and after restart; real authenticated HTTP only. */
export async function checkNativeAnnualClients(page: Page, base: string, restarted = false) {
  const saved = JSON.parse(readFileSync("/tmp/rentemester-rust-reports/native-annual-report.json", "utf8"));
  const { fiscalYearStart, fiscalYearEnd } = saved;
  if (typeof fiscalYearStart !== "string" || typeof fiscalYearEnd !== "string") throw new Error("Annual preparation evidence missing explicit fiscal bounds");
  const response = await page.request.get(`${base}/api/companies/synthetic/annual-report?${new URLSearchParams({ fiscalYearStart, fiscalYearEnd })}`);
  if (!response.ok()) throw new Error(`Actual annual HTTP read refused: ${response.status()}`);
  const expected = await response.json();
  const temp = mkdtempSync("/tmp/rm-native-annual-clients-");
  try {
    const cookieFile = join(temp, "session.cookie");
    writeFileSync(cookieFile, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; "), { mode: 0o600, flag: "wx" });
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookieFile];
    const cli = spawnSync(process.env.RM_NATIVE_CLI_BIN ?? resolve("target/debug/rentemester-rust"), ["remote", "annual-report", ...common, "--company", "synthetic", "--fiscal-year-start", fiscalYearStart, "--fiscal-year-end", fiscalYearEnd], { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
    if (cli.error || cli.status !== 0) throw new Error("Actual CLI annual-report failed; build rentemester-cli or configure RM_NATIVE_CLI_BIN");
    const input = [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-annual-check", version: "1" } } },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "annual_report", arguments: { company: "synthetic", fiscalYearStart, fiscalYearEnd } } },
    ].map(v => JSON.stringify(v)).join("\n") + "\n";
    const mcp = spawnSync(process.env.RM_NATIVE_MCP_BIN ?? resolve("target/debug/rentemester-mcp"), common, { input, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
    if (mcp.error || mcp.status !== 0) throw new Error("Actual MCP annual_report failed; build rentemester-mcp or configure RM_NATIVE_MCP_BIN");
    const call = mcp.stdout.trim().split("\n").map(line => JSON.parse(line)).find(v => v.id === 2);
    if (call?.error || call?.result?.isError || !call?.result?.structuredContent?.data) throw new Error("MCP annual_report returned protocol/tool error");
    function normalized(value: unknown): string {
      function sorted(v: unknown): unknown {
        if (Array.isArray(v)) return v.map(sorted);
        if (v !== null && typeof v === "object") return Object.fromEntries(Object.entries(v).sort(([a], [b]) => a.localeCompare(b)).map(([key, item]) => [key, sorted(item)]));
        return v;
      }
      return JSON.stringify(sorted(value));
    }
    if (normalized(expected.annualReport) !== normalized(saved)) throw new Error("Actual HTTP annual preparation differs from persisted browser evidence");
    for (const [name, value] of [["CLI", JSON.parse(cli.stdout).data], ["MCP", call.result.structuredContent.data]] as const) {
      if (normalized(value) !== normalized(expected)) throw new Error(`${name} annual preparation differs from full actual HTTP JSON`);
    }
    console.log(restarted ? "PASS: CLI/MCP full annual preparation equals authenticated HTTP and persisted evidence after restart" : "PASS: CLI/MCP full annual preparation equals authenticated HTTP and saved browser period evidence");
  } finally { rmSync(temp, { recursive: true, force: true }); }
}

/** Standalone extension of the existing normal-MFA runner; owned synthetic data only. */
export function createAnnualExportsProof(page: Page, base: string, work: string, member: Page) {
  const prefix = '/api/companies/synthetic/', start = '2020-01-01', end = '2020-12-31';
  const bounds = { fiscalYearStart: start, fiscalYearEnd: end }, period = { periodStart: start, periodEnd: end };
  const cookieFile = join(work, 'annual-export-owner.cookie'), memberFile = join(work, 'annual-export-member.cookie');
  const need = (v: unknown, message: string) => { if (!v) throw Error(message); };
  const sha = (v: string | Buffer) => createHash('sha256').update(v).digest('hex');
  let stable: unknown, saved: any, completed = false;
  const originals: { id: number; bytes: Buffer }[] = [];
  async function get(path: string, actor = page) { const r = await actor.request.get(base + path); need(r.ok(), 'Annual canonical HTTP ' + path + ' ' + r.status()); return r.json(); }
  async function post(path: string, data: unknown) { const r = await page.request.post(base + path, { headers: { origin: base }, data }); const v = await r.json(); need(r.ok() && v.ok !== false, 'Annual reviewed write ' + path + ' ' + JSON.stringify(v)); return v; }
  async function cookies(actor: Page, file: string) { const c = await actor.context().cookies(base); need(c.some(c => c.httpOnly), 'Normal MFA cookie'); writeFileSync(file, c.map(c => c.name + '=' + c.value).join('; '), { mode: 0o600 }); }
  async function books() { return { journal: await get(prefix + 'journal/entries?all=true'), trial: await get(prefix + 'trial-balance'), documents: await get(prefix + 'documents'), bank: await get(prefix + 'bank'), audit: await get(prefix + 'audit-log?limit=5000') }; }
  async function unchanged() { need(isDeepStrictEqual(await books(), stable), 'Annual pure reads preserve books and audit'); }
  async function read(kind: string) { return (await get(prefix + 'annual-exports?' + new URLSearchParams({ ...bounds, kind }))).annualExport; }
  function cli(kind: string, extra: string[] = [], file = cookieFile, exit = 0) {
    const r = spawnSync(join(work, 'rentemester-rust'), ['report', kind, '--server', base, '--cookie-file', file, '--allow-loopback-http', '--company', 'synthetic', '--from', start, '--to', end, '--json', ...extra], { encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    const secret = readFileSync(file, 'utf8'); need(!r.error && r.status === exit && !r.stdout.includes(secret) && !r.stderr.includes(secret), 'Annual CLI exact exit/privacy ' + kind);
    return r.stdout ? JSON.parse(r.stdout) : null;
  }
  function mcp(file = cookieFile, denied = false) {
    const input = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-annual135', version: '1' } } }, { jsonrpc: '2.0', method: 'notifications/initialized' }, { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'tax_return_prepare', arguments: { company: 'synthetic', from: start, to: end } } }].map(v => JSON.stringify(v)).join('\n') + '\n';
    const r = spawnSync(join(work, 'rentemester-mcp'), ['--server', base, '--cookie-file', file, '--allow-loopback-http'], { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!r.error && r.status === 0, 'Annual MCP transport'); const call = r.stdout.trim().split('\n').map(v => JSON.parse(v)).find(v => v.id === 2);
    const v = call?.result?.structuredContent; need(!call?.error && call.result.isError === denied && v?.ok === !denied && Array.isArray(v.errors), 'Original tax legacy envelope');
    need(isDeepStrictEqual(JSON.parse(call.result.content[0].text), v), 'Tax text/structured parity');
    if (denied) need(v.errors.length > 0, 'Tax business refusal evidence'); else need(v.errors.length === 0, 'Tax successful envelope');
    return denied ? undefined : { ...v.data, ok: v.ok, errors: v.errors, appliedRules: v.appliedRules };
  }
  async function close() {
    const review = await post(prefix + 'periods/close-review', { ...period, confirm: true });
    await post(prefix + 'periods/close', { ...period, kind: 'fiscal_year', packetHash: review.review.packet.hash, reviewId: review.review.id, confirm: true });
  }
  async function blocked() { const annual = (await get(prefix + 'annual-report?' + new URLSearchParams(bounds))).annualReport.report; need(annual.ok === false, 'Open year annual refuses'); for (const kind of ['tax', 'ixbrl']) { const v = await read(kind); need(v.ok === false && !v.xhtml && v.errors.length > 0, 'Open year export refuses ' + kind); } }
  async function projections() {
    const annual = (await get(prefix + 'annual-report?' + new URLSearchParams(bounds))).annualReport.report, tax = await read('tax'), ixbrl = await read('ixbrl');
    need(annual.ok && annual.aretsResultat === 881.25 && annual.balanceSheet.balanced, 'Nonempty annual source reconciliation');
    need(tax.ok && tax.bookkeptResult === 881.25 && tax.totalAdjustments === 89.06 && tax.taxableIncome === 970.31, 'Exact canonical representation addback');
    need(tax.corporateTax === null && tax.needsReview.length > 0, 'Unspecified company form requires review, never guessed ApS');
    need(isDeepStrictEqual(cli('annual'), annual) && isDeepStrictEqual(cli('tax'), tax) && isDeepStrictEqual(mcp(), tax), 'Original annual/tax CLI and MCP equal canonical reports');
    need(ixbrl.ok && ixbrl.sha256 === sha(ixbrl.xhtml) && (ixbrl.xhtml.match(/<ix:(?:nonNumeric|nonFraction)\b/g) ?? []).length === 20 && ixbrl.xhtml.includes('xmlns:iso4217='), 'Deterministic valid namespace and20facts');
    const file = join(work, 'annual-export.xhtml'); rmSync(file, { force: true }); const exported = cli('annual', ['--ixbrl-out', file]);
    need(readFileSync(file, 'utf8') === ixbrl.xhtml && exported.ixbrl.sha256 === ixbrl.sha256, 'Actual local XHTML bytes/hash'); rmSync(file);
    for (const original of originals) { const r = await page.request.get(base + prefix + 'documents/' + original.id + '/file'); need(r.ok() && (await r.body()).equals(original.bytes), 'Original annual source bytes retained'); }
    return { annual, tax, xhtml: ixbrl.xhtml, sha256: ixbrl.sha256 };
  }
  return {
    async run() {
      need(!completed, 'Annual fixture only once'); await cookies(page, cookieFile); await cookies(member, memberFile);
      for (const [accountNo, type, normalBalance] of [['1735', 'asset', 'debit'], ['3735', 'income', 'credit'], ['4735', 'expense', 'debit'], ['4100', 'vat', 'debit'], ['1835', 'asset', 'debit'], ['1535', 'asset', 'debit'], ['2635', 'liability', 'credit']]) await post(prefix + 'accounts', { accountNo, name: 'Synthetic annual ' + accountNo, type, normalBalance });
      for (const [role, accountNo] of [['input_vat', '4100'], ['bank', '1835'], ['debtors', '1535'], ['creditors', '2635']]) await post(prefix + 'account-roles', { role, accountNo, confirm: true });
      if (!(await get(prefix + 'company')).company.cvr) { const r = await page.request.patch(base + prefix + 'company', { headers: { origin: base }, data: { cvr: 'DK12345678' } }); need(r.ok(), 'Explicit synthetic CVR setup'); }
      const version = (await get(prefix + 'journal')).journal.nativePosting.ruleVersion;
      for (const [key, lines] of [['income', [{ accountNo: '1735', debitAmount: '1000.00', creditAmount: '0.00' }, { accountNo: '3735', debitAmount: '0.00', creditAmount: '1000.00' }]], ['representation', [{ accountNo: '4735', debitAmount: '100.00', creditAmount: '0.00', vatCode: 'REPRESENTATION_SPECIAL' }, { accountNo: '4735', debitAmount: '18.75', creditAmount: '0.00', vatCode: 'REPRESENTATION_NON_DEDUCTIBLE_VAT' }, { accountNo: '4100', debitAmount: '6.25', creditAmount: '0.00' }, { accountNo: '1735', debitAmount: '0.00', creditAmount: '125.00' }]]] as const) {
        const bytes = Buffer.from('Synthetic annual ' + key + '\n'), uploaded = await post(prefix + 'documents/ingest', { confirm: true, fileName: 'synthetic-annual-' + key + '.txt', fileBase64: bytes.toString('base64'), metadata: { source: 'Synthetic annual ' + key, documentType: 'cash_register_receipt', issueDate: '2020-06-15' } }); originals.push({ id: uploaded.document.id, bytes });
        await post(prefix + 'manual-journal', { confirm: true, input: { format: 'rentemester-manual-journal-post/1', expectedRuleVersion: version, idempotencyKey: 'synthetic-annual135-' + key, request: { transactionDate: '2020-06-15', text: 'Synthetic annual ' + key, currency: 'DKK', documentId: String(uploaded.document.id), lines } } });
      }
      await blocked(); await close(); await projections();
      await post(prefix + 'periods/reopen', { ...period, kind: 'fiscal_year', reason: 'Synthetic annual correction proof', confirm: true }); await blocked(); await close();
      stable = await books(); saved = await projections(); await unchanged();
      const memberId = (await get('/api/me', member)).user.id;
      await post('/api/workspace/members/company', { userId: memberId, action: 'grant', companySlug: 'synthetic', role: 'reader' });
      for (const kind of ['tax', 'ixbrl']) need(isDeepStrictEqual((await get(prefix + 'annual-exports?' + new URLSearchParams({ ...bounds, kind }), member)).annualExport, await read(kind)), 'Current reader sees same annual export');
      await post('/api/workspace/members/company', { userId: memberId, action: 'revoke', companySlug: 'synthetic' });
      for (const kind of ['tax', 'ixbrl']) { const r = await member.request.get(base + prefix + 'annual-exports?' + new URLSearchParams({ ...bounds, kind })); need(r.status() === 403, 'Retained nonmember cannot read annual exports'); }
      cli('tax', [], memberFile, 1); mcp(memberFile, true);
      const fullAudit = await get(prefix + 'audit-verify'); need(fullAudit.audit?.available && fullAudit.audit.ok && fullAudit.audit.entries === 2 && fullAudit.audit.components.length === 5 && fullAudit.audit.components.every((c: any) => c.available && c.ok), 'Complete source annual audit');
      completed = true; return { qualified: true, ordinaryMfa: true, readOnly: true, originalAnnualTaxClients: true, openAndReopenedYearRefused: true, representationExact: true, unknownFormReview: true, projections: saved, fullAudit, accountingFingerprint: sha(JSON.stringify(stable)), sourceFreeRestoreQualified: false, installedNativeQualified: false };
    },
    async reopen() { need(completed, 'Annual completed setup'); await cookies(page, cookieFile); await unchanged(); const current = await projections(); need(isDeepStrictEqual(current, saved), 'Annual tax/XHTML exact normalMFA restart'); await unchanged(); rmSync(cookieFile); rmSync(memberFile); return { qualified: true, ordinaryMfa: true, exactNormalMfaRestartState: true, projections: current, accountingFingerprint: sha(JSON.stringify(stable)), sourceFreeRestoreQualified: false, installedNativeQualified: false }; },
  };
}
