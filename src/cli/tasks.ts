import { realpathSync } from "node:fs";
import { requireMutationActorIdentity } from "../cli-actor";
import { readJsonObjectCliInput, type CommandContext, type CommandDispatch } from "../cli-dispatch";
import { resolveWorkspaceRoot } from "../core/workspace";
import type { TaskIdentity, TaskOperation } from "../core/tasks-types";
import { authorizeMcpTool, createMcpSecurityContextFromEnv } from "../mcp/security";
import { TASK_MCP_NAMES, TASK_MUTATIONS, TASK_OPERATION_INPUTS } from "../task-interface-input";



function readTaskInput(ctx: CommandContext, operation: TaskOperation): Record<string, unknown> {
  const path = ctx.arg("--input");
  if (path && [...ctx.parsedArgs.flags.keys()].some(flag => !["--input", "--workspace", "--actor", "--actor-via", "--confirm", "--format", "--json"].includes(flag))) ctx.fatal("--input cannot be combined with task selector or filter flags");
  let input: Record<string, unknown> = path ? readJsonObjectCliInput(ctx, path, "--input") : {};
  if (TASK_MUTATIONS.has(operation) && !path) ctx.fatal("--input <file.json> is required for task mutations");
  if (!path) {
    if (ctx.arg("--task-id")) input.taskId = ctx.arg("--task-id");
    for (const name of ["from", "to", "search", "status", "type"]) if (ctx.arg(`--${name}`) !== undefined) input[name] = ctx.arg(`--${name}`);
    if (ctx.hasFlag("--include-archived")) input.includeArchived = true;
    if (ctx.arg("--companies") !== undefined) input.companySlugs = ctx.arg("--companies")!.split(",").map(value => value.trim()).filter(Boolean);
    for (const [flag, name] of [["--show-done", "showDone"], ["--undated", "undated"], ["--unassigned", "unassigned"]]) {
      const value = ctx.arg(flag!);
      if (value !== undefined) {
        if (value !== "true" && value !== "false") ctx.fatal(`${flag} must be true or false`);
        input[name!] = value === "true";
      }
    }
  }
  const parsed = TASK_OPERATION_INPUTS[operation].safeParse(input);
  if (!parsed.success) ctx.fatal(parsed.error.issues.map(issue => `${issue.path.join(".") || "input"}: ${issue.message}`).join("; "));
  input = parsed.data;
  return input;
}

/** Workspace task commands deliberately bypass the company ledger/backup runtime. */
export async function handleTaskCommand(ctx: CommandContext, operation: TaskOperation): Promise<void> {
  const raw = ctx.trimToNull(ctx.arg("--workspace")) ?? ctx.trimToNull(process.env.RENTEMESTER_WORKSPACE);
  if (!raw) ctx.fatal("--workspace <dir> or RENTEMESTER_WORKSPACE is required");
  let workspaceRoot: string;
  try { workspaceRoot = resolveWorkspaceRoot(raw); } catch (error) { ctx.fatal(error instanceof Error ? error.message : "invalid workspace path"); }
  const input = readTaskInput(ctx, operation);
  const actor = TASK_MUTATIONS.has(operation) ? requireMutationActorIdentity(ctx.cliActor, ctx.fatal) : undefined;
  if (operation === "boards-save" && ctx.arg("--confirm") !== "yes") {
    ctx.emitResult({ ok: false, code: "CONFIRM_REQUIRED", errors: ["--confirm yes required for task-boards save"] });
    return;
  }
  try {
    // A supplied credential can never fall back to the trusted local path.
    const security = createMcpSecurityContextFromEnv();
    let identity: TaskIdentity = { principal: "local-operator", actor, local: true };
    if (security) {
      if (realpathSync(workspaceRoot) !== realpathSync(security.workspaceRoot)) {
        ctx.emitResult({ ok: false, code: "MCP_UNAUTHORIZED", errors: ["workspace must match authenticated workspace"] }); return;
      }
      const access = await authorizeMcpTool(security, TASK_MCP_NAMES[operation], { ...input, workspace: workspaceRoot });
      if (!access) { ctx.emitResult({ ok: false, code: "MCP_UNAUTHORIZED", errors: ["missing or invalid credentials"] }); return; }
      identity = { principal: `${access.principal.kind}:${access.principal.subjectId}`, userId: access.principal.subjectId, actor, local: false };
    }
    const { executeTaskOperation } = await import("../core/task-service");
    const result = await executeTaskOperation(workspaceRoot, operation, input, identity);
    ctx.emitResult({ ...result, ok: true, errors: [] });
  } catch (error) {
    const code = error && typeof error === "object" && "code" in error && typeof error.code === "string" ? error.code : "TASK_OPERATION_FAILED";
    if (code === "ACTOR_REQUIRED" || code === "ACTOR_DENIED") ctx.fatal(error instanceof Error ? error.message : "actor policy rejected the task operation");
    ctx.emitResult({ ok: false, code, errors: [code !== "TASK_OPERATION_FAILED" && error instanceof Error ? error.message : "task operation failed"] });
  }
}

export function register(dispatch: CommandDispatch): void {
  const operations = { list: "list", get: "get", create: "create", update: "update", move: "move", complete: "complete", reopen: "reopen", history: "history", sync: "sync", reminder: "reminder-set", notifications: "notifications", runtime: "runtime-status", run: "run" } as const;
  for (const [command, operation] of Object.entries(operations)) dispatch.on("tasks", command, ctx => handleTaskCommand(ctx, operation));
}
