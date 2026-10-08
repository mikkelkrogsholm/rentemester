import type { CommandDispatch } from "../cli-dispatch";
import { handleTaskCommand } from "./tasks";
export function register(dispatch: CommandDispatch): void {
  dispatch.on("task-series", "list", ctx => handleTaskCommand(ctx, "series-list"));
  dispatch.on("task-series", "save", ctx => handleTaskCommand(ctx, "series-save"));
  dispatch.on("task-series", "project", ctx => handleTaskCommand(ctx, "series-project"));
  dispatch.on("task-series", "materialize", ctx => handleTaskCommand(ctx, "series-materialize"));
}
