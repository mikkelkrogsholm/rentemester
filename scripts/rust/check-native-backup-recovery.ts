import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";

/** Read-only evidence across a source-free restore orchestrated by the runner. */
export function createBackupRecoveryProof(base: string, work: string) {
  const companies = ["synthetic", "synthetic-backup-inbox-target"];
  const resources = [
    { path: "journal", command: "journal" },
    { path: "bank", command: "bank" },
    { path: "trial-balance", command: "trial_balance" },
    { path: "balance", command: "balance" },
    { path: "documents", command: "documents" },
  ];
  const registrationPath = "/api/companies/synthetic/backup-registrations";
  const need = (value: unknown, message: string) => { if (!value) throw Error(message); };
  function ordered(value: any): any {
    if (Array.isArray(value)) return value.map(ordered);
    if (value && typeof value === "object") return Object.fromEntries(Object.keys(value).sort().map(key => [key, ordered(value[key])]));
    return value;
  }
  const fingerprint = (value: unknown) => createHash("sha256").update(JSON.stringify(ordered(value))).digest("hex");
  const get = async (page: Page, path: string) => { const response = await page.request.get(base + path); need(response.ok(), "Recovery authorized read refused"); return response.json(); };
  let sourceOwner = "", baseline: any, registrationHistory: any[], operations: any[], originals: any[], remote: any;
  let captured = false, restoredOnce = false;
  let cookieFile = "", credentialPhase = 0;
  async function credentials(page: Page, expectedId: string) {
    const me = await get(page, "/api/me");
    need(me.user?.id === expectedId && me.user?.twoFactorEnabled === true, "Recovery requires the expected current normal MFA human");
    const cookies = await page.context().cookies(base); need(cookies.some(cookie => cookie.httpOnly), "Recovery must use a normal HttpOnly session");
    cookieFile = join(work, `backup-recovery-owner-${++credentialPhase}.cookie`);
    writeFileSync(cookieFile, cookies.map(cookie => `${cookie.name}=${cookie.value}`).join("; ") + "\n", { mode: 0o600, flag: "wx" });
  }
  function client(kind: "cli" | "mcp", command: string, company?: string, selectors: { operationId?: string; destinationId?: string; id?: number } = {}) {
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookieFile];
    const flags = [
      ...(company ? ["--company", company] : []),
      ...(selectors.operationId ? ["--operation", selectors.operationId] : []),
      ...(selectors.destinationId ? ["--destination", selectors.destinationId] : []),
      ...(selectors.id === undefined ? [] : ["--id", String(selectors.id)]),
    ];
    const cliCommand = command === "system_backup_destination_list" ? "backup-registrations" : command === "system_backup_governance" ? "backup-governance" : command.replaceAll("_", "-");
    const args = kind === "cli" ? ["remote", cliCommand, ...common, ...flags] : common;
    const input = kind === "cli" ? "" : [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-recovery-proof", version: "1" } } },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: command, arguments: { ...(company ? { company } : {}), ...selectors } } },
    ].map(value => JSON.stringify(value)).join("\n") + "\n";
    const result = spawnSync(join(work, kind === "cli" ? "rentemester-rust" : "rentemester-mcp"), args, { input, encoding: "utf8", timeout: 45000, maxBuffer: 16 * 1024 * 1024 });
    need(!result.error && result.status === 0, "Read-only recovery native client refused");
    if (kind === "cli") { const value = JSON.parse(result.stdout); need(value.ok === true, "Recovery CLI envelope refused"); return value.data; }
    const value = result.stdout.trim().split("\n").map(line => JSON.parse(line)).find(reply => reply.id === 2);
    need(value?.result?.isError === false && value.result.structuredContent?.ok === true, "Recovery MCP envelope refused");
    return value.result.structuredContent.data;
  }
  async function business(page: Page, clients: boolean) {
    const snapshot: Record<string, Record<string, unknown>> = {};
    for (const company of companies) {
      snapshot[company] = {};
      for (const resource of resources) {
        const value = await get(page, `/api/companies/${company}/${resource.path}`); snapshot[company][resource.path] = value;
        if (clients) for (const kind of ["cli", "mcp"] as const) need(isDeepStrictEqual(client(kind, resource.command, company), value), "Restored business HTTP/CLI/MCP read parity differs");
      }
    }
    return snapshot;
  }
  async function originalParity(page: Page) {
    for (const original of originals) {
      const response = await page.request.get(`${base}/api/companies/synthetic-backup-inbox-target/documents/${original.id}/file`);
      need(response.ok() && fingerprint([...await response.body()]) === original.bytesFingerprint, "Restored original HTTP bytes differ");
      for (const kind of ["cli", "mcp"] as const) {
        const value = client(kind, "document_original", "synthetic-backup-inbox-target", { id: original.id }).original;
        need(value.contentType === original.contentType && fingerprint(value.bytes) === original.bytesFingerprint, "Restored original native bytes/content type differ");
      }
    }
  }
  async function verify(page: Page, newOwnerId: string) {
    need(captured && newOwnerId !== sourceOwner, "A distinct new normal MFA owner must admit source-free recovery");
    await credentials(page, newOwnerId);
    const current = await business(page, true); need(isDeepStrictEqual(current, baseline), "Recovered supported business reads differ from captured source");
    await originalParity(page);
    const registrations = await get(page, registrationPath);
    need(registrations.destinations.length === 0 && isDeepStrictEqual(registrations.history, registrationHistory), "Recovered registration history/tombstone differs");
    need(registrations.backups.length === 0 && registrations.operatorTargets.length === 0, "Fresh restored target must not import retained backup files/operator authority");
    for (const kind of ["cli", "mcp"] as const) need(isDeepStrictEqual(client(kind, "system_backup_destination_list", "synthetic"), registrations), "Recovered complete registration list native parity differs");
    for (let index = 0; index < operations.length; index++) {
      const expected = operations[index], operationId = registrationHistory[index].operationId;
      need(isDeepStrictEqual(await get(page, `${registrationPath}/operations/${operationId}`), expected), "Recovered original HTTP operation envelope differs");
      for (const kind of ["cli", "mcp"] as const) need(isDeepStrictEqual(client(kind, "backup_registration_operation", "synthetic", { operationId }), expected), "Recovered original operation native envelope differs");
    }
    for (const operation of registrations.history.filter((operation: any) => operation.action === "declare")) {
      need(operation.result.verified === false && operation.result.verifyMethod === "declared" && operation.result.remoteSyncVerified === false
        && operation.result.attestationIndependentlyVerified === false, "Imported declaration must remain unverified inert history");
    }
    for (const company of companies) {
      const value = await get(page, `/api/companies/${company}/backup-governance`), governance = value.governance;
      need(value.backup.latestBackupId === null && governance.latestBackupId === null && governance.latestBackupPlacementCount === 0
        && governance.latestBackupRemoteVerificationCount === 0 && governance.latestBackupExternalCopyObserved === false
        && governance.latestBackupExternalCopyAtDeclaredDestination === false && governance.attestationIndependentlyVerified === false
        && governance.recoveryVerified === false && governance.destinationCount === 0 && governance.destinations.length === 0, "Restored current governance borrowed historical source authority");
      // checkedAt and derived clock fields can vary; verify the qualification
      // boundary through each native client, not a fabricated shared timestamp.
      for (const kind of ["cli", "mcp"] as const) {
        const native = client(kind, "system_backup_governance", company);
        need(native.backup.latestBackupId === null && native.governance.latestBackupId === null
          && native.governance.latestBackupPlacementCount === 0 && native.governance.latestBackupRemoteVerificationCount === 0
          && native.governance.latestBackupExternalCopyObserved === false && native.governance.latestBackupExternalCopyAtDeclaredDestination === false
          && native.governance.recoveryVerified === false && native.governance.attestationIndependentlyVerified === false, "Native restored governance borrowed source evidence");
      }
    }
    const remotePath = `/api/workspace/backup-destinations/${remote.destinationId}/remote-verifications/${remote.operationId}`;
    const missing = await get(page, remotePath); need(missing.verification === null, "Source remote receipt crossed restored-instance boundary");
    for (const kind of ["cli", "mcp"] as const) need(isDeepStrictEqual(client(kind, "backup_remote_verification", undefined, { destinationId: remote.destinationId, operationId: remote.operationId }), missing), "Restored remote null boundary native parity differs");
    return { distinctNormalMfaOwner: true, supportedBusinessReadsExact: true, businessFingerprint: fingerprint(baseline), registrationHistoryFingerprint: fingerprint(registrationHistory), originalBytesFingerprints: originals.map(original => ({ documentId: original.id, sha256: original.sha256 })), exactThreeOriginalRegistrationReceipts: true, currentRetainedBackupInventoryEmpty: true, operatorTargetsEmpty: true, declarationsRemainUnverified: true, governanceDoesNotBorrowSourceEvidence: true, sourceRemoteReceiptInactive: true, httpCliMcpReadParity: true };
  }
  return {
    async capture(page: Page, sourceOwnerId: string, remoteBackup: any) {
      need(!captured, "Capture recovery source once"); sourceOwner = sourceOwnerId; await credentials(page, sourceOwnerId);
      need(remoteBackup?.registrations?.qualified === true && remoteBackup.receipt?.remoteSyncVerified === true, "Accepted source registration/remote proof required");
      registrationHistory = [remoteBackup.registrations.added, remoteBackup.registrations.declared, remoteBackup.registrations.removed];
      need(registrationHistory.every(operation => operation?.format === "rentemester-native-backup-registration-operation/1")
        && registrationHistory.map(operation => operation.action).join(",") === "add,declare,remove", "Exact source add/declare/remove receipts required");
      const registrations = await get(page, registrationPath);
      need(registrations.destinations.length === 0 && isDeepStrictEqual(registrations.history, registrationHistory), "Source registration capture differs from admitted proof");
      operations = [];
      for (const receipt of registrationHistory) { const envelope = await get(page, `${registrationPath}/operations/${receipt.operationId}`); need(isDeepStrictEqual(envelope.operation, receipt), "Source operation envelope differs"); operations.push(envelope); }
      remote = { destinationId: remoteBackup.receipt.destinationId, operationId: remoteBackup.receipt.operationId };
      const sourceRemote = await get(page, `/api/workspace/backup-destinations/${remote.destinationId}/remote-verifications/${remote.operationId}`);
      need(isDeepStrictEqual(sourceRemote.verification, remoteBackup.receipt), "Source remote capture differs from accepted exact receipt");
      baseline = await business(page, true); originals = [];
      const documents: any = baseline["synthetic-backup-inbox-target"].documents.documents;
      need(Array.isArray(documents.documents) && documents.documents.length > 0, "Capture actual target supporting document inventory");
      for (const document of documents.documents) {
        need(Number.isSafeInteger(document.id) && document.id > 0, "Actual target document identity required");
        const response = await page.request.get(`${base}/api/companies/synthetic-backup-inbox-target/documents/${document.id}/file`); need(response.ok(), "Source original bytes refused");
        const bytes = await response.body(); originals.push({ id: document.id, contentType: response.headers()["content-type"], bytesFingerprint: fingerprint([...bytes]), sha256: createHash("sha256").update(bytes).digest("hex") });
      }
      await originalParity(page); captured = true;
      return { sourceCaptured: true, supportedBusinessFingerprint: fingerprint(baseline), registrationHistoryFingerprint: fingerprint(registrationHistory), operationEnvelopesFingerprint: fingerprint(operations), originalSha256s: originals.map(original => original.sha256), readOnlySourceCapture: true };
    },
    async restored(page: Page, newOwnerId: string) { need(!restoredOnce, "Initial restored proof runs once"); const evidence = await verify(page, newOwnerId); restoredOnce = true; return { ...evidence, sourceFreeRestoredReadQualified: true }; },
    async reopen(page: Page, newOwnerId: string) { need(restoredOnce, "Initial restored proof required before reopen"); return { ...await verify(page, newOwnerId), restoredNormalMfaRestartQualified: true }; },
  };
}
