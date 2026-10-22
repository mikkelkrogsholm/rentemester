import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Run once against the existing ordinary-MFA runner. No database writes or financial replay. */
export function createDocumentsOriginalProof(base: string, work: string, pdf: (stream: string) => Buffer, onStep: (name: string) => void = () => {}) {
  const root = '/api/companies/synthetic/', original = root + 'documents/original';
  const cookie = join(work, 'original-documents.cookie'), key = join(work, 'original-documents.key');
  const transcript: any[] = [], documents: Record<string, any> = {}, evidence: Record<string, any> = {};
  let owner: Page, service: any, stable: any, ran = false;
  const need = (v: unknown, label: string) => { if (!v) { const error = Error('Original documents assertion'); Object.assign(error, { originalAssertion: label.split(';')[0].split(' exit ')[0].slice(0, 160) }); throw error; } };
  const hash = (b: Buffer | string) => createHash('sha256').update(b).digest('hex');
  async function http(path: string, data?: any, method = 'POST') {
    onStep(method + ':' + path);
    const r = data === undefined ? await owner.request.get(base + path) : await owner.request.fetch(base + path, { method, headers: { origin: base }, data });
    const value = await r.json(); need(r.ok() && value.ok !== false, `${method} ${path} HTTP${r.status()}`); return value;
  }
  const admin = (path: string, data: any) => http('/api/workspace/' + path, data);
  function execute(binary: string, args: string[], input?: string) {
    onStep(binary + ':' + (binary === 'rentemester-rust' ? args[1] : 'stdio'));
    const r = spawnSync(join(work, binary), args, { input, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!r.error && !r.stdout.includes(service.secret) && !r.stderr.includes(service.secret), 'bounded private client transport'); return r;
  }
  function cli(operation: string, flags: string[] = [], exit = 0, human = false, offline = false) {
    const r = execute('rentemester-rust', ['documents', operation, '--company', 'synthetic', '--server', offline ? 'http://127.0.0.1:1' : base, '--allow-loopback-http', '--cookie-file', cookie, ...(human ? ['--format', 'human'] : ['--json']), ...flags]);
    need(r.status === exit, `CLI ${operation} exit ${r.status}, expected ${exit}; ${r.stdout.slice(0, 800)} ${r.stderr.slice(0, 500)}`);
    if (exit === 2) { need(!r.stdout && !!r.stderr.trim(), 'usage stderr only'); return; }
    const value = human ? r.stdout : JSON.parse(r.stdout); transcript.push({ client: 'cli', operation, exit, value }); return value;
  }
  function rpc(name: string, args: any, mode = 'ok', offline = false) {
    const input = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-documents-original165', version: '1' } } }, { jsonrpc: '2.0', method: 'notifications/initialized' }, { jsonrpc: '2.0', id: 2, method: name === 'tools/list' ? name : 'tools/call', params: name === 'tools/list' ? {} : { name, arguments: args } }].map(v => JSON.stringify(v)).join('\n') + '\n';
    const r = execute('rentemester-mcp', ['--server', offline ? 'http://127.0.0.1:1' : base, '--allow-loopback-http', '--service-key-file', key], input);
    need(r.status === 0, 'stdio terminal'); const response = r.stdout.trim().split('\n').map(s => JSON.parse(s)).find(v => v.id === 2);
    if (mode === 'invalid') { need(response?.error?.code === -32602, 'invalid args before HTTP'); return; }
    if (name === 'tools/list') return response.result;
    const result = response?.result, value = result?.structuredContent;
    need(value?.ok === (mode === 'ok') && result.isError === (mode !== 'ok') && result.content.length === 1 && result.content[0].type === 'text' && equal(JSON.parse(result.content[0].text), value), 'source MCP envelope/text/isError');
    need(Array.isArray(value.errors) && (mode === 'ok' ? value.errors.length === 0 && value.data && !['ok', 'errors', 'appliedRules'].some(k => k in value.data) : value.errors.length > 0 && !('data' in value)), 'flat core result');
    transcript.push({ client: 'mcp', name, mode, value }); return value;
  }
  async function snapshot() { return { documents: await http(original + '/list'), journal: await http(root + 'journal'), payables: await http(root + 'payables?status=all'), evidence: await reads() }; }
  async function reads() {
    const result: any = {};
    for (const [kind, doc] of Object.entries(documents)) {
      const status = await http(root + `documents/${doc.documentId}/parse-status`), text = await http(root + `documents/${doc.documentId}/parsed-text?offset=0&limit=10`);
      need(status.parse?.sourceSha256 === doc.sha256 && equal(status.parse, text.parse) && text.nextOffset === null, 'verified public source binding ' + kind);
      const downloaded = await owner.request.get(base + root + `documents/${doc.documentId}/file`); need(downloaded.ok() && hash(await downloaded.body()) === doc.sha256, 'exact authorized original ' + kind);
      result[kind] = { status, text };
    }
    return result;
  }
  async function lists() {
    const rows = (await http(original + '/list')).documents, bare = cli('list');
    const snake = 'id,document_no,source,original_filename,document_type,invoice_date,amount_inc_vat,currency,status,stored_path,sender_vat_cvr,supplier_country_code,supplier_identifier_kind,supplier_identity_status,source_bank_transaction_id,accounting_rationale,prepared_by,prepared_by_program,purchase_vat_lines'.split(',');
    const camel = 'id,documentNo,source,originalFilename,documentType,invoiceDate,amountIncVat,currency,status,storedPath,senderVatOrCvr,supplierCountryCode,supplierIdentifierKind,supplierIdentityStatus,sourceBankTransactionId,accountingRationale,preparedBy,preparedByProgram,purchaseVatLines'.split(',');
    need(Array.isArray(bare) && bare.length === rows.length && rows.every((r: any, i: number) => equal(Object.keys(r).sort(), [...camel].sort()) && r.storedPath === null && r.id === bare[i].id && equal(Object.keys(bare[i]).sort(), [...snake].sort()) && bare[i].stored_path === null && (i === 0 || rows[i - 1].id > r.id)), 'exact private id-desc list schemas');
    for (let i = 0; i < rows.length; i++) for (let k = 0; k < camel.length; k++) need(equal(rows[i][camel[k]], bare[i][snake[k]]), 'snake/camel factual parity');
    const first = rpc('documents_list', { company: 'synthetic', limit: 1 }).data, rest = rpc('documents_list', { company: 'synthetic', offset: 1, limit: 5000 }).data;
    need(equal([ ...first.documents, ...rest.documents ], rows) && first.total === rows.length && first.count === 1 && first.nextOffset === 1 && first.hasMore === true && rest.hasMore === false && !('nextOffset' in rest), 'pure MCP pagination');
    need(cli('list', [], 0, true).includes(`Bilag (${rows.length})`), 'source human heading');
  }
  return {
    async run(page: Page) {
      onStep('begin');
      need(!ran, 'one effectful run only'); ran = true; owner = page;
      writeFileSync(cookie, (await owner.context().cookies(base)).map(c => c.name + '=' + c.value).join('; '), { mode: 0o600, flag: 'wx' });
      service = await admin('service-principals', { displayName: 'Synthetic original documents165', operationId: randomUUID(), confirm: true });
      writeFileSync(key, service.secret + '\n', { mode: 0o600, flag: 'wx' });
      await admin('members/access', { userId: service.serviceAccountId, action: 'set-role', workspaceRole: 'member' });
      const grant = (role: string) => admin('members/company', { userId: service.serviceAccountId, companySlug: 'synthetic', action: 'grant', role });
      await grant('bookkeeper');
      const before = await http(root + 'journal'); need(before.journal.journal.entries.length === 0, 'fresh journal');
      await http(root + 'company', { cvr: '87654321', address: 'Synthetic Street 165', postalCode: '1000', city: 'Synthetic City', vatPeriodType: 'quarter' }, 'PATCH');
      const company = (await http(root + 'company')).company;
      for (const account of [{ accountNo: '4865', name: 'Synthetic original expense', type: 'expense', normalBalance: 'debit', defaultVatCode: 'DK_PURCHASE_25' }, { accountNo: '6865', name: 'Synthetic input VAT', type: 'vat', normalBalance: 'debit' }, { accountNo: '7865', name: 'Synthetic creditors', type: 'liability', normalBalance: 'credit' }]) await http(root + 'accounts', account);
      await http(root + 'account-roles', { role: 'input_vat', accountNo: '6865', confirm: true }); await http(root + 'account-roles', { role: 'creditors', accountNo: '7865', confirm: true });
      const vendor = await http(root + 'master-data/original/vendors', { input: { name: 'Synthetic Supplier165', address: 'Supplier Street 165', vatOrCvr: 'DK12345678', countryCode: 'DK', identifierKind: 'dk_cvr' }, confirm: true });
      const vendorId = vendor.vendorId;
      need(Number.isSafeInteger(vendorId), 'ordinary API current vendor');
      const names = ['ingest', 'list', 'enrich', 'set_company_context', 'parse', 'parse_pending', 'parse_status', 'parsed_text'];
      const tools = rpc('tools/list', {}).tools;
      for (const name of names) {
        const t = tools.find((t: any) => t.name === 'documents_' + name), read = ['list', 'parse_status', 'parsed_text'].includes(name);
        need(t?.inputSchema && t.outputSchema && equal(t.annotations, { readOnlyHint: read, destructiveHint: false, idempotentHint: name !== 'ingest', openWorldHint: false }), 'source schemas/annotations ' + name);
      }
      const metadata: any = { source: 'Synthetic original165', documentType: 'purchase_sale', issueDate: (await http(root + 'scheduled-recurring')).scheduledRecurring.asOfDate, invoiceNo: 'SYN-165', deliveryDescription: 'Synthetic office supplies', amountIncVat: 125, vatAmount: 25, currency: 'DKK', sender: { name: 'Explicit Supplier165', address: 'Supplier Street 165', vatOrCvr: 'DK12345678', countryCode: 'DK', identifierKind: 'dk_cvr' }, purchaseVatLines: [{ classification: 'dk_purchase_25', netAmount: 100, vatAmount: 25 }] };
      const local = (name: string, value: any) => { const path = join(work, name); writeFileSync(path, JSON.stringify(value), { mode: 0o600, flag: 'wx' }); return path; };
      for (const name of ['ingest', 'enrich', 'set_company_context', 'parse', 'parse_pending']) {
        const args = { company: 'synthetic', ...(name === 'ingest' ? { filePath: join(work, 'never-read.pdf'), metadata } : name === 'parse_pending' ? {} : { documentId: 1 }), ...(name === 'enrich' ? { metadata } : {}), ...(name === 'set_company_context' ? { sourceReference: 'source', businessUseReason: 'reason' } : {}) };
        for (const confirm of [undefined, false]) need(equal(rpc('documents_' + name, { ...args, ...(confirm === undefined ? {} : { confirm }) }, 'refused', true), { ok: false, errors: ['confirm: true required for write tool documents_' + name], code: 'CONFIRM_REQUIRED' }), 'confirmation before file/HTTP');
        rpc('documents_' + name, { ...args, confirm: null }, 'invalid', true); rpc('documents_' + name, { ...args, confirm: true, actor: 'user:forged' }, 'invalid', true);
      }
      cli('parse', ['--document-id', '1'], 2, false, true); cli('enrich', ['--document-id', '1', '--metadata', join(work, 'absent.json')], 2, false, true);
      rpc('documents_list', { company: 'synthetic', limit: 5001 }, 'invalid', true); rpc('documents_parsed_text', { company: 'synthetic', documentId: 1, limit: 11 }, 'invalid', true);
      const paths: Record<string, string> = {};
      for (const kind of ['enrich', 'context', 'blank', 'malformed', 'force']) {
        const bytes = kind === 'malformed' ? Buffer.from('%PDF-1.4\nSynthetic malformed original165\n') : pdf(kind === 'blank' ? '' : `BT\n/F1 18 Tf\n72 720 Td\n(Synthetic original165 ${kind}) Tj\nET\n`);
        paths[kind] = join(work, `synthetic-original165-${kind}.pdf`); writeFileSync(paths[kind], bytes, { mode: 0o600, flag: 'wx' });
        let m: any = ['blank', 'malformed'].includes(kind) ? { source: 'Synthetic original165 ' + kind, documentType: 'cash_register_receipt' } : { ...metadata, invoiceNo: kind === 'enrich' ? 'SYN-165-ENRICH' : 'SYN-165-CONTEXT', ...(kind !== 'enrich' ? { danishSimplifiedPurchaseInvoice: true } : { documentType: 'cash_register_receipt' }) };
        if (kind === 'context') { m = { ...m, sender: { name: 'Explicit Supplier165' } }; }
        if (kind === 'force') rpc('documents_ingest', { company: 'synthetic', filePath: paths[kind], metadata: m, confirm: true }, 'refused');
        const value = ['enrich', 'blank'].includes(kind) ? cli('ingest', ['--file', paths[kind], '--metadata', local(kind + '.json', m)]) : rpc('documents_ingest', { company: 'synthetic', filePath: paths[kind], metadata: m, ...(kind === 'context' ? { vendorId } : {}), ...(kind === 'force' ? { force: true } : {}), confirm: true }).data;
        need(value.ok !== false && value.documentId > 0 && value.sha256 === hash(bytes) && value.storedPath === null, 'flat ingest receipt ' + kind); documents[kind] = value;
      }
      rpc('documents_ingest', { company: 'synthetic', filePath: paths.context, metadata: { ...metadata, invoiceNo: 'UNIQUE', danishSimplifiedPurchaseInvoice: true }, force: true, confirm: true }, 'refused');
      const enriched = { ...metadata, documentType: 'cash_register_receipt', invoiceNo: 'SYN-165-ENRICH', recipient: { name: company.name, address: company.address, vatOrCvr: company.cvr } }, enrichedFile = local('enriched.json', enriched);
      need(cli('enrich', ['--document-id', String(documents.enrich.documentId), '--metadata', enrichedFile, '--confirm', 'yes']).enriched === true, 'CLI enrichment');
      need(rpc('documents_enrich', { company: 'synthetic', documentId: documents.enrich.documentId, metadata: enriched, confirm: true }).data.enriched === false, 'MCP enrichment repeat');
      rpc('documents_enrich', { company: 'synthetic', documentId: documents.enrich.documentId, metadata: { ...enriched, paymentDetails: 'conflicting repeat' }, confirm: true }, 'refused');
      const ctx = { company: 'synthetic', documentId: documents.context.documentId, sourceReference: 'Synthetic paper receipt165', businessUseReason: 'Office supplies used solely for synthetic company', confirm: true };
      need(cli('set-company-context', ['--document-id', String(ctx.documentId), '--source-reference', ctx.sourceReference, '--business-use-reason', ctx.businessUseReason, '--confirm', 'yes']).applied === true, 'CLI verified context');
      need(rpc('documents_set_company_context', ctx).data.applied === false, 'MCP context repeat'); rpc('documents_set_company_context', { ...ctx, businessUseReason: 'conflicting' }, 'refused');
      need(cli('parse', ['--document-id', String(documents.enrich.documentId), '--confirm', 'yes']).parse.cached === false, 'CLI fresh parse');
      need(rpc('documents_parse', { company: 'synthetic', documentId: documents.enrich.documentId, confirm: true }).data.parse.cached === true, 'MCP cached parse');
      const pending = cli('parse-pending', ['--limit', '100', '--cursor', '0', '--confirm', 'yes']); need(pending.batch.requested === 4 && pending.batch.parsed === 4 && pending.batch.failed === 0, 'bounded pending retains cacheable failures');
      need(rpc('documents_parse_pending', { company: 'synthetic', confirm: true }).data.batch.requested === 0, 'MCP resolved pending');
      Object.assign(evidence, await reads());
      for (const [kind, doc] of Object.entries(documents)) {
        const id = String(doc.documentId), status = evidence[kind].status.parse;
        need(status.status === (kind === 'blank' ? 'no_text_layer' : kind === 'malformed' ? 'malformed_pdf' : 'ok'), 'declared parser outcome ' + kind);
        need(equal(cli('parse-status', ['--document-id', id]), evidence[kind].status) && equal(cli('parsed-text', ['--document-id', id]), evidence[kind].text), 'CLI exact PDF reads');
        need(equal(rpc('documents_parse_status', { company: 'synthetic', documentId: doc.documentId }).data, { parse: status }), 'MCP status/source reads');
        const text = { ...evidence[kind].text }; delete text.ok; need(equal(rpc('documents_parsed_text', { company: 'synthetic', documentId: doc.documentId }).data, text), 'MCP exact public text');
      }
      const filled = (await http(original + '/list')).documents.find((row: any) => row.id === documents.context.documentId);
      need(filled.senderVatOrCvr === 'DK12345678' && filled.supplierCountryCode === 'DK' && filled.supplierIdentifierKind === 'dk_cvr', 'vendor fills only missing supplier identity');
      await lists(); need(equal(await http(root + 'journal'), before), 'all intake/parse/history actions financially inert');
      // Exactly one accepted payable registration; never replay or duplicate this financial request.
      const posted = await http(root + 'payables', { documentId: documents.context.documentId, billDate: metadata.issueDate, dueDate: metadata.issueDate, expenseAccountNo: '4865', vatTreatment: 'standard', vendorId, confirm: true });
      const journal = (await http(root + 'journal')).journal.journal; need(journal.entries.length === 1 && journal.entries[0].documentId === documents.context.documentId && journal.entries[0].total === 125, 'one factual payable through verified context gate');
      const preRevoke = await snapshot(); await admin('members/company', { userId: service.serviceAccountId, companySlug: 'synthetic', action: 'revoke' });
      rpc('documents_list', { company: 'synthetic' }, 'refused'); rpc('documents_parse', { company: 'synthetic', documentId: documents.enrich.documentId, confirm: true }, 'refused');
      need(equal(await snapshot(), preRevoke), 'revoked service publishes no document/history/parser/accounting effect');
      stable = await snapshot(); return { documents, evidence, posted, stable, transcript, qualified: true, delayedScannerRevocationActualQualified: false, installedNativeQualified: false };
    },
    async assertStable(page: Page) { owner = page; need(equal(await snapshot(), stable), 'exact original/history/PDF/audit/payable source after restart/restore'); return { stable: true }; },
  };
}
