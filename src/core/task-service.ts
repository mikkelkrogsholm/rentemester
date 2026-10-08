/** Shared application boundary for HTTP, CLI and MCP. No accounting writes. */
import type { Database } from "bun:sqlite";
import { checkActorAllowlist, isCanonicalActorId, loadActorAllowlist } from "../cli-actor";
import { openWorkspaceControlDb, openWorkspaceControlReadOnlyDb } from "./workspace-control";
import { companyRootForSlug, isValidSlug, listWorkspaceCompanies } from "./workspace";
import { authorizeWorkspaceRoute } from "./workspace-access";
import { canAccessTaskScope, canReadTask, taskAccess, taskReferencesWithinScope, taskScopeCompanies, type TaskAccess } from "./tasks-access";
import { createTask, updateTask, moveTask, completeTask, reopenTask, getTask, listTasks, taskHistory, executeTaskMutation, TaskError, taskId } from "./tasks";
import { defaultTaskBoard, getTaskBoard, listTaskBoards, previewTaskBoard, saveTaskBoard } from "./task-boards";
import { getTaskSeries, listTaskSeries, saveTaskSeries, projectTaskSeries, materializeTaskSeries } from "./task-series";
import { SOURCE_COVERAGE, checkTaskSource, syncTaskSources } from "./task-sources";
import { configureTaskReminder, listTaskNotifications } from "./task-reminders";
import { taskRuntimeStatus, maintainTaskDb } from "./task-runtime";
import type { Task, TaskBoardDraft, TaskCompletionInput, TaskDraft, TaskIdentity, TaskMutationContext, TaskOperation, TaskPatch, TaskQuery, TaskReference, TaskReminder, TaskScope, TaskSeriesDraft, TaskStatus, TasksView } from "./tasks-types";

export const TASK_READ_OPERATIONS = new Set<TaskOperation>(["list", "get", "history", "series-list", "series-project", "boards-list", "boards-preview", "notifications", "runtime-status"]);

function scope(value: unknown): TaskScope {
  if (!value || typeof value !== "object") throw new TaskError("INVALID_INPUT", "Vælg et tydeligt opgavescope.");
  const input = value as Record<string, unknown>;
  if (input.kind === "company" && typeof input.companySlug === "string" && isValidSlug(input.companySlug)) return { kind: "company", companySlug: input.companySlug };
  if (input.kind === "workspace" && Array.isArray(input.companySlugs) && input.companySlugs.length <= 100 && input.companySlugs.every(slug => typeof slug === "string" && isValidSlug(slug))) {
    return { kind: "workspace", companySlugs: [...new Set(input.companySlugs as string[])].sort() };
  }
  throw new TaskError("INVALID_INPUT", "Opgavens scope er ugyldigt.");
}

function assertScope(access: TaskAccess, value: TaskScope, mode: "read" | "write" | "manage"): void {
  if (!canAccessTaskScope(access, value, mode)) throw new TaskError("ACCESS_DENIED", "Handlingen er ikke tilgængelig med din adgang.");
}

function currentTask(db: Database, taskId: unknown, access: TaskAccess, mode: "read" | "write" = "read"): Task {
  if (typeof taskId !== "string") throw new TaskError("INVALID_INPUT", "Opgavereference mangler.");
  const task = getTask(db, taskId);
  if (!task || !canReadTask(access, task)) throw new TaskError("NOT_FOUND", "Opgaven findes ikke eller er ikke tilgængelig.");
  assertScope(access, task.scope, mode);
  return task;
}

function assertActor(workspaceRoot: string, identity: TaskIdentity, target: TaskScope): void {
  const actor = identity.actor;
  if (!actor || !isCanonicalActorId(actor)) throw new TaskError("ACTOR_REQUIRED", "En kanonisk actor kræves for ændringer.");
  if (identity.enforceActorPolicy === false) return; // Browser actor is derived from its authenticated principal.
  const companies = taskScopeCompanies(target);
  if (companies.length) {
    for (const slug of companies) {
      if (!checkActorAllowlist(companyRootForSlug(workspaceRoot, slug), actor).allowed) throw new TaskError("ACTOR_DENIED", "Actor er ikke godkendt i selskabets lokale policy.");
    }
  } else {
    // A workspace may supply its own policy. Existing local company policy is
    // a compatible fallback for single-owner filesystem CLI installations.
    const ownPolicy = loadActorAllowlist(workspaceRoot);
    const roots = ownPolicy.size ? [workspaceRoot] : listWorkspaceCompanies(workspaceRoot).map(company => companyRootForSlug(workspaceRoot, company.slug));
    if (!roots.some(root => checkActorAllowlist(root, actor).allowed)) throw new TaskError("ACTOR_DENIED", "Tilføj actor til workspace- eller selskabspolicy før workspaceændringer.");
  }
}

