/** Workspace inbound polling: manifest entries are the authority, never caller keys/credentials. */
import { existsSync } from "node:fs";
import { openDb, migrate } from "../db";
import { openLedgerReadOnly } from "../ledger-inspection";
import { evaluateBackupLock } from "../backup-governance";
import { companyPaths } from "../paths";
import { listWorkspaceCompanies, companyRootForSlug } from "../workspace";
import { checkActorAllowlist, isCanonicalActorId } from "../../cli-actor";
import type { ResolveActorInput } from "../actor";
import { resolveDigisenseReceiver } from "./digisense-wiring";
import { pollDigisenseReceived } from "./digisense-receive";

export type WorkspaceDigisensePollResult = {
  ok: true;
  companies: Array<{ slug: string; status: "polled" | "skipped" | "failed"; reason?: string; documentsIngested?: number }>;
};

export type WorkspaceDigisensePollOptions = {
  actor: Required<Pick<ResolveActorInput, "createdBy" | "createdByProgram">>;
  /** Test seam after all policy/backup preflights have succeeded. */
  pollCompany?: (
    db: ReturnType<typeof openDb>,
    companyRoot: string,
    actor: Required<Pick<ResolveActorInput, "createdBy" | "createdByProgram">>,
  ) => Promise<{ ok: boolean; documentsIngested: number }>;
};

type WorkspaceTarget = { slug: string; root: string };

function activeInitializedTargets(workspaceRoot: string): {
  targets: WorkspaceTarget[];
  skipped: WorkspaceDigisensePollResult["companies"];
} {
  const targets: WorkspaceTarget[] = [];
  const skipped: WorkspaceDigisensePollResult["companies"] = [];
  for (const entry of listWorkspaceCompanies(workspaceRoot)) {
    if (entry.archived) {
      skipped.push({ slug: entry.slug, status: "skipped", reason: "archived" });
      continue;
    }
    const root = companyRootForSlug(workspaceRoot, entry.slug);
    if (!existsSync(companyPaths(root).db)) {
      skipped.push({ slug: entry.slug, status: "skipped", reason: "not initialized" });
      continue;
    }
    targets.push({ slug: entry.slug, root });
  }
  return { targets, skipped };
}

function preflightWorkspaceTargets(targets: WorkspaceTarget[], actor: string): void {
  // This entire pass completes before the first migration, network call, or
  // ledger write, so a later company cannot leave an earlier one mutated.
  if (!isCanonicalActorId(actor)) {
    throw new Error(`workspace DigiSense polling requires a canonical actor id (user:..., agent:..., or system:...)`);
  }
  for (const target of targets) {
    const decision = checkActorAllowlist(target.root, actor);
    if (!decision.allowed) throw new Error(`${target.slug}: ${decision.reason}`);
  }
  for (const target of targets) {
    // `openDb` changes persistent journal mode to WAL. A preflight must be
    // byte/state read-only until every target has passed, so use SQLite's
    // readonly handle directly and defer openDb/migrate to the execution pass.
    const db = openLedgerReadOnly(companyPaths(target.root).db);
    try {
      let lock: ReturnType<typeof evaluateBackupLock>;
      try {
        lock = evaluateBackupLock(db, target.root);
      } catch {
        throw new Error(`${target.slug}: backup status could not be evaluated`);
      }
      if (lock.errors.length > 0) throw new Error(`${target.slug}: ${lock.reason}`);
      if (lock.locked) throw new Error(`${target.slug}: ${lock.reason}`);
    } finally {
      db.close();
    }
  }
}

/** Continues after every company failure; archived and unconfigured companies are skipped. */
export async function pollWorkspaceDigisenseInbound(
  workspaceRoot: string,
  options: WorkspaceDigisensePollOptions,
): Promise<WorkspaceDigisensePollResult> {
  const { targets, skipped } = activeInitializedTargets(workspaceRoot);
  preflightWorkspaceTargets(targets, options.actor.createdBy ?? "");
  const companies: WorkspaceDigisensePollResult["companies"] = [...skipped];
  for (const target of targets) {
    const db = openDb(companyPaths(target.root).db);
    try {
      migrate(db);
      if (options.pollCompany) {
        const result = await options.pollCompany(db, target.root, options.actor);
        companies.push(result.ok
          ? { slug: target.slug, status: "polled", documentsIngested: result.documentsIngested }
          : { slug: target.slug, status: "failed", reason: "poll failed" });
        continue;
      }
      const resolved = resolveDigisenseReceiver(db, target.root);
      // This is a workspace surface: do not echo company bindings, config
      // paths, or provider errors from an individual ledger.
      if (!resolved.ok) { companies.push({ slug: target.slug, status: "skipped", reason: "not configured" }); continue; }
      const result = await pollDigisenseReceived(db, target.root, resolved.client, resolved.downloader, {
        companyKey: resolved.companyKey,
        actor: options.actor,
      });
      companies.push(result.ok ? { slug: target.slug, status: "polled", documentsIngested: result.documentsIngested } : { slug: target.slug, status: "failed", reason: "poll failed" });
    } catch { companies.push({ slug: target.slug, status: "failed", reason: "poll failed" }); }
    finally { db.close(); }
  }
  return { ok: true, companies };
}
