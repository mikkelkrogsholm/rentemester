// Test fixtures + a fetch mock that speaks the cockpit API's JSON envelope.

import { vi } from "vitest";
import type {
  CompanyArchiveYear,
  CompanyBalance,
  CompanyBank,
  CompanyCashflow,
  CompanyContacts,
  CompanyDashboard,
  CompanyDocuments,
  CompanyIncomeStatement,
  CompanyInvoices,
  CompanyJournal,
  CompanyMileage,
  CompanyMultiYear,
  CompanyObligations,
  CompanyOverview,
  CompanySettings,
  CompanySummary,
  CompanyTrialBalance,
  CompanyVat,
  FiscalYearEntry,
} from "../lib/types";

export function summary(over: Partial<CompanySummary> = {}): CompanySummary {
  return {
    slug: "acme-aps",
    name: "Acme ApS",
    cvr: "DK12345678",
    archived: false,
    ledgerMissing: false,
    fiscalYear: "2026",
    resultat: 13234.82,
    omsaetning: 17829.02,
    actualBankBalance: 23654.75,
    vat: { payable: 3371.2, deadline: "2026-09-01", daysRemaining: 103 },
    openTaskCount: 0,
    taskGroups: [],
    auditChainOk: true,
    openInvoiceCount: 0,
    openInvoiceTotal: 0,
    overdueInvoiceCount: 0,
    unlinkedBankCount: 0,
    openExceptionCount: 0,
    netVatPayable: 3371.2,
    ...over,
  };
}

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

const MONTHS = [
  "jan", "feb", "mar", "apr", "maj", "jun",
  "jul", "aug", "sep", "okt", "nov", "dec",
];

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

const STATEMENT_COMPANY = {
  name: "Acme ApS",
  cvr: "DK12345678",
  country: "DK",
  currency: "DKK",
  fiscalYearStartMonth: 1,
  fiscalYearLabelStrategy: "end-year",
};

const STATEMENT_FISCAL_YEARS = [
  { label: "2026", start: "2026-01-01", end: "2026-12-31", source: "live" as const },
  { label: "2025", start: null, end: null, source: "archive" as const },
];

export function incomeStatement(
  over: Partial<CompanyIncomeStatement> = {},
): CompanyIncomeStatement {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    archivedSource: null,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    income: [
      { accountNo: "1000", name: "Omsætning", amount: 17829.02, priorAmount: 0 },
    ],
    expense: [
      { accountNo: "3000", name: "Vareforbrug", amount: 4594.2, priorAmount: 0 },
    ],
    totalIncome: 17829.02,
    totalExpense: 4594.2,
    priorTotalIncome: 0,
    priorTotalExpense: 0,
    result: 13234.82,
    priorResult: 0,
    ...over,
  };
}

export function balance(over: Partial<CompanyBalance> = {}): CompanyBalance {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    archivedSource: null,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    asOfDate: "2026-12-31",
    assets: {
      lines: [
        {
          accountNo: "55000",
          name: "Bank",
          amount: 41388.03,
          priorAmount: 32000,
        },
      ],
      total: 41388.03,
      priorTotal: 32000,
    },
    liabilities: {
      lines: [
        {
          accountNo: "64000",
          name: "Salgsmoms",
          amount: 3371,
          priorAmount: 2500,
        },
      ],
      total: 3371,
      priorTotal: 2500,
    },
    equity: {
      // The period result is folded in as an "Årets resultat" line, so the
      // section total is the equity accounts (24782.21) plus the result.
      lines: [
        {
          accountNo: "51000",
          name: "Selskabskapital",
          amount: 24782.21,
          priorAmount: 24782.21,
        },
        {
          accountNo: "—",
          name: "Årets resultat",
          amount: 13234.82,
          priorAmount: 4717.79,
        },
      ],
      total: 38017.03,
      priorTotal: 29500,
    },
    periodResult: 13234.82,
    totalAssets: 41388.03,
    totalLiabilitiesAndEquity: 41388.03,
    priorTotalLiabilitiesAndEquity: 32000,
    balanced: true,
    ...over,
  };
}

export function trialBalance(
  over: Partial<CompanyTrialBalance> = {},
): CompanyTrialBalance {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    archivedSource: null,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    periodStart: "2026-01-01",
    periodEnd: "2026-12-31",
    rows: [
      {
        accountNo: "1000",
        name: "Omsætning",
        type: "income",
        debit: 0,
        credit: 17829.02,
        balance: -17829.02,
      },
      {
        accountNo: "55000",
        name: "Bank",
        type: "asset",
        debit: 41388.03,
        credit: 0,
        balance: 41388.03,
      },
    ],
    totalDebit: 59217.05,
    totalCredit: 59217.05,
    balanced: true,
    ...over,
  };
}

