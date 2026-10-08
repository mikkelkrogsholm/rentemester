/** Shared task contracts. Operational state belongs to the workspace, not a ledger. */
export type TaskStatus = "open" | "in_progress" | "waiting" | "done";
export type TaskScope = { kind: "company"; companySlug: string } | { kind: "workspace"; companySlugs: string[] };
export type TaskReference = { kind: "document" | "bank_transaction" | "period" | "approval" | "knowledge" | "external_receipt" | "party"; ref: string; companySlug?: string };
export type TaskAssignee = { kind: "member"; userId: string; name: string } | { kind: "external"; name: string };
export type TaskDeadline = { date: string; kind: "statutory" | "agreement" | "internal"; basis: string; certainty: "confirmed" | "unconfirmed"; ruleId?: string };
export type TaskPeriod = { from: string; to: string; label: string };
export type TaskSourceCheck = { state: "open" | "resolved" | "unknown"; observedAt: string; evidence?: TaskReference[]; reason?: string };
export type TaskSource = { kind: string; identity: string; companySlug: string; ref: string; state: "open" | "resolved" | "unknown"; contentHash: string; href: string };
export type TaskReminder = { reminderId: string; recipientId: string; enabled: boolean; kind: "deadline" | "follow_up"; daysBefore?: number; followUpDate?: string; frequency: "once" | "daily"; timeZone: string };
export type TaskCompletion = { outcome: "completed" | "not_relevant" | "cancelled" | "exception"; note: string; references: TaskReference[]; assurance: "user_reported" | "product_verified"; at: string; actor: string };
export type Task = {
  taskId: string; version: number; title: string; description: string; nextAction: string; scope: TaskScope;
  type: "ad_hoc" | "routine" | "obligation"; origin: "manual" | "system" | "proposal";
  status: TaskStatus; columnId: string; workspaceColumnId: string | null;
  assignee: TaskAssignee | null; waitingOn: string; workDate: string | null; deadline: TaskDeadline | null;
  period: TaskPeriod | null; references: TaskReference[]; source: TaskSource | null;
  evidenceRequired: boolean; relevance: "relevant" | "unknown" | "not_relevant"; verificationRequired: boolean;
  completion: TaskCompletion | null; seriesId: string | null; occurrenceKey: string | null;
  reminders: TaskReminder[]; createdAt: string; updatedAt: string;
};
export type TaskDraft = Pick<Task, "title" | "scope"> & Partial<Omit<Task, "taskId" | "version" | "createdAt" | "updatedAt" | "completion">> & { taskId?: string };
export type TaskPatch = Partial<Pick<Task, "title" | "description" | "nextAction" | "scope" | "assignee" | "waitingOn" | "workDate" | "deadline" | "period" | "references" | "evidenceRequired" | "relevance" | "verificationRequired" | "reminders">>;
export type TaskCompletionInput = { outcome: TaskCompletion["outcome"]; note: string; references?: TaskReference[] };
export type TaskMutationContext = { actor: string; principal: string; idempotencyKey: string; expectedVersion?: number; now?: string; sourceCheck?: TaskSourceCheck };
export type TaskEvent = { taskId: string; version: number; operation: string; actor: string; principal: string; at: string; task: Task };
export type TaskQuery = { companySlugs?: string[]; includeArchived?: boolean; status?: TaskStatus; type?: Task["type"]; search?: string; assigneeId?: string; from?: string; to?: string; undated?: boolean; unassigned?: boolean; showDone?: boolean };
export type TaskColumn = { columnId: string; name: string; status: TaskStatus; isDefault: boolean };
export type TaskBoard = { boardId: string; version: number; scope: TaskScope; columns: TaskColumn[]; updatedAt: string };
export type TaskBoardDraft = Omit<TaskBoard, "version" | "updatedAt"> & { relocations?: Record<string, string>; previewHash?: string };
export type TaskBoardPreview = { previewHash: string; affectedTaskIds: string[]; statusChanges: Array<{ taskId: string; from: TaskStatus; to: TaskStatus }> };
export type TaskSeries = { seriesId: string; version: number; title: string; scope: TaskScope; template: TaskDraft;
  cadence: "month" | "quarter" | "year" | "custom"; every: number; anchor: "calendar" | "fiscal"; startDate: string; endDate: string | null;
  fiscalYearStartMonth: number; workDayOffset: number; deadlineDayOffset: number | null; relevance: "relevant" | "unknown" | "activity";
  active: boolean; updatedAt: string };
export type TaskSeriesDraft = Omit<TaskSeries, "version" | "updatedAt">;
export type TaskProjection = { projectionId: string; seriesId: string; title: string; scope: TaskScope; period: TaskPeriod; workDate: string; deadline: TaskDeadline | null; relevance: TaskSeries["relevance"]; concreteTaskId: string | null };
export type TaskNotification = { notificationId: string; recipientId: string; taskId: string; reminderId: string; slotKey: string; createdAt: string; title: string };
export type TaskRuntimeStatus = { running: boolean; lastTickAt: string | null; lastError: string | null };
export type TasksView = { tasks: Task[]; count: number; boards: TaskBoard[]; series: TaskSeries[]; projections: TaskProjection[];
  companies: Array<{ slug: string; name: string; archived: boolean; canWrite: boolean; canManage: boolean }>;
  currentUserId: string; canManageWorkspace: boolean; runtime: TaskRuntimeStatus; notifications: TaskNotification[];
  sourceCoverage: string[] };
/** Identity comes from the transport, never from the task request body. */
export type TaskIdentity = { principal: string; actor?: string; userId?: string; local: boolean };
export type TaskOperation = "list" | "get" | "history" | "create" | "update" | "move" | "complete" | "reopen" | "sync"
  | "series-list" | "series-save" | "series-project" | "series-materialize" | "boards-list" | "boards-preview" | "boards-save"
  | "reminder-set" | "notifications" | "runtime-status" | "run";
