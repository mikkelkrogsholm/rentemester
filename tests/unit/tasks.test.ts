import { afterEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createTask, getTask, listTasks, taskHistory, updateTask, moveTask, completeTask, reopenTask, syncSourceTask, TaskError } from "../../src/core/tasks";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import type { TaskDraft, TaskMutationContext, TaskSource } from "../../src/core/tasks-types";
const cleanup: Array<() => void> = [];
afterEach(() => { for (const fn of cleanup.splice(0).reverse()) fn(); });
function fixture() {
  const root = mkdtempSync(join(tmpdir(), "rentemester-tasks-"));
  const db = openWorkspaceControlDb(root);
  cleanup.push(() => { db.close(); rmSync(root, { recursive: true, force: true }); });
  return { root, db };
}
const now = "2026-10-08T10:00:00.000Z";
const context = (key: string, version?: number): TaskMutationContext => ({ actor: "agent:synthetic", principal: "synthetic-member", idempotencyKey: key, expectedVersion: version, now });
const draft = (changes: Partial<TaskDraft> = {}): TaskDraft => ({ title: "Afstem perioden", scope: { kind: "company", companySlug: "synthetic-alpha" }, ...changes });
function source(state: TaskSource["state"] = "open", hash = "a"): TaskSource { return { kind: "reconciliation", identity: "2026-09", companySlug: "synthetic-alpha", state, contentHash: hash.repeat(64), ref: "period:2026-09", href: "/companies/synthetic-alpha/reconciliation" }; }
function code(action: () => unknown) { try { action(); return "success"; } catch (error) { if (error instanceof TaskError) return error.code; throw error; } }

