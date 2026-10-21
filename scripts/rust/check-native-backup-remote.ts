import { createBackupGovernanceProof } from "./check-native-backup-governance";
import type { Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { copyFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { isDeepStrictEqual } from "node:util";

// This harness copies an actual signed placement to an independently served owned
// file. It exercises native verification, never app upload or a real Drive account.
export function createBackupRemoteProof(page: Page, base: string, work: string, secondOwnerPage?: Page) {
  const destination = "synthetic-drive", folder = "synthetic-drive-folder";
  const path = `/api/workspace/backup-destinations/${destination}/remote-verifications`;
  const cookieFile = join(work, "backup-remote-owner.cookie");
  const need = (value: unknown, message: string) => { if (!value) throw Error(message); };
  const sha = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");
  const trace = (): { objectId: string; kind: string }[] => JSON.parse(readFileSync(join(work, "drive-trace.json"), "utf8"));
  const get = async (url: string) => { const response = await page.request.get(base + url); need(response.ok(), "Remote proof authorized read refused"); return response.json(); };
  const books = async () => Promise.all(["synthetic", "synthetic-backup-inbox-target"].map(async slug => ({ slug, journal: await get(`/api/companies/${slug}/journal`), bank: await get(`/api/companies/${slug}/bank`), trial: await get(`/api/companies/${slug}/trial-balance`) })));
  const cookies = async () => writeFileSync(cookieFile, (await page.context().cookies(base)).map(c => `${c.name}=${c.value}`).join("; ") + "\n", { mode: 0o600 });
  const governanceProof = createBackupGovernanceProof(page, base, work, () => trace().length);
  let placement = "", canonical: any, baseline: any, storageKey = "", browserPosts = 0;
  page.on("request", request => { if (request.url() === base + path && request.method() === "POST") browserPosts++; });
  function client(kind: "cli" | "mcp", operationId: string, object?: string) {
    const common = ["--server", base, "--allow-loopback-http", "--cookie-file", cookieFile];
    const args = kind === "cli" ? ["remote", object ? "verify-backup-remote" : "backup-remote-verification", ...common, "--destination", destination, "--operation", operationId, ...(object ? ["--placement-operation", placement, "--remote-object", object, "--confirm"] : [])] : common;
    const input = kind === "cli" ? "" : [
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "synthetic-remote-proof", version: "1" } } },
      { jsonrpc: "2.0", method: "notifications/initialized" },
      { jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: object ? "verify_backup_remote" : "backup_remote_verification", arguments: { destinationId: destination, operationId, ...(object ? { placementOperationId: placement, remoteObjectId: object, confirm: true } : {}) } } },
    ].map(value => JSON.stringify(value)).join("\n") + "\n";
    const result = spawnSync(join(work, kind === "cli" ? "rentemester-rust" : "rentemester-mcp"), args, { input, encoding: "utf8", timeout: 45000, maxBuffer: 4 * 1024 * 1024 });
    need(!result.error && result.status === 0, "Remote proof native client refused");
    if (kind === "cli") return JSON.parse(result.stdout).data.verification;
    const reply = result.stdout.trim().split("\n").map(value => JSON.parse(value)).find(value => value.id === 2);
    need(reply?.result?.isError === false, "Remote proof MCP refused"); return reply.result.structuredContent.data.verification;
  }
  async function inspect(operation: string) { return (await get(`${path}/${operation}`)).verification; }
  async function unchanged() { need(isDeepStrictEqual(await books(), baseline), "Remote verification changed accounting"); }
  async function parity(receipt: any, replay = true) {
    const before = trace().length;
    need(isDeepStrictEqual(await inspect(receipt.operationId), receipt), "HTTP retained receipt differs");
    for (const kind of ["cli", "mcp"] as const) {
      need(isDeepStrictEqual(client(kind, receipt.operationId), receipt), "Native read receipt differs");
      if (replay) need(isDeepStrictEqual(client(kind, receipt.operationId, receipt.remoteObjectId), receipt), "Same completed native intent differs");
    }
    need(trace().length === before, "Retained read/replay unexpectedly contacted Drive");
  }
  async function mount() {
    await page.goto(base + "/companies/synthetic/integritet");
    await page.getByRole("button", { name: "Vis backupdestinationer", exact: true }).click();
    await page.getByRole("button", { name: "Kontrollér placeringen", exact: true }).click();
    await page.getByText(/Backupfilen er genlæst/).waitFor();
  }
  return {
    async run() {
      baseline = await books(); await cookies();
      const governanceBefore = await governanceProof.beforePlacement();
      const destinations = await get("/api/workspace/backup-destinations");
      need(destinations.remoteVerificationDestinations.includes(destination), "Configured remote destination missing");
      storageKey = `rentemester-remote-backup:${destinations.workspaceId}:${destination}`;
      await page.goto(base + "/companies/synthetic/integritet");
      await page.getByRole("button", { name: "Vis backupdestinationer", exact: true }).click();
      await page.getByLabel("Backupdestination", { exact: true }).selectOption(destination);
      await page.getByRole("checkbox", { name: "Jeg vil danne og placere en backup af hele workspacet.", exact: true }).check();
      const placed = page.waitForResponse(response => response.url().endsWith(`/${destination}/place`) && response.request().method() === "POST");
      await page.getByRole("button", { name: "Dan og placér backup", exact: true }).click();
      const response = await placed; need(response.ok(), "Normal MFA web placement refused");
      const local = (await response.json()).placement; placement = local.operationId;
      const localPath = join(work, "backup-destination", `${placement}.native-backup.json`), remotePath = join(work, "drive-remote-object.native-backup.json");
      const bytes = readFileSync(localPath); need(sha(bytes) === local.archiveSha256 && bytes.length === local.archiveSizeBytes, "Actual placed wire differs");
      const verified = spawnSync(join(work, "rentemester-server"), ["--verify-backup", localPath, "--trusted-backup-key", join(work, "trusted.pub")], { encoding: "utf8", timeout: 60000 });
      need(!verified.error && verified.status === 0, "Trusted independent signed archive verification failed");
      need(!existsSync(remotePath), "Separate remote fixture file must be new"); copyFileSync(localPath, remotePath);
      need(readFileSync(remotePath).equals(bytes), "Independent fixture bytes differ");
      writeFileSync(join(work, "drive-object.json"), JSON.stringify({ fileName: `${placement}.native-backup.json`, folderId: folder, sha256: sha(bytes), size: bytes.length }), { mode: 0o600, flag: "wx" });
      await page.getByText(/Backupfilen er genlæst/).waitFor();
      const card = page.getByRole("region", { name: "Kontrol af Google Drive-kopi", exact: true });
      await card.getByLabel("Drive-filens id", { exact: true }).fill("synthetic_good");
      await card.getByRole("checkbox", { name: "Jeg vil kontrollere den præcise backupkopi i Google Drive.", exact: true }).check();
      let accepted: any, posts = 0;
      const lost = async (route: any) => { posts++; const result = await route.fetch(); need(result.ok(), "Server did not accept lost-reply operation"); accepted = (await result.json()).verification; await route.abort("failed"); };
      await page.route(base + path, lost);
      await card.getByRole("button", { name: "Kontrollér fjernkopien", exact: true }).click();
      await card.getByRole("alert").waitFor(); await page.unroute(base + path, lost);
      need(posts === 1 && accepted?.remoteSyncVerified === true, "Exactly one accepted UI request before lost reply");
      canonical = accepted;
      need(canonical.format === "rentemester-native-remote-backup-verification/1" && canonical.provider === "google-drive"
        && canonical.archiveSha256 === sha(bytes) && canonical.archiveSizeBytes === bytes.length
        && canonical.remoteFileName === `${placement}.native-backup.json` && canonical.remoteParentId === folder
        && canonical.signatureVerified === true && canonical.attestationIndependentlyVerified === false
        && canonical.verifyMethod === "google-drive-metadata-and-sha256-reread" && Number.isFinite(Date.parse(canonical.observedAt)), "Exact independently reread receipt bindings");
      const pending = await page.evaluate(key => JSON.parse(localStorage.getItem(key) ?? "null"), storageKey);
      need(pending?.operationId === canonical.operationId && pending.remoteObjectId === "synthetic_good", "Unknown outcome intent not persisted");
      const count = trace().length; need(count === 2, "One metadata + one independent media GET required");
      await mount();
      await page.getByRole("button", { name: "Hent fjernkopiens kvittering", exact: true }).click();
      await page.getByText(/Serveren verificerede fjernkopiens bytes/).waitFor();
      need(trace().length === count && browserPosts === 1, "Remount/reconcile repeated POST/provider work"); await parity(canonical);
      const negatives = [];
      for (const object of ["synthetic_wrong_parent", "synthetic_wrong_hash", "synthetic_oversized", "synthetic_missing"]) {
        const operationId = randomUUID(), before = trace().length;
        const failure = await page.request.post(base + path, { headers: { origin: base }, data: { operationId, placementOperationId: placement, remoteObjectId: object, confirm: true } });
        need(!failure.ok() && (await failure.json()).ok === false && await inspect(operationId) === null, "Invalid remote object gained receipt");
        const observed = trace().slice(before); need(observed.length === (["synthetic_oversized", "synthetic_wrong_hash"].includes(object) ? 2 : 1), "Failure retried or skipped expected exact GETs");
        negatives.push({ objectId: object, status: failure.status(), providerGets: observed }); await unchanged();
      }
      // The existing second MFA human protects the last-owner invariant while
      // the requester revokes its own company role. Restore both public grants.
      need(secondOwnerPage, "Existing second enrolled MFA context required");
      const second = secondOwnerPage!;
      const owner = (await get("/api/me")).user.id;
      const secondMe = await second.request.get(base + "/api/me"); need(secondMe.ok(), "Current second normal MFA identity required");
      const secondId = (await secondMe.json()).user.id;
      const prior = (await get("/api/workspace/members")).members.find((member: any) => member.userId === secondId);
      need(prior?.accessReady && prior.workspaceRole === "member", "Existing ready synthetic invited member required");
      const priorCompany = prior.memberships.find((membership: any) => membership.companySlug === "synthetic")?.role;
      const change = async (acting: Page, endpoint: string, data: unknown) => {
        const response = await acting.request.post(base + "/api/workspace/members/" + endpoint, { headers: { origin: base }, data, timeout: 5000 });
        need(response.ok(), "Normal public membership transition refused or owner mutex held");
      };
      let elevated = false, secondCompanyOwner = false, revoked = false, held: Promise<any> | undefined;
      const revokedOperation = randomUUID();
      try {
        await change(page, "access", { userId: secondId, action: "set-role", workspaceRole: "workspace_owner" }); elevated = true;
        await change(page, "company", { userId: secondId, action: "grant", companySlug: "synthetic", role: "owner" }); secondCompanyOwner = true;
        held = page.request.post(base + path, { headers: { origin: base }, data: { operationId: revokedOperation, placementOperationId: placement, remoteObjectId: "synthetic_held", confirm: true } });
        for (let count = 0; !existsSync(join(work, "drive-held-received")); count++) { need(count < 100, "Held metadata marker deadline"); await new Promise(resolve => setTimeout(resolve, 50)); }
        await change(page, "company", { userId: owner, action: "revoke", companySlug: "synthetic" }); revoked = true;
        writeFileSync(join(work, "drive-held-release"), "release\n", { mode: 0o600, flag: "wx" });
        const finish = await held; need(finish.status() === 403, "Successful provider result reused revoked company authority");
        const denied = await page.request.get(base + `${path}/${revokedOperation}`); need(denied.status() === 403, "Revoked current human read was admitted");
      } finally {
        if (held && !existsSync(join(work, "drive-held-release"))) writeFileSync(join(work, "drive-held-release"), "release\n", { mode: 0o600 });
        if (held) await held.catch(() => undefined);
        if (revoked) await change(second, "company", { userId: owner, action: "grant", companySlug: "synthetic", role: "owner" });
        if (secondCompanyOwner) await change(page, "company", { userId: secondId, companySlug: "synthetic", ...(priorCompany ? { action: "grant", role: priorCompany } : { action: "revoke" }) });
        if (elevated) await change(page, "access", { userId: secondId, action: "set-role", workspaceRole: prior.workspaceRole });
      }
      need(await inspect(revokedOperation) === null, "Revoked operation gained retained receipt");
      const beforeFresh = trace().length;
      const fresh = client("cli", randomUUID(), "synthetic_good"); need(trace().length === beforeFresh + 2, "New explicit CLI operation requires fresh metadata/media"); await parity(fresh);
      need(fresh.operationId !== canonical.operationId, "New explicit id must perform fresh verification");
      const freshMcp = client("mcp", randomUUID(), "synthetic_good"); need(trace().length === beforeFresh + 4, "New explicit MCP operation requires fresh metadata/media"); await parity(freshMcp);
      const governanceObserved = await governanceProof.observed([canonical, fresh, freshMcp]);
      const governanceNewer = await governanceProof.newerBackup(canonical.archiveSha256);
      await unchanged();
      return { governance: { qualified: true, beforePlacement: governanceBefore, observed: governanceObserved, newerBackup: governanceNewer }, placementOperationId: placement, receipt: canonical, newCliReceipt: fresh, newMcpReceipt: freshMcp, negatives, revokedDuringHeldProvider: true, independentTrustedSignatureVerified: true, exactLocalAndSeparateRemoteBytes: true, fixtureCopyByHarness: true, normalWebPlacement: true, normalWebRemoteVerification: true, lostReplyManualReconcile: true, persistedPendingId: true, providerTrace: trace(), unchangedAccounting: true, actualGoogleAccountQualified: false, offsiteQualified: false, installedNativeQualified: false };
    },
    async reopen() {
      const count = trace().length, posts = browserPosts;
      await cookies(); await parity(canonical); await unchanged(); await mount();
      await page.getByRole("button", { name: "Hent fjernkopiens kvittering", exact: true }).click();
      await page.getByText(/Serveren verificerede fjernkopiens bytes/).waitFor();
      need(trace().length === count && browserPosts === posts, "Restart readback must not POST or reread provider");
      const governanceReopen = await governanceProof.reopen();
      return { governance: governanceReopen, sameExactReceiptAfterNormalMfaRestart: true, originalObservedAt: canonical.observedAt, receipt: canonical, actualGoogleAccountQualified: false, offsiteQualified: false, installedNativeQualified: false };
    },
  };
}
