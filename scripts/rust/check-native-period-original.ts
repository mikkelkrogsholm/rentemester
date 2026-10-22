import type { Page } from '@playwright/test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual as equal } from 'node:util';

/** Finite, synthetic local proof. No external filing, restore or desktop claim. */
export function createOriginalPeriodProof(page: Page, base: string, work: string, member: Page) {
  return createProof(page, base, work, member, false);
}

export function createOriginalJournalProof(page: Page, base: string, work: string, member: Page) {
  return createProof(page, base, work, member, true);
}

function createProof(page: Page, base: string, work: string, member: Page, originalJournal: boolean) {
  const prefix = '/api/companies/synthetic/', ownerFile = join(work, 'period-original-owner.cookie'), memberFile = join(work, 'period-original-member.cookie');
  const reviews: any[] = [], refusals: any[] = [], closes: any[] = [], journalEvidence: any = {};
  let complete = false, stable: any, start = '', end = '', year = '', periodId = 0, closeHash = '';
  const reference = 'SYNTHETIC-ORIGINAL-PERIOD-RECEIPT';
  const need = (v: unknown, message: string) => { if (!v) throw Error('Original period: ' + message); };
  const fingerprint = (v: unknown) => createHash('sha256').update(JSON.stringify(v)).digest('hex');
  async function get(path: string, actor = page) { const r = await actor.request.get(base + path); need(r.ok(), 'canonical read ' + path); return r.json(); }
  async function post(path: string, data: unknown) { const r = await page.request.post(base + path, { headers: { origin: base }, data }); const v = await r.json(); need(r.ok() && v.ok !== false, 'native setup ' + path + ': ' + JSON.stringify(v)); return v; }
  async function cookie(actor: Page, file: string) { const cookies = await actor.context().cookies(base); need(cookies.some(c => c.httpOnly), 'ordinary MFA session'); writeFileSync(file, cookies.map(c => c.name + '=' + c.value).join('; '), { mode: 0o600 }); }
  function cli(operation: string, flags: Record<string, string | number> = {}, denied: boolean | 2 = false, file = ownerFile) {
    // Exactly one invocation. In particular, never retry a timed out/unknown write.
    const r = spawnSync(join(work, 'rentemester-rust'), ['period', operation, '--server', base, '--allow-loopback-http', '--cookie-file', file, '--company', 'synthetic', '--json', ...Object.entries(flags).flatMap(([k, v]) => ['--' + k, String(v)])], { encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    const secret = readFileSync(file, 'utf8');
    need(!r.error && r.status === (denied === 2 ? 2 : denied ? 1 : 0), operation + ' exact exit: ' + r.stdout.slice(0, 1500));
    need(secret && !r.stdout.includes(secret) && !r.stderr.includes(secret), 'session privacy');
    const v = JSON.parse(r.stdout || r.stderr);
    if (denied === 2) need(!r.stdout && Array.isArray(v) && v.length > 0 && v.every(e => typeof e === 'string'), 'source usage diagnostic');
    else if (denied) need(v.ok === false && Array.isArray(v.errors) && v.errors.length > 0, 'explicit refusal');
    return v;
  }
  function journalCli(operation: string, flags: Record<string, string | number>, denied = false) {
    const r = spawnSync(join(work, 'rentemester-rust'), ['journal', operation, '--server', base, '--allow-loopback-http', '--cookie-file', ownerFile, '--company', 'synthetic', '--json', ...Object.entries(flags).flatMap(([k, v]) => ['--' + k, String(v)])], { encoding: 'utf8', timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!r.error && r.status === (denied ? 1 : 0), 'original journal ' + operation + ' exact exit: ' + r.stdout.slice(0, 1500));
    const secret = readFileSync(ownerFile, 'utf8');
    need(!r.stdout.includes(secret) && !r.stderr.includes(secret), 'journal session privacy');
    const v = JSON.parse(r.stdout);
    need(v.ok === !denied && Array.isArray(v.appliedRules) && Array.isArray(v.errors) && (denied ? v.errors.length > 0 : v.errors.length === 0), 'flat journal result');
    if (!denied) need(Number.isSafeInteger(v.entryId) && v.entryId > 0 && typeof v.entryNo === 'string' && /^[a-f0-9]{64}$/.test(v.entryHash), 'numeric original journal identity');
    return v;
  }
  async function rollbackState() { return { books: await books(), sourceRevision: cli('readiness', { from: start, to: end }).packet.sourceRevision }; }
  async function journalRefusal(operation: string, flags: Record<string, string | number>) {
    const before = await rollbackState(), result = journalCli(operation, flags, true), after = await rollbackState();
    need(equal(before, after), 'original journal ' + operation + ' exact rollback');
    journalEvidence[operation + 'Refusal'] = { result, before, after, fingerprint: fingerprint(before) };
  }
  const bounds = () => ({ from: start, to: end });
  const periods = async () => (await get(prefix + 'periods')).periods;
  async function books() { return { journal: await get(prefix + 'journal/entries?all=true'), trial: await get(prefix + 'trial-balance'), periods: await periods(), audit: await get(prefix + 'audit-log?limit=5000'), documents: await get(prefix + 'documents') }; }
  function readiness(from = start, to = end) { const v = cli('readiness', { from, to }); need(v.ok === true && /^[a-f0-9]{64}$/.test(v.packet?.hash) && v.packet.blockers === 0, 'unmodified native readiness controls: ' + JSON.stringify(v)); return v.packet; }
  function review(from = start, to = end) {
    const packet = readiness(from, to), v = cli('review', { from, to, 'packet-hash': packet.hash, confirm: 'yes' });
    need(Object.keys(v).sort().join(',') === 'createdAt,id,packet,reviewerActor,reviewerPrincipal' && Number.isSafeInteger(v.id) && equal(v.packet, packet), 'bare source review object');
    reviews.push(v); need(equal(cli('status', { 'review-id': v.id }), { ok: true, review: v }), 'exact historical status'); return v;
  }
  function close(r: any, extra: Record<string, string> = {}, from = start, to = end) {
    const v = cli('close', { from, to, 'review-id': r.id, 'packet-hash': r.packet.hash, confirm: 'yes', ...extra });
    need(v.ok === true && Number.isSafeInteger(v.periodId) && !('id' in v) && equal(v.errors, []) && equal(v.appliedRules, ['DK-BOOKKEEPING-PERIOD-LOCK-001']), 'flat source close envelope'); closes.push(v); return v;
  }
  async function unchanged(before: any, context: string) { need(equal(await books(), before), context + ' changed books/periods/audit'); }
  async function refusal(operation: string, flags: Record<string, string | number>, exactError?: string) {
    const before = await books(), v = cli(operation, flags, true);
    if (exactError) need(equal(v.errors, [exactError]), 'source stale error');
    await unchanged(before, operation + ' refusal'); refusals.push({ operation, result: v }); return v;
  }
  async function historical() { for (const r of reviews) need(equal(cli('status', { 'review-id': r.id }), { ok: true, review: r }), 'saved review changed after source change/restart'); need(equal(cli('status', { 'review-id': 2147483647 }), { ok: true, review: null }), 'missing review is null'); }
  async function journal(key: string, text: string) {
    const version = (await get(prefix + 'journal')).journal.nativePosting.ruleVersion;
    return post(prefix + 'manual-journal', { confirm: true, input: { format: 'rentemester-manual-journal-post/1', expectedRuleVersion: version, idempotencyKey: key, request: { transactionDate: end, text, currency: 'DKK', lines: [{ accountNo: '8711', debitAmount: '3.25', creditAmount: '0.00' }, { accountNo: '8712', debitAmount: '0.00', creditAmount: '3.25' }] } } });
  }
  return {
    async run() {
      need(!complete, 'run once'); await cookie(page, ownerFile); await cookie(member, memberFile);
      const today = (await get('/api/portfolio')).portfolio.asOf; year = String(Number(today.slice(0, 4)) - 2); start = year + '-01-01'; end = year + '-03-31';
      const chart = (await get(prefix + 'accounts')).accounts;
      for (const [accountNo, type, normalBalance] of [['8711', 'equity', 'credit'], ['8712', 'equity', 'credit']]) await post(prefix + 'accounts', { accountNo, name: 'Synthetic original period ' + accountNo, type, normalBalance });
      for (const [role, accountNo, type, normalBalance] of [['bank', '1712', 'asset', 'debit'], ['debtors', '1713', 'asset', 'debit'], ['creditors', '7012', 'liability', 'credit']]) {
        need(!chart.accountRoles.ambiguous.includes(role), 'ambiguous role requires reconciliation');
        if (!chart.accountRoles.resolutions.some((r: any) => r.role === role && r.ok)) { await post(prefix + 'accounts', { accountNo, name: 'Synthetic original period ' + role, type, normalBalance }); await post(prefix + 'account-roles', { role, accountNo, confirm: true }); }
      }
      const ownerId = (await get('/api/me')).user.id, first = review();
      need(first.reviewerActor === 'user:' + ownerId && first.reviewerPrincipal?.subjectId === ownerId, 'current human attribution');
      if (originalJournal) {
        const input = { transactionDate: end, text: 'Synthetic original journal source change', lines: [{ accountNo: '8711', debitAmount: 3.25 }, { accountNo: '8712', creditAmount: 3.25 }] };
        const file = join(work, 'journal-original-input.json'); writeFileSync(file, JSON.stringify(input), { mode: 0o600 });
        const before = await rollbackState(), dryRun = journalCli('dry-run', { input: file }), after = await rollbackState();
        need(equal(before, after), 'dry-run exact books/audit/source rollback');
        need(typeof dryRun.previousHash === 'string' && equal(dryRun.accountEffects.map((e: any) => ({ accountNo: e.accountNo, balanceBefore: e.balanceBefore, balanceAfter: e.balanceAfter, delta: e.delta })), [{ accountNo: '8711', balanceBefore: 0, balanceAfter: 3.25, delta: 3.25 }, { accountNo: '8712', balanceBefore: 0, balanceAfter: -3.25, delta: -3.25 }]), 'exact numeric sorted account effects');
        const posted = journalCli('post', { input: file });
        need(['entryId', 'entryNo', 'entryHash'].every(k => posted[k] === dryRun[k]), 'provisional identity/no/hash equals first post');
        Object.assign(journalEvidence, { input, file, dryRun, posted, rollback: { before, after, fingerprint: fingerprint(before) }, post: await rollbackState() });
      } else await journal('synthetic-original-period-change', 'Synthetic original period source change');
      const changed = readiness(); need(changed.hash !== first.packet.hash, 'posted source changes hash');
      const stale = await refusal('review', { ...bounds(), 'packet-hash': first.packet.hash, confirm: 'yes' }, 'PERIOD_CLOSE_PACKET_STALE_OR_MISSING'); need(stale.packetHash === changed.hash, 'current stale hash');
      await refusal('close', { ...bounds(), 'review-id': first.id, 'packet-hash': first.packet.hash, confirm: 'yes' }); await historical();
      const fresh = review(), closed = close(fresh); periodId = closed.periodId;
      if (originalJournal) await journalRefusal('post', { input: journalEvidence.file });
      else {
      // Posting refusal uses the existing native route and a distinct, never retried key.
      const beforeLocked = await books(), version = (await get(prefix + 'journal')).journal.nativePosting.ruleVersion;
      const locked = await page.request.post(base + prefix + 'manual-journal', { headers: { origin: base }, data: { confirm: true, input: { format: 'rentemester-manual-journal-post/1', expectedRuleVersion: version, idempotencyKey: 'synthetic-original-period-locked', request: { transactionDate: end, text: 'Refused synthetic locked posting', currency: 'DKK', lines: [{ accountNo: '8711', debitAmount: '3.25', creditAmount: '0.00' }, { accountNo: '8712', debitAmount: '0.00', creditAmount: '3.25' }] } } } });
      need(locked.status() === 422 && (await locked.json()).ok === false, 'closed period refuses posting'); await unchanged(beforeLocked, 'locked posting');
      }
      const opened = cli('reopen', { ...bounds(), reason: 'Synthetic correction evidence reviewed' }); need(opened.ok && opened.periodId === periodId && equal(opened.errors, []), 'justified reopen source envelope');
      if (originalJournal) {
        const reversal = journalCli('reverse', { 'entry-id': journalEvidence.posted.entryId, 'entry-no': 'IGNORED-BY-ID-PRIORITY', date: end, reason: 'Synthetic original journal correction' });
        need(reversal.originalEntryId === journalEvidence.posted.entryId, 'entry-id priority original reversal');
        Object.assign(journalEvidence, { reversal, reversed: await rollbackState() });
        await journalRefusal('reverse', { 'entry-no': journalEvidence.posted.entryNo, date: end, reason: 'Duplicate must refuse while open' });
      } else await journal('synthetic-original-period-correction', 'Synthetic original period correction');
      const corrected = review(); need(close(corrected).periodId === periodId, 'reclose keeps original row'); closeHash = corrected.packet.hash;
      const reported = close(corrected, { status: 'reported', reference }); need(reported.periodId === periodId && reported.reference === reference, 'atomic existing report receipt');
      const beforeReplay = await books(); const { readinessPacket: originalPacket, ...replayEnvelope } = reported; need(equal(close(corrected, { status: 'reported', reference }), replayEnvelope), 'exact source reported replay envelope'); await unchanged(beforeReplay, 'reported replay');
      await refusal('close', { ...bounds(), 'review-id': corrected.id, 'packet-hash': closeHash, status: 'reported', reference: 'CONFLICTING-SYNTHETIC-RECEIPT', confirm: 'yes' });
      await refusal('reopen', { ...bounds(), reason: 'Synthetic reported refusal' });
      const beforeForce = await books(); cli('close', { ...bounds(), force: 'yes', reason: 'Synthetic force must refuse', confirm: 'yes' }, 2); await unchanged(beforeForce, 'source force usage refusal');
      // Two disjoint custom periods: optional reference really is absent in the first.
      for (const [from, to, receipt] of [[year + '-04-01', year + '-04-02', ''], [year + '-05-01', year + '-05-02', 'SYNTHETIC-DIRECT-RECEIPT']]) {
        const r = review(from, to), v = close(r, { kind: 'custom', status: 'reported', ...(receipt ? { reference: receipt } : {}) }, from, to);
        need(v.status === 'reported' && (receipt ? v.reference === receipt : !('reference' in v)), 'fresh direct reported optional reference');
      }
      await historical();
      const memberId = (await get('/api/me', member)).user.id;
      need((await member.request.get(base + prefix + 'periods/original/status?reviewId=' + first.id)).ok(), 'current member historical access');
      await post('/api/workspace/members/company', { userId: memberId, action: 'revoke', companySlug: 'synthetic' });
      const beforeRevoked = await books(); need((await member.request.get(base + prefix + 'periods/original/status?reviewId=' + first.id)).status() === 403, 'current revoked status access');
      cli('status', { 'review-id': first.id }, true, memberFile); cli('review', { ...bounds(), 'packet-hash': closeHash, confirm: 'yes' }, true, memberFile); await unchanged(beforeRevoked, 'revoked member');
      stable = await books(); complete = true;
      return { qualified: true, ordinaryMfa: true, originalPeriodQualified: true, ...(originalJournal ? { originalJournalQualified: true, journalEvidence, journalFingerprints: { dryRun: fingerprint(journalEvidence.dryRun), provisional: fingerprint({ entryId: journalEvidence.dryRun.entryId, entryNo: journalEvidence.dryRun.entryNo, previousHash: journalEvidence.dryRun.previousHash, entryHash: journalEvidence.dryRun.entryHash }), post: fingerprint(journalEvidence.post), reversal: fingerprint(journalEvidence.reversed) } } : {}), currentRevocationRefused: true, year, start, end, periodId, closeHash, reference, reviews, closes, refusals, accountingFingerprint: fingerprint(stable), installedNativeQualified: false, sourceFreeRestoreQualified: false, externalFilingQualified: false };
    },
    async reopen() {
      need(complete, 'completed fixture'); await cookie(page, ownerFile); await historical(); await unchanged(stable, 'exact normal MFA restart');
      rmSync(ownerFile); rmSync(memberFile);
      return { qualified: true, ordinaryMfa: true, exactNormalMfaRestartState: true, accountingFingerprint: fingerprint(stable), reviews, installedNativeQualified: false, sourceFreeRestoreQualified: false, externalFilingQualified: false };
    },
  };
}
