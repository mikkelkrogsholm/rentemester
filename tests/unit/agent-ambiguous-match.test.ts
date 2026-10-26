import { expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runAgentLoop } from "../../src/agent/loop";
import { importBankCsv } from "../../src/core/bank";
import { initialiseCompanyVolume } from "../../src/core/company";
import { openDb } from "../../src/core/db";
import { ingestDocument } from "../../src/core/documents";
import { companyPaths } from "../../src/core/paths";

function withMatchingDocuments(bankText: string, check: (root: string) => void): void {
  const root = mkdtempSync(join(tmpdir(), "rentemester-agent-ambiguity-"));
  try {
    initialiseCompanyVolume(root, { cvr: "DK12345678" });
    const db = openDb(companyPaths(root).db);
    try {
      for (const invoiceNo of ["TICKET-A", "TICKET-B"]) {
        const file = join(root, `${invoiceNo}.txt`);
        writeFileSync(file, `Synthetic train ticket ${invoiceNo}`);
        expect(ingestDocument(db, root, file, {
          source: "email", issueDate: "2026-05-15", invoiceNo,
          deliveryDescription: "Train travel", amountIncVat: 125, currency: "DKK", vatAmount: 25,
          sender: { name: "DSB Synthetic Travel", address: "Station 1", vatOrCvr: "DK11112222" },
          recipient: { name: "Synthetic Buyer ApS", address: "Testvej 1", vatOrCvr: "DK12345678" },
          paymentDetails: "Bank transfer",
        }).ok).toBe(true);
      }
      const bank = join(root, "bank.csv");
      writeFileSync(bank, `transaction_date,text,amount,currency\n2026-05-16,${bankText},-125,DKK\n`);
      expect(importBankCsv(db, root, bank).ok).toBe(true);
    } finally {
      db.close();
    }
    check(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("equally strong purchase matches require review and leave the bank and journal untouched on retries", () => {
  withMatchingDocuments("DSB Synthetic Travel", root => {
    for (let attempt = 0; attempt < 2; attempt++) {
      const report = runAgentLoop({ companyRoot: root, asOf: "2026-05-20" });
      expect(report.ok).toBe(true);
      expect(report.expensesBooked).toEqual([]);
      const ambiguous = report.openExceptions.filter(item => item.type === "AGENT_AMBIGUOUS_MATCH");
      expect(ambiguous).toHaveLength(1);
      expect(ambiguous[0]!.message).toContain("1, 2");
      const db = openDb(companyPaths(root).db);
      try {
        expect(db.query("SELECT COUNT(*) AS count FROM journal_entries").get()).toEqual({ count: 0 });
        expect(db.query("SELECT COUNT(*) AS count FROM bank_journal_reconciliations").get()).toEqual({ count: 0 });
      } finally {
        db.close();
      }
    }
  });
});

test("a stronger invoice-number match still books its identified document once", () => {
  withMatchingDocuments("DSB Synthetic Travel TICKET-B", root => {
    const report = runAgentLoop({ companyRoot: root, asOf: "2026-05-20" });
    expect(report.ok).toBe(true);
    expect(report.expensesBooked).toHaveLength(1);
    expect(report.expensesBooked[0]!.documentNo).toBe("TICKET-B");
    expect(report.openExceptions.some(item => item.type === "AGENT_AMBIGUOUS_MATCH")).toBe(false);
    expect(runAgentLoop({ companyRoot: root, asOf: "2026-05-20" }).expensesBooked).toEqual([]);
  });
});