function context(input: Record<string, unknown>, identity: TaskIdentity): TaskMutationContext {
  return { actor: identity.actor ?? "", principal: identity.principal, idempotencyKey: input.idempotencyKey as string, expectedVersion: input.expectedVersion as number | undefined };
}

function assertReferences(target: TaskScope, references: TaskReference[] | undefined): void {
  if (!Array.isArray(references ?? [])) throw new TaskError("INVALID_INPUT", "Referencer skal være en liste.");
  if (!taskReferencesWithinScope({ scope: target, references: references ?? [], source: null })) throw new TaskError("INVALID_REFERENCE", "Referencer skal høre til opgavens synlige scope.");
}

function assertAssignee(db: Database, workspaceRoot: string, target: TaskScope, assignee: Task["assignee"] | undefined, identity: TaskIdentity): void {
  if (!assignee || assignee.kind === "external") return;
  if (assignee.kind !== "member" || typeof assignee.userId !== "string") throw new TaskError("INVALID_INPUT", "Den ansvarlige er ugyldig.");
  if (assignee.userId === "local" && identity.local) return;
  const access = taskAccess(db, workspaceRoot, { principal: `user:${assignee.userId}`, userId: assignee.userId, local: false });
  if (!canAccessTaskScope(access, target, "read")) throw new TaskError("INVALID_ASSIGNEE", "Den ansvarlige har ikke adgang til opgaven. Brug en ekstern ansvarlig uden konto.");
}

const draftFields = new Set(["title", "description", "nextAction", "scope", "type", "assignee", "waitingOn", "workDate", "deadline", "period", "references", "evidenceRequired", "relevance", "verificationRequired"]);
function manualDraft(input: Record<string, unknown>): TaskDraft {
  const draft: Record<string, unknown> = {};
  for (const key of draftFields) if (key in input) draft[key] = input[key];
  for (const key of ["source", "completion", "status", "origin", "seriesId", "occurrenceKey", "reminders", "taskId", "columnId", "workspaceColumnId"]) {
    if (key in input) throw new TaskError("INVALID_INPUT", "Systemtilstand og afslutning ændres gennem særskilte handlinger.");
  }
  return { ...draft, scope: scope(input.scope), title: input.title as string, origin: "manual" } as TaskDraft;
}

function companySelection(input: Record<string, unknown>, access: TaskAccess, mode: "read" | "write" | "manage"): string[] {
  if (input.companySlugs === undefined) return [...access[mode]];
  if (!Array.isArray(input.companySlugs) || input.companySlugs.length > 100 || input.companySlugs.some(slug => typeof slug !== "string" || !access[mode].has(slug))) throw new TaskError("ACCESS_DENIED", "Selskabsvalget er ikke tilgængeligt.");
  return [...new Set(input.companySlugs as string[])];
}

function visibleBoards(db: Database, access: TaskAccess) {
  const stored = listTaskBoards(db).filter(board => board.boardId === "workspace" || canAccessTaskScope(access, board.scope, "read"));
  const defaults = [...access.read].map(companySlug => defaultTaskBoard({ kind: "company", companySlug }));
  // Board configuration contains only workspace-wide workflow vocabulary.
  defaults.push(defaultTaskBoard({ kind: "workspace", companySlugs: [] }));
  return defaults.map(board => stored.find(saved => saved.boardId === board.boardId) ?? board);
}

function authorizedBoardPreview(db: Database, workspaceRoot: string, board: TaskBoardDraft, access: TaskAccess) {
  const prior = getTaskBoard(db, board.boardId) ?? defaultTaskBoard(board.scope);
  const structural = prior.columns.some(column => !board.columns?.some(next => next.columnId === column.columnId && next.status === column.status));
  // Structural changes can touch any company represented by the aggregate
  // board. Check membership independently of whether private tasks exist.
  if (structural && board.scope.kind === "workspace") {
    if (listWorkspaceCompanies(workspaceRoot).some(company => !access.write.has(company.slug))) throw new TaskError("ACCESS_DENIED", "Strukturændringen kræver adgang til alle selskaber på boardet.");
  }
  const preview = previewTaskBoard(db, board);
  for (const id of preview.affectedTaskIds) currentTask(db, id, access, "write");
  return preview;
}

