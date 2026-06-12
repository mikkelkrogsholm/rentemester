import type { CompanyDashboard, CompanyOverview } from "../../lib/types";
import { MONTHS } from "./_shared";

export function dashboard(over: Partial<CompanyDashboard> = {}): CompanyDashboard {
  return {
    slug: "acme-aps",
    asOf: "2026-05-20",
    company: {
      name: "Acme ApS",
      cvr: "DK12345678",
      country: "DK",
      currency: "DKK",
      fiscalYearStartMonth: 1,
      fiscalYearLabelStrategy: "calendar",
    },
    invoices: { count: 0, openTotal: 0, rows: [] },
    overdueInvoices: { count: 0, rows: [] },
    unlinkedBank: { count: 0 },
    exceptions: { count: 0, rows: [] },
    vat: {
      periodStart: "2026-04-01",
      periodEnd: "2026-06-30",
      netVatPayable: 0,
      daysRemaining: 41,
      errors: [],
    },
    backup: {
      backupsFound: true,
      latestBackupAt: "2026-05-19T00:00:00.000Z",
      daysSinceLatestBackup: 1,
      hasActivitySinceBackup: false,
    },
    audit: { ok: true, entryCount: 3, firstError: null },
    recentActivity: [],
    ...over,
  };
}

export function overview(over: Partial<CompanyOverview> = {}): CompanyOverview {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    archivedSource: null,
    company: {
      name: "Acme ApS",
      cvr: "DK12345678",
      country: "DK",
      currency: "DKK",
      fiscalYearStartMonth: 1,
      fiscalYearLabelStrategy: "end-year",
    },
    fiscalYears: [
      { label: "2026", start: "2026-01-01", end: "2026-12-31", source: "live" },
      { label: "2025", start: null, end: null, source: "archive" },
    ],
    profitAndLoss: {
      omsaetning: 17829.02,
      udgifter: 4594.2,
      resultat: 13234.82,
      months: MONTHS.map((label, i) => ({
        month: i + 1,
        label,
        income: i === 0 ? 17829.02 : 0,
        expense: i === 0 ? 4563.04 : 0,
      })),
    },
    bank: { balance: 41388.03, actualBalance: 23654.75, difference: 17733.28 },
    receivables: { openCount: 0, openTotal: 0 },
    vat: {
      periodStart: "2026-04-01",
      periodEnd: "2026-06-30",
      periodLabel: "Q2 2026",
      outputVat: 4457,
      outputVatAdjustment: 0,
      inputVat: 1086,
      payable: 3371,
      deadline: "2026-09-01",
      daysRemaining: 103,
    },
    exceptions: { count: 0, rows: [], groups: [] },
    recentEntries: [],
    lastPostedDate: "2026-02-27",
    keyFigures: { bruttomargin: 0.7423, egenkapitalandel: 0.9186 },
    ...over,
  };
}
