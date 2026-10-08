/** Typed transport DTOs shared by CLI and MCP; identity is always supplied separately. */
import { z } from "zod";
import type { TaskOperation } from "./core/tasks-types";

const id = z.string().trim().min(1).max(200);
const slug = z.string().regex(/^[a-z0-9][a-z0-9-]*$/).max(120);
const date = z.string().regex(/^\d{4}-\d{2}-\d{2}$/).describe("ISO calendar date YYYY-MM-DD; dates are validated again by the domain.");
export const taskStatusInput = z.enum(["open", "in_progress", "waiting", "done"]);
export const taskScopeInput = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("company"), companySlug: slug }).strict(),
  z.object({ kind: z.literal("workspace"), companySlugs: z.array(slug).max(200) }).strict(),
]);
const reference = z.object({ kind: z.enum(["document", "bank_transaction", "period", "approval", "knowledge", "external_receipt", "party"]), ref: id, companySlug: slug.optional() }).strict();
const assignee = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("member"), userId: id, name: z.string().min(1).max(200) }).strict(),
  z.object({ kind: z.literal("external"), name: z.string().min(1).max(200) }).strict(),
]);
const deadline = z.object({ date, kind: z.enum(["statutory", "agreement", "internal"]), basis: z.string().min(1).max(2000), certainty: z.enum(["confirmed", "unconfirmed"]), ruleId: id.optional() }).strict();
const period = z.object({ from: date, to: date, label: z.string().min(1).max(200) }).strict();
const editable = {
  title: z.string().trim().min(1).max(300), scope: taskScopeInput,
  description: z.string().max(10000).optional(), nextAction: z.string().max(2000).optional(),
  assignee: assignee.nullable().optional(), waitingOn: z.string().max(500).optional(),
  workDate: date.nullable().optional(), deadline: deadline.nullable().optional(), period: period.nullable().optional(),
  references: z.array(reference).max(100).optional(), evidenceRequired: z.boolean().optional(),
  relevance: z.enum(["relevant", "unknown", "not_relevant"]).optional(), verificationRequired: z.boolean().optional(),
};
export const taskDraftInput = z.object({ ...editable, taskId: id.optional(), type: z.enum(["ad_hoc", "routine", "obligation"]).optional() }).strict();
const patch = z.object(editable).partial().strict();
const key = { idempotencyKey: id.describe("Durable retry key. Reuse only for an identical operation; a changed payload conflicts.") };
const existing = { taskId: id, expectedVersion: z.number().int().min(1), ...key };
const companies = { companySlugs: z.array(slug).max(200).optional() };
const query = {
  ...companies, includeArchived: z.boolean().optional(), status: taskStatusInput.optional(),
  type: z.enum(["ad_hoc", "routine", "obligation"]).optional(), search: z.string().max(300).optional(),
  assigneeId: id.optional(), from: date.optional(), to: date.optional(), undated: z.boolean().optional(),
  unassigned: z.boolean().optional(), showDone: z.boolean().optional(),
};
const reminder = z.object({ reminderId: id, recipientId: id, enabled: z.boolean(), kind: z.enum(["deadline", "follow_up"]), daysBefore: z.number().int().min(0).max(366).optional(), followUpDate: date.optional(), frequency: z.enum(["once", "daily"]), timeZone: z.string().min(1).max(100) }).strict();
const board = z.object({ boardId: id, scope: taskScopeInput, columns: z.array(z.object({ columnId: id, name: z.string().trim().min(1).max(100), status: taskStatusInput, isDefault: z.boolean() }).strict()).min(1).max(40), relocations: z.record(id, id).optional(), previewHash: id.optional() }).strict();
const series = z.object({ seriesId: id, title: z.string().trim().min(1).max(300), scope: taskScopeInput, template: taskDraftInput,
  cadence: z.enum(["month", "quarter", "year", "custom"]), every: z.number().int().min(1).max(120), anchor: z.enum(["calendar", "fiscal"]),
  startDate: date, endDate: date.nullable(), fiscalYearStartMonth: z.number().int().min(1).max(12), workDayOffset: z.number().int().min(-366).max(366),
  deadlineDayOffset: z.number().int().min(-366).max(366).nullable(), relevance: z.enum(["relevant", "unknown", "activity"]), active: z.boolean(),
}).strict();

export const TASK_OPERATION_INPUTS = {
  list: z.object(query).strict(), get: z.object({ taskId: id }).strict(), history: z.object({ taskId: id }).strict(),
  create: taskDraftInput.extend(key), update: z.object({ ...existing, patch }).strict(),
  move: z.object({ ...existing, status: taskStatusInput, columnId: id.optional(), workspaceColumnId: id.optional() }).strict(),
  complete: z.object({ ...existing, outcome: z.enum(["completed", "not_relevant", "cancelled", "exception"]), note: z.string().min(1).max(10000), references: z.array(reference).max(100).optional() }).strict(),
  reopen: z.object({ ...existing, reason: z.string().min(1).max(2000) }).strict(),
  sync: z.object({ ...companies, asOfDate: date.optional(), ...key }).strict(),
  "series-list": z.object({ ...companies, includeArchived: z.boolean().optional() }).strict(),
  "series-save": z.object({ series, expectedVersion: z.number().int().min(0), ...key }).strict(),
  "series-project": z.object({ ...companies, from: date, to: date, includeArchived: z.boolean().optional() }).strict(),
  "series-materialize": z.object({ ...companies, asOfDate: date.optional(), ...key }).strict(),
  "boards-list": z.object({ ...companies, includeArchived: z.boolean().optional() }).strict(),
  "boards-preview": z.object({ board, expectedVersion: z.number().int().min(0).optional() }).strict(),
  "boards-save": z.object({ board, expectedVersion: z.number().int().min(0), ...key }).strict(),
  "reminder-set": z.object({ ...existing, reminder }).strict(),
  notifications: z.object({}).strict(), "runtime-status": z.object({}).strict(),
  run: z.object({ asOfDate: date.optional(), ...key }).strict(),
} satisfies Record<TaskOperation, z.ZodObject>;

export const TASK_MUTATIONS: ReadonlySet<TaskOperation> = new Set(["create", "update", "move", "complete", "reopen", "sync", "series-save", "series-materialize", "boards-save", "reminder-set", "run"]);

export const TASK_MCP_NAMES: Record<TaskOperation, string> = {
  list: "tasks_list", get: "tasks_get", history: "tasks_history", create: "tasks_create", update: "tasks_update", move: "tasks_move", complete: "tasks_complete", reopen: "tasks_reopen", sync: "tasks_sync",
  "series-list": "task_series_list", "series-save": "task_series_save", "series-project": "task_series_project", "series-materialize": "task_series_materialize",
  "boards-list": "task_boards_list", "boards-preview": "task_boards_preview", "boards-save": "task_boards_save", "reminder-set": "tasks_reminder_set", notifications: "tasks_notifications", "runtime-status": "tasks_runtime", run: "tasks_run",
};