function view(db: Database, workspaceRoot: string, input: Record<string, unknown>, identity: TaskIdentity, access: TaskAccess): TasksView {
  const selected = companySelection(input, access, "read");
  const companies = listWorkspaceCompanies(workspaceRoot).filter(company => access.read.has(company.slug));
  const archived = new Set(companies.filter(company => company.archived).map(company => company.slug));
  const query = { ...input, companySlugs: undefined, from: undefined, to: undefined } as TaskQuery;
  // Calendar ranges constrain projections. They never hide outstanding tasks
  // from an earlier period or an overdue statutory deadline.
  const tasks = listTasks(db, query).filter(task => canReadTask(access, task))
    .filter(task => !input.companySlugs || taskScopeCompanies(task.scope).some(slug => selected.includes(slug)))
    .filter(task => input.includeArchived === true || taskScopeCompanies(task.scope).every(slug => !archived.has(slug)));
  const series = listTaskSeries(db).filter(series => canAccessTaskScope(access, series.scope, "read"))
    .filter(series => !input.companySlugs || taskScopeCompanies(series.scope).some(slug => selected.includes(slug)))
    .filter(series => input.includeArchived === true || taskScopeCompanies(series.scope).every(slug => !archived.has(slug)));
  const year = new Date().getUTCFullYear();
  const from = typeof input.from === "string" ? input.from : `${year}-01-01`;
  const to = typeof input.to === "string" ? input.to : `${year}-12-31`;
  const projections = projectTaskSeries(db, { from, to, companySlugs: selected, includeWorkspace: access.owner }).filter(projection => canAccessTaskScope(access, projection.scope, "read"))
    .filter(projection => !input.companySlugs || taskScopeCompanies(projection.scope).some(slug => selected.includes(slug)))
    .filter(projection => input.includeArchived === true || taskScopeCompanies(projection.scope).every(slug => !archived.has(slug)));
  const currentUserId = identity.userId ?? "local";
  const notifications = listTaskNotifications(db, currentUserId).filter(notification => {
    const task = getTask(db, notification.taskId);
    return task && task.status !== "done" && canReadTask(access, task)
      && task.reminders.some(reminder => reminder.reminderId === notification.reminderId && reminder.enabled && reminder.recipientId === currentUserId);
  });
  const assignees = identity.local ? [{ userId: "local", name: "Dig" }] : (db.query('SELECT id, name FROM "user" ORDER BY name').all() as Array<{ id: string; name: string }>).filter(user => {
    const candidate = taskAccess(db, workspaceRoot, { principal: `user:${user.id}`, userId: user.id, local: false });
    return selected.length ? selected.every(slug => candidate.read.has(slug)) : candidate.owner && access.owner;
  }).map(user => ({ userId: user.id, name: user.name }));
  return { tasks, count: tasks.length, boards: visibleBoards(db, access), series, projections,
    companies: companies.map(company => ({ slug: company.slug, name: company.name, archived: company.archived, canWrite: access.write.has(company.slug), canManage: access.manage.has(company.slug) })),
    currentUserId, canManageWorkspace: access.owner, assignees, runtime: taskRuntimeStatus(db, workspaceRoot), notifications, sourceCoverage: [...SOURCE_COVERAGE] };
}

