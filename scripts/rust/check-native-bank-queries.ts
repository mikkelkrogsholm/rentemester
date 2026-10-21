import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

/** Existing correction setup, now inspected through the original bank read/report family. */
export function createBankQueriesProof(page: Page, base: string, work: string, member: Page) {
  const prefix = '/api/companies/synthetic/', ownerFile = join(work, 'bank-owner.cookie'), memberFile = join(work, 'bank-member.cookie');
  let step = 'setup', today = '', old = '', complete = false, stable: any, selected: any, originalId = 0, replacementId = 0, reversalId = 0;
  let sources: { sha256: string; bytesBase64: string }[] = [];
  const importReceipts: any[] = [];
  const accountLifecycle: any = { created: [], updated: [] };
  const need = (v: unknown, message: string) => { if (!v) { const error = Error(message); error.name = 'BankQueries_' + step + '_' + message.replace(/[^A-Za-z0-9_]/g, '_').slice(0, 80); throw error; } };
  const hash = (v: Buffer | string) => createHash('sha256').update(v).digest('hex');
  const query = (v: Record<string, any>) => new URLSearchParams(Object.entries(v).map(([k, value]) => [k, String(value)])).toString();
  async function get(path: string, actor = page) { const r = await actor.request.get(base + path); need(r.ok(), 'Canonical_read_' + path.split('?')[0]); return r.json(); }
  async function post(path: string, data: unknown) { const r = await page.request.post(base + path, { headers: { origin: base }, data }); const v = await r.json(); need(r.ok() && v.ok !== false, 'Setup_' + path); return v; }
  async function cookie(actor: Page, file: string) { rmSync(file, { force: true }); writeFileSync(file, (await actor.context().cookies(base)).map(c => `${c.name}=${c.value}`).join('; '), { mode: 0o600, flag: 'wx' }); }
  function cli(name: string, args: Record<string, any> = {}, file = ownerFile, denied = false, human = false) {
    const commands: Record<string, string[]> = { list: ['bank', 'list'], report: ['reconcile', 'bank'], accounts: ['bank-account', 'list'], import: ['bank', 'import'], accountAdd: ['bank-account', 'add'], accountUpdate: ['bank-account', 'update'] };
    const flags: Record<string, string> = { textMatch: 'text-match', includeInactive: 'include-inactive', bankName: 'bank-name', registrationNo: 'registration-no', accountNo: 'account-no', accountOwner: 'account-owner', customerNo: 'customer-no', ledgerAccountNo: 'ledger-account' };
    const r = spawnSync(join(work, 'rentemester-rust'), [...commands[name], '--server', base, '--allow-loopback-http', '--cookie-file', file, '--company', 'synthetic', ...(human ? [] : ['--json']), ...Object.entries(args).flatMap(([key, v]) => ['--' + (flags[key] ?? key), String(v)])], { encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!r.error && r.status === (denied ? 1 : 0), 'Original_CLI_exit');
    const secret = readFileSync(file, 'utf8').trim(); need(secret && !r.stdout.includes(secret) && !r.stderr.includes(secret), 'CLI_session_privacy');
    return human ? r.stdout : JSON.parse(r.stdout);
  }
  function mcp(name: string, args: Record<string, any> = {}, file = ownerFile, denied = false, schemaError = false) {
    const names: Record<string, string> = { list: 'bank_list', report: 'reconcile_bank', accounts: 'bank_account_list', import: 'bank_import', accountUpdate: 'bank_account_update' };
    const wire = [{ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'synthetic-bank-queries141', version: '1' } } }, { jsonrpc: '2.0', method: 'notifications/initialized' }, { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: names[name], arguments: { company: 'synthetic', ...args } } }].map(v => JSON.stringify(v)).join('\n') + '\n';
    const r = spawnSync(join(work, 'rentemester-mcp'), ['--server', base, '--allow-loopback-http', '--cookie-file', file], { input: wire, encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    const secret = readFileSync(file, 'utf8').trim(); need(!r.error && r.status === 0 && !r.stdout.includes(secret) && !r.stderr.includes(secret), 'MCP_transport_privacy');
    const call = r.stdout.trim().split('\n').map(v => JSON.parse(v)).find(v => v.id === 2);
    if (schemaError) { need(call?.error?.code === -32602, 'MCP_schema_error'); return call.error; }
    const v = call?.result?.structuredContent;
    need(!call?.error && call.result.isError === denied && v?.ok === !denied && Array.isArray(v.errors) && isDeepStrictEqual(JSON.parse(call.result.content[0].text), v), 'MCP_original_envelope');
    if (denied) need(v.errors.length > 0 && v.data === undefined, 'No_refused_data');
    return v;
  }
  const endpoint = (kind: string) => prefix + 'bank/' + ({ list: 'transactions', report: 'reconciliation', accounts: 'accounts' } as Record<string, string>)[kind];
  async function canonical(kind: string, args: Record<string, any> = {}, actor = page) { return get(endpoint(kind) + '?' + query(args), actor); }
  const flattened = (v: any) => ({ ok: v.ok, ...(v.data ?? {}), errors: v.errors, ...(v.appliedRules ? { appliedRules: v.appliedRules } : {}), ...(v.code ? { code: v.code } : {}) });
  async function books() { return { journal: await get(prefix + 'journal/entries?all=true'), trial: await get(prefix + 'trial-balance'), bank: await canonical('list'), documents: await get(prefix + 'documents'), audit: await get(prefix + 'audit-log?limit=5000') }; }
  async function unchanged() { need(isDeepStrictEqual(await books(), stable), 'Read_only_books_sources_audit'); }
  async function sourceFiles() {
    for (const source of sources) { const bytes = readFileSync(join(work, 'workspace/synthetic/bank/processed', source.sha256 + '.txt')); need(hash(bytes) === source.sha256 && bytes.equals(Buffer.from(source.bytesBase64, 'base64')), 'Retained_exact_bank_original'); }
  }
  async function projections() {
    step = 'projections';
    const cases: Record<string, any>[] = [{}, { status: 'matched' }, { status: 'unmatched' }, { from: old, to: old }, { textMatch: ' FX ' }, { amount: 10 }, { account: 'synthetic-query-eur' }, { account: String(selected.fxAccountId) }, { from: today, to: today }, { from: today, to: old }, { limit: 1, offset: 0 }, { limit: 1, offset: 1 }, { limit: 1, offset: 2 }, { limit: 1, offset: 3 }];
    const out = [];
    for (const args of cases) {
      const http = await canonical('list', { limit: 500, offset: 0, ...args });
      need(http.ok && isDeepStrictEqual(mcp('list', args), http), 'Original_MCP_list_exact_page');
      if (!('limit' in args)) need(isDeepStrictEqual(cli('list', args), flattened(await canonical('list', args))), 'Original_CLI_unpaged_shape');
      out.push({ args, value: http });
    }
    const rows = out[0].value.data.rows; need(rows.length === 3 && rows.find((r: any) => r.id === selected.bankId)?.journalEntryId === replacementId && out[1].value.data.count === 1 && out[2].value.data.count === 2, 'Reversal_aware_corrected_relation');
    need(out[3].value.data.count === 1 && out[3].value.data.rows[0].transactionDate === old && out[4].value.data.rows[0].amount === 10 && out[5].value.data.rows[0].currency === 'EUR', 'All_years_text_and_original_currency');
    need(out[9].value.data.count === 0 && out[13].value.data.count === 0 && !out[13].value.data.hasMore && !('nextOffset' in out[13].value.data), 'Empty_range_and_end_page');
    const reportArgs = { from: old, to: today }, report = await canonical('report', reportArgs);
    need(isDeepStrictEqual(mcp('report', reportArgs), report) && isDeepStrictEqual(cli('report', reportArgs), flattened(report)), 'Original_report_exact_parity');
    need(report.data.matchedCount === 1 && report.data.unmatchedCount === 2 && report.data.matchedAmountTotal === 24.14 && report.data.unmatchedAmountTotal === 17.25 && isDeepStrictEqual(report.appliedRules, ['DK-BOOKKEEPING-RECONCILIATION-001']), 'Signed_original_unit_totals_and_rule');
    const accounts = await canonical('accounts'); need(isDeepStrictEqual(mcp('accounts'), accounts) && isDeepStrictEqual(cli('accounts'), flattened(accounts)) && accounts.data.accounts.some((a: any) => a.slug === 'synthetic-query-eur'), 'Original_account_projection');
    await sourceFiles(); await unchanged();
    return { cases: out, report, accounts, originalUnitsQualified: true, originalCliUnpagedQualified: true, correctedRelationQualified: true, rpcTextStructuredAgreement: true };
  }
  return {
    async run() {
      need(!complete, 'Run_once'); await cookie(page, ownerFile); await cookie(member, memberFile);
      today = (await get('/api/portfolio')).portfolio.asOf; old = String(Number(today.slice(0, 4)) - 1) + '-12-31';
      for (const [accountNo, type, normalBalance] of [['1741', 'asset', 'debit'], ['1742', 'asset', 'debit'], ['8741', 'equity', 'credit'], ['8742', 'equity', 'credit']]) await post(prefix + 'accounts', { accountNo, name: 'Synthetic query ' + accountNo, type, normalBalance });
      step = 'original_bank_account_lifecycle';
      for (const [slug, currency, ledgerAccountNo] of [['synthetic-query-dkk', 'DKK', '1741'], ['synthetic-query-eur', 'EUR', '1742']]) {
        const created = cli('accountAdd', { name: 'Synthetic query ' + currency, slug, currency, ledgerAccountNo, bankName: 'Synthetic bank143', accountNo: '123456143' });
        need(created.ok && created.account.slug === slug && created.account.currency === currency && created.account.ledgerAccountNo === ledgerAccountNo, 'Original_account_create_full_receipt'); accountLifecycle.created.push(created.account);
      }
      const firstAccount = accountLifecycle.created[0], secondAccount = accountLifecycle.created[1];
      const unconfirmedAccount = mcp('accountUpdate', { account: firstAccount.slug, active: false }, ownerFile, true); need(unconfirmedAccount.code === 'CONFIRM_REQUIRED', 'Account_confirmation_before_mutation');
      const edited = mcp('accountUpdate', { account: '  ' + firstAccount.slug + '  ', bankName: 'Synthetic edited143', accountNo: '', bic: 'SYNTHDK143', confirm: true }).data.account;
      need(edited.id === firstAccount.id && edited.slug === firstAccount.slug && edited.createdAt === firstAccount.createdAt && edited.bankName === 'Synthetic edited143' && edited.accountNo === null && edited.bic === 'SYNTHDK143' && edited.ledgerAccountNo === firstAccount.ledgerAccountNo, 'Account_identity_preserved_and_empty_cleared'); accountLifecycle.updated.push(edited);
      const browserEdit = await page.request.patch(base + prefix + 'bank-accounts/' + secondAccount.slug, { headers: { origin: base }, data: { confirm: true, bankName: 'Synthetic browser143', customerNo: '143', accountNo: null } }); const browserAccount = (await browserEdit.json()).bankAccount;
      need(browserEdit.ok() && browserAccount.id === secondAccount.id && browserAccount.createdAt === secondAccount.createdAt && browserAccount.customerNo === '143' && browserAccount.accountNo === null && browserAccount.bankName === 'Synthetic browser143', 'Existing_browser_PATCH_same_account'); accountLifecycle.updated.push(browserAccount);
      const csvs = [
        { account: 'synthetic-query-dkk', csvContent: `transaction_date,text,amount,currency,reference\n${old},Synthetic old query141,7.25,DKK,OLD141\n${today},Synthetic current query141,24.14,DKK,CURRENT141\n` },
        { account: 'synthetic-query-eur', csvContent: `transaction_date,text,amount,currency,reference,amount_dkk,fx_rate_to_dkk\n${today},Synthetic FX query141,10.00,EUR,FX141,75.00,7.5\n` },
      ];
      step = 'original_bank_import';
      const files = csvs.map((data, i) => { const file = join(work, `bank-import142-${i}.csv`); writeFileSync(file, data.csvContent, { mode: 0o600, flag: 'wx' }); return file; });
      const unconfirmed = mcp('import', { csvPath: join(work, 'absent142.csv') }, ownerFile, true);
      need(unconfirmed.code === 'CONFIRM_REQUIRED', 'Import_confirmation_before_file_access');
      for (const [i, data] of csvs.entries()) {
        const receipt = i === 0 ? cli('import', { file: files[i], account: '  ' + data.account + '  ' }) : flattened(mcp('import', { ...data, confirm: true }));
        need(receipt.ok && receipt.imported === (i === 0 ? 2 : 1) && receipt.skippedDuplicates === 0 && receipt.exceptionsCreated === receipt.imported && receipt.bankAccountSlug === data.account && receipt.sourceFileHash === hash(data.csvContent) && Array.isArray(receipt.errors) && receipt.errors.length === 0, 'Original_import_exact_counts_and_source');
        const duplicate = i === 0 ? flattened(mcp('import', { csvPath: files[i], csvContent: 'ignored inline payload', account: data.account, confirm: true })) : cli('import', { file: files[i], account: data.account });
        need(duplicate.ok && duplicate.imported === 0 && duplicate.skippedDuplicates === receipt.imported && duplicate.sourceFileHash === receipt.sourceFileHash && duplicate.exceptionsCreated === 0, 'Original_import_duplicate_no_rows_or_exceptions');
        importReceipts.push({ receipt, duplicate }); sources.push({ sha256: receipt.sourceFileHash, bytesBase64: Buffer.from(data.csvContent).toString('base64') });
      }
      const missing = mcp('import', { csvPath: join(work, 'absent142.csv'), csvContent: csvs[0].csvContent, confirm: true }, ownerFile, true);
      need(missing.ok === false, 'Missing_selected_file_does_not_fall_back_inline');
      const refusedResponse = await page.request.post(base + prefix + 'bank/import-original', { headers: { origin: base }, data: { ...csvs[0], account: 'missing-import142', confirm: true } });
      const refusedImport = await refusedResponse.json();
      need(refusedResponse.status() === 200 && refusedImport.ok === false && refusedImport.data === undefined && refusedImport.errors.length > 0, 'Original_import_business_refusal');
      need(isDeepStrictEqual(mcp('import', { ...csvs[0], account: 'missing-import142', confirm: true }, ownerFile, true), refusedImport), 'Original_import_MCP_business_errors');
      need(isDeepStrictEqual(cli('import', { file: files[0], account: 'missing-import142' }, ownerFile, true), flattened(refusedImport)), 'Original_import_CLI_business_errors');
      for (const file of files) rmSync(file);
      step = 'historical_account_constraints';
      const beforeHistorical = await books();
      for (const patch of [{ ledgerAccountNo: '1742' }, { ledgerAccountNo: '' }, { currency: 'EUR' }]) {
        const r = await page.request.post(base + prefix + 'bank/accounts-original/update', { headers: { origin: base }, data: { account: firstAccount.slug, confirm: true, ...patch } }); const failure = await r.json();
        need(r.status() === 200 && failure.ok === false && failure.data === undefined && failure.errors.length > 0, 'Historical_account_change_refused');
        need(isDeepStrictEqual(mcp('accountUpdate', { account: firstAccount.slug, confirm: true, ...patch }, ownerFile, true), failure), 'Historical_account_MCP_error_exact');
        need(isDeepStrictEqual(cli('accountUpdate', { account: firstAccount.slug, ...patch }, ownerFile, true), flattened(failure)), 'Historical_account_CLI_error_exact');
      }
      need(isDeepStrictEqual(await books(), beforeHistorical), 'Historical_refusals_have_no_effects');
      accountLifecycle.historicalRemapCurrencyRefused = true;
      const inactive = cli('accountUpdate', { account: String(firstAccount.id), active: false }); need(inactive.ok && !inactive.account.active && inactive.account.id === firstAccount.id, 'Deactivate_preserves_identity');
      const beforeInactive = await books(); need(beforeInactive.bank.data.count === 3, 'Deactivate_retains_imported_history'); accountLifecycle.deactivatedWithBankRows = beforeInactive.bank.data.count; mcp('import', { ...csvs[0], confirm: true }, ownerFile, true); need(isDeepStrictEqual(await books(), beforeInactive), 'Inactive_import_has_no_effects');
      const reactivated = mcp('accountUpdate', { account: firstAccount.slug, active: true, confirm: true }).data.account; need(reactivated.active && reactivated.id === firstAccount.id && reactivated.createdAt === firstAccount.createdAt, 'Reactivate_preserves_history');
      accountLifecycle.reactivated = reactivated;
      await page.goto(base + '/companies/synthetic/bank-accounts'); await page.getByRole('heading', { name: 'Bankkonti', exact: true }).waitFor(); await page.getByText('Synthetic browser143', { exact: true }).waitFor(); accountLifecycle.sharedReactReadQualified = true;
      const rows = (await canonical('list')).data.rows, row = rows.find((r: any) => r.reference === 'CURRENT141'), fx = rows.find((r: any) => r.reference === 'FX141'); need(row && fx, 'Actual_imported_row_ids'); selected = { bankId: row.id, fxAccountId: fx.bankAccountId };
      async function journal(text: string, equity: string, key: string) { return post(prefix + 'manual-journal', { confirm: true, input: { format: 'rentemester-manual-journal-post/1', expectedRuleVersion: (await get(prefix + 'journal')).journal.nativePosting.ruleVersion, idempotencyKey: key, request: { transactionDate: today, text, currency: 'DKK', lines: [{ accountNo: '1741', debitAmount: '24.14', creditAmount: '0.00' }, { accountNo: equity, debitAmount: '0.00', creditAmount: '24.14' }] } } }); }
      step = 'real_link_reversal_correction';
      originalId = (await journal('Synthetic query original141', '8741', 'synthetic-query141-original')).entryId;
      const plans = (await get(prefix + `bank/reconciliation-candidates?bankTransactionId=${row.id}`)).reconciliationCandidates.plans, plan = plans.find((p: any) => p.journalEntryId === originalId); need(plan, 'Existing_link_plan');
      const initial = await post(prefix + 'bank/reconciliation-link', { bankTransactionId: row.id, journalEntryId: originalId, planHash: plan.planHash, note: 'Synthetic reviewed query original', confirm: true });
      need((await canonical('list', { status: 'matched' })).data.rows.some((r: any) => r.journalEntryId === originalId), 'Initial_real_match');
      replacementId = (await journal('Synthetic query replacement141', '8742', 'synthetic-query141-replacement')).entryId;
      const reversal = await post(prefix + 'manual-journal-reversal', { confirm: true, input: { format: 'rentemester-manual-journal-reversal/1', expectedRuleVersion: (await get(prefix + 'journal')).journal.nativePosting.ruleVersion, idempotencyKey: 'synthetic-query141-reversal', request: { entryId: String(originalId), transactionDate: today, reason: 'Synthetic query reversed original' } } }); reversalId = reversal.entryId;
      // The existing accounting audit barrier refuses an uncorrected native link
      // after reversal. Do not replace this refusal with misleading successful rows.
      const reversedResponse = await page.request.get(base + endpoint('list'));
      const reversedBody = await reversedResponse.json();
      need(reversedResponse.status() === 422 && reversedBody.ok === false && reversedBody.code === 'bank_query_sources_unavailable', 'Reversed_native_chain_requires_correction');
      mcp('list', {}, ownerFile, true); cli('list', {}, ownerFile, true);
      const correction = (await get(prefix + `bank/reconciliation-correction-plan?bankTransactionId=${row.id}&replacementJournalEntryId=${replacementId}`)).plan.plan;
      await post(prefix + 'bank/reconciliation-correction', { bankTransactionId: row.id, replacementJournalEntryId: replacementId, expectedReconciliationId: initial.reconciliation.reconciliationId, planHash: correction.planHash, reason: 'Synthetic independently posted replacement', idempotencyKey: 'synthetic-query141-correction', confirm: true });
      stable = await books(); const proof = await projections();
      step = 'refusals';
      for (const [kind, args] of [['list', { from: '2026-02-30' }], ['list', { account: 'missing-query141' }], ['report', { from: today, to: old }]] as [string, Record<string, any>][]) {
        const v = await canonical(kind, args); need(v.ok === false && v.data === undefined && v.errors.length > 0, 'Canonical_business_refusal'); need(isDeepStrictEqual(mcp(kind, args, ownerFile, true), v), 'Original_MCP_business_refusal'); cli(kind, args, ownerFile, true);
      }
      mcp('list', { limit: 0 }, ownerFile, false, true); mcp('list', { forgedActor: 'owner' }, ownerFile, false, true);
      need(isDeepStrictEqual(await canonical('list', {}, member), await canonical('list')), 'Current_reader_exact_data');
      const memberId = (await get('/api/me', member)).user.id; await post('/api/workspace/members/company', { userId: memberId, action: 'revoke', companySlug: 'synthetic' });
      need((await member.request.get(base + endpoint('list'))).status() === 403, 'Current_revoked_HTTP_read'); mcp('list', {}, memberFile, true); cli('list', {}, memberFile, true);
      mcp('accountUpdate', { account: firstAccount.slug, bankName: 'Refused current grant143', confirm: true }, memberFile, true); cli('accountUpdate', { account: firstAccount.slug, bankName: 'Refused current grant143' }, memberFile, true);
      need((await member.request.patch(base + prefix + 'bank-accounts/' + firstAccount.slug, { headers: { origin: base }, data: { confirm: true, bankName: 'Refused browser143' } })).status() === 403, 'Current_revoked_browser_account_mutation');
      step = 'physical_source_refusal';
      const file = join(work, 'workspace/synthetic/bank/processed', sources[1].sha256 + '.txt'), bytes = readFileSync(file); writeFileSync(file, Buffer.from('Synthetic corrupt source141\n'));
      try { const r = await page.request.get(base + endpoint('list')); need(!r.ok(), 'Unmatched_original_corruption_refused'); mcp('list', {}, ownerFile, true); cli('list', {}, ownerFile, true); } finally { writeFileSync(file, bytes); }
      await sourceFiles(); await unchanged(); complete = true;
      return { qualified: true, ordinaryMfa: true, originalBankQueryQualified: true, originalBankImportQualified: true, importReceipts, originalBankAccountQualified: true, accountLifecycle, reversedChainRefused: true, sourceCorruptionRefused: true, currentRevocationRefused: true, sources, today, old, selected, originalId, replacementId, reversalId, projections: proof, accountingFingerprint: hash(JSON.stringify(stable)), installedNativeQualified: false, sourceFreeRestoreQualified: false };
    },
    async reopen() { need(complete, 'Completed_fixture'); await cookie(page, ownerFile); await unchanged(); const proof = await projections(); rmSync(ownerFile); rmSync(memberFile); return { qualified: true, ordinaryMfa: true, exactNormalMfaRestartState: true, projections: proof, accountingFingerprint: hash(JSON.stringify(stable)), installedNativeQualified: false, sourceFreeRestoreQualified: false }; },
  };
}
