import type { Database } from "bun:sqlite";
import { canonicalJson } from "./canonical-json";
import { appendTaskEvent, listTasks, TASK_STATUSES, taskDate, taskId, taskInstant, taskText, validateTaskDraft, validateTaskReferences, validateTaskScope } from "./tasks";
import { insertWorkspaceAudit } from "./workspace-control";
import type { Task, TaskBoard, TaskCompletion, TaskMutationContext, TaskNotification, TaskSeries, TaskScope } from "./tasks-types";

/** Only operational task evidence is portable. Runtime leases and identities are excluded. */
const COLUMNS = {
  rm_task_events: ["id", "task_id", "version", "operation", "reason", "payload_json", "actor", "principal", "created_at"],
  rm_task_board_events: ["id", "board_id", "version", "payload_json", "actor", "principal", "created_at"],
  rm_task_series_events: ["id", "series_id", "version", "payload_json", "actor", "principal", "created_at"],
  rm_task_receipts: ["principal", "idempotency_key", "operation", "payload_hash", "result_json", "created_at"],
  rm_task_notifications: ["notification_id", "recipient_id", "task_id", "reminder_id", "slot_key", "payload_json", "created_at"],
} as const;
type Table = keyof typeof COLUMNS;
type Cell = string | number | null;
type Row = Record<string, Cell>;
export type TaskSnapshotV1 = { version: 1; tables: Record<Table, Row[]> };
const TABLES = Object.keys(COLUMNS) as Table[];
const TASK_KEYS = ["taskId", "version", "title", "description", "nextAction", "scope", "type", "origin", "status", "columnId", "workspaceColumnId", "assignee", "waitingOn", "workDate", "deadline", "period", "references", "source", "evidenceRequired", "relevance", "verificationRequired", "completion", "seriesId", "occurrenceKey", "reminders", "createdAt", "updatedAt"];
const SERIES_KEYS = ["seriesId", "version", "title", "scope", "template", "cadence", "every", "anchor", "startDate", "endDate", "fiscalYearStartMonth", "workDayOffset", "deadlineDayOffset", "relevance", "active", "updatedAt"];
function invalid(message = "Invalid task snapshot"): never { throw new Error(message); }
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) invalid();
  return value as Record<string, unknown>;
}
function exactKeys(value: unknown, keys: readonly string[]): void {
  const actual = Object.keys(object(value)).sort();
  if (canonicalJson(actual) !== canonicalJson([...keys].sort())) invalid("Task snapshot contains unsupported or missing fields");
}
function integer(value: unknown, field: string, minimum = 1, maximum = Number.MAX_SAFE_INTEGER): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < minimum || value > maximum) invalid(`Invalid task snapshot ${field}`);
  return value;
}
function boolean(value: unknown): void { if (typeof value !== "boolean") invalid(); }
function json(value: Cell): unknown {
  if (typeof value !== "string" || value.length > 2_000_000) invalid();
  try { return JSON.parse(value); } catch { return invalid("Task snapshot contains invalid JSON"); }
}
function actor(value: unknown): string {
  const checked = taskText(value, "snapshot actor", 160, true);
  if (!/^(user|agent|system):[A-Za-z0-9][A-Za-z0-9._:@-]{0,149}$/.test(checked)) invalid("Invalid task snapshot actor");
  return checked;
}
function scope(value: unknown, companySlugs?: ReadonlySet<string>): void {
  const checked = validateTaskScope(value as Task["scope"]);
  exactKeys(value, checked.kind === "company" ? ["kind", "companySlug"] : ["kind", "companySlugs"]);
  const companies = checked.kind === "company" ? [checked.companySlug] : checked.companySlugs;
  if (companySlugs && companies.some((company) => !companySlugs.has(company))) invalid("Task snapshot refers to an unregistered company");
}
function completion(value: unknown, task: Task): void {
  if (value === null) { if (task.status === "done") invalid("Completed task lacks its recorded outcome"); return; }
  exactKeys(value, ["outcome", "note", "references", "assurance", "at", "actor"]);
  const result = value as TaskCompletion;
  if (task.status !== "done" || !["completed", "not_relevant", "cancelled", "exception"].includes(result.outcome) || !["user_reported", "product_verified"].includes(result.assurance)) invalid();
  taskText(result.note, "completion note", 8000, true); taskInstant(result.at); actor(result.actor);
  validateTaskReferences(result.references, task.scope); nestedFields({ references: result.references });
  if (result.assurance === "product_verified" && (!task.source || task.source.state !== "resolved" || !result.references.length || result.outcome !== "completed")) invalid("Invalid verified task evidence");
}
function nestedFields(value: Partial<Task>): void {
  if (value.deadline) exactKeys(value.deadline, ["date", "kind", "basis", "certainty", ...(value.deadline.ruleId !== undefined ? ["ruleId"] : [])]);
  if (value.period) exactKeys(value.period, ["from", "to", "label"]);
  if (value.source) exactKeys(value.source, ["kind", "identity", "companySlug", "ref", "state", "contentHash", "href"]);
  if (value.assignee) exactKeys(value.assignee, value.assignee.kind === "member" ? ["kind", "userId", "name"] : ["kind", "name"]);
  for (const reference of value.references ?? []) exactKeys(reference, ["kind", "ref", ...(reference.companySlug !== undefined ? ["companySlug"] : [])]);
  for (const reminder of value.reminders ?? []) {
    const required = ["reminderId", "recipientId", "enabled", "kind", "frequency", "timeZone"];
    const optional = ["daysBefore", "followUpDate"].filter((field) => field in reminder);
    exactKeys(reminder, [...required, ...optional]);
  }
}
function task(value: unknown, companySlugs?: ReadonlySet<string>): Task {
  exactKeys(value, TASK_KEYS);
  const checked = value as Task;
  taskId(checked.taskId); integer(checked.version, "version"); taskInstant(checked.createdAt); taskInstant(checked.updatedAt);
  scope(checked.scope, companySlugs);
  if (!TASK_STATUSES.includes(checked.status)) invalid();
  const { taskId: _id, version: _version, createdAt: _created, updatedAt: _updated, completion: _completion, ...draft } = checked;
  validateTaskDraft({ ...draft, status: checked.status === "done" ? "open" : checked.status });
  if (checked.workspaceColumnId !== null) taskId(checked.workspaceColumnId);
  if (checked.source === undefined || checked.assignee === undefined || checked.deadline === undefined || checked.period === undefined) invalid();
  if (checked.source) exactKeys(checked.source, ["kind", "identity", "companySlug", "ref", "state", "contentHash", "href"]);
  if (checked.assignee) exactKeys(checked.assignee, checked.assignee.kind === "member" ? ["kind", "userId", "name"] : ["kind", "name"]);
  if (checked.source?.state === "unknown" && !checked.verificationRequired) invalid("Unknown task source lacks verification flag");
  if (checked.relevance === "unknown" && !checked.verificationRequired) invalid("Unknown task relevance lacks verification flag");
  nestedFields(checked);
  completion(checked.completion, checked);
  return checked;
}
function board(value: unknown, companySlugs?: ReadonlySet<string>): TaskBoard {
  exactKeys(value, ["boardId", "version", "scope", "columns", "updatedAt"]);
  const checked = value as TaskBoard;
  taskId(checked.boardId); integer(checked.version, "board version"); scope(checked.scope, companySlugs); taskInstant(checked.updatedAt);
  if (checked.boardId !== (checked.scope.kind === "company" ? `company:${checked.scope.companySlug}` : "workspace") || (checked.scope.kind === "workspace" && checked.scope.companySlugs.length)) invalid();
  if (!Array.isArray(checked.columns) || checked.columns.length < 4 || checked.columns.length > 40) invalid();
  const ids = new Set<string>();
  for (const column of checked.columns) {
    exactKeys(column, ["columnId", "name", "status", "isDefault"]); taskId(column.columnId); taskText(column.name, "column name", 160, true); boolean(column.isDefault);
    if (!TASK_STATUSES.includes(column.status) || ids.has(column.columnId)) invalid(); ids.add(column.columnId);
  }
  for (const status of TASK_STATUSES) if (checked.columns.filter((column) => column.status === status && column.isDefault).length !== 1) invalid();
  return checked;
}
function series(value: unknown, companySlugs?: ReadonlySet<string>): TaskSeries {
  exactKeys(value, SERIES_KEYS);
  const checked = value as TaskSeries;
  taskId(checked.seriesId); integer(checked.version, "series version"); taskText(checked.title, "series title", 240, true); scope(checked.scope, companySlugs); taskInstant(checked.updatedAt);
  if (!["month", "quarter", "year", "custom"].includes(checked.cadence) || !["calendar", "fiscal"].includes(checked.anchor) || !["relevant", "unknown", "activity"].includes(checked.relevance)) invalid();
  integer(checked.every, "cadence", 1, 120); integer(checked.fiscalYearStartMonth, "fiscal year start", 1, 12); integer(checked.workDayOffset, "work offset", -3660, 3660);
  if (checked.deadlineDayOffset !== null) integer(checked.deadlineDayOffset, "deadline offset", -3660, 3660);
  taskDate(checked.startDate); if (checked.endDate !== null && taskDate(checked.endDate) < checked.startDate) invalid(); boolean(checked.active);
  validateTaskDraft(checked.template); nestedFields(checked.template); if (canonicalJson(checked.template.scope) !== canonicalJson(checked.scope)) invalid("Series template scope differs from series scope");
  return checked;
}
function notification(value: unknown): TaskNotification {
  exactKeys(value, ["notificationId", "recipientId", "taskId", "reminderId", "slotKey", "createdAt", "title"]);
  const checked = value as TaskNotification;
  for (const id of [checked.notificationId, checked.recipientId, checked.taskId, checked.reminderId]) taskId(id);
  taskText(checked.slotKey, "notification slot", 200, true); taskText(checked.title, "notification title", 240, true); taskInstant(checked.createdAt);
  return checked;
}
const TASK_OPERATIONS = new Set(["task-create", "task-update", "task-move", "task-complete", "task-reopen", "task-source-sync", "reminder-set"]);
const BULK_OPERATIONS = new Set(["sources-sync", "series-materialize", "runtime-run"]);
function receiptCompanies(value: TaskScope): Set<string> { return new Set(value.kind === "company" ? [value.companySlug] : value.companySlugs); }
function sourceSummary(value: unknown, selected: TaskScope): void {
  const summary = object(value);
  exactKeys(summary, ["created", "updated", "resolved", "reopened", "unknown", "errors", ...(summary.verified !== undefined ? ["verified"] : [])]);
  for (const key of ["created", "updated", "resolved", "reopened", "unknown", ...(summary.verified !== undefined ? ["verified"] : [])]) integer(summary[key], `source ${key}`, 0, 500_000);
  if (!Array.isArray(summary.errors) || summary.errors.length > 500_000) invalid("Invalid source summary errors");
  const companies = receiptCompanies(selected);
  for (const error of summary.errors) {
    exactKeys(error, ["companySlug", "reason"]); const entry = object(error);
    if (typeof entry.companySlug !== "string" || !companies.has(entry.companySlug)) invalid("Source summary error is outside receipt scope");
    taskText(entry.reason, "source summary reason", 2000, true);
  }
}
function materializedSummary(value: unknown): string[] {
  exactKeys(value, ["created", "existing", "taskIds"]); const summary = object(value);
  const created = integer(summary.created, "materialized created", 0, 500_000), existing = integer(summary.existing, "materialized existing", 0, 500_000);
  if (!Array.isArray(summary.taskIds) || summary.taskIds.length !== created + existing || summary.taskIds.length > 500_000) invalid("Invalid materialized task count");
  const ids = summary.taskIds.map((id) => taskId(id, "materialized task id"));
  if (new Set(ids).size !== ids.length) invalid("Duplicate materialized task id");
  return ids;
}
function bulkReceipt(value: unknown, operation: string, companySlugs?: ReadonlySet<string>): void {
  const result = object(value), key = operation === "sources-sync" ? "sync" : operation === "series-materialize" ? "materialized" : "run";
  exactKeys(result, ["scope", "includesWorkspace", key]); scope(result.scope, companySlugs); boolean(result.includesWorkspace);
  const selected = result.scope as TaskScope;
  if (operation === "sources-sync") sourceSummary(result.sync, selected);
  else if (operation === "series-materialize") materializedSummary(result.materialized);
  else {
    exactKeys(result.run, ["sync", "materialized", "delivered"]); const run = object(result.run);
    sourceSummary(run.sync, selected); materializedSummary(run.materialized); integer(run.delivered, "delivered notification count", 0, 500_000);
  }
}
function materializedReceiptIds(value: unknown, operation: string): string[] {
  const result = object(value);
  if (operation === "series-materialize") return materializedSummary(result.materialized);
  if (operation === "runtime-run") return materializedSummary(object(result.run).materialized);
  return [];
}
function receipt(row: Row, companySlugs?: ReadonlySet<string>): void {
  taskText(row.principal, "receipt principal", 160, true); taskId(row.idempotency_key); taskInstant(row.created_at);
  if (typeof row.payload_hash !== "string" || !/^[a-f0-9]{64}$/.test(row.payload_hash)) invalid("Invalid task receipt hash");
  const result = json(row.result_json!);
  if (TASK_OPERATIONS.has(String(row.operation))) task(result, companySlugs);
  else if (row.operation === "task-board-save") board(result, companySlugs);
  else if (row.operation === "series-save") series(result, companySlugs);
  else if (BULK_OPERATIONS.has(String(row.operation))) bulkReceipt(result, String(row.operation), companySlugs);
  else invalid("Task snapshot contains an unsupported receipt operation");
}
export function parseTaskSnapshot(raw: string, companySlugs?: ReadonlySet<string>): TaskSnapshotV1 {
  if (typeof raw !== "string" || Buffer.byteLength(raw) > 128 * 1024 * 1024) invalid("Task snapshot is too large");
  let value: unknown; try { value = JSON.parse(raw); } catch { return invalid("Task snapshot contains invalid JSON"); }
  exactKeys(value, ["version", "tables"]);
  const snapshot = value as TaskSnapshotV1;
  if (snapshot.version !== 1) invalid("Unsupported task snapshot version"); exactKeys(snapshot.tables, TABLES);
  let count = 0;
  const taskIds = new Set<string>(), entitySnapshots = new Set<string>(), seriesIds = new Set<string>();
  const taskVersions = new Map<string, Task[]>();
  for (const table of TABLES) {
    const rows = snapshot.tables[table];
    if (!Array.isArray(rows)) invalid("Invalid task snapshot row count");
    count += rows.length;
    if (count > 500_000) invalid("Invalid task snapshot row count");
    const versions = new Map<string, number>(), rowIds = new Set<number>(), receiptIds = new Set<string>(), slots = new Set<string>();
    for (const row of rows) {
      exactKeys(row, COLUMNS[table]);
      for (const cell of Object.values(row)) if (cell !== null && typeof cell !== "string" && typeof cell !== "number") invalid("Invalid task snapshot cell");
      taskInstant(row.created_at);
      if (table === "rm_task_receipts") { receipt(row, companySlugs); const key = canonicalJson([row.principal, row.idempotency_key]); if (receiptIds.has(key)) invalid("Duplicate task receipt"); receiptIds.add(key); continue; }
      const body = json(row.payload_json!);
      if (table === "rm_task_notifications") {
        const n = notification(body);
        if (n.notificationId !== row.notification_id || n.recipientId !== row.recipient_id || n.taskId !== row.task_id || n.reminderId !== row.reminder_id || n.slotKey !== row.slot_key || n.createdAt !== row.created_at) invalid("Task notification columns disagree with payload");
        if (receiptIds.has(n.notificationId)) invalid("Duplicate task notification id"); receiptIds.add(n.notificationId);
        const slot = canonicalJson([n.recipientId, n.taskId, n.reminderId, n.slotKey]); if (slots.has(slot)) invalid("Duplicate task notification slot"); slots.add(slot); continue;
      }
      const id = integer(row.id, "event id"); if (rowIds.has(id)) invalid("Duplicate task event id"); rowIds.add(id);
      actor(row.actor); taskText(row.principal, "event principal", 160, true);
      const entity = table === "rm_task_events" ? task(body, companySlugs) : table === "rm_task_board_events" ? board(body, companySlugs) : series(body, companySlugs);
      const entityId = table === "rm_task_events" ? (entity as Task).taskId : table === "rm_task_board_events" ? (entity as TaskBoard).boardId : (entity as TaskSeries).seriesId;
      const idColumn = table === "rm_task_events" ? "task_id" : table === "rm_task_board_events" ? "board_id" : "series_id";
      if (row[idColumn] !== entityId || row.version !== entity.version || row.created_at !== entity.updatedAt || entity.version !== (versions.get(entityId) ?? 0) + 1) invalid("Task snapshot event history is incomplete or inconsistent");
      versions.set(entityId, entity.version); entitySnapshots.add(canonicalJson(entity));
      if (table === "rm_task_series_events") seriesIds.add(entityId);
      if (table === "rm_task_events") { const history = taskVersions.get(entityId) ?? []; history.push(entity as Task); taskVersions.set(entityId, history); }
      if (table === "rm_task_events") { taskIds.add(entityId); taskText(row.operation, "task operation", 80, true); if (row.reason !== null) taskText(row.reason, "task event reason", 2000, true); }
    }
  }
  for (const row of snapshot.tables.rm_task_receipts) {
    const result = json(row.result_json!);
    if (!BULK_OPERATIONS.has(String(row.operation))) {
      if (!entitySnapshots.has(canonicalJson(result))) invalid("Task receipt has no matching historical effect");
      continue;
    }
    const bulk = object(result), selected = receiptCompanies(bulk.scope as TaskScope);
    for (const id of materializedReceiptIds(result, String(row.operation))) {
      const history = taskVersions.get(id);
      if (!history?.some((task) => {
        if (!task.seriesId || task.type !== "routine") return false;
        const companies = receiptCompanies(task.scope);
        return companies.size ? [...companies].every((company) => selected.has(company)) : bulk.includesWorkspace === true;
      })) invalid("Materialized receipt task has no matching occurrence history within scope");
    }
  }
  for (const history of taskVersions.values()) if (history.some((task) => task.seriesId && !seriesIds.has(task.seriesId))) invalid("Task occurrence references a missing series");
  for (const row of snapshot.tables.rm_task_notifications) {
    if (!taskIds.has(String(row.task_id))) invalid("Notification references a missing task");
    if (!taskVersions.get(String(row.task_id))?.some((task) => task.reminders.some((reminder) => reminder.reminderId === row.reminder_id && reminder.recipientId === row.recipient_id))) invalid("Notification references a missing reminder choice");
  }
  return snapshot;
}
export function exportTaskSnapshot(db: Database, companySlugs?: ReadonlySet<string>): TaskSnapshotV1 | null {
  return db.transaction(() => {
    const tables = Object.fromEntries(TABLES.map((table) => [table, db.query(`SELECT ${COLUMNS[table].join(",")} FROM ${table} ORDER BY ${table === "rm_task_receipts" ? "principal,idempotency_key" : table === "rm_task_notifications" ? "notification_id" : "id"}`).all()])) as Record<Table, Row[]>;
    if (!TABLES.some((table) => tables[table].length)) return null;
    return parseTaskSnapshot(JSON.stringify({ version: 1, tables }), companySlugs);
  })();
}
function restoredIdentities<T extends { assignee?: Task["assignee"]; reminders?: Task["reminders"] }>(value: T, users: ReadonlySet<string>): { value: T; changed: boolean } {
  const copy = structuredClone(value); let changed = false;
  if (copy.assignee?.kind === "member" && !users.has(copy.assignee.userId)) { copy.assignee = null; changed = true; }
  if (copy.reminders) copy.reminders = copy.reminders.map((reminder) => {
    if (reminder.enabled && reminder.recipientId !== "local" && !users.has(reminder.recipientId)) { changed = true; return { ...reminder, enabled: false }; }
    return reminder;
  });
  return { value: copy, changed };
}
const RESTORE_REASON = "Credential-free restore: missing member assignments cleared and reminders to missing recipients paused; restore access and choose recipients before reactivation.";
export function restoreTaskSnapshot(db: Database, input: TaskSnapshotV1, companySlugs?: ReadonlySet<string>, now = new Date().toISOString()): void {
  const snapshot = parseTaskSnapshot(JSON.stringify(input), companySlugs), at = taskInstant(now);
  db.transaction(() => {
    for (const table of TABLES) if (db.query(`SELECT 1 FROM ${table} LIMIT 1`).get()) invalid("Task restore requires an empty task store");
    for (const table of TABLES) {
      const columns = COLUMNS[table], insert = db.query(`INSERT INTO ${table}(${columns.join(",")}) VALUES(${columns.map(() => "?").join(",")})`);
      for (const row of snapshot.tables[table]) insert.run(...columns.map((column) => row[column]!));
    }
    const users = new Set((db.query('SELECT id FROM "user"').all() as { id: string }[]).map((user) => user.id));
    const ctx: TaskMutationContext = { actor: "system:workspace-restore", principal: "workspace-restore", idempotencyKey: "restore-identity-recovery", now: at };
    for (const current of listTasks(db, { showDone: true })) {
      const restored = restoredIdentities(current, users);
      if (restored.changed) appendTaskEvent(db, { ...restored.value, version: current.version + 1, updatedAt: at }, "restore_identity_reset", ctx, RESTORE_REASON);
    }
    const currentSeries = db.query("SELECT payload_json FROM rm_current_task_series").all() as { payload_json: string }[];
    for (const row of currentSeries) {
      const current = JSON.parse(row.payload_json) as TaskSeries, restored = restoredIdentities(current.template, users);
      if (!restored.changed) continue;
      const updated = { ...current, template: restored.value, version: current.version + 1, updatedAt: at };
      db.query("INSERT INTO rm_task_series_events(series_id,version,payload_json,actor,principal,created_at) VALUES(?,?,?,?,?,?)").run(updated.seriesId, updated.version, canonicalJson(updated), ctx.actor, ctx.principal, at);
      insertWorkspaceAudit(db, { eventType: "task_series_identity_reset_missing_members", entityType: "task_series", entityId: current.seriesId, createdBy: ctx.actor, createdByProgram: "workspace-restore" });
    }
    // A restored archive never restores a lease, successful tick or active delivery claim.
    db.query("UPDATE rm_task_runtime SET last_tick_at=NULL,last_error=NULL,lease_owner=NULL,lease_until=NULL WHERE id=1").run();
  }).immediate();
}
