import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createTask, getTask, moveTask, taskHistory, completeTask, TaskError } from "../../src/core/tasks";
import { defaultTaskBoard, getTaskBoard, listTaskBoards, previewTaskBoard, saveTaskBoard } from "../../src/core/task-boards";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import type { TaskBoardDraft, TaskMutationContext, TaskScope } from "../../src/core/tasks-types";
const cleanup: Array<() => void> = [];
afterEach(() => { for (const fn of cleanup.splice(0).reverse()) fn(); });
function fixture() { const root = mkdtempSync(join(tmpdir(), "rentemester-task-boards-")), db = openWorkspaceControlDb(root); cleanup.push(() => { db.close(); rmSync(root, { recursive: true, force: true }); }); return db; }
const scope: TaskScope = { kind: "company", companySlug: "synthetic-alpha" };
const context = (key: string, expectedVersion?: number): TaskMutationContext => ({ actor: "agent:synthetic", principal: "synthetic-member", idempotencyKey: key, expectedVersion, now: "2026-10-08T10:00:00.000Z" });
function draft(selectedScope = scope): TaskBoardDraft { const b = defaultTaskBoard(selectedScope); return { boardId: b.boardId, scope: b.scope, columns: b.columns }; }
function code(action: () => unknown) { try { action(); return "success"; } catch (error) { if (error instanceof TaskError) return error.code; throw error; } }

