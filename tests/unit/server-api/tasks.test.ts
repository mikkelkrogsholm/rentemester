import { describe, expect, test } from "bun:test";
import { config, get, makeWorkspace, rmSync } from "./_shared";
import { openWorkspaceControlDb } from "../../../src/core/workspace-control";

describe("task HTTP integration", () => {
  test("HTTP lifecycle, exact retry, conflict and metadata share one durable task", async () => {
    const ws = makeWorkspace("task-http", ["Synthetic Tasks ApS"]);
    try {
      openWorkspaceControlDb(ws).close();
      const cfg = config({ workspaceRoot: ws });
      const post = (path: string, value: unknown) => get(cfg, path, { method: "POST", headers: { "Content-Type": "application/json", Host: "localhost" }, body: JSON.stringify(value) });
      const input = { title: "Synthetic HTTP task", scope: { kind: "company", companySlug: "synthetic-tasks-aps" }, idempotencyKey: "http-create", confirm: true };
      const first = await post("/api/tasks", input); expect(first.status).toBe(200); expect(first.body.ok).toBe(true);
      const id = first.body.task.taskId;
      expect((await post("/api/tasks", input)).body.task.taskId).toBe(id);
      expect((await get(cfg, "/api/tasks/operations/http-create")).body.receipt).toMatchObject({ taskId: id, version: 1 });
      const changed = await post(`/api/tasks/${id}/update`, { patch: { title: "Changed" }, expectedVersion: 1, idempotencyKey: "http-edit", confirm: true }); expect(changed.body.task.version).toBe(2);
      const stale = await post(`/api/tasks/${id}/update`, { patch: { title: "Stale" }, expectedVersion: 1, idempotencyKey: "http-stale", confirm: true }); expect(stale.status).toBe(409);
      expect((await post(`/api/tasks/${id}/move`, { status: "done", expectedVersion: 2, idempotencyKey: "http-bypass", confirm: true })).status).toBe(400);
      const complete = await post(`/api/tasks/${id}/complete`, { outcome: "completed", note: "Synthetic documentation", expectedVersion: 2, idempotencyKey: "http-complete", confirm: true }); expect(complete.status).toBe(200);
      expect((await get(cfg, "/api/tasks")).body.count).toBe(0);
      expect((await get(cfg, "/api/tasks?showDone=true")).body.count).toBe(1);
      expect((await get(cfg, `/api/tasks/${id}`)).body.history).toHaveLength(3);
      expect((await post(`/api/tasks/${id}/reopen`, { reason: "Follow up", expectedVersion: 3, idempotencyKey: "http-reopen", confirm: true })).body.task.status).toBe("open");
      expect((await post("/api/tasks", { ...input, idempotencyKey: "http-spoof", actor: "user:owner" })).status).toBe(400);
      expect((await post("/api/tasks", { ...input, idempotencyKey: "no-confirm", confirm: false })).status).toBe(400);
      expect((await get(cfg, "/api/tasks?includeArchived=maybe")).status).toBe(400);
    } finally { rmSync(ws, { recursive: true, force: true }); }
  });
});