/** Resource authorization is checked before a retry receipt can be read. */
export async function executeTaskOperation(workspaceRoot: string, operation: TaskOperation, input: Record<string, unknown>, identity: TaskIdentity): Promise<Record<string, unknown>> {
  const readDb = openWorkspaceControlReadOnlyDb(workspaceRoot);
  let access: TaskAccess;
  let target: TaskScope | undefined;
  let existing: Task | undefined;
  try {
    access = taskAccess(readDb, workspaceRoot, identity);
    if (!identity.local && (!identity.userId || !authorizeIdentity(readDb, workspaceRoot, identity))) throw new TaskError("ACCESS_DENIED", "Adgang til opgaver kræves.");
    if (["get", "history", "update", "move", "complete", "reopen", "reminder-set"].includes(operation)) {
      existing = currentTask(readDb, input.taskId, access, TASK_READ_OPERATIONS.has(operation) ? "read" : "write");
      target = existing.scope;
    } else if (operation === "create") target = scope(input.scope);
    else if (operation === "series-save") {
      const series = input.series as TaskSeriesDraft;
      target = scope(series?.scope);
      const previous = typeof series?.seriesId === "string" ? getTaskSeries(readDb, series.seriesId) : null;
      if (previous) assertScope(access, previous.scope, "manage");
    } else if (operation === "boards-save" || operation === "boards-preview") {
      const board = input.board as TaskBoardDraft;
      target = scope(board?.scope);
      const previous = typeof board?.boardId === "string" ? getTaskBoard(readDb, board.boardId) : null;
      if (previous) assertScope(access, previous.scope, "manage");
    }
    if (target && !TASK_READ_OPERATIONS.has(operation)) {
      assertScope(access, target, operation === "boards-save" || operation === "series-save" ? "manage" : "write");
      assertActor(workspaceRoot, identity, target);
    }
    if (operation === "boards-preview" && target) assertScope(access, target, "manage");
    if (operation === "list") return view(readDb, workspaceRoot, input, identity, access) as unknown as Record<string, unknown>;
    if (operation === "get") return { task: existing, history: taskHistory(readDb, existing!.taskId).filter(event => canReadTask(access, event.task)) };
    if (operation === "history") return { history: taskHistory(readDb, existing!.taskId).filter(event => canReadTask(access, event.task)) };
    if (operation === "boards-list") return { boards: visibleBoards(readDb, access) };
    if (operation === "boards-preview") return { preview: authorizedBoardPreview(readDb, workspaceRoot, input.board as TaskBoardDraft, access) };
    if (operation === "series-list") return { series: view(readDb, workspaceRoot, input, identity, access).series };
    if (operation === "series-project") return { projections: view(readDb, workspaceRoot, input, identity, access).projections };
    if (operation === "notifications") return { notifications: view(readDb, workspaceRoot, {}, identity, access).notifications };
    if (operation === "runtime-status") return { runtime: taskRuntimeStatus(readDb, workspaceRoot) };
  } finally { readDb.close(); }
  const ctx = context(input, identity);
  const db = openWorkspaceControlDb(workspaceRoot);
  try {
    ctx.authorizeResult = value => {
      const live = taskAccess(db, workspaceRoot, identity);
      const result = value as Record<string, unknown>;
      if (result.includesWorkspace === true && !live.owner) throw new TaskError("ACCESS_DENIED", "Workspacekvitteringen er ikke længere tilgængelig.");
      const materialized = (result.materialized ?? (result.run as Record<string, unknown> | undefined)?.materialized) as { taskIds?: string[] } | undefined;
      for (const id of materialized?.taskIds ?? []) currentTask(db, id, live, "write");
      const recorded = (result.task ?? ("taskId" in result ? result : undefined)) as Task | undefined;
      if (recorded) {
        if (!canReadTask(live, recorded)) throw new TaskError("ACCESS_DENIED", "Kvitteringens scope er ikke længere tilgængeligt.");
        currentTask(db, recorded.taskId, live, "write");
      }
      const resource = (result.series ?? result.board ?? ("scope" in result ? result : undefined)) as { scope: TaskScope } | undefined;
      if (resource?.scope) assertScope(live, resource.scope, operation === "series-save" || operation === "boards-save" || operation === "series-materialize" ? "manage" : "write");
    };
    ctx.authorize = () => {
      const live = taskAccess(db, workspaceRoot, identity);
      if (!identity.local && !authorizeIdentity(db, workspaceRoot, identity)) throw new TaskError("ACCESS_DENIED", "Adgang til opgaver kræves.");
      if (existing) currentTask(db, existing.taskId, live, "write");
      if (target) { assertScope(live, target, operation === "boards-save" || operation === "series-save" ? "manage" : "write"); assertActor(workspaceRoot, identity, target); }
      if (operation === "update" && (input.patch as TaskPatch)?.scope) assertScope(live, scope((input.patch as TaskPatch).scope), "write");
      if (operation === "boards-save") authorizedBoardPreview(db, workspaceRoot, input.board as TaskBoardDraft, live);
      if (operation === "sync" || operation === "series-materialize" || operation === "run") {
        const selected = companySelection(input, live, operation === "series-materialize" ? "manage" : "write");
        for (const slug of selected) assertActor(workspaceRoot, identity, { kind: "company", companySlug: slug });
        if ((operation === "run" || operation === "series-materialize") && live.owner && input.companySlugs === undefined) assertActor(workspaceRoot, identity, { kind: "workspace", companySlugs: [] });
        if (operation === "run" && !live.owner) throw new TaskError("ACCESS_DENIED", "Runtime kræver workspaceejer.");
      }
    };
    // Recheck live membership after obtaining the write connection.
    access = taskAccess(db, workspaceRoot, identity);
    if (existing) { existing = currentTask(db, existing.taskId, access, "write"); target = existing.scope; }
    if (target) assertScope(access, target, operation === "boards-save" || operation === "series-save" ? "manage" : "write");
    if (operation === "create") {
      const draft = manualDraft(input);
      assertReferences(draft.scope, draft.references); assertAssignee(db, workspaceRoot, draft.scope, draft.assignee, identity);
      return { task: createTask(db, draft, ctx) };
    }
    if (operation === "update") {
      const patch = input.patch as TaskPatch;
      if (!patch || typeof patch !== "object" || Array.isArray(patch) || Object.keys(patch).some(key => !draftFields.has(key))) throw new TaskError("INVALID_INPUT", "Opdatering kræver almindelige opgavefelter i patch.");
      const nextScope = patch.scope ? scope(patch.scope) : existing!.scope;
      assertScope(access, nextScope, "write"); assertActor(workspaceRoot, identity, nextScope);
      if (existing!.source && JSON.stringify(nextScope) !== JSON.stringify(existing!.scope)) throw new TaskError("INVALID_SCOPE", "Kildestyret arbejde beholder sit selskab.");
      assertReferences(nextScope, patch.references ?? existing!.references);
      assertReferences(nextScope, existing!.completion?.references);
      assertAssignee(db, workspaceRoot, nextScope, patch.assignee === undefined ? existing!.assignee : patch.assignee, identity);
      return { task: updateTask(db, existing!.taskId, { ...patch, ...(patch.scope ? { scope: nextScope } : {}) }, ctx) };
    }
    if (operation === "move") return { task: moveTask(db, existing!.taskId, { status: input.status as TaskStatus, columnId: input.columnId as string | undefined, workspaceColumnId: input.workspaceColumnId as string | undefined }, ctx) };
    if (operation === "complete") {
      assertReferences(existing!.scope, input.references as TaskReference[] | undefined);
      const check = existing!.source ? await checkTaskSource(workspaceRoot, existing!) : undefined;
      return { task: completeTask(db, existing!.taskId, { outcome: input.outcome, note: input.note, references: input.references } as TaskCompletionInput, { ...ctx, sourceCheck: check }) };
    }
    if (operation === "reopen") return { task: reopenTask(db, existing!.taskId, input.reason as string, ctx) };
    if (operation === "reminder-set") {
      const reminder = input.reminder as TaskReminder;
      if (!reminder || (reminder.recipientId === "local" && !identity.local)) throw new TaskError("INVALID_REMINDER", "Påmindelsens modtager er ugyldig.");
      const recipient = reminder.recipientId === "local" ? access : taskAccess(db, workspaceRoot, { principal: `user:${reminder.recipientId}`, userId: reminder.recipientId, local: false });
      if (!canAccessTaskScope(recipient, existing!.scope, "read")) throw new TaskError("INVALID_REMINDER", "Modtageren har ikke adgang til opgaven.");
      return { task: configureTaskReminder(db, { taskId: existing!.taskId, reminder }, ctx) };
    }
    if (operation === "series-save") {
      const series = input.series as TaskSeriesDraft;
      const seriesScope = scope(series.scope);
      const { origin: _origin, ...templateFields } = series.template;
      const template = manualDraft(templateFields as Record<string, unknown>);
      if (JSON.stringify(template.scope) !== JSON.stringify(seriesScope)) throw new TaskError("INVALID_SCOPE", "Rutine og skabelon skal have samme scope.");
      assertReferences(seriesScope, template.references); assertAssignee(db, workspaceRoot, seriesScope, template.assignee, identity);
      return { series: saveTaskSeries(db, { ...series, scope: seriesScope, template }, ctx) };
    }
    if (operation === "boards-save") return { board: saveTaskBoard(db, input.board as TaskBoardDraft, ctx) };
    if (operation === "sync" || operation === "series-materialize" || operation === "run") {
      const companySlugs = companySelection(input, access, operation === "series-materialize" ? "manage" : "write");
      for (const companySlug of companySlugs) assertActor(workspaceRoot, identity, { kind: "company", companySlug });
      if (!companySlugs.length && !access.owner) throw new TaskError("ACCESS_DENIED", "Ingen skrivbare selskaber er valgt.");
      const asOfDate = typeof input.asOfDate === "string" ? input.asOfDate : new Date().toISOString().slice(0, 10);
      const resultScope: TaskScope = { kind: "workspace", companySlugs };
      const includesWorkspace = access.owner && input.companySlugs === undefined;
      if (operation === "sync") return executeTaskMutation(db, "sources-sync", input, ctx, () => ({ scope: resultScope, includesWorkspace: false, sync: syncTaskSources(db, workspaceRoot, { companySlugs, actor: ctx.actor, principal: ctx.principal, asOfDate }) }));
      if (includesWorkspace) assertActor(workspaceRoot, identity, { kind: "workspace", companySlugs: [] });
      if (operation === "series-materialize") return executeTaskMutation(db, "series-materialize", input, ctx, () => ({ scope: resultScope, includesWorkspace, materialized: materializeTaskSeries(db, { companySlugs, includeWorkspace: includesWorkspace, actor: ctx.actor, principal: ctx.principal, asOfDate }) }));
      if (!access.owner) throw new TaskError("ACCESS_DENIED", "Runtime kan kun køres af en workspaceejer.");
      return executeTaskMutation(db, "runtime-run", input, ctx, () => ({ scope: resultScope, includesWorkspace, run: maintainTaskDb(db, workspaceRoot, { now: new Date(`${asOfDate}T12:00:00Z`), identity, allowLocalRecipient: identity.local, companySlugs, includeWorkspace: includesWorkspace }) }));
    }
    throw new TaskError("INVALID_OPERATION", "Opgavehandlingen er ukendt.");
  } finally { db.close(); }
}

