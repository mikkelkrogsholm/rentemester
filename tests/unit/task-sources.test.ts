import { Database } from "bun:sqlite";
import { afterEach, describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, renameSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { migrate } from "../../src/core/db";
import { companyPaths, ensureCompanyDirs } from "../../src/core/paths";
import { initWorkspace, registerWorkspaceCompany } from "../../src/core/workspace";
import { openWorkspaceControlDb } from "../../src/core/workspace-control";
import { seedAccounts, postJournalEntry } from "../../src/core/ledger";
import { addBankAccount } from "../../src/core/bank";
import { linkBankTransactionToJournal } from "../../src/core/bank-journal-reconciliation";
import { approveBookkeepingBatchPlan, createBookkeepingBatchRun, planBookkeepingBatch } from "../../src/core/bookkeeping-batch";
import { completeTask, getTask, listTasks, taskHistory } from "../../src/core/tasks";
import { checkTaskSource, SOURCE_COVERAGE, syncTaskSources } from "../../src/core/task-sources";

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
function fixture(input: { cadence?: "month" | "quarter" | "half-year" | null; fiscalStart?: number } = {}) {
  const root = mkdtempSync(join(tmpdir(), "rm-task-sources-")); roots.push(root);
  initWorkspace(root);
  registerWorkspaceCompany(root, { slug: "synthetic", name: "Synthetic Company", createdAt: "2026-01-01T00:00:00.000Z", archived: false });
  const paths = ensureCompanyDirs(join(root, "synthetic"));
  const ledger = new Database(paths.db); migrate(ledger); seedAccounts(ledger);
  ledger.query("INSERT INTO companies(id,name,vat_period_type,fiscal_year_start_month) VALUES(1,?,?,?)")
    .run("Synthetic Company", input.cadence === undefined ? "quarter" : input.cadence, input.fiscalStart ?? 1);
  const bank = addBankAccount(ledger, { name: "Synthetic Bank", slug: "synthetic", ledgerAccountNo: "2000" }).account!;
  const opening = postJournalEntry(ledger, { transactionDate: "2026-01-01", text: "Opening", createdBy: "agent:test",
    lines: [{ accountNo: "2000", debitAmount: 1 }, { accountNo: "5000", creditAmount: 1 }] });
  ledger.query("INSERT INTO opening_balances(cut_over_date,journal_entry_id,journal_entry_no) VALUES(?,?,?)").run("2026-01-01", opening.entryId, "OPENING");
  return { root, ledger, db: openWorkspaceControlDb(root), bank, paths };
}
const syncInput = { companySlugs: ["synthetic"], actor: "agent:test", principal: "synthetic", asOfDate: "2026-10-08" };
function insertBank(ledger: Database, bankAccountId: number, id: number, date = "2026-01-04") {
  ledger.query("INSERT INTO bank_transactions(id,transaction_date,text,amount,currency,transaction_hash,bank_account_id) VALUES(?,?,?,?,?,?,?)")
    .run(id, date, "Synthetic protected recipient 0101701234 account 12345678901234", -12, "DKK", `bank-${id}`, bankAccountId);
}

describe("canonical task sources", () => {
  test("reads beyond 100 workbench rows, uses stable ledger identity, deduplicates overlap and never changes ledger bytes", () => {
    const { root, ledger, db, bank, paths } = fixture({ cadence: null });
    for (let id = 1; id <= 105; id++) insertBank(ledger, bank.id, id);
    ledger.query("INSERT INTO exceptions(type,severity,status,related_bank_transaction_id,message) VALUES('missing_receipt','high','open',1,?)")
      .run("Synthetic protected 0101701234");
    ledger.close();
    const before = createHash("sha256").update(readFileSync(paths.db)).digest("hex");
    const first = syncTaskSources(db, root, syncInput);
    expect(first.errors).toEqual([]);
    const sources = listTasks(db, { showDone: true }).filter(task => task.source?.kind === "bank_work");
    expect(sources).toHaveLength(105);
    expect(sources.find(task => task.source?.ref === "1")?.source?.identity).toMatch(/^[a-f0-9]{32}:bank_work:1$/);
    expect(listTasks(db).filter(task => task.source?.kind === "exception")).toHaveLength(0);
    expect(JSON.stringify(sources)).not.toContain("0101701234");
    expect(JSON.stringify(sources)).not.toContain("12345678901234");
    const version = sources[0]!.version;
    expect(syncTaskSources(db, root, syncInput)).toMatchObject({ created: 0, updated: 0, resolved: 0 });
    expect(getTask(db, sources[0]!.taskId)?.version).toBe(version);
    expect(createHash("sha256").update(readFileSync(paths.db)).digest("hex")).toBe(before);
    db.close();
  });

  test("a card cannot solve missing evidence; authoritative reconciliation completes it and a recurring exception reopens its history", () => {
    const { root, ledger, db, bank } = fixture({ cadence: null }); insertBank(ledger, bank.id, 1);
    syncTaskSources(db, root, syncInput);
    const task = listTasks(db).find(item => item.source?.kind === "bank_work")!;
    expect(checkTaskSource(root, task).state).toBe("open");
    expect(() => completeTask(db, task.taskId, { outcome: "completed", note: "Moved card" }, {
      actor: "agent:test", principal: "synthetic", idempotencyKey: "invalid-close", expectedVersion: task.version,
      sourceCheck: checkTaskSource(root, task),
    })).toThrow("Authoritative source");
    const journal = postJournalEntry(ledger, { transactionDate: "2026-01-04", text: "Actual booking", createdBy: "agent:test",
      lines: [{ accountNo: "2000", creditAmount: 12 }, { accountNo: "5000", debitAmount: 12 }] });
    expect(linkBankTransactionToJournal(ledger, { bankTransactionId: 1, journalEntryId: Number(journal.entryId), matchMethod: "manual-review", createdBy: "agent:test" }).ok).toBe(true);
    expect(syncTaskSources(db, root, syncInput).resolved).toBe(1);
    const closed = getTask(db, task.taskId)!;
    expect(closed).toMatchObject({ status: "done", completion: { assurance: "product_verified" } });
    ledger.query("INSERT INTO exceptions(type,severity,status,related_bank_transaction_id,message) VALUES('review_needed','high','open',1,'Synthetic recurring problem')").run();
    expect(syncTaskSources(db, root, syncInput).reopened).toBe(1);
    expect(getTask(db, task.taskId)?.status).toBe("open");
    expect(taskHistory(db, task.taskId).some(event => event.task.completion?.assurance === "product_verified")).toBe(true);
    expect(listTasks(db, { showDone: true }).filter(item => item.source?.kind === "bank_work")).toHaveLength(1);
    ledger.close(); db.close();
  });

  test("missing ledger produces uncertainty and preserves open work rather than closing from disappearance", () => {
    const { root, ledger, db, bank, paths } = fixture({ cadence: null }); insertBank(ledger, bank.id, 1);
    syncTaskSources(db, root, syncInput);
    const task = listTasks(db).find(item => item.source?.kind === "bank_work")!;
    ledger.close(); renameSync(paths.db, `${paths.db}.unavailable`);
    expect(checkTaskSource(root, task).state).toBe("unknown");
    const result = syncTaskSources(db, root, syncInput);
    expect(result.errors).toHaveLength(1);
    expect(result.resolved).toBe(0);
    expect(getTask(db, task.taskId)).toMatchObject({ status: "open", verificationRequired: true, source: { state: "unknown" } });
    const version = getTask(db, task.taskId)!.version;
    syncTaskSources(db, root, syncInput);
    expect(getTask(db, task.taskId)?.version).toBe(version);
    db.close();
  });

  test("registered VAT includes zero and refund periods, historical cadence and official monthly holiday deadlines", () => {
    const { root, ledger, db } = fixture({ cadence: "month" });
    syncTaskSources(db, root, syncInput);
    const vat = listTasks(db).filter(task => task.source?.kind === "vat_filing");
    expect(vat).toHaveLength(10);
    expect(vat.find(task => task.period?.from === "2026-06-01")?.deadline?.date).toBe("2026-08-17");
    expect(vat.find(task => task.period?.from === "2026-03-01")?.deadline?.date).toBe("2026-04-27");
    expect(vat.find(task => task.period?.from === "2026-09-01")?.deadline?.date).toBe("2026-10-26");
    const april = vat.find(task => task.period?.from === "2026-04-01")!;
    ledger.query("INSERT INTO vat_filing_evidence_events(period_start,period_end,field_name,amount_dkk,evidence_ref,event_type,actor,principal,created_at) VALUES(?,?,?,?,?,'recorded',?,?,?)")
      .run("2026-04-01", "2026-04-30", "elafgift", 12, "synthetic-refund", "agent:test", "synthetic", "2026-05-01T00:00:00.000Z");
    expect(checkTaskSource(root, april).state).toBe("open");
    ledger.query("INSERT INTO accounting_periods(period_start,period_end,kind,status,reference) VALUES('2026-04-01','2026-04-30','vat_period','reported','synthetic-filing-receipt')").run();
    expect(checkTaskSource(root, april)).toMatchObject({ state: "resolved", evidence: [{ kind: "period", ref: "2026-04-01:2026-04-30" }] });
    expect(syncTaskSources(db, root, syncInput).resolved).toBeGreaterThanOrEqual(1);
    expect(getTask(db, april.taskId)?.status).toBe("done");
    ledger.close(); db.close();
  });

  test("cadence and fiscal year are company-specific; annual filing is visibly unconfirmed without classification", () => {
    const { root, ledger, db } = fixture({ cadence: "half-year", fiscalStart: 7 });
    syncTaskSources(db, root, syncInput);
    const tasks = listTasks(db);
    expect(tasks.filter(task => task.source?.kind === "vat_filing").map(task => task.period?.from)).toEqual(["2026-01-01", "2026-07-01"]);
    const annual = tasks.find(task => task.source?.kind === "annual_reporting")!;
    expect(annual).toMatchObject({ origin: "proposal", relevance: "unknown", deadline: null, period: { from: "2026-07-01", to: "2027-06-30" } });
    expect(checkTaskSource(root, annual).state).toBe("unknown");
    expect(SOURCE_COVERAGE.some(item => item.includes("ingen generel lovfrist"))).toBe(true);
    ledger.close(); db.close();
  });

  test("open exceptions in earlier closed periods remain actionable and ledger identity changes fail closed", () => {
    const { root, ledger, db } = fixture({ cadence: null });
    ledger.query("INSERT INTO exceptions(type,severity,status,message) VALUES('missing_document','high','open','Synthetic historic exception')").run();
    ledger.query("INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2025-01-01','2025-12-31','fiscal_year','closed')").run();
    syncTaskSources(db, root, syncInput);
    const task = listTasks(db).find(item => item.source?.kind === "exception")!;
    expect(task.status).toBe("open");
    const mismatched = { ...task, source: { ...task.source!, identity: `${"0".repeat(32)}:exception:${task.source!.ref}` } };
    expect(checkTaskSource(root, mismatched)).toMatchObject({ state: "unknown", reason: "Kildens ledger-identitet er ændret." });
    expect(listTasks(db).some(item => item.period?.from === "2025-01-01")).toBe(true);
    ledger.close(); db.close();
  });

  test("approval tasks follow the exact latest batch revision and never approve or apply it themselves", () => {
    const { root, ledger, db, bank } = fixture({ cadence: null }); insertBank(ledger, bank.id, 1);
    const plan = planBookkeepingBatch(ledger, { companyId: 1, accountingFrom: "2026-01-01", accountingTo: "2026-01-31", bankFrom: "2026-01-01", bankTo: "2026-01-31" });
    const run = createBookkeepingBatchRun(ledger, { ...plan, actor: "agent:planner", principal: { kind: "user", subjectId: "planner" }, runKey: "synthetic-approval" });
    syncTaskSources(db, root, syncInput);
    const task = listTasks(db).find(item => item.source?.kind === "batch_approval")!;
    expect(task.status).toBe("open");
    expect(checkTaskSource(root, task).state).toBe("open");
    expect((ledger.query("SELECT COUNT(*) count FROM bookkeeping_batch_revision_approvals").get() as { count: number }).count).toBe(0);
    const before = (ledger.query("SELECT COUNT(*) count FROM journal_entries").get() as { count: number }).count;
    approveBookkeepingBatchPlan(ledger, { runId: run.runId, planHash: plan.planHash, actor: "user:reviewer", principal: { kind: "user", subjectId: "reviewer" } });
    expect(syncTaskSources(db, root, syncInput).resolved).toBe(1);
    expect(getTask(db, task.taskId)).toMatchObject({ status: "done", completion: { assurance: "product_verified" } });
    expect((ledger.query("SELECT COUNT(*) count FROM journal_entries").get() as { count: number }).count).toBe(before);
    ledger.close(); db.close();
  });
});
