import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";

/** Read-only governance qualification attached to the existing private Drive proof. */
export function createBackupGovernanceProof(page: Page, base: string, work: string, providerCount: () => number) {
  const endpoint = "/api/companies/synthetic/backup-governance";
  const cookieFile = join(work, "backup-governance-owner.cookie");
  const need = (value: unknown, message: string) => { if (!value) throw Error(message); };
  let newerSha = "", stableNewer: any;
  const cookies = async () => writeFileSync(cookieFile, (await page.context().cookies(base)).map(cookie => `${cookie.name}=${cookie.value}`).join("; ") + "\n", { mode: 0o600 });
  function stable(value: any) {
    need(value.ok === true && value.slug === "synthetic" && value.governance.format === "rentemester-native-backup-governance/1", "Governance public identity differs");
    need(value.backup.checkedAt === value.governance.checkedAt && value.backup.latestBackupId === value.governance.latestBackupId, "Governance must use same current backup observation");
    need(Number.isFinite(Date.parse(value.backup.checkedAt)), "Governance checkedAt must be an actual timestamp");
    need(value.governance.attestationIndependentlyVerified === false && value.governance.recoveryVerified === false, "Governance must not invent attestation or restore proof");
    need(value.governance.errors.length === 0 && value.backup.errors.length === 0, "Qualified fixture has current backup/evidence errors");
    if (value.backup.latestBackupAt !== null) {
      const days = (Date.parse(value.backup.checkedAt) - Date.parse(value.backup.latestBackupAt)) / 86400000;
      need(Math.abs(value.backup.daysSinceLatestBackup - days) <= 0.0050001, "Elapsed backup days must agree with this read's clock");
    }
    const copy = structuredClone(value);
    // These are independently validated above; each public read observes its own clock.
    copy.backup.checkedAt = null; copy.governance.checkedAt = null; copy.backup.daysSinceLatestBackup = null;
    return copy;
  }
  function client(kind: "cli" | "mcp") {
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookieFile];
    const args = kind === "cli" ? ["remote", "backup-governance", ...common, "--company", "synthetic"] : common;
    const input = kind === "cli" ? "" : [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-governance-proof", version: "1" } } },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "system_backup_governance", arguments: { company: "synthetic" } } },
    ].map(value => JSON.stringify(value)).join("\n") + "\n";
    const result = spawnSync(join(work, kind === "cli" ? "rentemester-rust" : "rentemester-mcp"), args, { input, encoding: "utf8", timeout: 45000, maxBuffer: 8 * 1024 * 1024 });
    need(!result.error && result.status === 0, "Native governance client refused");
    if (kind === "cli") { const reply = JSON.parse(result.stdout); need(reply.ok === true, "CLI governance envelope refused"); return reply.data; }
    const reply = result.stdout.trim().split("\n").map(line => JSON.parse(line)).find(value => value.id === 2);
    need(reply?.result?.isError === false, "MCP governance envelope refused"); return reply.result.structuredContent.data;
  }
  async function read() {
    const response = await page.request.get(base + endpoint); need(response.ok(), "Current human governance read refused"); return response.json();
  }
  async function parity(external: boolean, expectedSha?: string) {
    const count = providerCount(); await cookies();
    const value = await read(), comparison = stable(value);
    if (expectedSha) need(value.backup.latestBackupId === expectedSha, "Governance latest qualified SHA differs");
    need(value.governance.latestBackupExternalCopyObserved === external && value.governance.latestBackupExternalCopyAtDeclaredDestination === false, "External observation must be exact-archive scoped and separate from declarations");
    need(value.governance.destinationCount === 1 && value.governance.declaredOffsiteDestinationCount === 0, "Synthetic operator declaration counts differ");
    for (const kind of ["cli", "mcp"] as const) need(isDeepStrictEqual(stable(client(kind)), comparison), `${kind} full governance read differs`);
    const observed = page.waitForResponse(response => response.url() === base + endpoint && response.request().method() === "GET");
    await page.getByRole("button", { name: "Hent samlet backupstatus", exact: true }).click();
    const web = await observed; need(web.ok() && isDeepStrictEqual(stable(await web.json()), comparison), "Actual shared web governance response differs");
    const card = page.getByRole("region", { name: "Samlet backupstatus", exact: true });
    await card.getByText(external ? "Ekstern kopi observeret" : "Ingen verificeret ekstern kopi til denne backup", { exact: true }).waitFor();
    if (value.backup.latestBackupId) await card.getByText(value.backup.latestBackupId, { exact: false }).waitFor();
    need(providerCount() === count, "Read-only governance contacted Drive");
    return value;
  }
  return {
    async beforePlacement() {
      await page.goto(base + "/companies/synthetic/integritet");
      const value = await parity(false);
      need(value.governance.latestBackupRemoteVerificationCount === 0 && value.governance.latestBackupPlacementCount === 0, "Before first placement there must be no external evidence");
      return { priorLatestBackupId: value.backup.latestBackupId, noPriorExternalEvidence: true, readOnlyClientAndWebParity: true };
    },
    async observed(receipts: any[]) {
      need(receipts.length === 3 && receipts.every(receipt => receipt.archiveSha256 === receipts[0].archiveSha256), "Three existing successful exact-SHA observations required");
      const value = await parity(true, receipts[0].archiveSha256), governance = value.governance;
      need(governance.latestBackupPlacementCount === 1 && governance.latestBackupRemoteVerificationCount === receipts.length, "Exact current archive placement/observation counts differ");
      const actual = governance.destinations.flatMap((destination: any) => destination.remoteVerifications);
      need(actual.length === receipts.length && receipts.every(receipt => actual.some((candidate: any) => isDeepStrictEqual(candidate, receipt))), "Governance must retain exact original signed-archive observation receipts and timestamps");
      return { latestBackupId: value.backup.latestBackupId, matchingPlacements: 1, matchingRemoteObservations: receipts.length, originalObservedAt: receipts.map(receipt => receipt.observedAt), exactReceiptParity: true };
    },
    async newerBackup(oldSha: string) {
      const count = providerCount(), download = page.waitForEvent("download");
      await page.getByRole("checkbox", { name: "Jeg vil hente en backup af hele workspacet.", exact: true }).check();
      await page.getByRole("button", { name: "Dan og download backup", exact: true }).click();
      const artifact = await download, output = join(work, "backup-governance-newer.native-backup.json"); await artifact.saveAs(output);
      need(await artifact.failure() === null, "Newer signed UI backup download failed");
      const bytes = readFileSync(output); newerSha = createHash("sha256").update(bytes).digest("hex");
      need(newerSha !== oldSha, "Newer backup must have a distinct actual wire SHA");
      const verified = spawnSync(join(work, "rentemester-server"), ["--verify-backup", output, "--trusted-backup-key", join(work, "trusted.pub")], { encoding: "utf8", timeout: 60000 });
      need(!verified.error && verified.status === 0, "Newer archive signature/inventory independently refused");
      const value = await parity(false, newerSha);
      need(value.governance.latestBackupPlacementCount === 0 && value.governance.latestBackupRemoteVerificationCount === 0
        && value.governance.destinations.every((destination: any) => destination.placements.length === 0 && destination.remoteVerifications.length === 0), "New archive borrowed older placement or remote evidence");
      stableNewer = stable(value); need(providerCount() === count, "Newer local backup/governance must not contact Drive");
      return { oldArchiveSha256: oldSha, newArchiveSha256: newerSha, newerUiBackupIndependentlyVerified: true, olderExternalEvidenceNotBorrowed: true, unchangedDriveGetCount: true };
    },
    async reopen() {
      const value = await parity(false, newerSha);
      need(isDeepStrictEqual(stable(value), stableNewer), "New latest archive governance changed across normal MFA restart");
      return { sameNewLatestAfterNormalMfaRestart: true, latestBackupId: newerSha, historicalEvidenceNotBorrowed: true, readOnlyClientAndWebParity: true };
    },
  };
}
