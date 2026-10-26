import { afterEach, describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { activateWorkspaceUser, grantCompanyMembership, revokeCompanyMembership, disableWorkspaceUser } from "../../src/core/workspace-access";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { initWorkspace, registerWorkspaceCompany, setWorkspaceCompanyArchived } from "../../src/core/workspace";
import { executeTaskOperation, readTaskOperationReceipt } from "../../src/core/task-service";
import { createTask } from "../../src/core/tasks";
import { defaultTaskBoard } from "../../src/core/task-boards";
import { saveTaskSeries } from "../../src/core/task-series";
import { companyRootForSlug } from "../../src/core/workspace";
import { runTaskMaintenance, startTaskRuntime, taskRuntimeStatus } from "../../src/core/task-runtime";
import type { Task, TaskIdentity } from "../../src/core/tasks-types";

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
const audit = { createdBy: "agent:test", createdByProgram: "task-service-test" };
const local: TaskIdentity = { principal: "local", actor: "user:local", local: true, enforceActorPolicy: false };
function user(userId: string): TaskIdentity { return { principal: `user:${userId}`, userId, actor: `user:${userId}`, local: false, enforceActorPolicy: false }; }
function fixture() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-task-service-")); roots.push(root); initWorkspace(root);
  for (const slug of ["company-a", "company-b"]) registerWorkspaceCompany(root, { slug, name: `Synthetic ${slug}`, createdAt: "2026-01-01T00:00:00.000Z", archived: false });
  const db = openWorkspaceControlDb(root);
  for (const id of ["bookkeeper", "reviewer", "reader", "owner"]) {
    db.query('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES(?,?,?,1,?,?,1)').run(id, `Synthetic ${id}`, `${id}@example.test`, "2026-01-01T00:00:00.000Z", "2026-01-01T00:00:00.000Z");
    activateWorkspaceUser(db, { userId: id, workspaceRole: id === "owner" ? "workspace_owner" : "member", ...audit });
    if (id !== "owner") grantCompanyMembership(db, root, { userId: id, companySlug: "company-a", role: id as "bookkeeper" | "reviewer" | "reader", ...audit });
  }
  db.close(); return root;
}
async function create(root: string, key: string, companySlug = "company-a", identity = local) {
  return (await executeTaskOperation(root, "create", { title: `Synthetic ${key}`, scope: { kind: "company", companySlug }, idempotencyKey: key }, identity)).task as Task;
}

