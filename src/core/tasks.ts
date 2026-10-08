import type { Database } from "bun:sqlite";
import { createHash, randomUUID } from "node:crypto";
import { canonicalJson } from "./canonical-json";
import type { Task, TaskBoard, TaskCompletionInput, TaskDraft, TaskEvent, TaskMutationContext, TaskPatch, TaskQuery, TaskReference, TaskScope, TaskSourceCheck, TaskStatus } from "./tasks-types";

export class TaskError extends Error {
  constructor(public readonly code: string, message: string) { super(message); this.name = "TaskError"; }
}
export const TASK_STATUSES: TaskStatus[] = ["open", "in_progress", "waiting", "done"];
const SLUG = /^[a-z0-9][a-z0-9-]{0,119}$/;
const ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/;
const ACTOR = /^(user|agent|system):[A-Za-z0-9][A-Za-z0-9._:@-]{0,149}$/;
function fail(message: string): never { throw new TaskError("invalid_input", message); }
export function taskText(value: unknown, field: string, maximum: number, required = false): string {
  if (typeof value !== "string" || value.length > maximum || /[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]/.test(value)) fail(`Invalid ${field}`);
  if (required && !value.trim()) fail(`${field} is required`);
  return value.trim();
}
export function taskId(value: unknown, field = "id"): string {
  if (typeof value !== "string" || !ID.test(value)) fail(`Invalid ${field}`);
  return value;
}
export function taskDate(value: unknown, field = "date"): string {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(value) || !Number.isFinite(Date.parse(value)) || new Date(value).toISOString().slice(0, 10) !== value) fail(`Invalid ${field}`);
  return value;
}
export function taskInstant(value: unknown): string {
  if (typeof value !== "string" || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?Z$/.test(value) || !Number.isFinite(Date.parse(value))) fail("Invalid timestamp");
  taskDate(value.slice(0, 10));
  return new Date(value).toISOString();
}
export function validateTaskScope(value: TaskScope): TaskScope {
  if (!value || typeof value !== "object") fail("Explicit task scope is required");
  if (value.kind === "company") {
    if (typeof value.companySlug !== "string" || !SLUG.test(value.companySlug)) fail("Invalid company scope");
    return { kind: "company", companySlug: value.companySlug };
  }
  if (value.kind === "workspace" && Array.isArray(value.companySlugs) && value.companySlugs.length <= 100 && value.companySlugs.every((slug) => typeof slug === "string" && SLUG.test(slug))) {
    return { kind: "workspace", companySlugs: [...new Set(value.companySlugs)].sort() };
  }
  return fail("Invalid task scope");
}
export function taskScopeCompanies(scope: TaskScope): string[] { return scope.kind === "company" ? [scope.companySlug] : scope.companySlugs; }
export function validateTaskReferences(value: unknown, scope: TaskScope): TaskReference[] {
  if (!Array.isArray(value) || value.length > 100) fail("Invalid references");
  return value.map((reference: TaskReference) => {
    if (!reference || !["document", "bank_transaction", "period", "approval", "knowledge", "external_receipt", "party"].includes(reference.kind)) fail("Invalid reference kind");
    if (typeof reference.ref !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,239}$/.test(reference.ref) || reference.ref.includes("..") || reference.ref.includes("://")) fail("References must be opaque product references, not paths or URLs");
    if (reference.companySlug != null && (typeof reference.companySlug !== "string" || !SLUG.test(reference.companySlug) || !taskScopeCompanies(scope).includes(reference.companySlug))) fail("Reference company is outside the task scope");
    return { kind: reference.kind, ref: reference.ref, ...(reference.companySlug ? { companySlug: reference.companySlug } : {}) };
  });
}
function cleanObject(value: unknown): unknown {
  try {
    const json = JSON.stringify(value);
    if (!json || json.length > 1048576) fail("Task payload must be bounded JSON");
    return JSON.parse(json);
  } catch (error) {
    if (error instanceof TaskError) throw error;
    return fail("Task payload must be bounded JSON");
  }
}
export function taskPayloadHash(value: unknown): string { return createHash("sha256").update(canonicalJson(cleanObject(value))).digest("hex"); }
/** Holds the SQLite writer lock across the receipt lookup, effect and receipt insert. */
export function executeTaskMutation<T>(db: Database, operation: string, payload: unknown, ctx: TaskMutationContext, apply: (now: string) => T): T {
  if (!ctx || !ACTOR.test(ctx.actor)) fail("Actor is required");
  taskText(ctx.principal, "principal", 160, true);
  taskId(ctx.idempotencyKey, "idempotencyKey");
  taskText(operation, "operation", 80, true);
  const payloadHash = taskPayloadHash({ payload, expectedVersion: ctx.expectedVersion ?? null });
  const now = taskInstant(ctx.now ?? new Date().toISOString());
  return db.transaction(() => {
    (ctx as TaskMutationContext & { authorize?: () => void }).authorize?.();
    const prior = db.query("SELECT operation,payload_hash,result_json FROM rm_task_receipts WHERE principal=? AND idempotency_key=?").get(ctx.principal, ctx.idempotencyKey) as { operation: string; payload_hash: string; result_json: string } | null;
    if (prior) {
      if (prior.operation !== operation || prior.payload_hash !== payloadHash) throw new TaskError("idempotency_conflict", "Idempotency key was already used for another request");
      return JSON.parse(prior.result_json) as T;
    }
    const result = apply(now);
    const json = canonicalJson(cleanObject(result));
    if (json.length > 2_000_000) fail("Mutation result is too large");
    db.query("INSERT INTO rm_task_receipts(principal,idempotency_key,operation,payload_hash,result_json,created_at) VALUES(?,?,?,?,?,?)").run(ctx.principal, ctx.idempotencyKey, operation, payloadHash, json, now);
    return result;
  }).immediate();
}
export function assertTaskVersion(task: { version: number }, ctx: TaskMutationContext): void {
  if (!Number.isInteger(ctx.expectedVersion) || ctx.expectedVersion! < 1) throw new TaskError("version_required", "Expected version is required");
  if (ctx.expectedVersion !== task.version) throw new TaskError("version_conflict", "Task changed; reload its current version before applying your edits");
}
export function appendTaskEvent(db: Database, task: Task, operation: string, ctx: TaskMutationContext, reason?: string): void {
  db.query("INSERT INTO rm_task_events(task_id,version,operation,payload_json,actor,principal,created_at,reason) VALUES(?,?,?,?,?,?,?,?)").run(task.taskId, task.version, operation, canonicalJson(task), ctx.actor, ctx.principal, task.updatedAt, reason ? taskText(reason, "event reason", 2000, true) : null);
}
export function getTask(db: Database, id: string): Task | null {
  taskId(id, "taskId");
  const row = db.query("SELECT payload_json FROM rm_current_tasks WHERE task_id=?").get(id) as { payload_json: string } | null;
  return row ? JSON.parse(row.payload_json) as Task : null;
}
export function listTasks(db: Database, query: TaskQuery = {}): Task[] {
  if (query.search !== undefined) taskText(query.search, "search", 200);
  if (query.from) taskDate(query.from); if (query.to) taskDate(query.to);
  if (query.companySlugs && (!Array.isArray(query.companySlugs) || !query.companySlugs.every((slug) => typeof slug === "string" && SLUG.test(slug)))) fail("Invalid company filter");
  const rows = db.query("SELECT payload_json FROM rm_current_tasks ORDER BY created_at,task_id").all() as { payload_json: string }[];
  return rows.map((row) => JSON.parse(row.payload_json) as Task).filter((task) => {
    const companies = taskScopeCompanies(task.scope);
    if (query.companySlugs && (!companies.length || !companies.some((slug) => query.companySlugs!.includes(slug)))) return false;
    if (query.status ? task.status !== query.status : !query.showDone && task.status === "done") return false;
    if (query.type && task.type !== query.type) return false;
    if (query.assigneeId && (task.assignee?.kind !== "member" || task.assignee.userId !== query.assigneeId)) return false;
    if (query.undated && (task.workDate || task.deadline)) return false;
    if (query.unassigned && task.assignee) return false;
    if (query.search && !`${task.title} ${task.taskId} ${task.references.map((ref) => ref.ref).join(" ")}`.toLocaleLowerCase().includes(query.search.toLocaleLowerCase())) return false;
    const dates = [task.workDate, task.deadline?.date].filter((date): date is string => !!date);
    if ((query.from || query.to) && !dates.some((date) => (!query.from || date >= query.from) && (!query.to || date <= query.to))) return false;
    return true;
  });
}
export function taskHistory(db: Database, id: string): TaskEvent[] {
  taskId(id, "taskId");
  const rows = db.query("SELECT * FROM rm_task_events WHERE task_id=? ORDER BY version").all(id) as { task_id: string; version: number; operation: string; actor: string; principal: string; created_at: string; payload_json: string; reason: string | null }[];
  return rows.map((row) => ({ taskId: row.task_id, version: row.version, operation: row.operation, actor: row.actor, principal: row.principal, at: row.created_at, task: JSON.parse(row.payload_json), ...(row.reason ? { reason: row.reason } : {}) }));
}
function existing(db: Database, id: string, ctx: TaskMutationContext): Task {
  const task = getTask(db, id); if (!task) throw new TaskError("not_found", "Task is unavailable");
  assertTaskVersion(task, ctx); return task;
}
function status(value: unknown): TaskStatus { if (!TASK_STATUSES.includes(value as TaskStatus)) fail("Invalid task status"); return value as TaskStatus; }
function board(db: Database, scope: TaskScope): TaskBoard {
  const boardId = scope.kind === "company" ? `company:${scope.companySlug}` : "workspace";
  const row = db.query("SELECT payload_json FROM rm_current_task_boards WHERE board_id=?").get(boardId) as { payload_json: string } | null;
  return row ? JSON.parse(row.payload_json) : { boardId, version: 0, scope, updatedAt: "1970-01-01T00:00:00.000Z", columns: TASK_STATUSES.map((s) => ({ columnId: s, name: s, status: s, isDefault: true })) };
}
function placement(db: Database, scope: TaskScope, nextStatus: TaskStatus, requested?: string, ambiguous = false): string {
  const candidates = board(db, scope).columns.filter((column) => column.status === nextStatus);
  if (requested) {
    if (!candidates.some((column) => column.columnId === requested)) throw new TaskError("column_invalid", "Selected column does not match the task status");
    return requested;
  }
  if (ambiguous && candidates.length > 1) throw new TaskError("column_choice_required", "Choose the company's target column");
  const fallback = candidates.find((column) => column.isDefault);
  if (!fallback) throw new TaskError("column_invalid", "Board lacks a default column for this status");
  return fallback.columnId;
}
const PATCH_FIELDS = ["title", "description", "nextAction", "scope", "assignee", "waitingOn", "workDate", "deadline", "period", "references", "evidenceRequired", "relevance", "verificationRequired", "reminders"];
function validateFields(input: TaskPatch, scope: TaskScope): TaskPatch {
  if (!input || typeof input !== "object" || Array.isArray(input)) fail("Invalid task patch");
  const result: TaskPatch = {};
  for (const key of Object.keys(input)) if (!PATCH_FIELDS.includes(key)) fail(`Unsupported task field: ${key}`);
  if (input.title !== undefined) result.title = taskText(input.title, "title", 240, true);
  for (const key of ["description", "nextAction", "waitingOn"] as const) if (input[key] !== undefined) result[key] = taskText(input[key], key, key === "description" ? 8000 : 2000);
  if (input.scope !== undefined) result.scope = validateTaskScope(input.scope);
  if (input.workDate !== undefined) result.workDate = input.workDate === null ? null : taskDate(input.workDate, "workDate");
  if (input.deadline !== undefined) {
    const d = input.deadline;
    if (d === null) result.deadline = null;
    else {
      if (!d || !["statutory", "agreement", "internal"].includes(d.kind) || !["confirmed", "unconfirmed"].includes(d.certainty)) fail("Invalid deadline");
      result.deadline = { date: taskDate(d.date), kind: d.kind, certainty: d.certainty, basis: taskText(d.basis, "deadline basis", 2000, true), ...(d.ruleId ? { ruleId: taskId(d.ruleId, "ruleId") } : {}) };
    }
  }
  if (input.period !== undefined) {
    if (input.period === null) result.period = null;
    else {
      const from = taskDate(input.period?.from), to = taskDate(input.period?.to);
      if (from > to) fail("Period ends before its start");
      result.period = { from, to, label: taskText(input.period.label, "period label", 240, true) };
    }
  }
  if (input.references !== undefined) result.references = validateTaskReferences(input.references, scope);
  for (const key of ["evidenceRequired", "verificationRequired"] as const) if (input[key] !== undefined) {
    if (typeof input[key] !== "boolean") fail(`Invalid ${key}`); result[key] = input[key];
  }
  if (input.relevance !== undefined) {
    if (!["relevant", "unknown", "not_relevant"].includes(input.relevance)) fail("Invalid relevance"); result.relevance = input.relevance;
  }
  if (input.assignee !== undefined) {
    const a = input.assignee;
    if (a === null) result.assignee = null;
    else if (a?.kind === "member") result.assignee = { kind: "member", userId: taskId(a.userId, "userId"), name: taskText(a.name, "assignee name", 240, true) };
    else if (a?.kind === "external") result.assignee = { kind: "external", name: taskText(a.name, "assignee name", 240, true) };
    else fail("Invalid assignee");
  }
  if (input.reminders !== undefined) {
    if (!Array.isArray(input.reminders) || input.reminders.length > 20) fail("Invalid reminders");
    const seen = new Set<string>();
    result.reminders = input.reminders.map((r) => {
      if (!r || typeof r !== "object") fail("Invalid reminder");
      const reminderId = taskId(r.reminderId, "reminderId");
      if (seen.has(reminderId)) fail("Duplicate reminder id"); seen.add(reminderId);
      if (typeof r.enabled !== "boolean" || !["deadline", "follow_up"].includes(r.kind) || !["once", "daily"].includes(r.frequency)) fail("Invalid reminder");
      try { new Intl.DateTimeFormat("en", { timeZone: r.timeZone }).format(); } catch { fail("Invalid reminder time zone"); }
      if (r.kind === "deadline" && (!Number.isInteger(r.daysBefore) || r.daysBefore! < 0 || r.daysBefore! > 366)) fail("Invalid reminder daysBefore");
      if (r.kind === "follow_up") taskDate(r.followUpDate, "followUpDate");
      return { reminderId, recipientId: taskId(r.recipientId, "recipientId"), enabled: r.enabled, kind: r.kind, frequency: r.frequency, timeZone: taskText(r.timeZone, "timeZone", 80, true), ...(r.kind === "deadline" ? { daysBefore: r.daysBefore } : { followUpDate: r.followUpDate }) };
    });
  }
  return result;
}
export function validateTaskDraft(input: TaskDraft): TaskDraft {
  if (!input || typeof input !== "object") fail("Invalid task draft");
  const extra = ["taskId", "type", "origin", "status", "columnId", "workspaceColumnId", "source", "seriesId", "occurrenceKey"];
  for (const key of Object.keys(input)) if (!PATCH_FIELDS.includes(key) && !extra.includes(key)) fail(`Unsupported task field: ${key}`);
  const scope = validateTaskScope(input.scope);
  const fields = Object.fromEntries(Object.entries(input).filter(([key]) => PATCH_FIELDS.includes(key))) as TaskPatch;
  const result = { ...validateFields(fields, scope), title: taskText(input.title, "title", 240, true), scope } as TaskDraft;
  if (input.taskId !== undefined) result.taskId = taskId(input.taskId, "taskId");
  if (input.type !== undefined) { if (!["ad_hoc", "routine", "obligation"].includes(input.type)) fail("Invalid task type"); result.type = input.type; }
  if (input.origin !== undefined) { if (!["manual", "system", "proposal"].includes(input.origin)) fail("Invalid task origin"); result.origin = input.origin; }
  if (input.status !== undefined) { result.status = status(input.status); if (result.status === "done") fail("Use documented task completion"); }
  if (input.columnId !== undefined) result.columnId = taskId(input.columnId, "columnId");
  if (input.workspaceColumnId !== undefined) result.workspaceColumnId = input.workspaceColumnId === null ? null : taskId(input.workspaceColumnId, "workspaceColumnId");
  if (input.source !== undefined && input.source !== null) {
    const s = input.source;
    if (typeof s.companySlug !== "string" || !SLUG.test(s.companySlug) || !taskScopeCompanies(scope).includes(s.companySlug) || !["open", "resolved", "unknown"].includes(s.state)) fail("Invalid source scope or state");
    if (typeof s.ref !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,239}$/.test(s.ref) || s.ref.includes("..") || s.ref.includes("://")) fail("Invalid source reference");
    if (typeof s.href !== "string" || !s.href.startsWith(`/companies/${s.companySlug}/`) || s.href.includes("..") || !/^\/[A-Za-z0-9/._:-]+$/.test(s.href)) fail("Source requires a safe company product route");
    if (typeof s.contentHash !== "string" || !/^[a-f0-9]{64}$/.test(s.contentHash)) fail("Invalid source content hash");
    result.source = { kind: taskId(s.kind, "source kind"), identity: taskId(s.identity, "source identity"), companySlug: s.companySlug, ref: taskText(s.ref, "source ref", 240, true), state: s.state, contentHash: s.contentHash, href: s.href };
  }
  if (input.seriesId != null) result.seriesId = taskId(input.seriesId, "seriesId");
  if (input.occurrenceKey != null) result.occurrenceKey = taskId(input.occurrenceKey, "occurrenceKey");
  if (!!result.seriesId !== !!result.occurrenceKey) fail("Series and occurrence references must be provided together");
  return result;
}
function makeTask(db: Database, input: TaskDraft, now: string): Task {
  const value = validateTaskDraft(input), nextStatus = value.status ?? "open";
  if (value.relevance === "not_relevant") fail("Record non-applicability with a reason through completion");
  const task: Task = { taskId: value.taskId ?? `task-${randomUUID()}`, version: 1, title: value.title, description: value.description ?? "", nextAction: value.nextAction ?? "", scope: value.scope, type: value.type ?? "ad_hoc", origin: value.origin ?? "manual", status: nextStatus, columnId: placement(db, value.scope, nextStatus, value.columnId), workspaceColumnId: value.workspaceColumnId ? placement(db, { kind: "workspace", companySlugs: [] }, nextStatus, value.workspaceColumnId) : null, assignee: value.assignee ?? null, waitingOn: value.waitingOn ?? "", workDate: value.workDate ?? null, deadline: value.deadline ?? null, period: value.period ?? null, references: value.references ?? [], source: value.source ?? null, evidenceRequired: value.evidenceRequired ?? false, relevance: value.relevance ?? "relevant", verificationRequired: value.verificationRequired ?? false, completion: null, seriesId: value.seriesId ?? null, occurrenceKey: value.occurrenceKey ?? null, reminders: value.reminders ?? [], createdAt: now, updatedAt: now };
  if (task.source?.state === "unknown" || task.relevance === "unknown") task.verificationRequired = true;
  return task;
}
export function createTask(db: Database, input: TaskDraft, ctx: TaskMutationContext): Task {
  return executeTaskMutation(db, "task-create", input, ctx, (now) => {
    const task = makeTask(db, input, now);
    if (getTask(db, task.taskId)) throw new TaskError("task_exists", "Task reference already exists");
    appendTaskEvent(db, task, "created", ctx); return task;
  });
}
export function updateTask(db: Database, id: string, patch: TaskPatch, ctx: TaskMutationContext): Task {
  return executeTaskMutation(db, "task-update", { id, patch }, ctx, (now) => {
    if (!patch || typeof patch !== "object" || Array.isArray(patch)) fail("Invalid task patch");
    const prior = existing(db, id, ctx), scope = patch.scope ? validateTaskScope(patch.scope) : prior.scope;
    const fields = validateFields(patch, scope);
    const task = { ...prior, ...fields, version: prior.version + 1, updatedAt: now };
    if (prior.origin === "proposal" && patch.relevance === "relevant") task.origin = "system";
    validateTaskReferences(task.references, scope);
    if (task.source && !taskScopeCompanies(scope).includes(task.source.companySlug)) fail("Source company is outside the new scope");
    if (task.source && patch.evidenceRequired === false && prior.evidenceRequired) fail("Source evidence requirement cannot be removed");
    if (patch.relevance === "not_relevant" && prior.relevance !== "not_relevant") fail("Record non-applicability with a reason through completion");
    if (prior.source?.state === "unknown" && patch.verificationRequired === false) fail("Source must be verified first");
    if (prior.deadline?.kind === "statutory" && canonicalJson(prior.deadline) !== canonicalJson(task.deadline) && (!task.deadline || task.deadline.basis === prior.deadline.basis)) fail("Changing a statutory deadline requires a new visible basis");
    if (canonicalJson(prior.scope) !== canonicalJson(scope)) task.columnId = placement(db, scope, task.status);
    if (task.source?.state === "unknown" || task.relevance === "unknown") task.verificationRequired = true;
    appendTaskEvent(db, task, "updated", ctx); return task;
  });
}
export function moveTask(db: Database, id: string, input: { status: TaskStatus; columnId?: string; workspaceColumnId?: string }, ctx: TaskMutationContext): Task {
  return executeTaskMutation(db, "task-move", { id, input }, ctx, (now) => {
    if (!input || typeof input !== "object" || Array.isArray(input)) fail("Invalid task movement");
    for (const key of Object.keys(input)) if (!["status", "columnId", "workspaceColumnId"].includes(key)) fail(`Unsupported move field: ${key}`);
    const prior = existing(db, id, ctx), nextStatus = status(input.status);
    if (nextStatus === "done" && prior.status !== "done") throw new TaskError("completion_required", "Use documented completion before moving to done");
    if (prior.status === "done" && nextStatus !== "done") throw new TaskError("reopen_required", "Reopen with a reason before moving a completed task");
    const task = { ...prior, status: nextStatus, columnId: nextStatus === prior.status && !input.columnId ? prior.columnId : placement(db, prior.scope, nextStatus, input.columnId, prior.scope.kind === "company"), workspaceColumnId: input.workspaceColumnId ? placement(db, { kind: "workspace", companySlugs: [] }, nextStatus, input.workspaceColumnId) : prior.workspaceColumnId && nextStatus !== prior.status ? placement(db, { kind: "workspace", companySlugs: [] }, nextStatus) : prior.workspaceColumnId, version: prior.version + 1, updatedAt: now };
    appendTaskEvent(db, task, "moved", ctx); return task;
  });
}
function validateSourceCheck(check: TaskSourceCheck | undefined, now: string): TaskSourceCheck | undefined {
  if (!check) return undefined;
  if (!["open", "resolved", "unknown"].includes(check.state)) fail("Invalid source check");
  const observed = taskInstant(check.observedAt), age = Date.parse(now) - Date.parse(observed);
  if (age < -5000 || age > 60000) throw new TaskError("source_check_stale", "Source must be freshly checked before completion");
  return check;
}
export function completeTask(db: Database, id: string, input: TaskCompletionInput, ctx: TaskMutationContext): Task {
  return executeTaskMutation(db, "task-complete", { id, input }, ctx, (now) => {
    const prior = existing(db, id, ctx);
    if (prior.status === "done") throw new TaskError("already_completed", "Task is already complete");
    if (!input || typeof input !== "object" || Array.isArray(input)) fail("Invalid task completion");
    for (const key of Object.keys(input)) if (!["outcome", "note", "references"].includes(key)) fail(`Unsupported completion field: ${key}`);
    if (!["completed", "not_relevant", "cancelled", "exception"].includes(input.outcome)) fail("Invalid completion outcome");
    const note = taskText(input.note, "completion note", 8000, true);
    let references = validateTaskReferences(input.references ?? [], prior.scope);
    const check = validateSourceCheck(ctx.sourceCheck, now);
    if (prior.source && input.outcome !== "exception" && input.outcome !== "cancelled" && check?.state !== "resolved") throw new TaskError("source_unresolved", "Authoritative source has not confirmed completion");
    if (input.outcome === "completed" && prior.relevance === "unknown") throw new TaskError("relevance_unknown", "Task relevance must be clarified before completion");
    if (input.outcome === "completed" && prior.verificationRequired && !prior.source) throw new TaskError("verification_required", "External result must be verified before completion");
    const sourceEvidence = check?.state === "resolved" ? validateTaskReferences(check.evidence ?? [], prior.scope) : [];
    references = [...references, ...sourceEvidence].filter((ref, i, all) => all.findIndex((other) => canonicalJson(ref) === canonicalJson(other)) === i);
    if (prior.evidenceRequired && input.outcome === "completed" && !references.length) throw new TaskError("evidence_required", "Completion evidence is required; record a reasoned exception instead");
    const assurance = input.outcome === "completed" && prior.source && check?.state === "resolved" && sourceEvidence.length ? "product_verified" as const : "user_reported" as const;
    const task: Task = { ...prior, status: "done", columnId: placement(db, prior.scope, "done"), workspaceColumnId: prior.workspaceColumnId ? placement(db, { kind: "workspace", companySlugs: [] }, "done") : null, completion: { outcome: input.outcome, note, references, assurance, at: now, actor: ctx.actor }, source: prior.source && check ? { ...prior.source, state: check.state } : prior.source, relevance: input.outcome === "not_relevant" ? "not_relevant" : prior.relevance, verificationRequired: input.outcome === "completed" && (!prior.source || check?.state === "resolved") ? false : prior.verificationRequired, version: prior.version + 1, updatedAt: now };
    appendTaskEvent(db, task, "completed", ctx); return task;
  });
}
export function reopenTask(db: Database, id: string, reason: string, ctx: TaskMutationContext): Task {
  return executeTaskMutation(db, "task-reopen", { id, reason }, ctx, (now) => {
    const prior = existing(db, id, ctx); taskText(reason, "reopen reason", 2000, true);
    if (prior.status !== "done") throw new TaskError("not_completed", "Task is not complete");
    const task: Task = { ...prior, status: "open", columnId: placement(db, prior.scope, "open"), workspaceColumnId: prior.workspaceColumnId ? placement(db, { kind: "workspace", companySlugs: [] }, "open") : null, completion: null, relevance: prior.relevance === "not_relevant" ? "unknown" : prior.relevance, version: prior.version + 1, updatedAt: now };
    if (task.relevance === "unknown") task.verificationRequired = true;
    appendTaskEvent(db, task, "reopened", ctx, reason); return task;
  });
}
export function syncSourceTask(db: Database, input: TaskDraft, ctx: TaskMutationContext): Task {
  return executeTaskMutation(db, "task-source-sync", input, ctx, (now) => {
    const value = validateTaskDraft(input);
    if (!value.source) fail("Source synchronization requires a source");
    const source = value.source;
    const row = db.query("SELECT payload_json FROM rm_current_tasks WHERE json_extract(payload_json,'$.source.companySlug')=? AND json_extract(payload_json,'$.source.kind')=? AND json_extract(payload_json,'$.source.identity')=?").get(source.companySlug, source.kind, source.identity) as { payload_json: string } | null;
    if (!row) {
      const task = makeTask(db, { ...value, origin: value.origin === "proposal" ? "proposal" : "system", taskId: value.taskId ?? `source-${taskPayloadHash([source.companySlug, source.kind, source.identity]).slice(0, 32)}` }, now);
      if (getTask(db, task.taskId)) throw new TaskError("task_exists", "Task reference already exists");
      appendTaskEvent(db, task, "source_created", ctx); return task;
    }
    const prior = JSON.parse(row.payload_json) as Task;
    if (canonicalJson(prior.source) === canonicalJson(source)) return prior;
    const task: Task = { ...prior, source, verificationRequired: source.state === "unknown" || prior.relevance === "unknown", version: prior.version + 1, updatedAt: now };
    if (source.state !== "resolved" && prior.status === "done") {
      task.status = "open"; task.completion = null; task.columnId = placement(db, prior.scope, "open");
      task.workspaceColumnId = prior.workspaceColumnId ? placement(db, { kind: "workspace", companySlugs: [] }, "open") : null;
    }
    appendTaskEvent(db, task, source.state === "unknown" ? "source_verification_required" : prior.status === "done" && task.status === "open" ? "source_reopened" : "source_updated", ctx);
    return task;
  });
}
