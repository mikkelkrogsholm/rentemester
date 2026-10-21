import { createBackupMailInboxProof } from "./check-native-backup-mail-inbox";
import { request, type APIRequestContext, type Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { existsSync, lstatSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";

/** Actual synthetic financial lock→verified backup→one posting, using ordinary MFA. */
export async function createBackupLockProof(page: Page, base: string, work: string) {
  const prefix = "/api/companies/synthetic/", cookieFile = join(work, "backup-lock-owner.cookie"), keyFile = join(work, "backup-lock-service.key");
  const hash = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");
  const need = (value: unknown, label: string) => { if (!value) throw Error(label); };
  const today = new Date().toLocaleDateString("sv-SE", { timeZone: "Europe/Copenhagen" });
  const historical = `${today.slice(0, 4)}-01-02`;
  let key = "", serviceId = "", credentialId = "", service: APIRequestContext | undefined, stable: any, posting: any, accepted: any;
  let started = false, complete = false, stableMetadata: any, acceptedKnowledge: any;
  const knowledgeProposal = { predicate: "operating_status", value: "Synthetic backup lock metadata accepted once", source: { kind: "user", ref: "synthetic-backup-lock-knowledge-112" }, validFrom: today, certainty: "confirmed", confirm: true };
  const corporateOriginal = Buffer.from("Synthetic backup lock corporate source 112\nNo personal or production data.\n");
  const corporateIngest = { recordId: "synthetic-backup-lock-corporate-112", type: "articles", bytesBase64: corporateOriginal.toString("base64"), filename: "synthetic-backup-lock-corporate-112.txt", source: "Synthetic explicitly reviewed corporate source", receivedAt: today, uploader: "Synthetic Owner", sensitivity: "normal", links: [{ type: "company", id: "synthetic" }], confirm: true };
  function sessionFile(fresh: boolean) {
    return page.context().cookies(base).then(cookies => {
      need(cookies.some(cookie => cookie.httpOnly), "Normal MFA HttpOnly session required");
      writeFileSync(cookieFile, cookies.map(cookie => `${cookie.name}=${cookie.value}`).join("; ") + "\n", { mode: 0o600, flag: fresh ? "wx" : "w" });
    });
  }
  async function human(resource: string, body?: unknown, method = "POST") {
    const response = body === undefined ? await page.request.get(base + prefix + resource)
      : await page.request.fetch(base + prefix + resource, { method, headers: { origin: base }, data: body });
    need(response.ok(), `Owner backup lock ${resource} HTTP${response.status()}`);
    return response.json();
  }
  async function admin(resource: string, body: unknown) {
    const response = await page.request.post(base + "/api/workspace/" + resource, { headers: { origin: base }, data: body });
    need(response.ok(), `Normal service administration ${resource} HTTP${response.status()}`);
    return response.json();
  }
  function client(kind: "cli" | "mcp", command: string, body?: unknown, principal: "human" | "service" = "human", success = true, options: { company?: string; sourceId?: string; id?: number } = {}): any {
    const common = ["--server", base, "--allow-loopback-http", principal === "human" ? "--cookie-file" : "--service-key-file", principal === "human" ? cookieFile : keyFile];
    const envelope = ["post_journal", "reverse_journal"].includes(command);
    const confirmed = body !== undefined && !envelope;
    const selectors = command === "company_knowledge" ? { asOf: today, includeProposed: true }
      : command === "corporate_records" ? { cursor: 0, limit: 100 } : {};
    const selectorFlags = command === "company_knowledge" ? ["--as-of", today, "--include-proposed", "true"]
      : command === "corporate_records" ? ["--cursor", "0", "--limit", "100"] : [];
    const input = kind === "cli" ? body === undefined ? "" : JSON.stringify(body) : [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-backup-lock", version: "1" } } },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: command, arguments: { company: options.company ?? "synthetic", ...selectors, ...(options.sourceId === undefined ? {} : { sourceId: options.sourceId }), ...(options.id === undefined ? {} : { id: options.id }), ...(body === undefined ? {} : envelope ? { envelope: JSON.stringify(body) } : { body: JSON.stringify(body), confirm: true }) } } },
    ].map(value => JSON.stringify(value)).join("\n") + "\n";
    const argv = kind === "cli" ? ["remote", command.replaceAll("_", "-"), ...common, "--company", options.company ?? "synthetic", ...selectorFlags, ...(options.sourceId === undefined ? [] : ["--source-id", options.sourceId]), ...(options.id === undefined ? [] : ["--id", String(options.id)]), ...(confirmed ? ["--confirm"] : [])] : common;
    const result = spawnSync(join(work, kind === "cli" ? "rentemester-rust" : "rentemester-mcp"), argv, { input, encoding: "utf8", timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!result.error && (!key || !result.stdout.includes(key) && !result.stderr.includes(key)), `${kind} process/credential safety`);
    if (kind === "cli") {
      need(result.status === (success ? 0 : 1), `CLI ${command} exit${result.status}`);
      const value = JSON.parse(result.stdout);
      if (!success) { need(value.ok === false && value.code === "server_refused", "CLI must reach server refusal, not parser/transport failure"); return { refused: true, code: value.code }; }
      need(value.ok === true, "CLI successful public response"); return value.data;
    }
    need(result.status === 0, "MCP process terminal status");
    const reply = result.stdout.trim().split("\n").map(line => JSON.parse(line)).find(value => value.id === 2);
    need(!reply?.error && reply?.result?.isError === !success, "MCP public operation outcome");
    if (!success) { need(JSON.stringify(reply.result).includes("server_refused"), "MCP reaches server refusal"); return { refused: true }; }
    return reply.result.structuredContent.data;
  }
  const mailInboxProof = createBackupMailInboxProof(page, base, (kind, command, body, options = {}) => client(kind, command, body, "human", options.success ?? true, options));
  async function books() {
    return { journal: (await human("journal")).journal, bank: (await human("bank")).bank,
      trialBalance: (await human("trial-balance")).trialBalance, balance: (await human("balance")).balance };
  }
  async function unchanged(expected: any) { need(isDeepStrictEqual(await books(), expected), "Lock refusal changed canonical financial state"); }
  async function metadata() {
    return { knowledge: await human(`knowledge?asOf=${today}&includeProposed=true`), corporateRecords: await human("corporate-records?cursor=0&limit=100") };
  }
  async function unchangedMetadata(expected: any, financial: any) {
    need(isDeepStrictEqual(await metadata(), expected), "Refused metadata action changed canonical knowledge/history/corporate records");
    await unchanged(financial);
  }
  async function metadataParity(expected: any) {
    for (const kind of ["cli", "mcp"] as const) {
      need(isDeepStrictEqual(client(kind, "company_knowledge"), expected.knowledge), `${kind} canonical knowledge context/history differs`);
      need(isDeepStrictEqual(client(kind, "corporate_records"), expected.corporateRecords), `${kind} canonical corporate list differs`);
    }
  }
  async function refusedMetadata(expected: any, financial: any) {
    const outcomes: Record<string, unknown> = {};
    for (const [resource, command, input] of [
      ["knowledge/propose", "propose_company_knowledge", knowledgeProposal],
      ["corporate-records", "ingest_corporate_record", corporateIngest],
    ] as const) {
      const response = await page.request.post(base + prefix + resource, { headers: { origin: base }, data: input });
      const body = await response.json();
      need(!response.ok() && body.ok === false && body.code === "backup_overdue", `Owner ${resource} must reach current backup lock`);
      await unchangedMetadata(expected, financial);
      const clients: Record<string, unknown> = {};
      for (const kind of ["cli", "mcp"] as const) {
        clients[kind] = client(kind, command, input, "human", false);
        await unchangedMetadata(expected, financial);
      }
      outcomes[command] = { http: { status: response.status(), code: body.code }, clients };
    }
    await metadataParity(expected);
    return outcomes;
  }
  function normalizedStatus(value: any) {
    const copy = structuredClone(value);
    if (!copy.backup.latestBackupId && copy.backup.backupDue) {
      need(copy.backup.requiredBy === copy.backup.checkedAt, "Never-backed freshness is due at each observation time");
      delete copy.backup.requiredBy;
    }
    delete copy.backup.checkedAt; return copy;
  }
  async function status(locked: boolean) {
    const value = await human("backup-status");
    need(value.ok === true && value.backup?.available === true && value.backup.scope === "local-retained-snapshots" && value.lock?.locked === locked, "Current qualified backup/lock status");
    for (const kind of ["cli", "mcp"] as const) need(isDeepStrictEqual(normalizedStatus(client(kind, "backup_status")), normalizedStatus(value)), `${kind} backup status canonical parity`);
    return value;
  }
  async function refused(principal: "human" | "service", expected: any, clients = true) {
    const response = await (principal === "human" ? page.request : service!).post(base + prefix + "manual-journal", { headers: { origin: base }, data: posting });
    const body = await response.json();
    need(!response.ok() && body.ok === false && body.code === "backup_overdue", `Current ${principal} financial mutation must reach backup_overdue`);
    await unchanged(expected);
    const results: any = { http: { status: response.status(), code: body.code } };
    if (clients) for (const kind of ["cli", "mcp"] as const) { results[kind] = client(kind, "post_journal", posting, principal, false); await unchanged(expected); }
    return results;
  }
  async function run() {
    need(!started, "Backup lock proof executes once"); started = true;
    need(Date.parse(today) - Date.parse(historical) >= 10 * 86400000, "Current-year historical activity needs at least ten days; never manipulate the server clock");
    const fresh = await books(); need(fresh.journal.entries.length === 0 && fresh.bank.transactions.length === 0, "Fresh synthetic financial workspace required");
    for (const row of [
      { accountNo: "1711", name: "Synthetic lock bank", type: "asset", normalBalance: "debit" },
      { accountNo: "8711", name: "Synthetic lock equity", type: "equity", normalBalance: "credit" },
    ]) await human("accounts", row);
    const bankAccount = (await human("bank-accounts", { name: "Synthetic lock bank", slug: "synthetic-backup-lock", currency: "DKK", ledgerAccountNo: "1711" })).bankAccount;
    const bytes = Buffer.from(`date,text,amount,currency,reference\n${historical},Synthetic historical lock activity,7.25,DKK,SYNTHETIC-BACKUP-LOCK-111\n`);
    const imported = (await human("bank/import", { csvBase64: bytes.toString("base64"), account: "synthetic-backup-lock", confirm: true })).import;
    need(imported.imported === 1 && imported.sourceFileHash === hash(bytes) && imported.bankAccountId === bankAccount.id, "One exact historical bank row establishes activity without SQL/time seeds");
    const baseline = await books(); need(baseline.journal.entries.length === 0 && baseline.bank.transactions.length === 1, "Historical import must not post");
    const metadataBaseline = await metadata();
    need(metadataBaseline.knowledge.history.length === 0 && metadataBaseline.corporateRecords.rows.length === 0, "Fresh synthetic metadata inventory required");
    posting = { confirm: true, input: { format: "rentemester-manual-journal-post/1", expectedRuleVersion: baseline.journal.nativePosting.ruleVersion, idempotencyKey: "synthetic-backup-lock-one-post-111", request: {
      transactionDate: today, text: "Synthetic backup lock accepted once", currency: "DKK", lines: [
        { accountNo: "1711", debitAmount: "7.25", creditAmount: "0.00" }, { accountNo: "8711", debitAmount: "0.00", creditAmount: "7.25" },
      ],
    } } };
    const issued = await admin("service-principals", { displayName: "Synthetic backup lock bookkeeper", operationId: randomUUID(), confirm: true });
    need(typeof issued.secret === "string" && issued.secret.length > 0, "Owner-issued service credential");
    key = issued.secret; serviceId = issued.serviceAccountId; credentialId = issued.credentialId;
    writeFileSync(keyFile, key + "\n", { mode: 0o600, flag: "wx" });
    await admin("members/access", { userId: serviceId, action: "set-role", workspaceRole: "member" });
    await admin("members/company", { userId: serviceId, action: "grant", companySlug: "synthetic", role: "bookkeeper" });
    service = await request.newContext({ extraHTTPHeaders: { origin: base, "x-rentemester-service-key": key } });
    need((await service.get(base + prefix + "journal")).ok(), "Current service bookkeeper can read accounting");
    await sessionFile(true);
    const inboxPrepared = await mailInboxProof.beforeALock(historical);
    const before = await status(false); need(before.backup.backupDue === true && !before.lock.enforced, "Never-backed old activity is due before explicit policy change");
    await page.goto(base + "/companies/synthetic/integritet");
    await page.getByRole("button", { name: "Hent backupstatus", exact: true }).click();
    await page.getByRole("checkbox", { name: "Lås bogføring, når en backup er forfalden ud over fristen", exact: true }).check();
    await page.getByRole("spinbutton", { name: "Ekstra frist i dage", exact: true }).fill("0");
    await page.getByRole("checkbox", { name: "Jeg har gennemgået ændringen af bogføringslåsen.", exact: true }).check();
    const changed = page.waitForResponse(response => response.url().endsWith(prefix + "backup-lock") && response.request().method() === "POST");
    await page.getByRole("button", { name: "Gem bogføringslås", exact: true }).click();
    need((await changed).ok(), "Explicit web policy change succeeds");
    const locked = await status(true); need(locked.lock.enforced && locked.lock.graceDays === 0, "Persisted explicit lock policy");
    for (const kind of ["cli", "mcp"] as const) {
      const configured = client(kind, "backup_lock", { confirm: true, enforced: true, graceDays: 0 });
      need(configured.lock.enforced && configured.lock.locked && configured.lock.graceDays === 0, `${kind} reviewed policy write remains available while locked`);
      await unchanged(baseline);
    }
    const refusals = { human: await refused("human", baseline), service: await refused("service", baseline) };
    const metadataRefusals = await refusedMetadata(metadataBaseline, baseline);
    const inboxLocked = await mailInboxProof.locked();
    const serviceLockedRead = await service.get(base + prefix + "journal");
    need(serviceLockedRead.ok() && isDeepStrictEqual((await serviceLockedRead.json()).journal, baseline.journal), "Locked service accounting reads remain available");
    for (const kind of ["cli", "mcp"] as const) need(isDeepStrictEqual(client(kind, "journal", undefined, "service").journal, baseline.journal), "Locked service client read parity");
    // Read routes and backup remain usable while ordinary financial writes are locked.
    const uiDownload = page.waitForEvent("download");
    await page.getByRole("checkbox", { name: "Jeg vil hente en backup af hele workspacet.", exact: true }).check();
    await page.getByRole("button", { name: "Dan og download backup", exact: true }).click();
    const download = await uiDownload, artifactPath = join(work, "backup-lock-ui.native-backup.json");
    need(!existsSync(artifactPath), "Fresh private download path required"); await download.saveAs(artifactPath);
    need(await download.failure() === null, "UI artifact download finished");
    const artifact = readFileSync(artifactPath), archiveSha256 = hash(artifact);
    const verified = spawnSync(join(work, "rentemester-server"), ["--verify-backup", artifactPath, "--trusted-backup-key", join(work, "trusted.pub")], { encoding: "utf8", timeout: 60000 });
    need(verified.status === 0, "Independent current public key verifies actual backup signature/inventory/SQLite");
    const unlocked = await status(false); need(unlocked.lock.enforced && unlocked.backup.ok && !unlocked.backup.backupDue, "Qualified artifact unlocks current enforced policy");
    await unchanged(baseline);
    const retained = join(work, "workspace/native-backups", archiveSha256 + ".native-backup.json"), parked = retained + ".acceptance-held";
    const info = lstatSync(retained); need(info.isFile() && !info.isSymbolicLink() && info.uid === process.getuid!() && info.nlink === 1 && (info.mode & 0o777) === 0o600 && readFileSync(retained).equals(artifact) && !existsSync(parked), "Exact private retained artifact must match downloaded bytes");
    let missingStatus: any, tamperedStatus: any;
    try {
      renameSync(retained, parked); missingStatus = await status(true); await refused("human", baseline, false);
    } finally { if (existsSync(parked)) renameSync(parked, retained); }
    await status(false);
    const tampered = Buffer.from(artifact); tampered[0] ^= 1;
    try {
      writeFileSync(retained, tampered, { flag: "r+" }); tamperedStatus = await status(true); await refused("service", baseline, false);
    } finally { writeFileSync(retained, artifact, { flag: "r+" }); }
    need(hash(readFileSync(retained)) === archiveSha256, "Synthetic integrity probe restores exact original bytes");
    await status(false); await unchanged(baseline);
    const inboxUnlocked = await mailInboxProof.unlocked();
    acceptedKnowledge = await human("knowledge/propose", knowledgeProposal);
    need(acceptedKnowledge.ok === true && typeof acceptedKnowledge.assertion?.assertionId === "string" && acceptedKnowledge.assertion.reviewState === "proposed", "Previously refused valid metadata proposal succeeds after qualified backup");
    stableMetadata = await metadata();
    need(stableMetadata.knowledge.history.length === 1
      && stableMetadata.knowledge.history[0].assertionId === acceptedKnowledge.assertion.assertionId
      && stableMetadata.knowledge.history[0].value === knowledgeProposal.value
      && stableMetadata.knowledge.history[0].reviewState === "proposed"
      && isDeepStrictEqual(stableMetadata.corporateRecords, metadataBaseline.corporateRecords), "One exact accepted knowledge proposal and zero refused corporate records");
    await unchanged(baseline); await metadataParity(stableMetadata);
    accepted = await human("manual-journal", posting);
    need(accepted.entryId > 0 && typeof accepted.entryHash === "string" && accepted.receipt?.replayed === false, "One exact current-key manual journal receipt");
    stable = await books();
    need(stable.journal.entries.length === 1 && stable.bank.transactions.length === 1 && stable.trialBalance.balanced, "One posting after unlock; no refused posting effects");
    const entry = stable.journal.entries[0];
    need(entry.id === accepted.entryId && entry.text === posting.input.request.text && entry.lines.length === 2
      && entry.lines.some((line: any) => line.accountNo === "1711" && line.debit === 7.25 && line.credit === 0)
      && entry.lines.some((line: any) => line.accountNo === "8711" && line.credit === 7.25 && line.debit === 0), "Exact balanced accepted journal readback");
    for (const kind of ["cli", "mcp"] as const) { const replay = client(kind, "post_journal", posting); need(replay.entryId === accepted.entryId && replay.entryHash === accepted.entryHash && replay.receipt?.replayed, `${kind} same receipt without duplicate`); await unchanged(stable); }
    complete = true;
    await page.getByRole("button", { name: "Hent backupstatus", exact: true }).click();
    await page.screenshot({ path: process.env.RM_SCANNER_SCREENSHOT!, fullPage: true });
    return { serviceAccountId: serviceId, credentialId, historicalDate: historical, bankSourceSha256: hash(bytes), before, locked, refusals, unlocked,
      artifact: { sha256: archiveSha256, bytes: artifact.length, independentlyVerified: true, uiDownloadComplete: true },
      missingCurrentFileLocked: missingStatus.lock.locked, tamperedCurrentFileLocked: tamperedStatus.lock.locked, lockedServiceReadsQualified: true,
      exactArtifactRestored: true, accepted, canonical: stable, financialCoreAndServiceQualified: true, cliMcpPolicyWritesQualified: true,
      directControlMetadataGateQualified: true, directControlMetadataScope: ["company-knowledge", "corporate-record-ingest"],
      metadataRefusals, inboxPrepared, inboxLocked, inboxUnlocked, twoCompanyInboxTargetGateQualified: true, humanMailAliasGateQualified: true,
      acceptedKnowledge, canonicalMetadata: stableMetadata, multiCompanyMetadataGateQualified: false,
      partiesGateQualified: false, mailIntakeConfigurationGateQualified: false, smtpDispatchGateQualified: false, keyRotationQualified: false, installedNativeQualified: false, offsiteQualified: false };
  }
  async function reopen() {
    need(complete && stable, "Completed whole backup lock flow required before restart proof");
    await sessionFile(false); const current = await status(false);
    need(current.lock.enforced && current.lock.graceDays === 0, "Policy and verified file persist across restart");
    await unchanged(stable);
    need((await service!.get(base + prefix + "journal")).ok(), "Ordinary service credential remains current after restart");
    for (const kind of ["cli", "mcp"] as const) {
      const value = client(kind, "post_journal", posting); need(value.receipt?.replayed && value.entryId === accepted.entryId && value.entryHash === accepted.entryHash, "Restart retains exact accepted receipt");
      need(isDeepStrictEqual(client(kind, "journal").journal, stable.journal), "Restart accounting client parity");
    }
    await unchanged(stable); await unchangedMetadata(stableMetadata, stable); await metadataParity(stableMetadata);
    need(stableMetadata.knowledge.history[0].assertionId === acceptedKnowledge.assertion.assertionId, "Accepted metadata identity retained across normal MFA restart");
    const inboxReopen = await mailInboxProof.reopen();
    await service!.dispose();
    return { inboxReopen, twoCompanyInboxTargetGateQualified: true, humanMailAliasGateQualified: true, normalMfaRestartQualified: true, exactAcceptedReceiptSurvives: true, currentQualifiedArtifactUnlocks: true, metadataRestartQualified: true, canonicalMetadata: stableMetadata, canonical: stable };
  }
  return { run, reopen };
}