describe("shared task application security", () => {
  test("one common A+B task is counted once and requires both company memberships", async () => {
    const root = fixture();
    const db = openWorkspaceControlDb(root); grantCompanyMembership(db, root, { userId: "bookkeeper", companySlug: "company-b", role: "bookkeeper", ...audit }); db.close();
    await executeTaskOperation(root, "create", { title: "Common monthly review", scope: { kind: "workspace", companySlugs: ["company-a", "company-b"] }, idempotencyKey: "common-review" }, user("bookkeeper"));
    expect((await executeTaskOperation(root, "list", {}, user("bookkeeper"))).count).toBe(1);
    expect((await executeTaskOperation(root, "list", { companySlugs: ["company-a"] }, user("bookkeeper"))).count).toBe(1);
    expect((await executeTaskOperation(root, "list", {}, user("reader"))).count).toBe(0);
  });
  test("server lifecycle catches up once and live membership revocation removes reminder visibility", async () => {
    const root = fixture();
    const task = await create(root, "live-reminder");
    const date = new Intl.DateTimeFormat("en-CA", { timeZone: "Europe/Copenhagen" }).format(new Date());
    await executeTaskOperation(root, "reminder-set", { taskId: task.taskId, expectedVersion: 1, idempotencyKey: "live-reminder-plan", reminder: { reminderId: "followup", recipientId: "bookkeeper", enabled: true, kind: "follow_up", followUpDate: date, frequency: "daily", timeZone: "Europe/Copenhagen" } }, user("bookkeeper"));
    const stop = startTaskRuntime(root, { allowLocalRecipient: false });
    try {
      const db = openWorkspaceControlDb(root); expect(taskRuntimeStatus(db).running).toBe(true); db.close();
      expect((await executeTaskOperation(root, "runtime-status", {}, user("bookkeeper"))).runtime).toMatchObject({ running: true });
      expect((await executeTaskOperation(root, "notifications", {}, user("bookkeeper"))).notifications).toHaveLength(1);
      expect(runTaskMaintenance(root, { allowLocalRecipient: false })).toMatchObject({ delivered: 0 });
      const accessDb = openWorkspaceControlDb(root); revokeCompanyMembership(accessDb, root, { userId: "bookkeeper", companySlug: "company-a", ...audit }); accessDb.close();
      expect(runTaskMaintenance(root, { allowLocalRecipient: false })).toMatchObject({ delivered: 0 });
      expect((await executeTaskOperation(root, "notifications", {}, user("bookkeeper"))).notifications).toHaveLength(0);
    } finally { stop(); }
    const db = openWorkspaceControlDb(root); expect(taskRuntimeStatus(db).running).toBe(false); db.close();
  });
  test("filters tasks before visible counts, search and reference results, without owner company implication", async () => {
    const root = fixture(); await create(root, "visible"); await create(root, "private", "company-b");
    expect((await executeTaskOperation(root, "list", {}, user("reader"))).count).toBe(1);
    expect((await executeTaskOperation(root, "list", { search: "private" }, user("reader"))).count).toBe(0);
    expect((await executeTaskOperation(root, "list", {}, user("owner"))).count).toBe(0);
    await expect(create(root, "reader-write", "company-a", user("reader"))).rejects.toMatchObject({ code: "ACCESS_DENIED" });
    expect((await create(root, "reviewer-write", "company-a", user("reviewer"))).version).toBe(1);
    await expect(executeTaskOperation(root, "create", { title: "Linked", scope: { kind: "workspace", companySlugs: ["company-a", "company-b"] }, idempotencyKey: "linked" }, user("bookkeeper"))).rejects.toMatchObject({ code: "ACCESS_DENIED" });
  });
  test("receipts are principal-specific and revoked live access blocks retry and metadata", async () => {
    const root = fixture(); const input = { title: "Retry", scope: { kind: "company", companySlug: "company-a" }, idempotencyKey: "retry" };
    const first = await executeTaskOperation(root, "create", input, user("bookkeeper"));
    expect(await executeTaskOperation(root, "create", input, user("bookkeeper"))).toEqual(first);
    expect(readTaskOperationReceipt(root, "retry", user("bookkeeper"))).toMatchObject({ receipt: { taskId: (first.task as Task).taskId, version: 1 } });
    expect(readTaskOperationReceipt(root, "retry", user("reviewer"))).toEqual({ receipt: null });
    const db = openWorkspaceControlDb(root); revokeCompanyMembership(db, root, { userId: "bookkeeper", companySlug: "company-a", ...audit }); db.close();
    expect(readTaskOperationReceipt(root, "retry", user("bookkeeper"))).toEqual({ receipt: null });
    await expect(executeTaskOperation(root, "create", input, user("bookkeeper"))).rejects.toMatchObject({ code: "ACCESS_DENIED" });
  });
  test("history from a formerly different scope never leaks to newly authorized readers", async () => {
    const root = fixture(); const task = await create(root, "historic-private", "company-b");
    const moved = await executeTaskOperation(root, "update", { taskId: task.taskId, patch: { scope: { kind: "company", companySlug: "company-a" }, title: "Shared now" }, expectedVersion: 1, idempotencyKey: "scope-change" }, local);
    expect((moved.task as Task).version).toBe(2);
    const read = await executeTaskOperation(root, "get", { taskId: task.taskId }, user("reader"));
    expect(read.history).toHaveLength(1); expect(JSON.stringify(read)).not.toContain("historic-private");
  });
  test("archived company work remains behind retained membership and an explicit entry", async () => {
    const root = fixture(); const task = await create(root, "archive"); setWorkspaceCompanyArchived(root, "company-a", true);
    expect((await executeTaskOperation(root, "list", {}, user("reader"))).count).toBe(0);
    expect((await executeTaskOperation(root, "list", { includeArchived: true }, user("reader"))).count).toBe(1);
    expect((await executeTaskOperation(root, "get", { taskId: task.taskId }, user("reader"))).task).toMatchObject({ status: "open" });
    await expect(executeTaskOperation(root, "get", { taskId: task.taskId }, user("owner"))).rejects.toMatchObject({ code: "NOT_FOUND" });
  });
  test("assignment and evidence cannot grant company access; hosted cannot spoof local member", async () => {
    const root = fixture();
    await expect(executeTaskOperation(root, "create", { title: "Bad assignee", scope: { kind: "company", companySlug: "company-a" }, assignee: { kind: "member", userId: "owner", name: "Owner" }, idempotencyKey: "assignment" }, user("bookkeeper"))).rejects.toMatchObject({ code: "INVALID_ASSIGNEE" });
    await expect(executeTaskOperation(root, "create", { title: "Spoof", scope: { kind: "company", companySlug: "company-a" }, assignee: { kind: "member", userId: "local", name: "Local" }, idempotencyKey: "local-spoof" }, user("bookkeeper"))).rejects.toMatchObject({ code: "INVALID_ASSIGNEE" });
    const task = await create(root, "references");
    await expect(executeTaskOperation(root, "complete", { taskId: task.taskId, outcome: "completed", note: "Private", references: [{ kind: "document", ref: "synthetic-doc", companySlug: "company-b" }], expectedVersion: 1, idempotencyKey: "evidence" }, user("bookkeeper"))).rejects.toMatchObject({ code: "INVALID_REFERENCE" });
  });
  test("ordinary lifecycle never opens or changes an accounting ledger and stopped runtime is truthful", async () => {
    const root = fixture(); const data = join(companyRootForSlug(root, "company-a"), "data"); mkdirSync(data, { recursive: true });
    const ledger = join(data, "ledger.sqlite"); writeFileSync(ledger, "synthetic immutable sentinel");
    const hash = () => createHash("sha256").update(readFileSync(ledger)).digest("hex"); const before = hash();
    const task = await create(root, "ledger-independent");
    const completed = (await executeTaskOperation(root, "complete", { taskId: task.taskId, outcome: "completed", note: "Documented manually", expectedVersion: 1, idempotencyKey: "complete" }, local)).task as Task;
    await executeTaskOperation(root, "reopen", { taskId: task.taskId, reason: "Follow up", expectedVersion: completed.version, idempotencyKey: "reopen" }, local);
    expect(hash()).toBe(before);
    const db = openWorkspaceControlDb(root); db.query("UPDATE rm_task_runtime SET last_tick_at=? WHERE id=1").run(new Date().toISOString()); expect(taskRuntimeStatus(db).running).toBe(false); db.close();
  });
  test("transaction authorization runs before durable retry lookup", () => {
    const root = fixture(); const db = openWorkspaceControlDb(root);
    const ctx = { actor: "agent:test", principal: "principal", idempotencyKey: "atomic-auth" };
    createTask(db, { title: "Atomic", scope: { kind: "company", companySlug: "company-a" } }, ctx);
    expect(() => createTask(db, { title: "Atomic", scope: { kind: "company", companySlug: "company-a" } }, { ...ctx, authorize: () => { throw new Error("revoked"); } })).toThrow("revoked");
    disableWorkspaceUser(db, { userId: "reader", ...audit }); db.close();
  });
  test("workspace board structural previews reveal nothing about inaccessible company workload", async () => {
    const root = fixture(); const board = defaultTaskBoard({ kind: "workspace", companySlugs: [] });
    const draft = { boardId: board.boardId, scope: board.scope, columns: board.columns.map(column => column.status === "waiting" ? { ...column, columnId: "new_waiting" } : column) };
    const renamed = { ...draft, columns: board.columns.map(column => ({ ...column, name: `${column.name} ny` })) };
    const beforeRename = (await executeTaskOperation(root, "boards-preview", { board: renamed }, user("owner"))).preview;
    await expect(executeTaskOperation(root, "boards-preview", { board: draft }, user("owner"))).rejects.toMatchObject({ code: "ACCESS_DENIED" });
    const task = await create(root, "hidden-waiting", "company-b");
    await executeTaskOperation(root, "move", { taskId: task.taskId, status: "waiting", expectedVersion: 1, idempotencyKey: "hidden-move" }, local);
    await expect(executeTaskOperation(root, "boards-preview", { board: draft }, user("owner"))).rejects.toMatchObject({ code: "ACCESS_DENIED" });
    expect((await executeTaskOperation(root, "boards-preview", { board: renamed }, user("owner"))).preview).toEqual(beforeRename);
  });
  test("bulk receipt scopes and moved occurrence scopes are checked on every request", async () => {
    const root = fixture(); const db = openWorkspaceControlDb(root);
    grantCompanyMembership(db, root, { userId: "bookkeeper", companySlug: "company-b", role: "bookkeeper", ...audit });
    for (const slug of ["company-a", "company-b"]) {
      const scope = { kind: "company" as const, companySlug: slug };
      saveTaskSeries(db, { seriesId: `series-${slug}`, title: "Synthetic routine", scope, template: { title: "Synthetic routine", scope }, cadence: "month", every: 1, anchor: "calendar", startDate: "2026-01-01", endDate: null, fiscalYearStartMonth: 1, workDayOffset: 0, deadlineDayOffset: null, relevance: "relevant", active: true }, { actor: "agent:test", principal: "setup", idempotencyKey: `setup-${slug}`, now: "2026-01-01T00:00:00.000Z" });
    }
    db.close();
    const input = { asOfDate: "2026-01-01", idempotencyKey: "bulk" };
    const first = await executeTaskOperation(root, "series-materialize", input, user("bookkeeper"));
    expect((first.materialized as { taskIds: string[] }).taskIds).toHaveLength(2);
    const edit = openWorkspaceControlDb(root); revokeCompanyMembership(edit, root, { userId: "bookkeeper", companySlug: "company-b", ...audit }); edit.close();
    await expect(executeTaskOperation(root, "series-materialize", input, user("bookkeeper"))).rejects.toMatchObject({ code: "NOT_FOUND" });
    expect(readTaskOperationReceipt(root, "bulk", user("bookkeeper"))).toEqual({ receipt: null });
    const view = await executeTaskOperation(root, "list", {}, local); const a = (view.tasks as Task[]).find(task => task.scope.kind === "company" && task.scope.companySlug === "company-a")!;
    await executeTaskOperation(root, "update", { taskId: a.taskId, patch: { scope: { kind: "company", companySlug: "company-b" }, title: "Secret B" }, expectedVersion: 1, idempotencyKey: "move-routine" }, local);
    const projected = await executeTaskOperation(root, "series-project", { companySlugs: ["company-a"], from: "2026-01-01", to: "2026-01-31" }, user("bookkeeper")); expect(JSON.stringify(projected)).not.toContain("Secret B");
    const materialized = await executeTaskOperation(root, "series-materialize", { companySlugs: ["company-a"], asOfDate: "2026-01-01", idempotencyKey: "new-bulk" }, user("bookkeeper")); expect((materialized.materialized as { taskIds: string[] }).taskIds).toEqual([]);
  });
  test("client version actors work and explicit company selection cannot bypass workspace actor policy", async () => {
    const root = fixture(); const companyRoot = companyRootForSlug(root, "company-a"); mkdirSync(join(companyRoot, "config"), { recursive: true }); mkdirSync(join(root, "config"), { recursive: true });
    writeFileSync(join(companyRoot, "config", "policy.yaml"), "actor_allowlist:\n  agents:\n    - approved/1.0\n"); writeFileSync(join(root, "config", "policy.yaml"), "actor_allowlist:\n  agents:\n    - denied\n");
    const identity = { ...local, actor: "agent:approved/1.0", enforceActorPolicy: true };
    expect((await create(root, "mcp-actor", "company-a", identity)).version).toBe(1);
    await expect(executeTaskOperation(root, "series-materialize", { asOfDate: "2026-01-01", idempotencyKey: "workspace-policy" }, identity)).rejects.toMatchObject({ code: "ACTOR_DENIED" });
    const result = await executeTaskOperation(root, "series-materialize", { companySlugs: ["company-a"], asOfDate: "2026-01-01", idempotencyKey: "company-only" }, identity); expect(result.includesWorkspace).toBe(false);
  });
});
