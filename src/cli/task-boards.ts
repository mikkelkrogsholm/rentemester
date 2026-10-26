import type { CommandDispatch } from "../cli-dispatch";
import { handleTaskCommand } from "./tasks";
export function register(dispatch: CommandDispatch): void {
  dispatch.on("task-boards", "list", ctx => handleTaskCommand(ctx, "boards-list"));
  dispatch.on("task-boards", "preview", ctx => handleTaskCommand(ctx, "boards-preview"));
  dispatch.on("task-boards", "save", ctx => handleTaskCommand(ctx, "boards-save"));
}
