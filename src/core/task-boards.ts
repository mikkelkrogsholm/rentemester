import type { Database } from "bun:sqlite";
import { canonicalJson } from "./canonical-json";
import { appendTaskEvent, executeTaskMutation, getTask, listTasks, TASK_STATUSES, TaskError, taskId, taskPayloadHash, taskText, validateTaskScope } from "./tasks";
import type { Task, TaskBoard, TaskBoardDraft, TaskBoardPreview, TaskMutationContext, TaskScope } from "./tasks-types";
const LABELS = { open: "Åben", in_progress: "I gang", waiting: "Afventer", done: "Færdig" };
export function defaultTaskBoard(scope: TaskScope): TaskBoard {
  const checked = validateTaskScope(scope);
  return { boardId: checked.kind === "company" ? `company:${checked.companySlug}` : "workspace", version: 0, scope: checked.kind === "company" ? checked : { kind: "workspace", companySlugs: [] }, columns: TASK_STATUSES.map((status) => ({ columnId: status, name: LABELS[status], status, isDefault: true })), updatedAt: "1970-01-01T00:00:00.000Z" };
}
export function getTaskBoard(db: Database, id: string): TaskBoard | null {
  taskId(id, "boardId");
  const row = db.query("SELECT payload_json FROM rm_current_task_boards WHERE board_id=?").get(id) as { payload_json: string } | null;
  return row ? JSON.parse(row.payload_json) as TaskBoard : null;
}
export function listTaskBoards(db: Database): TaskBoard[] {
  const rows = db.query("SELECT payload_json FROM rm_current_task_boards ORDER BY board_id").all() as { payload_json: string }[];
  return rows.map((row) => JSON.parse(row.payload_json) as TaskBoard);
}
function validateBoard(input: TaskBoardDraft): TaskBoardDraft {
  if (!input || typeof input !== "object") throw new TaskError("invalid_input", "Invalid board");
  for (const key of Object.keys(input)) if (!["boardId", "scope", "columns", "relocations", "previewHash"].includes(key)) throw new TaskError("invalid_input", `Unsupported board field: ${key}`);
  const scope = validateTaskScope(input.scope), expectedId = scope.kind === "company" ? `company:${scope.companySlug}` : "workspace";
  if (input.boardId !== expectedId || (scope.kind === "workspace" && scope.companySlugs.length)) throw new TaskError("invalid_input", "Board must match its company or workspace scope");
  if (!Array.isArray(input.columns) || input.columns.length < 4 || input.columns.length > 40) throw new TaskError("invalid_input", "A board requires 4 to 40 columns");
  const ids = new Set<string>();
  const columns = input.columns.map((column) => {
    if (!column || !TASK_STATUSES.includes(column.status) || typeof column.isDefault !== "boolean") throw new TaskError("invalid_input", "Invalid board column");
    const columnId = taskId(column.columnId, "columnId");
    if (ids.has(columnId)) throw new TaskError("invalid_input", "Column ids must be unique"); ids.add(columnId);
    return { columnId, name: taskText(column.name, "column name", 160, true), status: column.status, isDefault: column.isDefault };
  });
  for (const status of TASK_STATUSES) if (columns.filter((column) => column.status === status && column.isDefault).length !== 1) throw new TaskError("invalid_input", "Each status requires exactly one visible default column");
  const relocations = input.relocations ?? {};
  if (typeof relocations !== "object" || Array.isArray(relocations) || Object.keys(relocations).length > 40) throw new TaskError("invalid_input", "Invalid column relocations");
  for (const [from, to] of Object.entries(relocations)) { taskId(from, "old column"); taskId(to, "new column"); if (!ids.has(to)) throw new TaskError("invalid_input", "Relocation target is not a board column"); }
  return { boardId: expectedId, scope, columns, relocations, ...(input.previewHash ? { previewHash: input.previewHash } : {}) };
}
function columnForTask(task: Task, board: TaskBoard): string {
  const selected = board.scope.kind === "company" ? task.columnId : task.workspaceColumnId;
  if (selected && board.columns.some((column) => column.columnId === selected && column.status === task.status)) return selected;
  return board.columns.find((column) => column.status === task.status && column.isDefault)!.columnId;
}
function tasksOnBoard(db: Database, scope: TaskScope): Task[] {
  return listTasks(db, { showDone: true }).filter((task) => scope.kind === "workspace" || (task.scope.kind === "company" && task.scope.companySlug === scope.companySlug));
}
function changes(db: Database, input: TaskBoardDraft) {
  const draft = validateBoard(input), prior = getTaskBoard(db, draft.boardId) ?? defaultTaskBoard(draft.scope);
  const tasks = tasksOnBoard(db, draft.scope);
  const affected: Array<{ task: Task; columnId: string; from: Task["status"]; to: Task["status"] }> = [];
  for (const task of tasks) {
    const oldColumn = columnForTask(task, prior);
    let columnId = oldColumn;
    if (!draft.columns.some((column) => column.columnId === oldColumn)) {
      columnId = draft.relocations?.[oldColumn] ?? "";
      if (!columnId) throw new TaskError("relocation_required", "Move every populated removed column to a retained column");
    }
    const nextColumn = draft.columns.find((column) => column.columnId === columnId)!;
    if (columnId !== oldColumn || nextColumn.status !== task.status) affected.push({ task, columnId, from: task.status, to: nextColumn.status });
  }
  const previewHash = taskPayloadHash({ prior, draft: { boardId: draft.boardId, scope: draft.scope, columns: draft.columns, relocations: draft.relocations }, tasks: tasks.map((task) => ({ id: task.taskId, version: task.version, columnId: columnForTask(task, prior), status: task.status })) });
  const statusChanges = affected.filter((change) => change.from !== change.to).map((change) => ({ taskId: change.task.taskId, from: change.from, to: change.to }));
  const meaningChanged = prior.columns.some((column) => draft.columns.some((next) => next.columnId === column.columnId && next.status !== column.status));
  return { draft, prior, affected, meaningChanged, preview: { previewHash, affectedTaskIds: affected.map((change) => change.task.taskId), statusChanges } satisfies TaskBoardPreview };
}
export function previewTaskBoard(db: Database, input: TaskBoardDraft): TaskBoardPreview { return changes(db, input).preview; }
function chooseRelatedColumn(db: Database, task: Task, nextStatus: Task["status"], workspace: boolean): string {
  const scope: TaskScope = workspace ? { kind: "workspace", companySlugs: [] } : task.scope;
  const fallback = defaultTaskBoard(scope), related = getTaskBoard(db, fallback.boardId) ?? fallback;
  const candidates = related.columns.filter((column) => column.status === nextStatus);
  if (candidates.length > 1 && !workspace) throw new TaskError("column_choice_required", "Choose the company's target column before changing board meaning");
  return candidates.find((column) => column.isDefault)!.columnId;
}
export function saveTaskBoard(db: Database, input: TaskBoardDraft, ctx: TaskMutationContext): TaskBoard {
  return executeTaskMutation(db, "task-board-save", input, ctx, (now) => {
    const { draft, prior, affected, meaningChanged, preview } = changes(db, input);
    if (!Number.isInteger(ctx.expectedVersion) || ctx.expectedVersion !== prior.version) throw new TaskError("version_conflict", "Board changed; reload before saving");
    if ((meaningChanged || preview.statusChanges.length) && draft.previewHash !== preview.previewHash) throw new TaskError("preview_required", "Review current task consequences before changing a column's meaning");
    for (const change of affected) {
      if (change.to === "done" && change.from !== "done") throw new TaskError("completion_required", "Board changes cannot bypass documented completion");
      if (change.from === "done" && change.to !== "done") throw new TaskError("reopen_required", "Reopen completed tasks before changing their column meaning");
      const task = getTask(db, change.task.taskId)!;
      task.status = change.to; task.version += 1; task.updatedAt = now;
      if (draft.scope.kind === "company") {
        task.columnId = change.columnId;
        if (change.from !== change.to && task.workspaceColumnId) task.workspaceColumnId = chooseRelatedColumn(db, task, change.to, true);
      } else {
        task.workspaceColumnId = change.columnId;
        if (change.from !== change.to) task.columnId = chooseRelatedColumn(db, task, change.to, false);
      }
      appendTaskEvent(db, task, "board_changed", ctx);
    }
    const result: TaskBoard = { boardId: draft.boardId, scope: draft.scope, columns: draft.columns, version: prior.version + 1, updatedAt: now };
    db.query("INSERT INTO rm_task_board_events(board_id,version,payload_json,actor,principal,created_at) VALUES(?,?,?,?,?,?)").run(result.boardId, result.version, canonicalJson(result), ctx.actor, ctx.principal, now);
    return result;
  });
}
