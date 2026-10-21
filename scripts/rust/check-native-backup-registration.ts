import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";

/** Inert SSH destination metadata/declarations; never opens SSH or a user path. */
export function createBackupRegistrationProof(page: Page, base: string, work: string, providerCount: () => number, nonAdminPage?: Page) {
  const endpoint = "/api/companies/synthetic/backup-registrations";
  const label = "Synthetic SSH declaration only";
  const cookieFile = join(work, "backup-registration-owner.cookie");
  const need = (value: unknown, message: string) => { if (!value) throw Error(message); };
  const sha = (path: string) => createHash("sha256").update(readFileSync(path)).digest("hex");
  const get = async (path: string) => { const response = await page.request.get(base + path); need(response.ok(), "Registration authorized read refused"); return response.json(); };
  const list = () => get(endpoint);
  const inspect = (operation: string) => get(`${endpoint}/operations/${operation}`);
  const cookies = async () => writeFileSync(cookieFile, (await page.context().cookies(base)).map(cookie => `${cookie.name}=${cookie.value}`).join("; ") + "\n", { mode: 0o600 });
  let final: any, added: any, declared: any, removed: any, retainedPath = "", retainedSha = "", landedPath = "", landedSha = "", browserPosts = 0;
  page.on("request", request => { if (request.url() === base + endpoint && request.method() === "POST") browserPosts++; });
  function client(kind: "cli" | "mcp", action: "list" | "inspect" | "declare" | "remove", body?: any, success = true) {
    const cli = { list: "backup-registrations", inspect: "backup-registration-operation", declare: "backup-confirm-placement", remove: "backup-remove-destination" }[action];
    const mcp = { list: "system_backup_destination_list", inspect: "backup_registration_operation", declare: "system_backup_confirm_placement", remove: "system_backup_destination_remove" }[action];
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookieFile];
    const mutation = ["declare", "remove"].includes(action);
    const args = kind === "cli" ? ["remote", cli, ...common, "--company", "synthetic", ...(mutation ? ["--confirm"] : action === "inspect" ? ["--operation", body.operationId] : [])] : common;
    const input = kind === "cli" ? mutation ? JSON.stringify(body) : "" : [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-registration-proof", version: "1" } } },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: mcp, arguments: { company: "synthetic", ...(mutation ? { body: JSON.stringify(body), confirm: true } : action === "inspect" ? { operationId: body.operationId } : {}) } } },
    ].map(value => JSON.stringify(value)).join("\n") + "\n";
    const result = spawnSync(join(work, kind === "cli" ? "rentemester-rust" : "rentemester-mcp"), args, { input, encoding: "utf8", timeout: 45000, maxBuffer: 8 * 1024 * 1024 });
    need(!result.error, "Registration native client transport failed");
    if (kind === "cli") {
      need(result.status === (success ? 0 : 1), "Registration CLI public status differs");
      const reply = JSON.parse(result.stdout); need(reply.ok === success, "Registration CLI envelope differs");
      if (!success) { need(reply.code === "server_refused", "Conflict must reach server, not parser/transport"); return null; } return reply.data;
    }
    need(result.status === 0, "Registration MCP process failed");
    const reply = result.stdout.trim().split("\n").map(line => JSON.parse(line)).find(value => value.id === 2);
    need(reply?.result?.isError === !success, "Registration MCP public status differs"); return success ? reply.result.structuredContent.data : null;
  }
  async function parity(operation?: any) {
    const current = await list();
    for (const kind of ["cli", "mcp"] as const) {
      need(isDeepStrictEqual(client(kind, "list"), current), "Registration full list/history native parity differs");
      if (operation) need(isDeepStrictEqual(client(kind, "inspect", { operationId: operation.operationId }), await inspect(operation.operationId)), "Registration exact operation native parity differs");
    }
    return current;
  }
  async function governanceUnverified() {
    const value = await get("/api/companies/synthetic/backup-governance");
    need(value.governance.latestBackupId === retainedSha && value.governance.latestBackupExternalCopyObserved === false
      && value.governance.latestBackupExternalCopyAtDeclaredDestination === false
      && value.governance.latestBackupPlacementCount === 0 && value.governance.latestBackupRemoteVerificationCount === 0
      && value.governance.attestationIndependentlyVerified === false && value.governance.recoveryVerified === false, "Manual transfer declaration must not become verified evidence");
  }
  function filesUnchanged() {
    need(sha(retainedPath) === retainedSha && sha(landedPath) === landedSha, "Metadata registration/removal changed backup files");
  }
  return {
    async run(backupId: string, placementOperationId: string) {
      const count = providerCount(); await cookies();
      retainedSha = backupId; retainedPath = join(work, "workspace/native-backups", `${backupId}.native-backup.json`);
      landedPath = join(work, "backup-destination", `${placementOperationId}.native-backup.json`); landedSha = sha(landedPath); filesUnchanged();
      const initial = await list(); need(initial.destinations.length === 0 && initial.history.length === 0, "Fresh synthetic registration history required");
      const selectedBackup = initial.backups.find((backup: any) => backup.backupId === backupId);
      need(selectedBackup?.archiveSha256 === backupId, "Current independently qualified archive must be selectable");
      await page.goto(base + "/companies/synthetic/integritet");
      const card = page.getByRole("region", { name: "Registrerede backupdestinationer", exact: true });
      await card.getByRole("button", { name: "Hent registrerede destinationer", exact: true }).click();
      await card.getByLabel("Destinationsnavn", { exact: true }).fill(label);
      await card.getByLabel("Destinationstype", { exact: true }).selectOption("ssh");
      await card.getByLabel("Placering eller beskrivelse", { exact: true }).fill("Synthetic SSH host description; no connection or upload requested");
      await card.getByLabel("Attesteret af", { exact: true }).fill("Synthetic MFA Owner");
      await card.getByRole("checkbox", { name: "Jeg har gennemgået registreringen.", exact: true }).check();
      let posts = 0;
      const lost = async (route: any) => { posts++; const response = await route.fetch(); need(response.ok(), "Server did not accept web registration before lost reply"); added = (await response.json()).operation; await route.abort("failed"); };
      await page.route(base + endpoint, lost);
      await card.getByRole("button", { name: "Registrér destination", exact: true }).click();
      await card.getByRole("alert").waitFor(); await page.unroute(base + endpoint, lost);
      need(posts === 1 && added?.action === "add" && added.result.kind === "ssh", "One inert SSH metadata operation must be committed");
      // UI generates both ids; use its accepted immutable identity in later writes.
      const actualDestination = added.destinationId;
      need(typeof actualDestination === "string" && /^[a-z0-9-]{8,64}$/.test(actualDestination), "Accepted destination identity required");
      need(added.result.inEeaOrEu === false && added.result.nonRelatedParty === false, "Unchecked synthetic declarations must remain explicit false");
      const pending = await page.evaluate(key => localStorage.getItem(key), `rentemester:backup-registrations:${initial.workspaceId}:synthetic`);
      need(typeof pending === "string" && pending.includes(added.operationId), "Lost accepted operation id must persist before remount");
      await page.reload();
      const remounted = page.getByRole("region", { name: "Registrerede backupdestinationer", exact: true });
      await remounted.getByRole("button", { name: "Hent registrerede destinationer", exact: true }).click();
      const readback = page.waitForResponse(response => response.url() === base + `${endpoint}/operations/${added.operationId}` && response.request().method() === "GET");
      await remounted.getByRole("button", { name: "Hent registreringens resultat", exact: true }).click();
      const reconciled = await readback; need(reconciled.ok() && isDeepStrictEqual((await reconciled.json()).operation, added), "Reload must reconcile the same accepted operation");
      await remounted.getByRole("button", { name: "Hent registrerede destinationer", exact: true }).click();
      await remounted.getByText(label, { exact: true }).first().waitFor();
      need(browserPosts === 1, "Lost reply/remount must not automatically POST again");
      const current = await parity(added); need(current.destinations.length === 1 && current.destinations[0].destinationId === actualDestination, "SSH registration not visible in current state");
      const declaration = { operationId: randomUUID(), destinationId: actualDestination, backupId, note: "Synthetic owner declares transfer; no SSH request or external evidence", confirm: true };
      declared = client("cli", "declare", declaration).operation;
      need(declared.action === "declare" && declared.result.backupId === backupId && declared.result.archiveSha256 === backupId
        && declared.result.archiveSizeBytes === selectedBackup.archiveSizeBytes && declared.result.backupCreatedAt === selectedBackup.backupCreatedAt
        && declared.result.verified === false && declared.result.remoteSyncVerified === false && declared.result.attestationIndependentlyVerified === false
        && declared.result.verifyMethod === "declared", "Declared transfer acquired invented verification claims");
      need(isDeepStrictEqual(client("mcp", "declare", declaration).operation, declared), "Exact declaration replay must retain original actor/time/result");
      const afterDeclare = await parity(declared); need(afterDeclare.history.length === initial.history.length + 2, "Exact replay duplicated declaration audit history");
      client("cli", "declare", { ...declaration, note: "Conflicting same operation" }, false);
      need(isDeepStrictEqual(await list(), afterDeclare), "Conflicting declaration modified history");
      await governanceUnverified();
      need(nonAdminPage, "Existing normal MFA nonadmin context required");
      const deniedRead = await nonAdminPage!.request.get(base + endpoint);
      const deniedMutation = await nonAdminPage!.request.post(base + endpoint, { headers: { origin: base }, data: { action: "declare", ...declaration, operationId: randomUUID() } });
      need(deniedRead.status() === 403 && deniedMutation.status() === 403, "Current nonadmin registration read/write admitted");
      need(isDeepStrictEqual(await list(), afterDeclare), "Refused nonadmin changed registration metadata");
      const removal = { operationId: randomUUID(), destinationId: actualDestination, confirm: true };
      removed = client("mcp", "remove", removal).operation;
      need(removed.action === "remove" && removed.result.active === false, "Removal must retain a tombstone");
      need(isDeepStrictEqual(client("cli", "remove", removal).operation, removed), "Exact removal replay differs");
      final = await parity(removed);
      need(isDeepStrictEqual(final.operatorTargets, initial.operatorTargets) && isDeepStrictEqual(final.backups, initial.backups), "Inert registration/removal changed operator targets or qualified backup inventory");
      need(final.destinations.length === 0 && final.history.length === initial.history.length + 3
        && [added, declared, removed].every(operation => final.history.some((entry: any) => isDeepStrictEqual(entry, operation))), "Removal must preserve exact registration/declaration/tombstone history");
      await governanceUnverified(); filesUnchanged(); need(providerCount() === count, "Registration/declaration contacted Drive");
      return { qualified: true, added, declared, removed, lostReplyManualSameOperationReconcile: true, noAutomaticPost: true, currentNonadminRefused: true, exactNativeListAndOperationParity: true, removePreservesDeclarationHistory: true, declarationIsNotVerifiedEvidence: true, physicalBackupFilesUnchanged: true, noExtraDriveGets: true, actualSshQualified: false, actualExternalTransferQualified: false };
    },
    async reopen() {
      const count = providerCount(); await cookies();
      need(isDeepStrictEqual(await parity(removed), final), "Registration list/history changed after normal MFA restart");
      for (const operation of [added, declared, removed]) need(isDeepStrictEqual((await inspect(operation.operationId)).operation, operation), "Original registration operation receipt changed after restart");
      await governanceUnverified(); filesUnchanged(); need(providerCount() === count, "Registration restart contacted Drive");
      return { sameHistoryAfterNormalMfaRestart: true, exactOriginalOperationReceipts: true, declarationIsNotVerifiedEvidence: true, physicalBackupFilesUnchanged: true, noExtraDriveGets: true, actualSshQualified: false };
    },
  };
}
