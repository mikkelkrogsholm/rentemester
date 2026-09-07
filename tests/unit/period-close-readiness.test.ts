import { afterEach, describe, expect, test } from "bun:test";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { openDb, migrate } from "../../src/core/db";
import { postJournalEntry, seedAccounts } from "../../src/core/ledger";
import { seedNativeAccountRoles } from "../../src/core/account-roles";
import { createPeriodCloseReadinessPacket, projectHumanReadiness, reviewPeriodCloseReadiness } from "../../src/core/period-close-readiness";
import { closeAccountingPeriod, reopenAccountingPeriod } from "../../src/core/periods";
import { ensureCompanyDirs } from "../../src/core/paths";
import { createSystemBackup } from "../../src/core/system-backups";
import { restoreSystemBackup } from "../../src/core/system-restore";
import { CURRENT_SCHEMA_VERSION, readSchemaMigrations } from "../../src/core/schema-version";
import { recordImportedReceivableSchedule } from "../../src/core/imported-receivables";
import { postIssuedInvoiceToLedger } from "../../src/core/invoice-booking";

const roots: string[] = [];
function fixture() { const root = mkdtempSync(join(tmpdir(), "rm-period-close-")); roots.push(root); const db = openDb(join(root, "ledger.sqlite")); migrate(db); seedAccounts(db); seedNativeAccountRoles(db); return db; }
afterEach(() => { while (roots.length) rmSync(roots.pop()!, { recursive: true, force: true }); });

/** Canonical synthetic bank evidence: statement row plus its matching posted
 * bank journal. This deliberately exercises the public ledger write, rather
 * than smuggling a balance into a close fixture. */
function bookStatementBankMovement(db: ReturnType<typeof openDb>, date: string, amount: number, balanceAfter: number | null) {
  db.run("INSERT INTO bank_transactions(transaction_date,text,amount,currency,balance_after) VALUES(?,?,?,?,?)", date, `synthetic bank ${date} ${amount}`, amount, "DKK", balanceAfter);
  const bank = db.query("SELECT id FROM bank_transactions ORDER BY id DESC LIMIT 1").get() as { id: number };
  const result = postJournalEntry(db, {
    transactionDate: date, text: `Synthetic bank evidence ${bank.id}`, sourceBankTransactionId: bank.id,
    lines: amount >= 0
      ? [{ accountNo: "2000", debitAmount: amount }, { accountNo: "5000", creditAmount: amount }]
      : [{ accountNo: "2000", creditAmount: -amount }, { accountNo: "5000", debitAmount: -amount }],
  });
  expect(result.ok, result.ok ? "" : result.errors.join("; ")).toBe(true);
}

function dkkControl(db: ReturnType<typeof openDb>, start = "2025-01-01", end = "2025-01-31") {
  return createPeriodCloseReadinessPacket(db, { periodStart: start, periodEnd: end }).items.find((item) => item.code === "DKK_CONTROL_ACCOUNTS")!;
}