describe("task boards", () => {
  test("read-only defaults match UI without creating state", () => {
    const db = fixture();
    expect(defaultTaskBoard(scope).boardId).toBe("company:synthetic-alpha"); expect(defaultTaskBoard({ kind: "workspace", companySlugs: [] }).boardId).toBe("workspace");
    expect(defaultTaskBoard(scope).columns.map((column) => column.columnId)).toEqual(["open", "in_progress", "waiting", "done"]);
    expect(defaultTaskBoard(scope).version).toBe(0); expect(listTaskBoards(db)).toEqual([]); expect(getTaskBoard(db, "workspace")).toBeNull();
  });
  test("save is versioned, principal-specific and exactly retriable", () => {
    const db = fixture(), input = draft();
    const created = saveTaskBoard(db, input, context("save", 0));
    expect(created.version).toBe(1); expect(listTaskBoards(db)).toHaveLength(1);
    expect(saveTaskBoard(db, input, context("save", 0))).toEqual(created);
    expect(code(() => saveTaskBoard(db, input, context("stale", 0)))).toBe("version_conflict");
    expect(code(() => saveTaskBoard(db, { ...input, columns: input.columns.map((c) => ({ ...c, name: "Changed" })) }, context("save", 0)))).toBe("idempotency_conflict");
  });
  test("every status has exactly one default and column ids are unique", () => {
    const db = fixture(), input = draft();
    expect(code(() => saveTaskBoard(db, { ...input, columns: input.columns.filter((c) => c.status !== "done") }, context("missing", 0)))).toBe("invalid_input");
    expect(code(() => saveTaskBoard(db, { ...input, columns: [...input.columns, { columnId: "open", name: "Extra", status: "open", isDefault: false }] }, context("duplicate", 0)))).toBe("invalid_input");
    expect(code(() => saveTaskBoard(db, { ...input, columns: [...input.columns, { columnId: "extra", name: "Extra", status: "open", isDefault: true }] }, context("defaults", 0)))).toBe("invalid_input");
    expect(code(() => saveTaskBoard(db, { ...input, boardId: "company:synthetic-beta" }, context("scope", 0)))).toBe("invalid_input");
  });
  test("renaming and reordering columns preserve task status and version", () => {
    const db = fixture(), task = createTask(db, { title: "Afstem", scope }, context("create"));
    const input = draft();
    input.columns = input.columns.map((column) => ({ ...column, name: `${column.name} nu` })).reverse();
    expect(previewTaskBoard(db, input).affectedTaskIds).toEqual([]);
    saveTaskBoard(db, input, context("save", 0));
    expect(getTask(db, task.taskId)?.version).toBe(1); expect(getTask(db, task.taskId)?.status).toBe("open");
  });
  test("deleting a populated column requires atomic relocation and keeps evidence", () => {
    const db = fixture(), task = createTask(db, { title: "Afstem", scope, references: [{ kind: "document", ref: "document:12" }] }, context("create"));
    const input = draft(); input.columns = input.columns.map((column) => column.columnId === "open" ? { ...column, columnId: "new-open", name: "Ny åben" } : column);
    expect(code(() => previewTaskBoard(db, input))).toBe("relocation_required");
    input.relocations = { open: "new-open" };
    const preview = previewTaskBoard(db, input); expect(preview.affectedTaskIds).toEqual([task.taskId]);
    saveTaskBoard(db, input, context("save", 0));
    const moved = getTask(db, task.taskId)!; expect(moved.columnId).toBe("new-open"); expect(moved.status).toBe("open"); expect(moved.version).toBe(2); expect(moved.references).toEqual(task.references);
    expect(taskHistory(db, task.taskId)[1]?.operation).toBe("board_changed");
  });
  test("meaning changes require the current consequence hash", () => {
    const db = fixture(), task = createTask(db, { title: "Afstem", scope }, context("create"));
    const input = draft(); input.columns = input.columns.map((column) => column.columnId === "open" ? { ...column, status: "waiting", isDefault: false } : column);
    input.columns.push({ columnId: "new-open", name: "Åben ny", status: "open", isDefault: true });
    const preview = previewTaskBoard(db, input); expect(preview.statusChanges).toEqual([{ taskId: task.taskId, from: "open", to: "waiting" }]);
    expect(code(() => saveTaskBoard(db, input, context("unreviewed", 0)))).toBe("preview_required");
    expect(getTask(db, task.taskId)?.status).toBe("open");
    saveTaskBoard(db, { ...input, previewHash: preview.previewHash }, context("save", 0));
    expect(getTask(db, task.taskId)?.status).toBe("waiting"); expect(getTask(db, task.taskId)?.version).toBe(2);
  });
  test("stale previews reject if a task changed after review", () => {
    const db = fixture(), task = createTask(db, { title: "Afstem", scope }, context("create"));
    const input = draft(); input.columns = input.columns.map((column) => column.columnId === "open" ? { ...column, status: "waiting", isDefault: false } : column); input.columns.push({ columnId: "new-open", name: "Åben ny", status: "open", isDefault: true });
    const preview = previewTaskBoard(db, input);
    moveTask(db, task.taskId, { status: "in_progress" }, context("work", 1));
    expect(code(() => saveTaskBoard(db, { ...input, previewHash: preview.previewHash }, context("stale", 0)))).toBe("preview_required");
    expect(listTaskBoards(db)).toHaveLength(0); expect(getTask(db, task.taskId)?.status).toBe("in_progress");
  });
  test("board changes cannot bypass completion or reopen controls", () => {
    const db = fixture(), task = createTask(db, { title: "Afstem", scope }, context("create"));
    const input = draft(); input.columns = input.columns.map((column) => column.columnId === "open" ? { ...column, status: "done", isDefault: false } : column); input.columns.push({ columnId: "new-open", name: "Åben ny", status: "open", isDefault: true });
    const preview = previewTaskBoard(db, input);
    expect(code(() => saveTaskBoard(db, { ...input, previewHash: preview.previewHash }, context("fake-complete", 0)))).toBe("completion_required");
    expect(getTask(db, task.taskId)?.status).toBe("open"); expect(listTaskBoards(db)).toHaveLength(0);
    completeTask(db, task.taskId, { outcome: "completed", note: "Kontrolleret" }, context("done", 1));
    const input2 = draft(); input2.columns = input2.columns.map((column) => column.columnId === "done" ? { ...column, status: "open", isDefault: false } : column); input2.columns.push({ columnId: "new-done", name: "Færdig ny", status: "done", isDefault: true });
    expect(code(() => saveTaskBoard(db, { ...input2, previewHash: previewTaskBoard(db, input2).previewHash }, context("fake-reopen", 0)))).toBe("reopen_required");
  });
  test("workspace movement must choose an ambiguous company column", () => {
    const db = fixture(), input = draft(); input.columns.push({ columnId: "accountant", name: "Hos revisor", status: "waiting", isDefault: false });
    saveTaskBoard(db, input, context("save", 0));
    const task = createTask(db, { title: "Afstem", scope }, context("create"));
    expect(code(() => moveTask(db, task.taskId, { status: "waiting", workspaceColumnId: "waiting" }, context("ambiguous", 1)))).toBe("column_choice_required");
    const moved = moveTask(db, task.taskId, { status: "waiting", columnId: "accountant", workspaceColumnId: "waiting" }, context("chosen", 1));
    expect(moved.status).toBe("waiting"); expect(moved.columnId).toBe("accountant"); expect(moved.workspaceColumnId).toBe("waiting");
  });
  test("workspace moves within a status preserve the company's chosen column", () => {
    const db = fixture(), local = draft(); local.columns.push({ columnId: "accountant", name: "Hos revisor", status: "waiting", isDefault: false });
    saveTaskBoard(db, local, context("local", 0));
    const global = draft({ kind: "workspace", companySlugs: [] }); global.columns.push({ columnId: "review", name: "Review", status: "waiting", isDefault: false });
    saveTaskBoard(db, global, context("global", 0));
    const task = createTask(db, { title: "Afstem", scope, status: "waiting", columnId: "accountant", workspaceColumnId: "waiting" }, context("create"));
    const moved = moveTask(db, task.taskId, { status: "waiting", workspaceColumnId: "review" }, context("workspace-move", 1));
    expect(moved.columnId).toBe("accountant"); expect(moved.workspaceColumnId).toBe("review");
  });
  test("workspace column relocation updates placement without duplicating a task", () => {
    const db = fixture(), task = createTask(db, { title: "Afstem", scope }, context("create"));
    const input = draft({ kind: "workspace", companySlugs: [] }); input.columns = input.columns.map((column) => column.columnId === "open" ? { ...column, columnId: "workspace-open" } : column); input.relocations = { open: "workspace-open" };
    saveTaskBoard(db, input, context("workspace-board", 0));
    const moved = getTask(db, task.taskId)!; expect(moved.columnId).toBe("open"); expect(moved.workspaceColumnId).toBe("workspace-open"); expect(taskHistory(db, task.taskId)).toHaveLength(2);
  });
});