describe("workspace task domain", () => {
  test("cards, notes and named assignees reject identity/payment details and evidence controls cannot be removed", () => {
    const { db } = fixture();
    for (const title of ["Honorar 010101-1234", "Betal DK50 0040 0440 1162 43", "Bankkonto 1234-1234567890"]) expect(code(() => createTask(db, draft({ title }), context(`privacy-${title.length}`)))).toBe("private_data");
    const task = createTask(db, draft({ evidenceRequired: true, verificationRequired: true, relevance: "unknown" }), context("controlled"));
    expect(code(() => updateTask(db, task.taskId, { assignee: { kind: "external", name: "Revisor 010101-1234" } }, context("private-assignee", 1)))).toBe("private_data");
    expect(code(() => completeTask(db, task.taskId, { outcome: "exception", note: "Bankkonto 1234-1234567890" }, context("private-note", 1)))).toBe("private_data");
    expect(code(() => updateTask(db, task.taskId, { evidenceRequired: false }, context("no-evidence", 1)))).toBe("invalid_input");
    expect(code(() => updateTask(db, task.taskId, { verificationRequired: false }, context("no-verification", 1)))).toBe("invalid_input");
    const clarified = updateTask(db, task.taskId, { verificationRequired: false, relevance: "relevant", description: "Rådgiverens resultat er kontrolleret mod kvitteringen.", references: [{ kind: "external_receipt", ref: "SYNTHETIC-RECEIPT" }] }, context("clarified", 1));
    expect(clarified.verificationRequired).toBe(false);
  });
  test("minimal creation has explicit scope and does not touch a company ledger", () => {
    const { db, root } = fixture();
    const ledger = join(root, "synthetic-alpha", "ledger.sqlite"); mkdirSync(join(root, "synthetic-alpha")); writeFileSync(ledger, "ledger sentinel");
    const before = readFileSync(ledger);
    const task = createTask(db, draft(), context("create"));
    expect(task.status).toBe("open"); expect(task.version).toBe(1); expect(task.columnId).toBe("open");
    expect(task.assignee).toBeNull(); expect(task.reminders).toEqual([]); expect(readFileSync(ledger)).toEqual(before);
    expect(getTask(db, task.taskId)).toEqual(task); expect(taskHistory(db, task.taskId)).toHaveLength(1);
  });
  test("exact principal-specific retry returns the original effect after later changes", () => {
    const { db } = fixture(), input = draft();
    const created = createTask(db, input, context("create"));
    updateTask(db, created.taskId, { title: "Ny titel" }, context("edit", 1));
    expect(createTask(db, input, { ...context("create"), now: "2026-10-09T12:00:00.000Z" })).toEqual(created);
    expect(taskHistory(db, created.taskId)).toHaveLength(2);
    expect(code(() => createTask(db, draft({ title: "Changed" }), context("create")))).toBe("idempotency_conflict");
    const other = createTask(db, input, { ...context("create"), principal: "different-member" });
    expect(other.taskId).not.toBe(created.taskId);
  });
  test("live authorization runs inside the writer lock before effects and receipt disclosure", () => {
    const { db } = fixture();
    let allowed = true, calls = 0;
    const guarded = { ...context("guarded"), authorize: () => { calls += 1; expect(db.inTransaction).toBe(true); if (!allowed) throw new TaskError("forbidden", "Access revoked"); } };
    const task = createTask(db, draft(), guarded);
    expect(calls).toBe(1); allowed = false;
    expect(code(() => createTask(db, draft(), guarded))).toBe("forbidden");
    expect(calls).toBe(2); expect(taskHistory(db, task.taskId)).toHaveLength(1);
    expect(code(() => createTask(db, draft(), { ...guarded, idempotencyKey: "denied-effect" }))).toBe("forbidden");
    expect(listTasks(db)).toHaveLength(1);
  });
  test("actor, key, scope, dates and state injection fail closed", () => {
    const { db } = fixture();
    expect(code(() => createTask(db, draft(), { ...context("a"), actor: "" }))).toBe("invalid_input");
    expect(code(() => createTask(db, draft(), { ...context("a"), idempotencyKey: "" }))).toBe("invalid_input");
    expect(code(() => createTask(db, draft({ workDate: "2026-02-30" }), context("bad-date")))).toBe("invalid_input");
    expect(code(() => createTask(db, draft({ status: "done" }), context("bad-status")))).toBe("invalid_input");
    expect(code(() => createTask(db, { ...draft(), completion: { assurance: "product_verified" } } as unknown as TaskDraft, context("forged-completion")))).toBe("invalid_input");
    expect(code(() => createTask(db, { ...draft(), version: 400 } as unknown as TaskDraft, context("forged-version")))).toBe("invalid_input");
    expect(code(() => createTask(db, draft({ relevance: "not_relevant" }), context("no-reason")))).toBe("invalid_input");
    expect(listTasks(db)).toEqual([]);
  });
  test("two connections cannot silently overwrite task versions", () => {
    const { root, db } = fixture(), second = openWorkspaceControlDb(root); cleanup.push(() => second.close());
    const task = createTask(db, draft(), context("create"));
    updateTask(db, task.taskId, { title: "First writer" }, context("first", 1));
    expect(code(() => updateTask(second, task.taskId, { title: "Second writer" }, context("second", 1)))).toBe("version_conflict");
    expect(getTask(second, task.taskId)?.title).toBe("First writer");
    expect(code(() => updateTask(db, task.taskId, { title: "Missing version" }, context("missing")))).toBe("version_required");
    expect(taskHistory(db, task.taskId)).toHaveLength(2);
  });
  test("writer lock failures are retriable without partial events or receipts", () => {
    const { root, db } = fixture(), second = openWorkspaceControlDb(root); cleanup.push(() => second.close());
    db.exec("BEGIN IMMEDIATE");
    expect(() => createTask(second, draft(), context("busy"))).toThrow(/locked|busy/i);
    db.exec("COMMIT");
    const task = createTask(second, draft(), context("busy"));
    expect(taskHistory(db, task.taskId)).toHaveLength(1);
  });
  test("work date changes preserve statutory deadline and changing it requires basis", () => {
    const { db } = fixture(), task = createTask(db, draft({ deadline: { date: "2026-10-10", kind: "statutory", basis: "Synthetic known rule", certainty: "confirmed" } }), context("create"));
    const edited = updateTask(db, task.taskId, { workDate: "2026-11-01" }, context("plan", 1));
    expect(edited.deadline?.date).toBe("2026-10-10");
    expect(code(() => updateTask(db, task.taskId, { deadline: { ...edited.deadline!, date: "2026-11-10" } }, context("illegal", 2)))).toBe("invalid_input");
    expect(code(() => updateTask(db, task.taskId, { deadline: null }, context("remove", 2)))).toBe("invalid_input");
    expect(updateTask(db, task.taskId, { deadline: { ...edited.deadline!, date: "2026-11-10", basis: "Synthetic documented extension" } }, context("extension", 2)).deadline?.date).toBe("2026-11-10");
  });
  test("references remain opaque and confined to scope", () => {
    const { db } = fixture();
    expect(code(() => createTask(db, draft({ references: [{ kind: "document", ref: "/private/secret" }] }), context("path")))).toBe("invalid_input");
    expect(code(() => createTask(db, draft({ references: [{ kind: "document", ref: "https://other.test" }] }), context("url")))).toBe("invalid_input");
    expect(code(() => createTask(db, draft({ references: [{ kind: "party", ref: "party:123", companySlug: "synthetic-beta" }] }), context("other")))).toBe("invalid_input");
    const task = createTask(db, draft({ references: [{ kind: "document", ref: "document:12", companySlug: "synthetic-alpha" }] }), context("create"));
    expect(code(() => updateTask(db, task.taskId, { scope: { kind: "company", companySlug: "synthetic-beta" } }, context("new-scope", 1)))).toBe("invalid_input");
  });
  test("simple documented completion and reopen preserve prior evidence", () => {
    const { db } = fixture(), task = createTask(db, draft(), context("create"));
    expect(code(() => moveTask(db, task.taskId, { status: "done" }, context("fake-done", 1)))).toBe("completion_required");
    expect(code(() => completeTask(db, task.taskId, { outcome: "completed", note: "" }, context("empty", 1)))).toBe("invalid_input");
    const done = completeTask(db, task.taskId, { outcome: "completed", note: "Kontrolleret", references: [{ kind: "external_receipt", ref: "receipt:123" }] }, context("complete", 1));
    expect(done.completion?.assurance).toBe("user_reported");
    expect(code(() => moveTask(db, task.taskId, { status: "open" }, context("fake-reopen", 2)))).toBe("reopen_required");
    const reopened = reopenTask(db, task.taskId, "Ny dokumentation", context("reopen", 2));
    expect(reopened.completion).toBeNull();
    const events = taskHistory(db, task.taskId); expect(events[1]?.task.completion?.references).toHaveLength(1); expect(events[2]?.operation).toBe("reopened");
    expect(db.query("SELECT reason FROM rm_task_events WHERE task_id=? AND version=3").get(task.taskId)).toEqual({ reason: "Ny dokumentation" });
  });
  test("evidence requirement supports distinct reasoned exception and non-applicability", () => {
    const { db } = fixture(), task = createTask(db, draft({ evidenceRequired: true }), context("create"));
    expect(code(() => completeTask(db, task.taskId, { outcome: "completed", note: "Bare færdig" }, context("missing-proof", 1)))).toBe("evidence_required");
    const exception = completeTask(db, task.taskId, { outcome: "exception", note: "Dokumentation kan ikke fremskaffes; vurdering registreret" }, context("exception", 1));
    expect(exception.completion?.outcome).toBe("exception"); expect(exception.completion?.assurance).toBe("user_reported");
    const task2 = createTask(db, draft(), context("create2"));
    const irrelevant = completeTask(db, task2.taskId, { outcome: "not_relevant", note: "Ingen aktivitet denne periode" }, context("not-relevant", 1));
    expect(irrelevant.completion?.outcome).toBe("not_relevant"); expect(irrelevant.relevance).toBe("not_relevant");
  });
  test("source tasks cannot fake completion, rely on stale check, or claim uploaded evidence is verified", () => {
    const { db } = fixture(), task = syncSourceTask(db, draft({ source: source(), evidenceRequired: true }), context("sync"));
    const input = { outcome: "completed" as const, note: "Afstemt", references: [{ kind: "document" as const, ref: "document:5" }] };
    expect(code(() => completeTask(db, task.taskId, input, context("no-source", 1)))).toBe("source_unresolved");
    expect(code(() => completeTask(db, task.taskId, input, { ...context("open-source", 1), sourceCheck: { state: "open", observedAt: now } }))).toBe("source_unresolved");
    expect(code(() => completeTask(db, task.taskId, input, { ...context("stale-source", 1), sourceCheck: { state: "resolved", observedAt: "2026-10-08T09:00:00.000Z" } }))).toBe("source_check_stale");
    const done = completeTask(db, task.taskId, input, { ...context("source", 1), sourceCheck: { state: "resolved", observedAt: now } });
    expect(done.completion?.assurance).toBe("user_reported");
  });
  test("fresh resolved source evidence is the only route to product verified assurance", () => {
    const { db } = fixture(), task = syncSourceTask(db, draft({ source: source(), evidenceRequired: true }), context("sync"));
    const done = completeTask(db, task.taskId, { outcome: "completed", note: "Kilde afstemt" }, { ...context("done", 1), sourceCheck: { state: "resolved", observedAt: now, evidence: [{ kind: "period", ref: "period:2026-09", companySlug: "synthetic-alpha" }] } });
    expect(done.completion?.assurance).toBe("product_verified"); expect(done.source?.state).toBe("resolved");
    const manual = createTask(db, draft(), context("manual"));
    expect(completeTask(db, manual.taskId, { outcome: "completed", note: "Manuel" }, { ...context("manual-done", 1), sourceCheck: { state: "resolved", observedAt: now, evidence: [{ kind: "document", ref: "document:3" }] } }).completion?.assurance).toBe("user_reported");
  });
  test("stable source identity deduplicates changed content and reopens recurring problems", () => {
    const { db } = fixture(), task = syncSourceTask(db, draft({ source: source() }), context("sync"));
    const changed = syncSourceTask(db, draft({ source: source("open", "b") }), context("changed"));
    expect(changed.taskId).toBe(task.taskId); expect(listTasks(db)).toHaveLength(1);
    const done = completeTask(db, task.taskId, { outcome: "completed", note: "Afstemt" }, { ...context("done", 2), sourceCheck: { state: "resolved", observedAt: now, evidence: [{ kind: "period", ref: "period:2026-09" }] } });
    const reopened = syncSourceTask(db, draft({ source: source("open", "c") }), context("reappeared"));
    expect(reopened.taskId).toBe(done.taskId); expect(reopened.status).toBe("open"); expect(reopened.completion).toBeNull();
    expect(taskHistory(db, task.taskId).some((event) => event.task.completion?.assurance === "product_verified")).toBe(true);
  });
  test("proposals require explicit relevance application and stay accepted on repeat sync", () => {
    const { db } = fixture();
    const proposal = syncSourceTask(db, draft({ origin: "proposal", relevance: "unknown", source: source("unknown") }), context("proposal"));
    expect(proposal.origin).toBe("proposal"); expect(proposal.relevance).toBe("unknown");
    const applied = updateTask(db, proposal.taskId, { relevance: "relevant" }, context("apply", 1));
    expect(applied.origin).toBe("system");
    const repeated = syncSourceTask(db, draft({ origin: "proposal", relevance: "unknown", source: source("unknown", "b") }), context("repeat"));
    expect(repeated.taskId).toBe(proposal.taskId); expect(repeated.origin).toBe("system"); expect(repeated.relevance).toBe("relevant");
  });
  test("unknown source and relevance flags cannot be silently cleared", () => {
    const { db } = fixture(), task = syncSourceTask(db, draft({ source: source("unknown") }), context("unknown"));
    expect(task.verificationRequired).toBe(true);
    expect(code(() => updateTask(db, task.taskId, { verificationRequired: false }, context("clear", 1)))).toBe("invalid_input");
    const unclear = createTask(db, draft({ relevance: "unknown" }), context("unclear"));
    expect(code(() => completeTask(db, unclear.taskId, { outcome: "completed", note: "Måske" }, context("unclear-done", 1)))).toBe("relevance_unknown");
  });
  test("filters count tasks once when work date and deadline overlap", () => {
    const { db } = fixture();
    createTask(db, draft({ workDate: "2026-10-10", deadline: { date: "2026-10-10", kind: "internal", basis: "Aftalt", certainty: "confirmed" } }), context("dated"));
    createTask(db, draft({ title: "Udateret" }), context("undated"));
    expect(listTasks(db, { from: "2026-10-01", to: "2026-10-31" })).toHaveLength(1);
    expect(listTasks(db, { undated: true, unassigned: true })[0]?.title).toBe("Udateret");
    expect(listTasks(db, { search: "AFSTEM" })).toHaveLength(1);
  });
  test("event, receipt and notification storage is immutable", () => {
    const { db } = fixture(); createTask(db, draft(), context("create"));
    for (const table of ["rm_task_events", "rm_task_board_events", "rm_task_series_events", "rm_task_receipts", "rm_task_notifications"]) {
      expect(db.query("SELECT name FROM sqlite_master WHERE type='trigger' AND name IN (?,?)").all(`${table}_no_update`, `${table}_no_delete`)).toHaveLength(2);
    }
    expect(() => db.exec("UPDATE rm_task_events SET version=8")).toThrow("append-only");
    expect(() => db.exec("DELETE FROM rm_task_receipts")).toThrow("append-only");
    expect(getTask(db, listTasks(db)[0]!.taskId)?.version).toBe(1);
  });
});
