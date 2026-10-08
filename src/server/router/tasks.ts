import { executeTaskOperation, readTaskOperationReceipt, TASK_READ_OPERATIONS } from "../../core/task-service";
import { TaskError } from "../../core/tasks";
import type { TaskIdentity, TaskOperation } from "../../core/tasks-types";
import type { ServerConfig } from "../config";
import { resolveCockpitActor } from "../actor";
import { assertLocalhostWriteAllowed, assertMutationContentType, assertMutationOriginAllowed } from "../mutations";
import { ApiError } from "../errors";
import { okResponse } from "./_shared";
import { TASK_OPERATION_INPUTS } from "../../task-interface-input";

function identity(config: ServerConfig): TaskIdentity {
  const principal = config.requestPrincipal;
  if (!principal) throw ApiError.unauthorized("missing or invalid credentials");
  return { principal: principal.userId ? `user:${principal.userId}` : "local", actor: resolveCockpitActor(principal).createdBy,
    userId: principal.userId, local: principal.via === "localhost-trusted" || principal.via === "shared-secret", enforceActorPolicy: false };
}

async function body(request: Request): Promise<Record<string, unknown>> {
  const reader = request.body?.getReader();
  if (!reader) throw ApiError.badRequest("request body must be valid JSON");
  const chunks: Uint8Array[] = []; let size = 0;
  try {
    while (true) {
      const chunk = await reader.read();
      if (chunk.done) break;
      size += chunk.value.byteLength;
      if (size > 131072) { await reader.cancel(); throw ApiError.badRequest("Opgaveinput er for stort."); }
      chunks.push(chunk.value);
    }
    const bytes = new Uint8Array(size); let offset = 0;
    for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
    const input: unknown = JSON.parse(new TextDecoder().decode(bytes));
    if (!input || typeof input !== "object" || Array.isArray(input)) throw new Error("object required");
    return input as Record<string, unknown>;
  } catch (error) {
    if (error instanceof ApiError) throw error;
    throw ApiError.badRequest("request body must be valid JSON object");
  } finally { reader.releaseLock(); }
}

function query(url: URL): Record<string, unknown> {
  const input: Record<string, unknown> = {};
  for (const key of ["search", "status", "type", "assigneeId", "from", "to"]) {
    const value = url.searchParams.get(key); if (value !== null) input[key] = value;
  }
  const slugs = [...url.searchParams.getAll("companySlug"), ...url.searchParams.getAll("companySlugs")];
  if (slugs.length) input.companySlugs = [...new Set(slugs)];
  for (const key of ["showDone", "includeArchived", "undated", "unassigned"]) {
    const value = url.searchParams.get(key);
    if (value !== null) {
      if (value !== "true" && value !== "false") throw ApiError.badRequest(`${key} kræver true eller false.`);
      input[key] = value === "true";
    }
  }
  return input;
}

export async function dispatchTaskRoute(request: Request, config: ServerConfig, path: string, method: string, url: URL): Promise<Response | null> {
  const receipt = /^\/api\/tasks\/operations\/([^/]+)$/.exec(path);
  if (receipt) {
    try { return okResponse(readTaskOperationReceipt(config.workspaceRoot, decodeURIComponent(receipt[1]!), identity(config))); }
    catch (error) { return taskFailure(error); }
  }
  let operation: TaskOperation | undefined; let taskId: string | undefined;
  if (path === "/api/tasks") operation = method === "GET" ? "list" : "create";
  else if (path === "/api/tasks/sync") operation = "sync";
  else if (path === "/api/tasks/run") operation = "run";
  else if (path === "/api/tasks/runtime") operation = "runtime-status";
  else if (path === "/api/tasks/notifications") operation = "notifications";
  else if (path === "/api/task-series") operation = method === "GET" ? "series-list" : "series-save";
  else if (path === "/api/task-series/project") operation = "series-project";
  else if (path === "/api/task-series/materialize") operation = "series-materialize";
  else if (path === "/api/task-boards") operation = method === "GET" ? "boards-list" : "boards-save";
  else if (path === "/api/task-boards/preview") operation = "boards-preview";
  else {
    const match = /^\/api\/tasks\/([^/]+)(?:\/(update|move|complete|reopen|reminder|history))?$/.exec(path);
    if (!match) return null;
    taskId = decodeURIComponent(match[1]!);
    operation = match[2] === "reminder" ? "reminder-set" : (match[2] ?? "get") as TaskOperation;
  }
  const input = method === "GET" ? query(url) : await body(request);
  if (taskId) input.taskId = taskId;
  if (!TASK_READ_OPERATIONS.has(operation)) {
    assertLocalhostWriteAllowed(request, config);
    assertMutationContentType(request); assertMutationOriginAllowed(request, config);
    if (input.confirm !== true) throw ApiError.badRequest("confirm: true kræves for ændringer.", { subcode: "CONFIRM_REQUIRED" });
  }
  const { confirm: _confirm, ...fields } = input;
  const parsed = TASK_OPERATION_INPUTS[operation].safeParse(fields);
  if (!parsed.success) throw ApiError.badRequest("Opgaveinput er ugyldigt.", { subcode: "TASK_INPUT_INVALID" });
  try { return okResponse(await executeTaskOperation(config.workspaceRoot, operation, parsed.data, identity(config))); }
  catch (error) { return taskFailure(error); }
}

function taskFailure(error: unknown): never {
    if (!(error instanceof TaskError)) throw error;
    const code = error.code.toUpperCase();
    if (code === "NOT_FOUND") throw ApiError.notFound(error.message);
    if (code === "ACCESS_DENIED" || code === "ACTOR_DENIED") throw ApiError.unauthorized("Handlingen er ikke tilgængelig med din adgang.");
    if (/CONFLICT|VERSION|IDEMPOTENCY|PREVIEW|STALE/.test(code)) throw ApiError.conflict(error.message, { subcode: error.code });
    throw ApiError.badRequest(error.message, { subcode: error.code });
}
