import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { getTask, taskHistory, listTasks, updateTask } from "../../src/core/tasks";
import { getTaskSeries, listTaskSeries, materializeTaskSeries, projectTaskSeries, saveTaskSeries } from "../../src/core/task-series";
import type { TaskMutationContext, TaskSeriesDraft } from "../../src/core/tasks-types";

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
function fixture() { const root = mkdtempSync(join(tmpdir(), "rm-task-series-")); roots.push(root); return openWorkspaceControlDb(root); }
const ctx = (idempotencyKey: string, expectedVersion?: number): TaskMutationContext => ({ actor: "agent:test", principal: "synthetic", idempotencyKey, expectedVersion, now: "2026-01-01T00:00:00.000Z" });
function draft(overrides: Partial<TaskSeriesDraft> = {}): TaskSeriesDraft {
  const scope = { kind: "company" as const, companySlug: "synthetic" };
  return { seriesId: "monthly", title: "Månedlig afstemning", scope, template: { title: "Månedlig afstemning", scope },
    cadence: "month", every: 1, anchor: "calendar", startDate: "2026-01-01", endDate: null, fiscalYearStartMonth: 1,
    workDayOffset: 0, deadlineDayOffset: 14, relevance: "relevant", active: true, ...overrides };
}

describe("task series and annual-wheel projections", () => {
  test("projects future periods without writes or implicitly confirmed deadlines", () => {
    const db = fixture();
    saveTaskSeries(db, draft(), ctx("create"));
    const before = db.query("SELECT COUNT(*) count FROM rm_task_events").get();
    const projection = projectTaskSeries(db, { from: "2026-01-01", to: "2026-12-31" });
    expect(projection).toHaveLength(12);
    expect(projection[1]).toMatchObject({ period: { from: "2026-02-01", to: "2026-02-28" }, workDate: "2026-02-01", concreteTaskId: null,
      deadline: { date: "2026-02-15", kind: "internal", certainty: "unconfirmed" } });
    expect(db.query("SELECT COUNT(*) count FROM rm_task_events").get()).toEqual(before);
    expect(projectTaskSeries(db, { from: "2026-01-01", to: "2026-12-31", companySlugs: ["other"] })).toEqual([]);
    db.close();
  });

  test("old unfinished and current periods are independent and repeated materialization is quiet", () => {
    const db = fixture(); saveTaskSeries(db, draft(), ctx("create"));
    const input = { asOfDate: "2026-02-10", actor: "agent:test", principal: "synthetic" };
    expect(materializeTaskSeries(db, input).created).toBe(2);
    const tasks = listTasks(db, { showDone: true });
    expect(tasks.map(task => task.period?.from).sort()).toEqual(["2026-01-01", "2026-02-01"]);
    expect(tasks.every(task => task.status === "open")).toBe(true);
    const count = db.query("SELECT COUNT(*) count FROM rm_task_events").get();
    expect(materializeTaskSeries(db, input)).toMatchObject({ created: 0, existing: 2 });
    expect(db.query("SELECT COUNT(*) count FROM rm_task_events").get()).toEqual(count);
    db.close();
  });

  test("fiscal quarters and custom intervals have stable period anchors", () => {
    const db = fixture();
    saveTaskSeries(db, draft({ seriesId: "fiscal", cadence: "quarter", anchor: "fiscal", startDate: "2026-07-01", fiscalYearStartMonth: 7 }), ctx("fiscal"));
    const fiscal = projectTaskSeries(db, { from: "2026-07-01", to: "2027-06-30" });
    expect(fiscal.map(item => [item.period.from, item.period.to])).toEqual([
      ["2026-07-01", "2026-09-30"], ["2026-10-01", "2026-12-31"], ["2027-01-01", "2027-03-31"], ["2027-04-01", "2027-06-30"],
    ]);
    saveTaskSeries(db, draft({ seriesId: "custom", cadence: "custom", every: 2 }), ctx("custom"));
    const custom = projectTaskSeries(db, { from: "2026-01-01", to: "2026-06-30" }).filter(item => item.seriesId === "custom");
    expect(custom.map(item => item.period.from)).toEqual(["2026-01-01", "2026-03-01", "2026-05-01"]);
    db.close();
  });

  test("editing or pausing a series preserves each existing task and its evidence history", () => {
    const db = fixture();
    const series = saveTaskSeries(db, draft(), ctx("create"));
    const materialized = materializeTaskSeries(db, { asOfDate: "2026-02-01", actor: "agent:test", principal: "synthetic" });
    const january = getTask(db, materialized.taskIds[0]!)!;
    const edited = updateTask(db, january.taskId, { nextAction: "Afventer bilag", references: [{ kind: "document", ref: "synthetic-proof", companySlug: "synthetic" }] }, ctx("task-edit", january.version));
    const history = taskHistory(db, january.taskId);
    saveTaskSeries(db, { ...draft(), title: "Ny titel", workDayOffset: 9, active: false }, ctx("pause", series.version));
    expect(getTask(db, january.taskId)).toEqual(edited);
    expect(taskHistory(db, january.taskId)).toEqual(history);
    expect(materializeTaskSeries(db, { asOfDate: "2026-04-01", actor: "agent:test", principal: "synthetic" }).created).toBe(0);
    const projections = projectTaskSeries(db, { from: "2026-01-01", to: "2026-04-30" });
    expect(projections).toHaveLength(2);
    expect(projections.find(item => item.concreteTaskId === january.taskId)?.title).toBe(january.title);
    db.close();
  });

  test("activity relevance remains unknown until inspected, never silently non-applicable", () => {
    const db = fixture(); saveTaskSeries(db, draft({ relevance: "activity" }), ctx("create"));
    materializeTaskSeries(db, { asOfDate: "2026-01-02", actor: "agent:test", principal: "synthetic" });
    expect(listTasks(db)[0]).toMatchObject({ relevance: "unknown", status: "open", completion: null });
    db.close();
  });

  test("shared and workspace routines cannot leak through a partial company selection", () => {
    const db = fixture();
    const shared = { kind: "workspace" as const, companySlugs: ["synthetic", "other"] };
    saveTaskSeries(db, draft({ seriesId: "shared", scope: shared, template: { title: "Fælles opgave", scope: shared } }), ctx("shared"));
    const workspace = { kind: "workspace" as const, companySlugs: [] };
    saveTaskSeries(db, draft({ seriesId: "workspace", scope: workspace, template: { title: "Workspaceopgave", scope: workspace } }), ctx("workspace"));
    expect(projectTaskSeries(db, { from: "2026-01-01", to: "2026-01-31", companySlugs: ["synthetic"] })).toEqual([]);
    expect(materializeTaskSeries(db, { asOfDate: "2026-01-02", actor: "agent:test", principal: "synthetic", companySlugs: ["synthetic"] }).created).toBe(0);
    expect(projectTaskSeries(db, { from: "2026-01-01", to: "2026-01-31", companySlugs: ["synthetic", "other"] }).map(item => item.seriesId)).toEqual(["shared"]);
    expect(projectTaskSeries(db, { from: "2026-01-01", to: "2026-01-31", companySlugs: [], includeWorkspace: true }).map(item => item.seriesId)).toEqual(["workspace"]);
    db.close();
  });

  test("future-only template edits preserve the earlier backlog and changed cadence keeps concrete periods visible", () => {
    const db = fixture(); const original = saveTaskSeries(db, draft(), ctx("create"));
    saveTaskSeries(db, { ...draft(), title: "Ny rutine", workDayOffset: 9 }, { ...ctx("edit", original.version), now: "2026-03-01T00:00:00.000Z" });
    const projected = projectTaskSeries(db, { from: "2026-01-01", to: "2026-04-30" });
    expect(projected.find(item => item.period.from === "2026-01-01")?.title).toBe("Månedlig afstemning");
    expect(projected.find(item => item.period.from === "2026-03-01")?.title).toBe("Ny rutine");
    materializeTaskSeries(db, { asOfDate: "2026-03-10", actor: "agent:test", principal: "synthetic" });
    const oldTask = listTasks(db).find(task => task.period?.from === "2026-01-01")!;
    const current = getTaskSeries(db, "monthly")!;
    saveTaskSeries(db, { ...draft(), cadence: "quarter", title: "Kvartalsrutine" }, { ...ctx("quarter", current.version), now: "2026-04-01T00:00:00.000Z" });
    expect(getTask(db, oldTask.taskId)?.title).toBe("Månedlig afstemning");
    expect(projectTaskSeries(db, { from: "2026-01-01", to: "2026-06-30" }).some(item => item.concreteTaskId === oldTask.taskId)).toBe(true);
    db.close();
  });

  test("series writes are idempotent and reject lost updates, invalid cadence and implicit activation", () => {
    const db = fixture(); const input = draft();
    const created = saveTaskSeries(db, input, ctx("create"));
    expect(saveTaskSeries(db, input, ctx("create"))).toEqual(created);
    expect(listTaskSeries(db)).toHaveLength(1);
    expect(getTaskSeries(db, created.seriesId)?.version).toBe(1);
    expect(() => saveTaskSeries(db, input, ctx("stale", 8))).toThrow("Rutinen er ændret");
    expect(() => saveTaskSeries(db, draft({ every: 0 }), ctx("invalid"))).toThrow("Rutinen kræver");
    const withReminder = draft({ template: { ...input.template, reminders: [{ reminderId: "r", recipientId: "u", enabled: true,
      kind: "deadline", daysBefore: 0, frequency: "once", timeZone: "Europe/Copenhagen" }] } });
    expect(() => saveTaskSeries(db, withReminder, ctx("activation"))).toThrow("aktivere påmindelser automatisk");
    db.close();
  });
});
