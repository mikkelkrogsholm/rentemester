import { afterEach, describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { canonicalJson } from "../../src/core/canonical-json";
import { exportTaskSnapshot, parseTaskSnapshot, restoreTaskSnapshot } from "../../src/core/task-snapshot";
import { createTask, completeTask, reopenTask, getTask, taskHistory } from "../../src/core/tasks";
import { defaultTaskBoard, getTaskBoard, saveTaskBoard } from "../../src/core/task-boards";
import { createWorkspaceSnapshot, restoreWorkspaceSnapshot } from "../../src/core/workspace-snapshot";
import { openWorkspaceControlDb, workspaceControlPaths } from "../../src/core/workspace-control";
import { companyRootForSlug } from "../../src/core/workspace";
import { companyPaths } from "../../src/core/paths";
import { openDb } from "../../src/core/db";
import { createTar, readTar } from "../../src/core/tar";
import { makeWorkspace } from "./server-api/_shared";
import type { TaskMutationContext, TaskNotification, TaskSeries } from "../../src/core/tasks-types";
const cleanup: Array<() => void> = [];
afterEach(() => { for (const fn of cleanup.splice(0).reverse()) fn(); });
const now = "2026-10-08T10:00:00.000Z";
function fixture() { const root = mkdtempSync(join(tmpdir(), "rentemester-task-snapshot-")), db = openWorkspaceControlDb(root); cleanup.push(() => { db.close(); rmSync(root, { recursive: true, force: true }); }); return { root, db }; }
function ctx(key: string, expectedVersion?: number): TaskMutationContext { return { actor: "agent:synthetic", principal: "snapshot-agent", idempotencyKey: key, expectedVersion, now }; }
const scope = { kind: "company" as const, companySlug: "alpha-company" };
function addUser(db: ReturnType<typeof openWorkspaceControlDb>, id = "synthetic-owner") { db.query('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES(?,?,?,1,?,?,1)').run(id, "Synthetic member", `${id}@example.test`, now, now); }
function populated(db: ReturnType<typeof openWorkspaceControlDb>) {
  addUser(db);
  const task = createTask(db, { title: "Afstem september", scope, assignee: { kind: "member", userId: "synthetic-owner", name: "Synthetic member" }, workDate: "2026-10-09", reminders: [{ reminderId: "owner-reminder", recipientId: "synthetic-owner", enabled: true, kind: "follow_up", followUpDate: "2026-10-09", frequency: "once", timeZone: "Europe/Copenhagen" }, { reminderId: "local-reminder", recipientId: "local", enabled: true, kind: "follow_up", followUpDate: "2026-10-09", frequency: "once", timeZone: "Europe/Copenhagen" }] }, ctx("create"));
  const done = completeTask(db, task.taskId, { outcome: "completed", note: "Kontrolleret", references: [{ kind: "document", ref: "document:5" }] }, ctx("complete", 1));
  reopenTask(db, task.taskId, "Ny dokumentation", ctx("reopen", 2));
  const defaultBoard = defaultTaskBoard(scope);
  const savedBoard = saveTaskBoard(db, { boardId: defaultBoard.boardId, scope, columns: defaultBoard.columns.map((column) => ({ ...column, name: `${column.name} syntetisk` })) }, ctx("board", 0));
  const series: TaskSeries = { seriesId: "series-monthly", version: 1, title: "Månedsrutine", scope, template: { title: "Månedsrutine", scope, assignee: task.assignee, reminders: task.reminders }, cadence: "month", every: 1, anchor: "fiscal", startDate: "2026-01-01", endDate: null, fiscalYearStartMonth: 7, workDayOffset: 5, deadlineDayOffset: 10, relevance: "relevant", active: true, updatedAt: now };
  db.query("INSERT INTO rm_task_series_events(series_id,version,payload_json,actor,principal,created_at) VALUES(?,?,?,?,?,?)").run(series.seriesId, 1, canonicalJson(series), "agent:synthetic", "snapshot-agent", now);
  const n: TaskNotification = { notificationId: "notification-1", recipientId: "synthetic-owner", taskId: task.taskId, reminderId: "owner-reminder", slotKey: "2026-10-09", createdAt: now, title: task.title };
  db.query("INSERT INTO rm_task_notifications(notification_id,recipient_id,task_id,reminder_id,slot_key,payload_json,created_at) VALUES(?,?,?,?,?,?,?)").run(n.notificationId, n.recipientId, n.taskId, n.reminderId, n.slotKey, canonicalJson(n), now);
  db.query("UPDATE rm_task_runtime SET last_tick_at=?,lease_owner='private-runtime-lease',lease_until=? WHERE id=1").run(now, now);
  db.query('INSERT INTO "account"(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,?, ?,?,?,?,?)').run("private-account", "synthetic-owner", "credential", "credential", "synthetic-owner", "private-password", now, now);
  return { task, done, savedBoard, series };
}

describe("credential-free task snapshots", () => {
  test("preserves complete history and receipts while resetting missing identities in new events", () => {
    const source = fixture(), target = fixture(), { task, savedBoard } = populated(source.db);
    const ledgerSentinel = join(source.root, "ledger-sentinel"); writeFileSync(ledgerSentinel, "unchanged ledger"); const before = readFileSync(ledgerSentinel);
    const snapshot = exportTaskSnapshot(source.db, new Set(["alpha-company"]))!;
    expect(snapshot).not.toBeNull(); const text = JSON.stringify(snapshot);
    expect(text).not.toContain("private-password"); expect(text).not.toContain("private-runtime-lease"); expect(text).not.toContain("lease_until");
    restoreTaskSnapshot(target.db, snapshot, new Set(["alpha-company"]), "2026-10-09T10:00:00.000Z");
    expect(readFileSync(ledgerSentinel)).toEqual(before);
    const restored = getTask(target.db, task.taskId)!;
    expect(restored.version).toBe(4); expect(restored.assignee).toBeNull();
    expect(restored.reminders.find((r) => r.recipientId === "synthetic-owner")?.enabled).toBe(false);
    expect(restored.reminders.find((r) => r.recipientId === "local")?.enabled).toBe(true);
    const events = taskHistory(target.db, task.taskId); expect(events.slice(0, 3)).toEqual(taskHistory(source.db, task.taskId));
    expect(events[1]?.task.completion?.references).toEqual([{ kind: "document", ref: "document:5" }]); expect(events[3]?.operation).toBe("restore_identity_reset");
    expect(target.db.query("SELECT reason FROM rm_task_events WHERE task_id=? AND version=4").get(task.taskId)).toMatchObject({ reason: expect.stringContaining("missing member") });
    expect(target.db.query("SELECT * FROM rm_task_receipts ORDER BY principal,idempotency_key").all()).toEqual(source.db.query("SELECT * FROM rm_task_receipts ORDER BY principal,idempotency_key").all());
    expect(getTaskBoard(target.db, savedBoard.boardId)).toEqual(savedBoard);
    expect(target.db.query("SELECT * FROM rm_task_notifications").all()).toEqual(source.db.query("SELECT * FROM rm_task_notifications").all());
    const restoredSeries = JSON.parse((target.db.query("SELECT payload_json FROM rm_current_task_series").get() as { payload_json: string }).payload_json) as TaskSeries;
    expect(restoredSeries.version).toBe(2); expect(restoredSeries.template.assignee).toBeNull(); expect(restoredSeries.template.reminders?.[0]?.enabled).toBe(false); expect(restoredSeries.fiscalYearStartMonth).toBe(7);
    expect(target.db.query("SELECT event_type FROM workspace_audit WHERE entity_id='series-monthly'").get()).toEqual({ event_type: "task_series_identity_reset_missing_members" });
    expect(target.db.query("SELECT last_tick_at,last_error,lease_owner,lease_until FROM rm_task_runtime").get()).toEqual({ last_tick_at: null, last_error: null, lease_owner: null, lease_until: null });
    expect(target.db.query('SELECT count(*) n FROM "user"').get()).toEqual({ n: 0 }); expect(target.db.query('SELECT count(*) n FROM "account"').get()).toEqual({ n: 0 });
  });
  test("retains known member assignments and original reminder choices", () => {
    const source = fixture(), target = fixture(), { task } = populated(source.db); addUser(target.db);
    restoreTaskSnapshot(target.db, exportTaskSnapshot(source.db)!, undefined, now);
    expect(getTask(target.db, task.taskId)).toEqual(getTask(source.db, task.taskId));
    expect(taskHistory(target.db, task.taskId)).toHaveLength(3);
  });
  test("rejects unauthorized tables, columns, inconsistent versions and foreign scopes before writes", () => {
    const source = fixture(), target = fixture(); populated(source.db);
    const valid = exportTaskSnapshot(source.db)!;
    for (const corrupt of [
      { ...valid, tables: { ...valid.tables, account: [] } },
      { ...valid, tables: { ...valid.tables, rm_task_events: valid.tables.rm_task_events.map((row) => ({ ...row, password: "forged" })) } },
      { ...valid, tables: { ...valid.tables, rm_task_events: valid.tables.rm_task_events.slice(1) } },
    ]) {
      expect(() => parseTaskSnapshot(JSON.stringify(corrupt))).toThrow();
      expect(() => restoreTaskSnapshot(target.db, corrupt as typeof valid)).toThrow();
      expect(target.db.query("SELECT count(*) n FROM rm_task_events").get()).toEqual({ n: 0 });
    }
    expect(() => parseTaskSnapshot(JSON.stringify(valid), new Set(["other-company"]))).toThrow("unregistered company");
  });
  test("rejects hidden nested fields, forged receipt results and mismatched notification evidence", () => {
    const source = fixture(); populated(source.db); const valid = exportTaskSnapshot(source.db)!;
    const nested = structuredClone(valid), event = nested.tables.rm_task_events[0]!;
    const t = JSON.parse(String(event.payload_json)); t.assignee.password = "private"; event.payload_json = JSON.stringify(t);
    expect(() => parseTaskSnapshot(JSON.stringify(nested))).toThrow("unsupported or missing fields");
    const receipt = structuredClone(valid); receipt.tables.rm_task_receipts[0]!.result_json = JSON.stringify({ password: "private" });
    expect(() => parseTaskSnapshot(JSON.stringify(receipt))).toThrow();
    const notification = structuredClone(valid); notification.tables.rm_task_notifications[0]!.task_id = "forged-task";
    expect(() => parseTaskSnapshot(JSON.stringify(notification))).toThrow("columns disagree");
  });
  test("rejects valid-looking receipt results that have no historical effect", () => {
    const source = fixture(); populated(source.db); const corrupt = exportTaskSnapshot(source.db)!;
    const row = corrupt.tables.rm_task_receipts[0]!, result = JSON.parse(String(row.result_json));
    if (result.taskId) result.title = "Forged retry effect"; else result.columns[0].name = "Forged retry effect";
    row.result_json = JSON.stringify(result);
    expect(() => parseTaskSnapshot(JSON.stringify(corrupt))).toThrow("no matching historical effect");
  });
  test("refuses a nonempty task store instead of merging conflicting history", () => {
    const source = fixture(), target = fixture(); populated(source.db); createTask(target.db, { title: "Existing", scope }, ctx("existing"));
    expect(() => restoreTaskSnapshot(target.db, exportTaskSnapshot(source.db)!)).toThrow("empty task store");
    expect(target.db.query("SELECT count(*) n FROM rm_task_events").get()).toEqual({ n: 1 });
  });
  test("snapshot manifest roundtrip includes tasks, omits auth and preserves ledger postings", () => {
    const workspace = makeWorkspace("tasks-full-snapshot", ["Alpha Company"]), artifactDir = mkdtempSync(join(tmpdir(), "tasks-snapshot-artifact-"));
    cleanup.push(() => { rmSync(workspace, { recursive: true, force: true }); rmSync(artifactDir, { recursive: true, force: true }); });
    const source = openWorkspaceControlDb(workspace); const { task } = populated(source); source.close();
    const before = openDb(companyPaths(companyRootForSlug(workspace, "alpha-company")).db); const entries = before.query("SELECT id,entry_hash FROM journal_entries ORDER BY id").all(); before.close();
    const outPath = join(artifactDir, "snapshot.tar"), target = join(artifactDir, "restored");
    expect(createWorkspaceSnapshot(workspace, { outPath, createdAt: now })).toMatchObject({ ok: true });
    const archive = readTar(readFileSync(outPath)), manifest = JSON.parse(new TextDecoder().decode(archive.find((item) => item.path === "manifest.json")!.content));
    expect(manifest.tasks.path).toBe("tasks.json"); expect(manifest.tasks.sha256).toMatch(/^[a-f0-9]{64}$/);
    expect(readFileSync(outPath).includes(Buffer.from("private-password"))).toBe(false);
    expect(restoreWorkspaceSnapshot({ snapshotPath: outPath, targetWorkspaceRoot: target })).toMatchObject({ ok: true });
    const restored = openWorkspaceControlDb(target); expect(getTask(restored, task.taskId)?.assignee).toBeNull(); expect(taskHistory(restored, task.taskId)).toHaveLength(4); restored.close();
    const ledger = openDb(companyPaths(companyRootForSlug(target, "alpha-company")).db); expect(ledger.query("SELECT id,entry_hash FROM journal_entries ORDER BY id").all()).toEqual(entries); ledger.close();
  });
  test("old snapshots without task payload remain supported without creating task DB", () => {
    const workspace = makeWorkspace("tasks-legacy-snapshot", ["Alpha Company"]), artifactDir = mkdtempSync(join(tmpdir(), "tasks-legacy-artifact-"));
    cleanup.push(() => { rmSync(workspace, { recursive: true, force: true }); rmSync(artifactDir, { recursive: true, force: true }); });
    const outPath = join(artifactDir, "snapshot.tar"), target = join(artifactDir, "restored");
    expect(createWorkspaceSnapshot(workspace, { outPath, createdAt: now }).ok).toBe(true);
    const entries = readTar(readFileSync(outPath)); expect(entries.some((entry) => entry.path === "tasks.json")).toBe(false);
    expect(restoreWorkspaceSnapshot({ snapshotPath: outPath, targetWorkspaceRoot: target }).ok).toBe(true); expect(existsSync(workspaceControlPaths(target).db)).toBe(false);
  });
  test("archive corruption fails checksum and never publishes a target", () => {
    const workspace = makeWorkspace("tasks-corrupt-snapshot", ["Alpha Company"]), artifactDir = mkdtempSync(join(tmpdir(), "tasks-corrupt-artifact-"));
    cleanup.push(() => { rmSync(workspace, { recursive: true, force: true }); rmSync(artifactDir, { recursive: true, force: true }); });
    const db = openWorkspaceControlDb(workspace); populated(db); db.close();
    const outPath = join(artifactDir, "snapshot.tar"), badPath = join(artifactDir, "corrupt.tar"), target = join(artifactDir, "restored");
    expect(createWorkspaceSnapshot(workspace, { outPath, createdAt: now }).ok).toBe(true);
    const entries = readTar(readFileSync(outPath)).map((entry) => entry.path === "tasks.json" ? { ...entry, content: Buffer.from('{"version":999}\n') } : entry);
    writeFileSync(badPath, createTar(entries)); const result = restoreWorkspaceSnapshot({ snapshotPath: badPath, targetWorkspaceRoot: target });
    expect(result.ok).toBe(false); expect(result.errors.join(" ")).toContain("checksum mismatch"); expect(existsSync(target)).toBe(false);
  });
  test("structural corruption with recomputed checksum still fails before publishing", () => {
    const workspace = makeWorkspace("tasks-schema-corrupt-snapshot", ["Alpha Company"]), artifactDir = mkdtempSync(join(tmpdir(), "tasks-schema-corrupt-artifact-"));
    cleanup.push(() => { rmSync(workspace, { recursive: true, force: true }); rmSync(artifactDir, { recursive: true, force: true }); });
    const db = openWorkspaceControlDb(workspace); populated(db); db.close(); const outPath = join(artifactDir, "snapshot.tar"), badPath = join(artifactDir, "corrupt.tar"), target = join(artifactDir, "restored");
    expect(createWorkspaceSnapshot(workspace, { outPath, createdAt: now }).ok).toBe(true);
    const entries = readTar(readFileSync(outPath)), manifestEntry = entries.find((entry) => entry.path === "manifest.json")!, tasksEntry = entries.find((entry) => entry.path === "tasks.json")!;
    const payload = JSON.parse(new TextDecoder().decode(tasksEntry.content)); payload.tables.account = [{ password: "forged-credential" }]; tasksEntry.content = Buffer.from(JSON.stringify(payload));
    const manifest = JSON.parse(new TextDecoder().decode(manifestEntry.content)); manifest.tasks.sha256 = createHash("sha256").update(tasksEntry.content).digest("hex"); manifest.tasks.sizeBytes = tasksEntry.content.byteLength; manifestEntry.content = Buffer.from(JSON.stringify(manifest));
    writeFileSync(badPath, createTar(entries)); const result = restoreWorkspaceSnapshot({ snapshotPath: badPath, targetWorkspaceRoot: target }); expect(result.ok).toBe(false); expect(result.errors.join(" ")).toContain("unsupported or missing fields"); expect(existsSync(target)).toBe(false);
  });
});