function authorizeIdentity(db: Database, workspaceRoot: string, identity: TaskIdentity): boolean {
  return Boolean(identity.userId) && authorizeWorkspaceRoute(db, workspaceRoot, { userId: identity.userId, permission: "workspace.tasks.read" }).allowed;
}

/** Receipt existence is private. Return only metadata after current resource access. */
export function readTaskOperationReceipt(workspaceRoot: string, key: string, identity: TaskIdentity): Record<string, unknown> {
  taskId(key, "idempotencyKey");
  const db = openWorkspaceControlReadOnlyDb(workspaceRoot);
  try {
    const access = taskAccess(db, workspaceRoot, identity);
    if (!identity.local && !authorizeIdentity(db, workspaceRoot, identity)) throw new TaskError("ACCESS_DENIED", "Adgang til opgaver kræves.");
    const row = db.query("SELECT operation,result_json FROM rm_task_receipts WHERE principal=? AND idempotency_key=?").get(identity.principal, key) as { operation: string; result_json: string } | null;
    if (!row) return { receipt: null };
    const value = JSON.parse(row.result_json) as Task | Record<string, unknown>;
    const result = value as Record<string, unknown>;
    if (result.includesWorkspace === true && !access.owner) return { receipt: null };
    const materialized = (result.materialized ?? (result.run as Record<string, unknown> | undefined)?.materialized) as { taskIds?: string[] } | undefined;
    for (const id of materialized?.taskIds ?? []) { const task = getTask(db, id); if (!task || !canReadTask(access, task)) return { receipt: null }; }
    const recordedTask = ("taskId" in value ? value : result.task) as Task | undefined;
    if (recordedTask?.taskId) {
      const current = getTask(db, recordedTask.taskId);
      if (!current || !canReadTask(access, recordedTask) || !canReadTask(access, current)) return { receipt: null };
      return { receipt: { operation: row.operation, taskId: current.taskId, version: recordedTask.version } };
    }
    const resource = (result.series ?? result.board ?? ("scope" in result ? result : undefined)) as { scope: TaskScope; seriesId?: string; boardId?: string; version: number } | undefined;
    if (resource?.scope) {
      const current = resource.seriesId ? getTaskSeries(db, resource.seriesId) : resource.boardId ? getTaskBoard(db, resource.boardId) : null;
      if ((!resource.seriesId && !resource.boardId) && canAccessTaskScope(access, resource.scope, "read")) return { receipt: { operation: row.operation } };
      if (!current || !canAccessTaskScope(access, resource.scope, "read") || !canAccessTaskScope(access, current.scope, "read")) return { receipt: null };
      return { receipt: { operation: row.operation, version: resource.version } };
    }
    // Bulk results do not contain enough scope to prove the caller's present
    // access. A missing confirmation must remain an uncertain outcome.
    return { receipt: null };
  } finally { db.close(); }
}