export function journal(over: Partial<CompanyJournal> = {}): CompanyJournal {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    archivedSource: null,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    periodStart: "2026-01-01",
    periodEnd: "2026-12-31",
    entries: [
      {
        id: 1,
        entryNo: "B-2026-0001",
        date: "2026-03-15",
        text: "Salg af ydelse",
        total: 22286.28,
        lines: [
          {
            accountNo: "55000",
            accountName: "Bank",
            debit: 22286.28,
            credit: 0,
            text: null,
          },
          {
            accountNo: "1000",
            accountName: "Omsætning",
            debit: 0,
            credit: 17829.02,
            text: null,
          },
          {
            accountNo: "64000",
            accountName: "Salgsmoms",
            debit: 0,
            credit: 4457.26,
            text: null,
          },
        ],
        documentId: 7,
        documentNo: "BIL-2026-0001",
      },
    ],
    accountFilter: null,
    ...over,
  };
}

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

export function vat(over: Partial<CompanyVat> = {}): CompanyVat {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    // A past, already-ended quarter (#301): closing it is the normal flow with
    // no "period not over yet" warning. Tests that exercise the future-end
    // warning override `periodEnd` with a date in the future.
    periodStart: "2026-01-01",
    periodEnd: "2026-03-31",
    periodLabel: "Q1 2026",
    outputVat: 4457,
    outputVatAdjustment: 0,
    inputVat: 1086,
    payable: 3371,
    deadline: "2026-06-01",
    daysRemaining: 30,
    // #303: the VAT period's effective lifecycle state. The fixture defaults to
    // an OPEN, not-yet-filing-ready period — the historical pre-#303 behaviour
    // — so the "Luk momsperiode" action is offered. Tests that need a closed
    // (reopenable) or reported period override these two fields.
    periodStatus: "open",
    momsangivelseReady: false,
    rubrikker: {
      salgsmoms: 4457,
      momsAfVarekobUdland: 0,
      momsAfYdelseskobUdland: 250,
      kobsmoms: 1086,
      momstilsvar: 3621,
      rubrikA: 1000,
      rubrikB: 0,
      rubrikC: 0,
    },
    ...over,
  };
}

export function obligations(
  over: Partial<CompanyObligations> = {},
): CompanyObligations {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    obligations: [
      {
        kind: "vat",
        label: "Moms — Q2 2026",
        amount: 3371,
        dueDate: "2026-09-01",
        daysRemaining: 103,
        accountNo: null,
      },
      {
        kind: "annual-report",
        label: "Årsrapport — regnskabsår 2026",
        amount: 0,
        dueDate: "2027-05-01",
        daysRemaining: 344,
        accountNo: null,
      },
      {
        kind: "corporation-tax",
        label: "Skyldig selskabsskat",
        amount: 2906.66,
        dueDate: "2027-11-01",
        daysRemaining: 529,
        accountNo: "63060",
      },
      {
        kind: "creditors",
        label: "Kreditorer (leverandørgæld)",
        amount: 1250,
        dueDate: null,
        daysRemaining: null,
        accountNo: "63000",
      },
    ],
    totalOwed: 7527.66,
    ...over,
  };
}

const MILEAGE_MONTH_LABELS = [
  "jan", "feb", "mar", "apr", "maj", "jun",
  "jul", "aug", "sep", "okt", "nov", "dec",
];

export function mileage(
  over: Partial<CompanyMileage> = {},
): CompanyMileage {
  // One trip in March, one in May — exercises the per-month breakdown and
  // the "newest trip first" ordering of the entries table.
  const months = MILEAGE_MONTH_LABELS.map((label, i) => ({
    month: i + 1,
    label,
    tripCount: i === 2 || i === 4 ? 1 : 0,
    kilometers: i === 2 ? 312 : i === 4 ? 84 : 0,
    amountBasis: i === 2 ? 1182.48 : i === 4 ? 318.36 : 0,
  }));
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    periodStart: "2026-01-01",
    periodEnd: "2026-12-31",
    entries: [
      {
        id: 2,
        entryNo: "MIL-2026-000002",
        tripDate: "2026-05-10",
        purpose: "Møde Odense",
        fromLocation: "København",
        toLocation: "Odense",
        kilometers: 84,
        vehicle: "Privat bil",
        driver: "Owner",
        ratePerKm: 3.79,
        amountBasis: 318.36,
        rateBasis: "SKAT 2026, høj sats",
        rateSource: null,
        notes: null,
        createdAt: "2026-05-10T08:00:00.000Z",
      },
      {
        id: 1,
        entryNo: "MIL-2026-000001",
        tripDate: "2026-03-15",
        purpose: "Kundebesøg Aarhus",
        fromLocation: "København",
        toLocation: "Aarhus",
        kilometers: 312,
        vehicle: "Privat bil",
        driver: "Owner",
        ratePerKm: 3.79,
        amountBasis: 1182.48,
        rateBasis: "SKAT 2026, høj sats",
        rateSource: "https://skat.dk/",
        notes: null,
        createdAt: "2026-03-15T08:00:00.000Z",
      },
    ],
    totalKilometers: 396,
    totalAmountBasis: 1500.84,
    tripCount: 2,
    months,
    ...over,
  };
}

