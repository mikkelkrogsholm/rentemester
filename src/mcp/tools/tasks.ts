/** Workspace task adapters. The service owns live resource access and atomic retry receipts. */
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type { TaskIdentity, TaskOperation } from "../../core/tasks-types";
import { resolveWorkspaceRoot } from "../../core/workspace";
import { TASK_MUTATIONS, TASK_OPERATION_INPUTS } from "../../task-interface-input";
import { deriveMcpActor } from "../actor";
import { envelopeShape, envelopeToCallResult, errorEnvelope, successEnvelope } from "../envelope";
import { currentMcpAuthenticatedPrincipal } from "../security";
import { confirmField, strictMcpReadOnlyHandler } from "../tool-runtime";

const workspace = z.string().trim().min(1).max(2000).describe("Explicit workspace root. An authenticated session is bound to its configured workspace.");
const actor = z.string().regex(/^(user|agent|system):\S.+$/).max(200).optional().describe("Optional policy-approved audit attribution. Authorization comes from the verified transport principal, never this actor.");
const read = { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } as const;
const write = { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false } as const;
const description = (operation: TaskOperation, text: string) => TASK_MUTATIONS.has(operation)
  ? `${text} Workspace state only; no ledger, payment, filing or approval effects. Requires policy-approved actor and confirm:true. Reuse idempotencyKey only for an identical retry; read back canonical state after an uncertain result. write-reversible.`
  : `${text} Read-only, access filtered before counts and results; never changes a ledger.`;
function config(operation: TaskOperation, title: string, text: string) {
  const mutation = TASK_MUTATIONS.has(operation);
  return { title, description: description(operation, text), inputSchema: { workspace, ...TASK_OPERATION_INPUTS[operation].shape, ...(mutation ? { actor, confirm: confirmField } : {}) }, outputSchema: envelopeShape, annotations: mutation ? write : read };
}
function handler(server: McpServer, operation: TaskOperation, name: string) {
  const run = async (args: Record<string, unknown>) => {
    const mutation = TASK_MUTATIONS.has(operation);
    if (mutation && args.confirm !== true) return envelopeToCallResult(errorEnvelope(`confirm: true required for write tool ${name}`, { code: "CONFIRM_REQUIRED" }));
    const { workspace: root, actor: auditActor, confirm: _confirm, ...input } = args;
    const parsed = TASK_OPERATION_INPUTS[operation].safeParse(input);
    if (!parsed.success) return envelopeToCallResult(errorEnvelope(parsed.error.issues.map(issue => `${issue.path.join(".") || "input"}: ${issue.message}`).join("; "), { code: "TASK_INPUT_INVALID" }));
    const principal = currentMcpAuthenticatedPrincipal();
    const identity: TaskIdentity = {
      principal: principal ? `${principal.kind}:${principal.subjectId}` : "local-operator",
      userId: principal?.subjectId, local: !principal,
      actor: mutation ? typeof auditActor === "string" ? auditActor : deriveMcpActor(server.server.getClientVersion()).createdBy : undefined,
    };
    try {
      const { executeTaskOperation } = await import("../../core/task-service");
      const result = await executeTaskOperation(resolveWorkspaceRoot(String(root)), operation, parsed.data, identity);
      return envelopeToCallResult(successEnvelope(result));
    } catch (error) {
      const code = error && typeof error === "object" && "code" in error && typeof error.code === "string" ? error.code : "TASK_OPERATION_FAILED";
      const message = code !== "TASK_OPERATION_FAILED" && error instanceof Error ? error.message : "task operation failed";
      return envelopeToCallResult(errorEnvelope(message, { code }));
    }
  };
  return TASK_MUTATIONS.has(operation) ? run : strictMcpReadOnlyHandler(run);
}

export function registerTaskTools(server: McpServer): void {
  server.registerTool("tasks_list", config("list", "List tasks", "List tasks and the permitted company/board/series view."), handler(server, "list", "tasks_list"));
  server.registerTool("tasks_get", config("get", "Read task", "Read one stable task identity and current version."), handler(server, "get", "tasks_get"));
  server.registerTool("tasks_history", config("history", "Read task history", "Read preserved task history and completion evidence."), handler(server, "history", "tasks_history"));
  server.registerTool("tasks_create", config("create", "Create task", "Create an explicitly scoped task with title; other fields are optional."), handler(server, "create", "tasks_create"));
  server.registerTool("tasks_update", config("update", "Update task", "Update editable task fields at the exact expectedVersion."), handler(server, "update", "tasks_update"));
  server.registerTool("tasks_move", config("move", "Move task", "Change the common task status with explicit local/aggregate column choice when ambiguous."), handler(server, "move", "tasks_move"));
  server.registerTool("tasks_complete", config("complete", "Complete task", "Record completion outcome, note and required evidence; source-controlled work checks its real source."), handler(server, "complete", "tasks_complete"));
  server.registerTool("tasks_reopen", config("reopen", "Reopen task", "Reopen with a reason, preserving previous completion evidence."), handler(server, "reopen", "tasks_reopen"));
  server.registerTool("tasks_sync", config("sync", "Synchronize task sources", "Synchronize authoritative source problems without duplicate tasks."), handler(server, "sync", "tasks_sync"));
  server.registerTool("tasks_reminder_set", config("reminder-set", "Set task reminder", "Explicitly activate or disable an in-product reminder; assignment never sends an external message."), handler(server, "reminder-set", "tasks_reminder_set"));
  server.registerTool("tasks_notifications", config("notifications", "Read task notifications", "Read the authenticated recipient's access-filtered reminder overview."), handler(server, "notifications", "tasks_notifications"));
  server.registerTool("tasks_runtime", config("runtime-status", "Read reminder runtime", "Read whether the reminder runner is actually running and its last observed tick."), handler(server, "runtime-status", "tasks_runtime"));
  server.registerTool("tasks_run", config("run", "Run task reminders", "Execute one deterministic reminder tick; starts no LLM work and sends no mail."), handler(server, "run", "tasks_run"));
  server.registerTool("task_series_list", config("series-list", "List task series", "Read permitted recurring task series."), handler(server, "series-list", "task_series_list"));
  server.registerTool("task_series_save", config("series-save", "Save task series", "Create or change future recurring occurrences, preserving outstanding work and history. expectedVersion:0 creates a new series."), handler(server, "series-save", "task_series_save"));
  server.registerTool("task_series_project", config("series-project", "Project annual wheel", "Read expected future occurrences for from/to without materializing task cards."), handler(server, "series-project", "task_series_project"));
  server.registerTool("task_series_materialize", config("series-materialize", "Materialize task occurrences", "Create due recurring occurrences once using their stable period identities."), handler(server, "series-materialize", "task_series_materialize"));
  server.registerTool("task_boards_list", config("boards-list", "List task boards", "Read permitted local and workspace boards."), handler(server, "boards-list", "task_boards_list"));
  server.registerTool("task_boards_preview", config("boards-preview", "Preview board changes", "Read a previewHash and affected tasks/statuses before changing column meaning or deleting populated columns."), handler(server, "boards-preview", "task_boards_preview"));
  server.registerTool("task_boards_save", config("boards-save", "Save task board", "Save columns with explicit common meanings and required relocations/previewHash. expectedVersion:0 creates a new board."), handler(server, "boards-save", "task_boards_save"));
}
