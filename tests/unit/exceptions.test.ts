// Tests: src/core/exceptions.ts
import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { ensureCompanyDirs } from "../../src/core/paths";
import { openDb, migrate } from "../../src/core/db";
import { seedAccounts } from "../../src/core/ledger";
import { importBankCsv } from "../../src/core/bank";
import { listExceptions, recordException, resolveOpenExceptionsForBankTransaction, syncUnmatchedBankTransactionExceptions } from "../../src/core/exceptions";

import { cleanupDir } from "../helpers/cleanup";
describe("exceptions workflow", () => {
  test("syncs unmatched bank transactions once and resolves them without creating duplicates", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-exceptions-"));
    const company = ensureCompanyDirs(root);
    const db = openDb(company.db);
    migrate(db);
    seedAccounts(db);
    db.run(`INSERT INTO companies (id, name, cvr, fiscal_year_start_month, fiscal_year_label_strategy) VALUES (1, 'Rentemester ApS', 'DK12345678', 1, 'end-year')`);

    const csv = join(root, "bank.csv");
    writeFileSync(csv, "transaction_date,text,amount\n2026-05-18,Customer payment,2500\n");
    const imported = importBankCsv(db, root, csv);
    expect(imported.ok).toBe(true);

    const firstSync = syncUnmatchedBankTransactionExceptions(db);
    const secondSync = syncUnmatchedBankTransactionExceptions(db);
    expect(firstSync.created).toBe(1);
    expect(secondSync.created).toBe(0);

    const before = listExceptions(db, { status: "open" });
    expect(before.count).toBe(1);
    expect(before.rows[0].type).toBe("UNMATCHED_BANK_TRANSACTION");
    expect(before.rows[0].sourceEvidence.bankTransactionId).toBe(1);

    const resolved = resolveOpenExceptionsForBankTransaction(db, 1, "Resolved automatically by test workflow", "agent:test");
    expect(resolved.ok).toBe(true);
    expect(resolved.resolvedCount).toBe(1);

    const after = listExceptions(db, { status: "resolved" });
    expect(after.count).toBe(1);
    expect(after.rows[0].resolutionNote).toContain("Resolved automatically by test workflow");
    expect(after.rows[0].resolvedBy).toBe("agent:test");

    db.close();
    cleanupDir(root);
  });

  // #237: the unmatched-bank exception text must be sign-aware. A money-IN
  // line (customer payment / payout) is income, not a deductible expense.
  test("describes a money-in bank line as income, not as a deductible expense", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-exc-income-"));
    const company = ensureCompanyDirs(root);
    const db = openDb(company.db);
    migrate(db);
    seedAccounts(db);

    const csv = join(root, "bank.csv");
    writeFileSync(csv, "transaction_date,text,amount\n2026-05-18,Stripe payout,4200\n");
    expect(importBankCsv(db, root, csv).ok).toBe(true);

    expect(syncUnmatchedBankTransactionExceptions(db).created).toBe(1);

    const listed = listExceptions(db, { status: "open" });
    expect(listed.count).toBe(1);
    const row = listed.rows[0];
    // Income wording — never the expense/momsfradrag sentence.
    expect(row.message).toContain("Indbetalingen \"Stripe payout\"");
    expect(row.requiredAction).toContain("indtægten");
    expect(row.requiredAction).not.toContain("udgiften kan ikke bogføres");
    expect(row.requiredAction).not.toContain("momsen ikke fratrækkes");

    db.close();
    cleanupDir(root);
  });

  // #237: a money-OUT line stays expense-shaped and keeps the momsfradrag text.
  test("describes a money-out bank line as an expense", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-exc-expense-"));
    const company = ensureCompanyDirs(root);
    const db = openDb(company.db);
    migrate(db);
    seedAccounts(db);

    const csv = join(root, "bank.csv");
    writeFileSync(csv, "transaction_date,text,amount\n2026-05-18,Kontorartikler,-850\n");
    expect(importBankCsv(db, root, csv).ok).toBe(true);

    expect(syncUnmatchedBankTransactionExceptions(db).created).toBe(1);

    const row = listExceptions(db, { status: "open" }).rows[0];
    expect(row.message).toContain("Banktransaktionen \"Kontorartikler\"");
    expect(row.requiredAction).toContain("momsen ikke fratrækkes");

    db.close();
    cleanupDir(root);
  });

  // #237: when a matching bilag IS ingested, the text must name the real
  // reason it still needs attention — never falsely claim "no bilag found".
  test("names the real reason when a matching restaurant bilag exists", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-exc-bilag-"));
    const company = ensureCompanyDirs(root);
    const db = openDb(company.db);
    migrate(db);
    seedAccounts(db);

    // A restaurant receipt of exactly 1.205,00 kr. is ingested as a document.
    db.run(
      `INSERT INTO documents (document_no, source, sha256_hash, supplier_name, amount_inc_vat, document_type, delivery_description)
       VALUES ('BILAG-501', 'email', 'hash-restaurant-501', 'Restaurant Kanalen', 1205.00, 'cash_register_receipt', 'Frokost')`,
    );
    const csv = join(root, "bank.csv");
    writeFileSync(csv, "transaction_date,text,amount\n2026-05-18,Restaurant Kanalen,-1205\n");
    expect(importBankCsv(db, root, csv).ok).toBe(true);

    expect(syncUnmatchedBankTransactionExceptions(db).created).toBe(1);

    const row = listExceptions(db, { status: "open" }).rows[0];
    // Does NOT falsely say no bilag was found.
    expect(row.message).not.toContain("Der er ikke fundet et bilag");
    // Names the bilag and the real reason: missing purpose + attendees.
    expect(row.message).toContain("bilag BILAG-501");
    expect(row.message).toContain("mangler formål og deltagere");
    expect(row.relatedDocumentId).toBeGreaterThan(0);

    db.close();
    cleanupDir(root);
  });

  // #264: re-running the sync after a matching bilag was ingested must not
  // attempt a forbidden UPDATE of the immutable exception row. The stale
  // exception is resolved and a fresh, corrected one is opened in its place.
  test("re-syncing after a bilag is ingested resolves the stale exception and opens a fresh one", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-exc-restale-"));
    const company = ensureCompanyDirs(root);
    const db = openDb(company.db);
    migrate(db);
    seedAccounts(db);

    const csv = join(root, "bank.csv");
    writeFileSync(csv, "transaction_date,text,amount\n2026-05-18,Restaurant Kanalen,-1205\n");
    expect(importBankCsv(db, root, csv).ok).toBe(true);

    // First sync: no bilag yet — generic "no bilag found" exception.
    const first = syncUnmatchedBankTransactionExceptions(db);
    expect(first.ok).toBe(true);
    expect(first.created).toBe(1);
    const original = listExceptions(db, { status: "open" }).rows[0];
    expect(original.message).toContain("Der er ikke fundet et bilag");

    // A matching restaurant receipt is ingested afterwards.
    db.run(
      `INSERT INTO documents (document_no, source, sha256_hash, supplier_name, amount_inc_vat, document_type, delivery_description)
       VALUES ('BILAG-501', 'email', 'hash-restaurant-501', 'Restaurant Kanalen', 1205.00, 'cash_register_receipt', 'Frokost')`,
    );

    // Second sync must NOT throw an immutability error.
    const second = syncUnmatchedBankTransactionExceptions(db);
    expect(second.ok).toBe(true);

    // Exactly one open exception, with the corrected message naming the bilag.
    const open = listExceptions(db, { status: "open" });
    expect(open.count).toBe(1);
    expect(open.rows[0].message).not.toContain("Der er ikke fundet et bilag");
    expect(open.rows[0].message).toContain("bilag BILAG-501");
    expect(open.rows[0].relatedDocumentId).toBeGreaterThan(0);

    // The stale exception is resolved, not mutated — audit chain intact.
    const all = listExceptions(db, { status: "all" });
    const stale = all.rows.find((r) => r.id === original.id)!;
    expect(stale.status).toBe("resolved");
    expect(stale.message).toContain("Der er ikke fundet et bilag");

    // A third sync is idempotent — no further churn.
    const third = syncUnmatchedBankTransactionExceptions(db);
    expect(third.ok).toBe(true);
    expect(listExceptions(db, { status: "open" }).count).toBe(1);

    db.close();
    cleanupDir(root);
  });

  // #269: a non-DKK bank line must show its amount in its own currency
  // ("100,00 EUR"), never as a DKK-formatted figure with "kr." plus the
  // currency code tacked on ("100,00 kr. EUR"), which is numerically wrong.
  test("formats a non-DKK bank line amount in its own currency, not as kroner", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-exc-fx-"));
    const company = ensureCompanyDirs(root);
    const db = openDb(company.db);
    migrate(db);
    seedAccounts(db);

    const csv = join(root, "bank.csv");
    writeFileSync(
      csv,
      "transaction_date,text,amount,currency,amount_dkk,fx_rate_to_dkk\n" +
        "2026-05-18,Foreign supplier,-100,EUR,-745,7.45\n",
    );
    expect(importBankCsv(db, root, csv).ok).toBe(true);

    expect(syncUnmatchedBankTransactionExceptions(db).created).toBe(1);

    const row = listExceptions(db, { status: "open" }).rows[0];
    expect(row.message).toContain("100,00 EUR");
    expect(row.message).not.toContain("kr. EUR");

    db.close();
    cleanupDir(root);
  });

  test("records generic blocked-work exceptions with evidence", () => {
    const root = mkdtempSync(join(tmpdir(), "rentemester-exception-record-"));
    const db = openDb(ensureCompanyDirs(root).db);
    migrate(db);

    const inserted = recordException(db, {
      type: "DOCUMENT_INGEST_BLOCKED",
      severity: "medium",
      message: "Document ingest blocked for /tmp/bad.txt",
      requiredAction: "Fix metadata and retry.",
      sourceEvidence: { file: "/tmp/bad.txt", errors: ["sender.name is required"] },
      postingPreview: { retryCommand: "documents ingest --company <path> --file <file> --metadata <file.json>" },
    });
    expect(inserted.ok).toBe(true);

    const resolved = resolveOpenExceptionsForBankTransaction(db, 999);
    expect(resolved.ok).toBe(true);
    expect(resolved.resolvedCount).toBe(0);

    const listed = listExceptions(db, { status: "all" });
    expect(listed.count).toBe(1);
    expect(listed.rows[0].sourceEvidence.errors[0]).toContain("sender.name");

    db.close();
    cleanupDir(root);
  });
});