const CASHFLOW_MONTHS = [
  "jan", "feb", "mar", "apr", "maj", "jun",
  "jul", "aug", "sep", "okt", "nov", "dec",
];

export function cashflow(
  over: Partial<CompanyCashflow> = {},
): CompanyCashflow {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    periodStart: "2026-01-01",
    periodEnd: "2026-12-31",
    hasTransactions: true,
    months: CASHFLOW_MONTHS.map((label, i) => ({
      month: i + 1,
      label,
      indbetalinger: i === 1 ? 12000 : i === 4 ? 5829.02 : 0,
      udbetalinger: i === 1 ? 2000 : i === 4 ? 2594.2 : 0,
      netto: i === 1 ? 10000 : i === 4 ? 3234.82 : 0,
    })),
    balanceSeries: [
      { date: "2026-02-10", balance: 12000 },
      { date: "2026-02-20", balance: 10000 },
      { date: "2026-05-05", balance: 13234.82 },
    ],
    openingBalance: 4000,
    closingBalance: 13234.82,
    totalIn: 17829.02,
    totalOut: 4594.2,
    ...over,
  };
}

export function documents(
  over: Partial<CompanyDocuments> = {},
): CompanyDocuments {
  return {
    slug: "acme-aps",
    company: STATEMENT_COMPANY,
    documents: [
      {
        id: 1,
        documentNo: "DOC-2026-000001",
        source: "dinero-import",
        filename: "2026-Bilag-1.pdf",
        documentType: "purchase_sale",
        supplierName: "Leverandør ApS",
        invoiceNo: "F-1001",
        invoiceDate: "2026-02-01",
        amountIncVat: 1250,
        currency: "DKK",
        status: "ingested",
        voucherRef: "1",
        journalEntryNo: "B-2026-0002",
        journalEntryId: 2,
        journalEntryText: "Køb af kontorartikler",
        journalEntryTotal: 1250,
        hasFile: true,
      },
    ],
    linkedCount: 1,
    unlinkedCount: 0,
    ...over,
  };
}

/** The fiscal-years payload — drives the Arkiv view's year selector. */
export function fiscalYears(
  over: FiscalYearEntry[] = STATEMENT_FISCAL_YEARS,
): { slug: string; years: FiscalYearEntry[] } {
  return { slug: "acme-aps", years: over };
}

export function archive(
  over: Partial<CompanyArchiveYear> = {},
): CompanyArchiveYear {
  return {
    slug: "acme-aps",
    company: STATEMENT_COMPANY,
    year: "2025",
    sourceSystem: "dinero",
    importedAt: "2026-01-10T09:00:00.000Z",
    saldoBalance: [
      { accountNo: "1000", name: "Omsætning", amount: -42000 },
      { accountNo: "3000", name: "Vareforbrug", amount: 12500 },
      { accountNo: "55000", name: "Bank", amount: 29500 },
    ],
    postings: { count: 84, grossTotal: 168000 },
    ...over,
  };
}

export function multiYear(
  over: Partial<CompanyMultiYear> = {},
): CompanyMultiYear {
  return {
    slug: "acme-aps",
    company: STATEMENT_COMPANY,
    years: [
      {
        year: "2023",
        source: "archive",
        omsaetning: 31000,
        udgifter: 9000,
        resultat: 22000,
        balancesum: 64000,
        egenkapital: 41000,
        bruttomargin: 22000 / 31000,
        egenkapitalandel: 41000 / 64000,
      },
      {
        year: "2024",
        source: "archive",
        omsaetning: 38000,
        udgifter: 11000,
        resultat: 27000,
        balancesum: 72000,
        egenkapital: 48000,
        bruttomargin: 27000 / 38000,
        egenkapitalandel: 48000 / 72000,
      },
      {
        year: "2025",
        source: "archive",
        omsaetning: 42000,
        udgifter: 12500,
        resultat: 29500,
        balancesum: 81000,
        egenkapital: 56000,
        bruttomargin: 29500 / 42000,
        egenkapitalandel: 56000 / 81000,
      },
      {
        year: "2026",
        source: "live",
        omsaetning: 17829.02,
        udgifter: 4594.2,
        resultat: 13234.82,
        balancesum: 90000,
        egenkapital: 62000,
        bruttomargin: 13234.82 / 17829.02,
        egenkapitalandel: 62000 / 90000,
      },
    ],
    ...over,
  };
}

