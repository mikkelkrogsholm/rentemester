import type { CompanyAssets } from "../../lib/types";
import { STATEMENT_COMPANY } from "./_shared";

/** Anlægskartoteket fixture (#336). */
export function assets(
  over: Partial<CompanyAssets> = {},
): CompanyAssets {
  return {
    slug: "acme-aps",
    company: STATEMENT_COMPANY,
    assets: [
      {
        assetId: 1,
        name: "MacBook Pro",
        category: "hardware",
        acquisitionDate: "2026-01-10",
        cost: 48000,
        usefulLifeMonths: 36,
        postedPeriods: 6,
        accumulatedDepreciation: 8000,
        netBookValue: 40000,
        status: "active",
        remainingPeriods: 30,
      },
      {
        assetId: 2,
        name: "Server rack",
        category: "hardware",
        acquisitionDate: "2024-03-01",
        cost: 24000,
        usefulLifeMonths: 24,
        postedPeriods: 24,
        accumulatedDepreciation: 24000,
        netBookValue: 0,
        status: "fully-depreciated",
        remainingPeriods: 0,
      },
    ],
    writeOffs: [
      {
        id: 1,
        name: "Tastatur",
        category: "smaaanskaffelser",
        acquisitionDate: "2026-02-01",
        writeOffDate: "2026-02-01",
        cost: 2500,
        expenseAccountNo: "3000",
        thresholdDkk: 33100,
        thresholdRuleSource: "AL §6 stk. 1 nr. 2 — småanskaffelser",
        note: null,
        purchaseDocumentId: 5,
        journalEntryId: 12,
      },
    ],
    totals: {
      cost: 72000,
      accumulatedDepreciation: 32000,
      netBookValue: 40000,
      activeCount: 1,
      fullyDepreciatedCount: 1,
      writeOffCount: 1,
      writeOffTotal: 2500,
    },
    ...over,
  };
}
