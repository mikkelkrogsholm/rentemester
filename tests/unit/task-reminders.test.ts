import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { activateWorkspaceUser, disableWorkspaceUser } from "../../src/core/workspace-access";
import { completeTask, createTask, getTask, updateTask } from "../../src/core/tasks";
import { configureTaskReminder, listTaskNotifications, runTaskReminders } from "../../src/core/task-reminders";
import type { TaskDraft, TaskMutationContext, TaskReminder } from "../../src/core/tasks-types";

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
function fixture() {
  const root = mkdtempSync(join(tmpdir(), "rm-task-reminders-")); roots.push(root);
  const db = openWorkspaceControlDb(root);
  for (const id of ["user-one", "user-two"]) {
    db.query('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,?,?)')
      .run(id, id, `${id}@example.test`, "2026-01-01", "2026-01-01");
    activateWorkspaceUser(db, { userId: id, workspaceRole: "member", createdBy: "agent:test" });
  }
  return db;
}
const ctx = (idempotencyKey: string, expectedVersion?: number): TaskMutationContext => ({ actor: "agent:test", principal: "synthetic", idempotencyKey, expectedVersion, now: "2026-01-01T00:00:00.000Z" });
const reminder = (overrides: Partial<TaskReminder> = {}): TaskReminder => ({ reminderId: "deadline", recipientId: "user-one", enabled: true,
  kind: "deadline", daysBefore: 2, frequency: "once", timeZone: "Europe/Copenhagen", ...overrides });
const draft = (overrides: Partial<TaskDraft> = {}): TaskDraft => ({ title: "Afstem perioden", scope: { kind: "company", companySlug: "synthetic" },
  deadline: { date: "2026-02-10", kind: "internal", basis: "Valgt intern frist", certainty: "confirmed" }, ...overrides });

