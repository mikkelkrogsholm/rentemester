import type { CompanyBank } from "../../lib/types";
import { STATEMENT_COMPANY, STATEMENT_FISCAL_YEARS } from "./_shared";

export function bank(over: Partial<CompanyBank> = {}): CompanyBank {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    periodStart: "2026-01-01",
    periodEnd: "2026-12-31",
    accounts: [
      {
        id: 1,
        name: "Driftskonto",
        bankName: "Danske Bank",
        accountNo: "12345678",
        ledgerAccountNo: "55000",
      },
    ],
    bookedBalance: 41388.03,
    actualBalance: 23654.75,
    difference: 17733.28,
    bankStatementStatus: "known",
    transactions: [
      {
        id: 1,
        date: "2026-03-15",
        text: "Indbetaling faktura 1001",
        amount: 22286.28,
        runningBalance: 22286.28,
        reconciliationStatus: "matched",
        journalEntryNo: "B-2026-0001",
      },
      {
        id: 2,
        date: "2026-04-02",
        text: "Gebyr",
        amount: -45,
        runningBalance: 22241.28,
        reconciliationStatus: "unmatched",
        journalEntryNo: null,
      },
    ],
    matchedCount: 1,
    unmatchedCount: 1,
    ...over,
  };
}
