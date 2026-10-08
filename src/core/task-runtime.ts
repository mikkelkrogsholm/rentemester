import type { Database } from "bun:sqlite";
import { randomUUID } from "node:crypto";
import { openWorkspaceControlDb, workspaceControlPaths } from "./workspace-control";
import { resolve } from "node:path";
import { listWorkspaceCompanies } from "./workspace";
import { canReadTask, taskAccess } from "./tasks-access";
import { materializeTaskSeries } from "./task-series";
import { syncTaskSources } from "./task-sources";
import { runTaskReminders } from "./task-reminders";
import type { TaskIdentity, TaskRuntimeStatus } from "./tasks-types";

// A stored heartbeat alone cannot prove a timer is still running. The process
// that serves this workspace must also own an active lifecycle registration.
const runtimes = new Set<string>();
export function taskRuntimeStatus(db: Database, workspaceRoot?: string): TaskRuntimeStatus {
  const row = db.query("SELECT last_tick_at,last_error FROM rm_task_runtime WHERE id=1").get() as { last_tick_at: string | null; last_error: string | null };
  const age = row.last_tick_at ? Date.now() - Date.parse(row.last_tick_at) : Infinity;
  // Read-only connections use temporary SQLite snapshots; identify the live
  // workspace explicitly so that reads do not mistake a running timer for stopped.
  const key = workspaceRoot ? resolve(workspaceControlPaths(workspaceRoot).db) : db.filename;
  return { running: runtimes.has(key) && age >= 0 && age < 120_000 && !row.last_error,
    lastTickAt: row.last_tick_at, lastError: row.last_error };
}

/** Uses the caller's connection so an explicit run and its receipt are atomic. */
export function maintainTaskDb(db: Database, workspaceRoot: string, input: { now: Date; identity: TaskIdentity; allowLocalRecipient: boolean; companySlugs?: string[]; includeWorkspace?: boolean }) {
  const access = taskAccess(db, workspaceRoot, input.identity);
  const archived = new Set(listWorkspaceCompanies(workspaceRoot).filter(company => company.archived).map(company => company.slug));
  const companySlugs = [...access.write].filter(slug => !archived.has(slug) && (!input.companySlugs || input.companySlugs.includes(slug)));
  const manageSlugs = [...access.manage].filter(slug => !archived.has(slug) && (!input.companySlugs || input.companySlugs.includes(slug)));
  const asOfDate = new Intl.DateTimeFormat("en-CA", { timeZone: "Europe/Copenhagen", year: "numeric", month: "2-digit", day: "2-digit" }).format(input.now);
  const actor = input.identity.actor ?? "system:task-runtime";
  const principal = input.identity.principal;
  const sync = syncTaskSources(db, workspaceRoot, { companySlugs, actor, principal, asOfDate });
  const materialized = materializeTaskSeries(db, { companySlugs: manageSlugs, includeWorkspace: access.owner && input.includeWorkspace !== false, actor, principal, asOfDate });
  const reminders = runTaskReminders(db, { now: input.now, canRead: (recipientId, task) => {
    // An explicit run cannot deliver inaccessible company information either.
    if (!canReadTask(access, task)) return false;
    const recipient = taskAccess(db, workspaceRoot, { principal: recipientId === "local" ? "local" : `user:${recipientId}`,
      userId: recipientId === "local" ? undefined : recipientId, local: recipientId === "local" && input.allowLocalRecipient });
    return canReadTask(recipient, task);
  } });
  return { sync, materialized, delivered: reminders.delivered };
}

/** Short durable lease prevents two server processes delivering the same tick. */
export function runTaskMaintenance(workspaceRoot: string, input: { now?: Date; identity?: TaskIdentity; allowLocalRecipient?: boolean } = {}) {
  const now = input.now ?? new Date();
  if (!Number.isFinite(now.getTime())) throw new Error("Invalid task runtime time");
  const db = openWorkspaceControlDb(workspaceRoot);
  const owner = randomUUID();
  try {
    const acquired = db.transaction(() => {
      const row = db.query("SELECT lease_until FROM rm_task_runtime WHERE id=1").get() as { lease_until: string | null };
      if (row.lease_until && row.lease_until > now.toISOString()) return false;
      db.query("UPDATE rm_task_runtime SET lease_owner=?,lease_until=? WHERE id=1").run(owner, new Date(now.getTime() + 55_000).toISOString());
      return true;
    }).immediate();
    if (!acquired) return { skipped: true };
    try {
      const identity = input.identity ?? { principal: "system:task-runtime", actor: "system:task-runtime", local: true, enforceActorPolicy: false };
      const result = maintainTaskDb(db, workspaceRoot, { now, identity, allowLocalRecipient: input.allowLocalRecipient ?? identity.local });
      db.query("UPDATE rm_task_runtime SET last_tick_at=?,last_error=NULL,lease_owner=NULL,lease_until=NULL WHERE id=1 AND lease_owner=?").run(now.toISOString(), owner);
      return { skipped: false, ...result };
    } catch (error) {
      db.query("UPDATE rm_task_runtime SET last_tick_at=?,last_error=?,lease_owner=NULL,lease_until=NULL WHERE id=1 AND lease_owner=?").run(now.toISOString(), "Påmindelsesruntime kunne ikke gennemføre seneste kørsel. Kontrollér workspace og kilder.", owner);
      throw error;
    }
  } finally { db.close(); }
}

export function startTaskRuntime(workspaceRoot: string, options: { allowLocalRecipient: boolean }): () => void {
  const db = openWorkspaceControlDb(workspaceRoot); const key = db.filename; db.close();
  runtimes.add(key);
  let busy = false;
  const tick = () => {
    if (busy) return;
    busy = true;
    try { runTaskMaintenance(workspaceRoot, options); }
    catch { process.stderr.write("[tasks] Runtimekørsel fejlede; se status i Rentemester.\n"); }
    finally { busy = false; }
  };
  tick();
  const timer = setInterval(tick, 60_000);
  return () => { clearInterval(timer); runtimes.delete(key); };
}