describe("in-product task reminders", () => {
  test("no default reminders, and explicit activation survives restart without duplicate catch-up", () => {
    const db = fixture(); const task = createTask(db, draft(), ctx("create"));
    const tick = { now: "2026-02-09T12:00:00.000Z", canRead: () => true };
    expect(runTaskReminders(db, tick)).toEqual({ delivered: 0 });
    const input = { taskId: task.taskId, reminder: reminder() };
    const configured = configureTaskReminder(db, input, ctx("activate", task.version));
    expect(configureTaskReminder(db, input, ctx("activate", task.version))).toEqual(configured);
    expect(runTaskReminders(db, tick)).toEqual({ delivered: 1 });
    db.close();
    const resumed = openWorkspaceControlDb(roots.at(-1)!);
    expect(runTaskReminders(resumed, tick)).toEqual({ delivered: 0 });
    expect(runTaskReminders(resumed, { ...tick, now: "2026-02-15T12:00:00.000Z" })).toEqual({ delivered: 0 });
    expect(listTaskNotifications(resumed, "user-one")).toHaveLength(1);
    resumed.close();
  });

  test("daily notifications use the chosen time zone, including DST and delayed startup", () => {
    const db = fixture(); const task = createTask(db, draft({ deadline: null }), ctx("create"));
    configureTaskReminder(db, { taskId: task.taskId, reminder: reminder({ kind: "follow_up", followUpDate: "2026-03-29", frequency: "daily" }) }, ctx("activate", task.version));
    expect(runTaskReminders(db, { now: "2026-03-28T23:30:00.000Z", canRead: () => true }).delivered).toBe(1);
    expect(runTaskReminders(db, { now: "2026-03-29T12:00:00.000Z", canRead: () => true }).delivered).toBe(0);
    expect(runTaskReminders(db, { now: "2026-04-04T12:00:00.000Z", canRead: () => true }).delivered).toBe(1);
    expect(listTaskNotifications(db, "user-one")).toHaveLength(2);
    db.close();
  });

  test("work-date movement cannot cancel a deadline warning; changed deadline hides obsolete warning", () => {
    const db = fixture(); const original = createTask(db, draft(), ctx("create"));
    let task = configureTaskReminder(db, { taskId: original.taskId, reminder: reminder() }, ctx("activate", original.version));
    task = updateTask(db, task.taskId, { workDate: "2026-03-20" }, ctx("work", task.version));
    expect(runTaskReminders(db, { now: "2026-02-09T12:00:00.000Z", canRead: () => true }).delivered).toBe(1);
    updateTask(db, task.taskId, { deadline: { date: "2026-03-10", kind: "internal", certainty: "confirmed", basis: "Ny intern frist" } }, ctx("deadline", task.version));
    expect(listTaskNotifications(db, "user-one")).toEqual([]);
    expect(runTaskReminders(db, { now: "2026-02-09T12:00:00.000Z", canRead: () => true }).delivered).toBe(0);
    expect(runTaskReminders(db, { now: "2026-03-09T12:00:00.000Z", canRead: () => true }).delivered).toBe(1);
    db.close();
  });

  test("access loss, disabled users and completion stop delivery; explicit recipients survive reassignment", () => {
    const db = fixture(); const original = createTask(db, draft({ assignee: { kind: "member", userId: "user-one", name: "Synthetic One" } }), ctx("create"));
    let task = configureTaskReminder(db, { taskId: original.taskId, reminder: reminder() }, ctx("activate", original.version));
    const tick = { now: "2026-02-09T12:00:00.000Z", canRead: () => true };
    expect(runTaskReminders(db, { ...tick, canRead: () => false }).delivered).toBe(0);
    disableWorkspaceUser(db, { userId: "user-one", createdBy: "agent:test" });
    expect(runTaskReminders(db, tick).delivered).toBe(0);
    activateWorkspaceUser(db, { userId: "user-one", workspaceRole: "member", createdBy: "agent:test" });
    task = updateTask(db, task.taskId, { assignee: { kind: "member", userId: "user-two", name: "Synthetic Two" } }, ctx("reassign", task.version));
    expect(runTaskReminders(db, tick).delivered).toBe(1);
    task = updateTask(db, task.taskId, { assignee: null }, ctx("unassign", task.version));
    expect(runTaskReminders(db, tick).delivered).toBe(0);
    completeTask(db, task.taskId, { outcome: "completed", note: "Intern opgave udført" }, ctx("complete", task.version));
    expect(runTaskReminders(db, tick).delivered).toBe(0);
    expect(listTaskNotifications(db, "user-one")).toEqual([]);
    db.close();
  });

  test("external assignees get no account or messages and unknown recipients fail closed", () => {
    const db = fixture(); const task = createTask(db, draft({ assignee: { kind: "external", name: "Synthetic Accountant" } }), ctx("create"));
    expect(() => configureTaskReminder(db, { taskId: task.taskId, reminder: reminder({ recipientId: "accountant-without-account" }) }, ctx("unknown", task.version)))
      .toThrow("aktiv workspacebruger");
    expect(getTask(db, task.taskId)?.reminders).toEqual([]);
    expect(listTaskNotifications(db, "accountant-without-account")).toEqual([]);
    expect(() => configureTaskReminder(db, { taskId: task.taskId, reminder: reminder({ timeZone: "invalid-zone" }) }, ctx("invalid", task.version)))
      .toThrow("tidszone er ugyldig");
    db.close();
  });

  test("trusted local recipient requires no user row and still obeys runtime authorization", () => {
    const root = mkdtempSync(join(tmpdir(), "rm-task-local-reminders-")); roots.push(root);
    const db = openWorkspaceControlDb(root);
    const task = createTask(db, draft(), ctx("create"));
    configureTaskReminder(db, { taskId: task.taskId, reminder: reminder({ recipientId: "local" }) }, ctx("activate", task.version));
    const tick = { now: "2026-02-09T12:00:00.000Z", canRead: (recipientId: string) => recipientId === "local" };
    expect(runTaskReminders(db, { ...tick, canRead: () => false }).delivered).toBe(0);
    expect(runTaskReminders(db, tick).delivered).toBe(1);
    expect(listTaskNotifications(db, "local")).toHaveLength(1);
    db.close();
  });
});