describe("#580 period-close readiness", () => {
  test("#656 projects immutable readiness into the common human vocabulary", () => {
    const packet = (items: any[]) => ({ version: 4 as const, periodStart: "2025-01-01", periodEnd: "2025-01-31", cutoff: "2025-01-31", controlsRun: [], items, blockers: 0, warnings: 0, hash: "x" });
    expect(projectHumanReadiness(packet([]))).toMatchObject({ status: "Klar", openControls: 0 });
    expect(projectHumanReadiness(packet([]), "reported").status).toBe("Lukket/endelig");
    expect(projectHumanReadiness(packet([{ status: "warning", waivable: true }])) .status).toBe("Kræver stillingtagen");
    expect(projectHumanReadiness(packet([{ status: "blocked", waivable: true }])) .status).toBe("Kræver stillingtagen");
    expect(projectHumanReadiness(packet([{ status: "blocked", waivable: false }])) .status).toBe("Ikke klar");
    expect(projectHumanReadiness(packet([{ status: "unavailable", waivable: false }])) .status).toBe("Ikke klar");
  });
  test("readiness is deterministic and does not persist a packet or audit event", () => {
    const db = fixture();
    const before = db.query("SELECT COUNT(*) AS n FROM period_close_readiness_packets").get() as { n: number };
    const first = createPeriodCloseReadinessPacket(db, { periodStart: "2025-01-01", periodEnd: "2025-01-31" });
    const second = createPeriodCloseReadinessPacket(db, { periodStart: "2025-01-01", periodEnd: "2025-01-31" });
    expect(second).toEqual(first);
    expect(db.query("SELECT COUNT(*) AS n FROM period_close_readiness_packets").get()).toEqual(before);
    expect(first.items).toHaveLength(first.controlsRun.length);
    expect(first.items.every((item) => item.sourceHash.length === 64)).toBe(true);
    db.close();
  });
  test("persists a replay-safe packet, rejects stale/missing hashes, and records a normal decision", () => {
    const db = fixture(); const packet = createPeriodCloseReadinessPacket(db, { periodStart: "2025-01-01", periodEnd: "2025-01-31" });
    expect(closeAccountingPeriod(db, { periodStart: "2025-01-01", periodEnd: "2025-01-31", kind: "custom", readinessPacketHash: "0".repeat(64) }).errors).toContain("PERIOD_CLOSE_PACKET_STALE_OR_MISSING");
    const review = reviewPeriodCloseReadiness(db, { packet, reviewerActor: "user:test", reviewerPrincipal: { kind: "local-trusted", subjectId: "test" } });
    const closed = closeAccountingPeriod(db, { periodStart: "2025-01-01", periodEnd: "2025-01-31", kind: "custom", readinessPacketHash: packet.hash, readinessReviewId: review.id, createdBy: "user:test" });
    expect(closed.ok).toBe(true);
    expect(db.query("SELECT decision FROM period_close_decisions").all()).toEqual([{ decision: "closed" }]);
    expect(() => db.run("UPDATE period_close_readiness_packets SET cutoff='x'")).toThrow(); db.close();
  });

  test("fails closed when DKK control-account assurance is unavailable, while a supported zero-activity ledger closes", () => {
    const clean = fixture();
    const cleanPacket = createPeriodCloseReadinessPacket(clean, { periodStart: "2025-06-01", periodEnd: "2025-06-30" });
    expect(cleanPacket.items.find((item) => item.code === "RECEIVABLE_OUTSTANDING")?.status).toBe("passed");
    expect(cleanPacket.items.find((item) => item.code === "DKK_CONTROL_ACCOUNTS")?.status).toBe("passed");
    const cleanReview = reviewPeriodCloseReadiness(clean, { packet: cleanPacket, reviewerActor: "user:test", reviewerPrincipal: { kind: "local-trusted", subjectId: "test" } });
    expect(closeAccountingPeriod(clean, { periodStart: "2025-06-01", periodEnd: "2025-06-30", kind: "custom", readinessPacketHash: cleanPacket.hash, readinessReviewId: cleanReview.id, createdBy: "user:test" }).ok).toBe(true);
    clean.close();

    const db = fixture();
    // A pre-period bank movement is outside the bank-item control's scope,
    // but its balance remains part of the DKK control-account assurance as of
    // cutoff. Without an independent reconciliation it must fail closed.
    db.run("INSERT INTO bank_transactions(transaction_date,text,amount,currency) VALUES('2025-05-31','synthetic opening movement',100,'DKK')");
    const packet = createPeriodCloseReadinessPacket(db, { periodStart: "2025-06-01", periodEnd: "2025-06-30" });
    expect(packet.items.find((item) => item.code === "DKK_CONTROL_ACCOUNTS")?.status).toBe("unavailable");
    const review = reviewPeriodCloseReadiness(db, { packet, reviewerActor: "user:test", reviewerPrincipal: { kind: "local-trusted", subjectId: "test" } });
    const rejected = closeAccountingPeriod(db, { periodStart: "2025-06-01", periodEnd: "2025-06-30", kind: "custom", readinessPacketHash: packet.hash, readinessReviewId: review.id, createdBy: "user:test" });
    expect(rejected.ok).toBe(false);
    expect(rejected.errors).toEqual(["PERIOD_CLOSE_ASSURANCE_UNAVAILABLE:1"]);
    expect(db.query("SELECT COUNT(*) AS n FROM accounting_periods").get()).toEqual({ n: 0 });
    db.close();
  });

  test("does not let force waive unavailable independent control reconciliation", () => {
    const db = fixture(); db.run("INSERT INTO bank_transactions(transaction_date,text,amount,currency) VALUES('2025-01-02','synthetic',100,'DKK')");
    const packet = createPeriodCloseReadinessPacket(db, { periodStart: "2025-01-01", periodEnd: "2025-01-31" }); expect(packet.items.map(x => x.code)).toContain("BANK_UNRECONCILED");
    expect(closeAccountingPeriod(db, { periodStart: "2025-01-01", periodEnd: "2025-01-31", kind: "custom", readinessPacketHash: packet.hash, createdBy: "user:test" }).ok).toBe(false);
    const review = reviewPeriodCloseReadiness(db, { packet, reviewerActor: "user:test", reviewerPrincipal: { kind: "local-trusted", subjectId: "test" } });
    const forced = closeAccountingPeriod(db, { periodStart: "2025-01-01", periodEnd: "2025-01-31", kind: "custom", readinessPacketHash: packet.hash, readinessReviewId: review.id, force: true, forceAuthorization: { principal: { kind: "local-trusted", subjectId: "test" }, permissions: ["company.period.force-close"] }, forceConfirmed: true, forceReason: "synthetic waiver", createdBy: "user:test" });
    expect(forced.ok).toBe(false); expect(forced.errors).toContain("PERIOD_CLOSE_HAS_NONWAIVABLE_BLOCKERS"); db.close();
  });

  test("reconciles mapped bank ledger balance to an unambiguous DKK statement endpoint with source IDs", () => {
    const db = fixture();
    bookStatementBankMovement(db, "2025-01-15", 125.55, 125.55);
    const item = dkkControl(db);
    expect(item.status).toBe("passed");
    expect(item.waivable).toBe(false);
    expect(item.evidence).toContainEqual(expect.objectContaining({ role: "bank", ledgerBalanceDkk: 125.55, sourceBalanceDkk: 125.55, differenceDkk: 0, sourceCount: 1 }));
    db.close();
  });

  test("blocks an exact-cent DKK bank control mismatch", () => {
    const db = fixture();
    bookStatementBankMovement(db, "2025-01-15", 125.55, 125.54);
    const item = dkkControl(db);
    expect(item.status).toBe("blocked");
    expect(item.amount).toBe(0.01);
    expect(item.evidence).toContainEqual(expect.objectContaining({ role: "bank", differenceDkk: 0.01 }));
    db.close();
  });

  test("fails closed for missing or ambiguous latest DKK statement balances", () => {
    const missing = fixture();
    bookStatementBankMovement(missing, "2025-01-15", 100, null);
    expect(dkkControl(missing).status).toBe("unavailable");
    missing.close();

    const ambiguous = fixture();
    bookStatementBankMovement(ambiguous, "2025-01-15", 100, 100);
    bookStatementBankMovement(ambiguous, "2025-01-15", 100, 100);
    const item = dkkControl(ambiguous);
    expect(item.status).toBe("unavailable");
    expect(item.evidence[0]).toEqual(expect.objectContaining({ detail: expect.stringContaining("ambiguous") }));
    ambiguous.close();
  });

  test("uses opening statement and ledger balances as of the cutoff, not only in-period activity", () => {
    const db = fixture();
    bookStatementBankMovement(db, "2024-12-31", 80, 80);
    const item = dkkControl(db, "2025-01-01", "2025-01-31");
    expect(item.status).toBe("passed");
    expect(item.evidence).toContainEqual(expect.objectContaining({ role: "bank", ledgerBalanceDkk: 80, sourceBalanceDkk: 80 }));
    db.close();
  });

  test("combines imported claims only through their exact cut-over and rejects native overlap", () => {
    const db = fixture();
    const h = (letter: string) => letter.repeat(64);
    db.query("INSERT INTO dinero_import_sources(id,raw_sha256,raw_size_bytes,canonical_listing_sha256,canonical_listing_count) VALUES(1,?,1,?,0)").run(h("a"),h("b"));
    db.query("INSERT INTO dinero_import_inventories(id,source_id,source_raw_sha256,canonical_listing_sha256,canonical_listing_count,entry_count,total_size_bytes) VALUES(1,1,?,?,0,0,0)").run(h("a"),h("b"));
    db.query("INSERT INTO dinero_import_attempts(id,inventory_id,source_id,source_raw_sha256,parser_contract,actor,cutover_date,outcome,result_sha256) VALUES(1,1,1,?,'synthetic-v1','agent:test','2025-01-01','accepted',?)").run(h("a"),h("c"));
    expect(recordImportedReceivableSchedule(db,1,{contract:"rentemester-imported-receivables-v1",sourceDocumentHash:h("d"),invoices:[{id:"IMPORTED-1",invoiceDate:"2025-01-01",grossAmount:100,controlAccountNo:"1100",recognitionRef:"opening",documentHash:h("e"),payments:[]}]},"2025-01-31").ok).toBe(true);
    const importedPosting=postJournalEntry(db,{transactionDate:"2025-01-01",text:"Imported receivable control",lines:[{accountNo:"1100",debitAmount:100},{accountNo:"5000",creditAmount:100}]});
    expect(importedPosting.ok, importedPosting.errors.join("; ")).toBe(true);

    const addNative = (date:string, amount:number, hash:string) => {
      const net=amount/1.25;
      const payload={invoiceType:"full",vatTreatment:"standard",issueDate:date,currency:"DKK",lines:[{description:"Synthetic service",quantity:1,unitPriceExVat:net,lineTotalExVat:net}],totals:{netAmount:net,vatRate:0.25,vatAmount:amount-net,grossAmount:amount}};
      const document=db.query("INSERT INTO documents(source,sha256_hash,document_type,invoice_date,amount_inc_vat,vat_amount,invoice_no,currency,payload_json) VALUES('synthetic',?,'issued_invoice',?,?,?,?, 'DKK',?) RETURNING id").get(hash,date,amount,amount-net,`NATIVE-${date}-${amount}`,JSON.stringify(payload)) as {id:number};
      const posted=postIssuedInvoiceToLedger(db,{invoiceDocumentId:document.id,transactionDate:date});
      expect(posted.ok, posted.errors.join("; ")).toBe(true);
    };
    addNative("2025-02-01",50,h("f"));
    const combined=dkkControl(db,"2025-01-01","2025-02-28");
    expect(combined.status).toBe("passed");
    expect(combined.evidence.find((row)=>row.role==="debtors")).toEqual(expect.objectContaining({sourceBalanceDkk:150}));

    addNative("2025-01-15",10,h("1"));
    const overlap=dkkControl(db,"2025-01-01","2025-02-28");
    expect(overlap.status).toBe("unavailable");
    expect(overlap.evidence[0]).toEqual(expect.objectContaining({detail:"control execution failed"}));
    db.close();
  });

  test("reopen appends and supersedes history", () => {
    const db = fixture(); const packet = createPeriodCloseReadinessPacket(db, { periodStart: "2025-02-01", periodEnd: "2025-02-28" }); const review = reviewPeriodCloseReadiness(db, { packet, reviewerActor: "user:test", reviewerPrincipal: { kind: "local-trusted", subjectId: "test" } }); const closed = closeAccountingPeriod(db, { periodStart: "2025-02-01", periodEnd: "2025-02-28", kind: "custom", readinessPacketHash: packet.hash, readinessReviewId: review.id, createdBy: "user:test" });
    const reopened = reopenAccountingPeriod(db, { periodStart: "2025-02-01", periodEnd: "2025-02-28", kind: "custom", reason: "synthetic correction", createdBy: "user:test" }); expect(reopened.ok).toBe(true);
    expect(db.query("SELECT decision,supersedes_decision_id FROM period_close_decisions ORDER BY id").all()).toEqual([{ decision: "closed", supersedes_decision_id: null }, { decision: "reopened", supersedes_decision_id: closed.periodId }]); db.close();
  });

  test("v24 upgrade plus signed backup/restore preserve append-only close evidence", () => {
    const root = mkdtempSync(join(tmpdir(), "rm-period-close-v24-"));
    const restored = join(tmpdir(), `rm-period-close-restored-${Date.now()}-${Math.random()}`);
    const paths = ensureCompanyDirs(root); const db = openDb(paths.db); migrate(db); seedAccounts(db); seedNativeAccountRoles(db);
    try {
      const packet = createPeriodCloseReadinessPacket(db, { periodStart: "2025-03-01", periodEnd: "2025-03-31" });
      // Model the exact v24 state: v22 packet/decision/open-item evidence is
      // present, the v25 review table and its migration identity are absent.
      db.exec("DROP TABLE period_close_reviews");
      db.run("DELETE FROM schema_migrations WHERE id>24");
      db.run("INSERT INTO period_close_readiness_packets(packet_hash,period_start,period_end,cutoff,packet_json) VALUES(?,?,?,?,?)", packet.hash, packet.periodStart, packet.periodEnd, packet.cutoff, JSON.stringify(packet));
      db.run("INSERT INTO accounting_periods(period_start,period_end,kind,status,closed_at) VALUES('2025-03-01','2025-03-31','custom','closed',CURRENT_TIMESTAMP)");
      db.run("INSERT INTO period_close_decisions(period_id,packet_hash,decision,actor) VALUES(1,?,'forced_closed','user:synthetic')", packet.hash);
      db.run("INSERT INTO period_close_open_items(decision_id,period_id,packet_hash,code,severity,count,amount,evidence_json,reason,actor) VALUES(1,1,?,'BANK_UNRECONCILED','blocker',1,10,'[]','synthetic','user:synthetic')", packet.hash);
      migrate(db);
      expect(readSchemaMigrations(db)).toHaveLength(CURRENT_SCHEMA_VERSION);
      const review = reviewPeriodCloseReadiness(db, { packet, reviewerActor: "user:synthetic", reviewerPrincipal: { kind: "local-trusted", subjectId: "synthetic" } });
      expect(review.id).toBeGreaterThan(0);
      const backup = createSystemBackup(db, root, { createdAt: "2026-01-01T00:00:00.000Z" });
      expect(backup.ok).toBe(true);
      const restore = restoreSystemBackup({ backupDir: backup.backupDir!, targetCompanyRoot: restored });
      expect(restore.ok).toBe(true);
      const check = openDb(ensureCompanyDirs(restored).db);
      try {
        expect(check.query("SELECT COUNT(*) AS n FROM period_close_readiness_packets").get()).toEqual({ n: 1 });
        expect(check.query("SELECT COUNT(*) AS n FROM period_close_reviews").get()).toEqual({ n: 1 });
        expect(check.query("SELECT decision FROM period_close_decisions").all()).toEqual([{ decision: "forced_closed" }]);
        expect(check.query("SELECT code,status FROM period_close_open_items").all()).toEqual([{ code: "BANK_UNRECONCILED", status: "open" }]);
      } finally { check.close(); }
    } finally { db.close(); rmSync(root, { recursive: true, force: true }); if (existsSync(restored)) rmSync(restored, { recursive: true, force: true }); }
  });

  test("independent SQLite connections serialize close and a deterministic decision fault leaves no partial effects", () => {
    const root = mkdtempSync(join(tmpdir(), "rm-period-close-atomic-"));
    const path = join(root, "ledger.sqlite"); const db = openDb(path); migrate(db); seedAccounts(db); seedNativeAccountRoles(db);
    const packet = createPeriodCloseReadinessPacket(db, { periodStart: "2025-04-01", periodEnd: "2025-04-30" });
    const review = reviewPeriodCloseReadiness(db, { packet, reviewerActor: "user:synthetic", reviewerPrincipal: { kind: "local-trusted", subjectId: "synthetic" } });
    const input = { periodStart: "2025-04-01", periodEnd: "2025-04-30", kind: "custom" as const, readinessPacketHash: packet.hash, readinessReviewId: review.id, createdBy: "user:synthetic" };
    const concurrent = openDb(path);
    concurrent.exec("PRAGMA busy_timeout = 1");
    try {
      // A second independent connection cannot observe a partial close while
      // the first owns SQLite's immediate writer lock.
      db.exec("BEGIN IMMEDIATE");
      expect(() => closeAccountingPeriod(concurrent, input)).toThrow(/locked|busy/i);
      db.exec("ROLLBACK");
      expect(concurrent.query("SELECT COUNT(*) AS n FROM accounting_periods").get()).toEqual({ n: 0 });
      expect(closeAccountingPeriod(concurrent, input).ok).toBe(true);
      expect(closeAccountingPeriod(db, input).errors).toContain("PERIOD_CLOSE_PACKET_STALE_OR_MISSING");
      // A controlled fault in the decision write rolls back lifecycle and
      // audit evidence together; no close without its decision can escape.
      const next = createPeriodCloseReadinessPacket(db, { periodStart: "2025-05-01", periodEnd: "2025-05-31" });
      const nextReview = reviewPeriodCloseReadiness(db, { packet: next, reviewerActor: "user:synthetic" });
      db.exec("CREATE TRIGGER fail_period_close_decision BEFORE INSERT ON period_close_decisions WHEN NEW.packet_hash = '" + next.hash + "' BEGIN SELECT RAISE(ABORT, 'synthetic decision fault'); END");
      expect(() => closeAccountingPeriod(db, { ...input, periodStart: "2025-05-01", periodEnd: "2025-05-31", readinessPacketHash: next.hash, readinessReviewId: nextReview.id })).toThrow("synthetic decision fault");
      expect(db.query("SELECT COUNT(*) AS n FROM accounting_periods WHERE period_start='2025-05-01'").get()).toEqual({ n: 0 });
      expect(db.query("SELECT COUNT(*) AS n FROM audit_log WHERE message LIKE '%2025-05-01%'").get()).toEqual({ n: 0 });
      expect(db.query("SELECT COUNT(*) AS n FROM period_close_decisions WHERE packet_hash=?", next.hash).get()).toEqual({ n: 0 });
      expect(db.query("SELECT COUNT(*) AS n FROM period_close_open_items WHERE packet_hash=?", next.hash).get()).toEqual({ n: 0 });
    } finally { concurrent.close(); db.close(); rmSync(root, { recursive: true, force: true }); }
  });
});