export function invoices(
  over: Partial<CompanyInvoices> = {},
): CompanyInvoices {
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    periodStart: "2026-01-01",
    periodEnd: "2026-12-31",
    invoices: [
      {
        documentId: 1,
        invoiceNo: "2026-00001",
        invoiceDate: "2026-03-15",
        customerName: "Kunde A/S",
        customerEmail: "faktura@kunde.dk",
        buyerEanNumber: null,
        buyerPublicRecipient: false,
        peppolStatus: null,
        lastEmailedAt: null,
        lastReminderAt: null,
        lastReminderSequence: 0,
        grossAmount: 12500,
        openBalance: 0,
        currency: "DKK",
        status: "paid",
        effectiveDueDate: "2026-04-14",
        overdueDays: 0,
      },
      {
        documentId: 2,
        invoiceNo: "2026-00002",
        invoiceDate: "2026-04-01",
        customerName: "Beta ApS",
        customerEmail: "faktura@beta.dk",
        buyerEanNumber: null,
        buyerPublicRecipient: false,
        peppolStatus: null,
        lastEmailedAt: null,
        lastReminderAt: null,
        lastReminderSequence: 0,
        grossAmount: 6250,
        openBalance: 6250,
        currency: "DKK",
        status: "overdue",
        effectiveDueDate: "2026-04-30",
        overdueDays: 21,
      },
    ],
    totalGross: 18750,
    totalOpen: 6250,
    overdueCount: 1,
    ...over,
  };
}

export function contacts(
  over: Partial<CompanyContacts> = {},
): CompanyContacts {
  return {
    slug: "acme-aps",
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    customers: [
      {
        id: 1,
        name: "Kunde A/S",
        vatOrCvr: "DK87654321",
        email: "faktura@kunde.dk",
        paymentTermsDays: 30,
        defaultCurrency: "DKK",
        address: null,
        phone: null,
        website: null,
        eanNumber: null,
        notes: null,
      },
    ],
    vendors: [
      {
        id: 1,
        name: "Leverandør ApS",
        vatOrCvr: "DK11223344",
        defaultExpenseAccount: "3000",
        defaultVatTreatment: "standard",
        address: null,
        email: null,
        phone: null,
        website: null,
        notes: null,
      },
    ],
    ...over,
  };
}

/** The full company settings row, backing GET /api/companies/:slug/company. */
export function companySettings(
  over: Partial<CompanySettings> = {},
): CompanySettings {
  return {
    id: 1,
    name: "Acme ApS",
    country: "DK",
    currency: "DKK",
    cvr: "DK12345678",
    fiscalYearStartMonth: 1,
    fiscalYearLabelStrategy: "end-year",
    address: null,
    postalCode: null,
    city: null,
    companyForm: null,
    industryCode: null,
    industryText: null,
    cvrStatus: null,
    auditWaived: null,
    cvrSyncedAt: null,
    // #300: the VAT settlement cadence — defaults to the historical `quarter`.
    vatPeriodType: "quarter",
    payment: null,
    ...over,
  };
}

type RouteMap = Record<string, unknown>;

/**
 * Installs a `fetch` mock. `routes` maps a `METHOD path` (path matched by
 * prefix, query stripped) to either a success payload, or `{__error}` to make
 * the route fail with the cockpit error envelope.
 */
export function mockFetch(routes: RouteMap) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0];
      const method = (init?.method ?? "GET").toUpperCase();
      const key =
        Object.keys(routes).find((k) => {
          const [m, p] = k.split(" ");
          return m === method && path === p;
        }) ?? `${method} ${path}`;
      const payload = routes[key];
      if (payload === undefined) {
        return jsonResponse(
          { ok: false, error: { code: "not_found", message: "no route" } },
          404,
        );
      }
      if (
        payload &&
        typeof payload === "object" &&
        "__error" in (payload as Record<string, unknown>)
      ) {
        const e = (payload as { __error: { code: string; message: string } })
          .__error;
        return jsonResponse(
          { ok: false, error: e },
          e.code === "conflict" ? 409 : 400,
        );
      }
      return jsonResponse({ ok: true, ...(payload as object) });
    }),
  );
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
