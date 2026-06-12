// Tests: src/core/dashboard.ts (dashboard rendering)
import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  auditStatusPill,
  formatDkk,
  metricCard,
  renderDashboard,
  type DashboardInput,
} from "../../src/core/dashboard";

const REPO_ROOT = process.cwd();
const SNAPSHOT_PATH = join(REPO_ROOT, "tests", "snapshots", "dashboard.html");
const DASHBOARD_SOURCE_PATH = join(REPO_ROOT, "src", "core", "dashboard.ts");

// --------------------------------------------------------------------------
// Fixture: a minimal but realistic DashboardInput with known values.
// Every field is fully spelled out so the snapshot is reproducible.
// --------------------------------------------------------------------------

function buildFixture(): DashboardInput {
  return {
    asOfDate: "2026-05-17",
    generatedAt: "2026-05-17T02:10:00.000Z",
    commitSha: "abc1234",
    ruleBundleVersion: "2026-05",
    company: {
      id: 1,
      name: "Eksempel Snedker ApS",
      country: "DK",
      currency: "DKK",
      cvr: "DK12345678",
      fiscalYearStartMonth: 1,
      fiscalYearLabelStrategy: "end-year",
    },
    invoices: {
      ok: true,
      count: 3,
      status: "open",
      asOfDate: "2026-05-17",
      errors: [],
      rows: [
        {
          documentId: 101,
          invoiceNumber: "2026-0001",
          invoiceDate: "2026-05-01",
          customerName: "Kunde A/S",
          customerCvr: "11112222",
          grossAmount: 1250,
          currency: "DKK",
          openBalance: 1250,
          claimOpenBalance: 0,
          status: "open",
          effectiveDueDate: "2026-06-15",
          isOverdue: false,
          overdueDays: 0,
        },
        {
          documentId: 102,
          invoiceNumber: "2026-0002",
          invoiceDate: "2026-05-02",
          customerName: "Anden ApS",
          customerCvr: "33334444",
          grossAmount: 8750,
          currency: "DKK",
          openBalance: 8750,
          claimOpenBalance: 0,
          status: "open",
          effectiveDueDate: "2026-07-02",
          isOverdue: false,
          overdueDays: 0,
        },
        {
          documentId: 103,
          invoiceNumber: "2026-0003",
          invoiceDate: "2026-05-14",
          customerName: "Tredje I/S",
          customerCvr: null,
          grossAmount: 2500,
          currency: "DKK",
          openBalance: 2500,
          claimOpenBalance: 0,
          status: "open",
          effectiveDueDate: "2026-07-14",
          isOverdue: false,
          overdueDays: 0,
        },
      ],
    },
    overdueInvoices: {
      ok: true,
      count: 1,
      status: "overdue",
      asOfDate: "2026-05-17",
      errors: [],
      rows: [
        {
          documentId: 90,
          invoiceNumber: "2025-0042",
          invoiceDate: "2025-03-01",
          customerName: "Gammel Kunde",
          customerCvr: null,
          grossAmount: 4500,
          currency: "DKK",
          openBalance: 4500,
          claimOpenBalance: 0,
          status: "open",
          effectiveDueDate: "2025-04-01",
          isOverdue: true,
          overdueDays: 411,
        },
      ],
    },
    unlinkedBank: {
      ok: true,
      count: 4,
      rows: [],
      errors: [],
    },
    exceptions: {
      ok: true,
      count: 2,
      rows: [
        {
          id: 1,
          type: "UNMATCHED_BANK_TRANSACTION",
          severity: "medium",
          status: "open",
          message:
            "Banktransaktionen \"MobilePay overførsel\" den 2026-05-12 på 1.205,00 kr. er endnu ikke bogført. Der er ikke fundet et bilag (kvittering eller faktura), der passer til beløbet.",
          requiredAction:
            "Find kvitteringen eller fakturaen for denne betaling og læg den i bogføringen. Uden et bilag kan udgiften ikke bogføres og momsen ikke fratrækkes.",
        },
        {
          id: 2,
          type: "MAIL_INTAKE_NO_ATTACHMENT",
          severity: "high",
          status: "open",
          message:
            "Mail fra leverandoer@eksempel.dk modtaget 2026-05-14 indeholdt ingen vedhæftede filer, så der kunne ikke indlæses et bilag.",
          requiredAction:
            "Bed afsenderen sende bilaget igen som vedhæftet PDF, eller indlæs bilaget manuelt.",
        },
      ],
      errors: [],
    },
    vatPeriod: {
      ok: true,
      appliedRules: ["DK-VAT-REPORT-001"],
      periodStart: "2026-04-01",
      periodEnd: "2026-06-30",
      outputVat: 8125,
      inputVat: 1875,
      netVatPayable: 6250,
      purchaseBase25: 7500,
      salesBase25: 32500,
      reverseChargeSalesBase: 0,
      reverseChargePurchaseBase: 0,
      representationPurchaseBase: 0,
      badDebtReliefBase25: 0,
      journalEntryCount: 5,
      reversedJournalEntryCount: 0,
      reversalJournalEntryCount: 0,
      totalJournalEntryCount: 5,
      linesConsidered: 12,
      reversedLinesConsidered: 0,
      reversalLinesConsidered: 0,
      totalLinesConsidered: 12,
      warnings: [],
      errors: [],
    },
    vatDaysRemaining: 44,
    recentActivity: [
      { id: 200, eventType: "system_backup", entityType: "system", entityId: null, message: "Backup created", actor: "system", createdAt: "2026-05-17 02:09:00" },
      { id: 199, eventType: "journal_reverse", entityType: "journal", entityId: "55", message: "Reversed journal entry 2026-J-0014: posting error in VAT account", actor: "cli", createdAt: "2026-05-17 01:55:00" },
      { id: 198, eventType: "document_ingest", entityType: "document", entityId: "102", message: "Ingested supporting document DOC-2026-000004 (a8626d2599f1b3c0)", actor: "cli", createdAt: "2026-05-16 14:21:00" },
    ],
    backup: {
      ok: true,
      appliedRules: ["DK-BACKUP-001"],
      latestBackupAt: "2026-05-17T02:09:00.000Z",
      latestBackupId: "backup-20260517T020900Z",
      backupDue: false,
      hasActivitySinceBackup: false,
      daysSinceLatestBackup: 0,
      requiredBy: "2026-05-24",
      checkedAt: "2026-05-17T02:10:00.000Z",
      backupsFound: 1,
      evidence: {
        latestJournalEntryAt: "2026-05-16T14:21:00.000Z",
        latestDocumentAt: "2026-05-16T14:21:00.000Z",
        latestBankImportAt: "2026-05-15T09:00:00.000Z",
      },
      errors: [],
    },
    audit: {
      ok: true,
      entryCount: 142,
    },
  };
}

// --------------------------------------------------------------------------
// Snapshot test (committed)
// --------------------------------------------------------------------------

describe("renderDashboard — snapshot", () => {
  test("matches committed snapshot for fixture input", () => {
    const html = renderDashboard(buildFixture());

    // Refresh snapshot via SNAPSHOT_UPDATE=1 bun test tests/unit/dashboard-render.test.ts
    if (process.env.SNAPSHOT_UPDATE === "1") {
      writeFileSync(SNAPSHOT_PATH, html, "utf8");
    }

    expect(existsSync(SNAPSHOT_PATH)).toBe(true);
    const expected = readFileSync(SNAPSHOT_PATH, "utf8");
    expect(html).toBe(expected);
  });
});

// --------------------------------------------------------------------------
// Structural / contract checks
// --------------------------------------------------------------------------

describe("renderDashboard — structure", () => {
  const html = renderDashboard(buildFixture());

  test("emits a complete HTML5 document", () => {
    expect(html.startsWith("<!DOCTYPE html>")).toBe(true);
    expect(html).toContain("<html lang=\"da\">");
    expect(html).toContain("<head>");
    expect(html).toContain("</head>");
    expect(html).toContain("<body>");
    expect(html).toContain("</body>");
    expect(html.trimEnd().endsWith("</html>")).toBe(true);
  });

  test("opening and closing tag counts balance for major elements", () => {
    for (const tag of ["html", "head", "body", "main", "header", "footer", "section", "table", "thead", "tbody", "tr"]) {
      const open = (html.match(new RegExp(`<${tag}(\\s|>)`, "g")) ?? []).length;
      const close = (html.match(new RegExp(`</${tag}>`, "g")) ?? []).length;
      expect(open, `unbalanced <${tag}>: ${open} open vs ${close} close`).toBe(close);
    }
  });

  test("contains fixture company name and invoice numbers", () => {
    expect(html).toContain("Eksempel Snedker ApS");
    expect(html).toContain("CVR DK12345678");
    expect(html).toContain("2026-0001");
    expect(html).toContain("2026-0002");
    expect(html).toContain("2026-0003");
  });

  test("contains the 8 spec sections (header, metrics, deadline, invoices, activity, backup, audit, footer)", () => {
    // Section heading text from the dashboard render.
    expect(html).toMatch(/<header class="header">/);
    expect(html).toMatch(/<section class="metrics">/);
    expect(html).toContain("Næste deadline");
    expect(html).toContain("Åbne fakturaer");
    expect(html).toContain("Seneste aktivitet");
    expect(html).toContain("Backup-status");
    expect(html).toContain("Audit-chain");
    expect(html).toMatch(/<footer class="footer">/);
  });

  test("amounts are formatted with Danish locale (canonical kr. suffix)", () => {
    // #314: the dashboard now uses the single canonical `formatKronerDa`, so
    // amounts carry a regular-space " kr." suffix (not an NBSP + "DKK").
    expect(html).toContain("1.250,00 kr.");
    expect(html).toContain("8.750,00 kr.");
    expect(html).toContain("6.250,00 kr.");
  });

  test("output is under 100 KB", () => {
    const bytes = Buffer.byteLength(html, "utf8");
    expect(bytes).toBeLessThan(100 * 1024);
  });

  test("contains exactly one <style> block (no external stylesheets beyond fonts)", () => {
    const styles = (html.match(/<style>/g) ?? []).length;
    expect(styles).toBe(1);
    // Only stylesheet link allowed is Google Fonts.
    const linkMatches = html.match(/<link[^>]+rel="stylesheet"[^>]*>/g) ?? [];
    for (const link of linkMatches) {
      expect(link).toContain("fonts.googleapis.com");
    }
  });

  test("contains no JavaScript", () => {
    expect(html).not.toMatch(/<script/i);
    expect(html).not.toMatch(/on(click|load|mouseover)=/i);
  });

  // #233: the recent-activity strip must read in plain Danish — no internal
  // event codes, no mid-word truncation, no "ULINKEDE BANK-TX" jargon.
  test("recent activity renders plain-Danish event labels, not internal codes", () => {
    expect(html).toContain("Backup oprettet");
    expect(html).toContain("Finanspostering tilbageført");
    expect(html).toContain("Bilag indlæst");
    // The raw snake_case codes must NOT leak into the rendered HTML.
    expect(html).not.toContain("system_backup");
    expect(html).not.toContain("journal_reverse");
    expect(html).not.toContain("document_ingest");
  });

  test("recent activity messages are shown in full without truncation", () => {
    // The fixture's longest message must appear verbatim, not cut mid-word.
    expect(html).toContain("Bilag DOC-2026-000004 indlæst (a8626d2599f1b3c0)");
    expect(html).not.toContain("a8626d2599f1b3c0…");
  });

  // #286: the "Seneste aktivitet" strip must read in plain Danish — the audit
  // log persists English detail messages ("Created customer ...", "Rendered
  // invoice PDF ...", "Company volume initialized"), but the Danish-facing
  // dashboard must translate them, like it already does the event headings.
  test("recent activity detail text is rendered in Danish, not English", () => {
    const englishMessages: DashboardInput = {
      ...buildFixture(),
      recentActivity: [
        { id: 300, eventType: "customer_create", entityType: "customer", entityId: "7", message: "Created customer Storkunde A/S", actor: "cli", createdAt: "2026-05-17 09:00:00" },
        { id: 301, eventType: "invoice_render_pdf", entityType: "invoice", entityId: "9", message: "Rendered invoice PDF 2026-0007", actor: "cli", createdAt: "2026-05-17 09:05:00" },
        { id: 302, eventType: "init", entityType: "company", entityId: null, message: "Company volume initialized", actor: "system", createdAt: "2026-05-17 08:00:00" },
      ],
    };
    const out = renderDashboard(englishMessages);
    // The translated Danish detail text appears, the English source text does not.
    expect(out).toContain("Kunde oprettet: Storkunde A/S");
    expect(out).toContain("Faktura-PDF genereret: 2026-0007");
    expect(out).toContain("Virksomhed oprettet");
    expect(out).not.toContain("Created customer");
    expect(out).not.toContain("Rendered invoice PDF");
    expect(out).not.toContain("Company volume initialized");
  });

  test("metric label avoids internal jargon for unlinked bank entries", () => {
    expect(html).not.toContain("ULINKEDE BANK-TX");
    expect(html).toContain("BANKPOSTER UDEN BILAG");
  });

  // #236: the deadline box must count down to the real SKAT filing/payment
  // deadline (1st of the third month after the quarter ends), not the
  // period-end date. The fixture's 2026-05-17 is in Q2 → due 2026-09-01.
  test("deadline box shows the real SKAT filing deadline, not the period end", () => {
    expect(html).toContain("SKAT-frist:");
    expect(html).toContain("2026-09-01");
    // The old behaviour counted to 30-06; 44 days (the wrong period-end
    // countdown) must not appear.
    expect(html).not.toContain("44 dage tilbage");
    // 2026-05-17 → 2026-09-01 is 107 days.
    expect(html).toContain("107 dage tilbage");
  });

  // #281: the "Næste deadline" box must describe the quarter the supplied
  // `vatPeriod` actually covers — the earliest unreported VAT quarter with
  // activity the CLI selected — NOT whichever calendar quarter `asOfDate`
  // happens to fall in. An owner who sees the current empty quarter when an
  // earlier quarter still owes 5.400 kr to SKAT misses the payment.
  test("deadline box describes the supplied vatPeriod quarter, not the as-of quarter", () => {
    // asOfDate is 2026-05-22 (Q2) but the supplied vatPeriod is Q1 2026 with
    // 5.400 kr of booked output VAT still unreported.
    const q1Due: DashboardInput = {
      ...buildFixture(),
      asOfDate: "2026-05-22",
      vatPeriod: {
        ...buildFixture().vatPeriod,
        periodStart: "2026-01-01",
        periodEnd: "2026-03-31",
        outputVat: 5400,
        inputVat: 0,
        netVatPayable: 5400,
      },
    };
    const out = renderDashboard(q1Due);
    // The deadline box names Q1 2026 and counts down to its real SKAT
    // deadline (1 June 2026), not Q2.
    expect(out).toContain("Q1 2026");
    expect(out).toContain("2026-06-01");
    expect(out).not.toContain("Q2 2026");
    // The deadline card carries the real 5.400 kr payable, never 0,00.
    const cardMatch = /<div class="deadline-card">[\s\S]*?<\/div>\s*<\/div>\s*<\/div>/.exec(out);
    expect(cardMatch).not.toBeNull();
    const card = cardMatch![0];
    // The payable amount is the real 5.400 kr, never 0.
    expect(card).toContain("5.400,00 kr.");
    expect(card).toMatch(/amount-lg">5\.400,00 kr\.</);
  });

  // #263: the dashboard must not stop at a bare exception count — it lists
  // each open exception as a short line so the owner sees *what* needs
  // attention without a trip to the terminal.
  test("lists each open exception, not just the count", () => {
    // The fixture has 2 open exceptions; both must be named on the dashboard.
    expect(html).toContain("Banktransaktion mangler afstemning");
    expect(html).toContain("Indkommen mail uden vedhæftet bilag");
    // The "Åbne exceptions" section heading is present.
    expect(html).toContain("Åbne exceptions");
  });

  // #270: the static dashboard must reach parity with the Cockpit SPA — a
  // human Danish heading for the exception type (never the raw machine code),
  // a Danish severity label (never the English code), the FULL message (not a
  // mid-sentence-truncated fragment), and the "Sådan løser du den" guidance.
  test("exceptions render a human Danish type heading, never the raw code", () => {
    expect(html).toContain("Banktransaktion mangler afstemning");
    expect(html).toContain("Indkommen mail uden vedhæftet bilag");
    // The raw SCREAMING_SNAKE machine codes must NOT leak into the HTML.
    expect(html).not.toContain("UNMATCHED_BANK_TRANSACTION");
    expect(html).not.toContain("MAIL_INTAKE_NO_ATTACHMENT");
  });

  test("exception severity is a Danish label, not the English code", () => {
    // The fixture has a medium and a high severity exception.
    expect(html).toContain(">Mellem<");
    expect(html).toContain(">Høj<");
    // The raw English severity codes must not appear as the pill text.
    expect(html).not.toContain(">medium<");
    expect(html).not.toContain(">high<");
  });

  test("exception message is shown in full, not truncated mid-sentence", () => {
    // The fixture's first exception message is well past the old 110-char
    // truncation. It must appear verbatim (HTML-escaped quotes), no ellipsis.
    expect(html).toContain(
      "Banktransaktionen &quot;MobilePay overførsel&quot; den 2026-05-12 på 1.205,00 kr. er endnu ikke bogført. Der er ikke fundet et bilag (kvittering eller faktura), der passer til beløbet.",
    );
    // No mid-sentence ellipsis in the exceptions section.
    expect(html).not.toContain("der passer til be…");
  });

  test("exception renders its requiredAction as 'Sådan løser du den' guidance", () => {
    expect(html).toContain("Sådan løser du den:");
    expect(html).toContain(
      "Find kvitteringen eller fakturaen for denne betaling og læg den i bogføringen.",
    );
  });

  test("an exception with no requiredAction omits the guidance line", () => {
    const noAction: DashboardInput = {
      ...buildFixture(),
      exceptions: {
        ok: true,
        count: 1,
        rows: [
          {
            id: 9,
            type: "UNMATCHED_BANK_TRANSACTION",
            severity: "low",
            status: "open",
            message: "En transaktion mangler afstemning.",
          },
        ],
        errors: [],
      },
    };
    const noActionHtml = renderDashboard(noAction);
    // The exception is still listed, but with no "Sådan løser du den" line.
    expect(noActionHtml).toContain("Banktransaktion mangler afstemning");
    expect(noActionHtml).not.toContain("Sådan løser du den:");
  });

  test("exceptions section shows an empty state when there are no open exceptions", () => {
    const empty: DashboardInput = {
      ...buildFixture(),
      exceptions: { ok: true, count: 0, rows: [], errors: [] },
    };
    const emptyHtml = renderDashboard(empty);
    expect(emptyHtml).toContain("Ingen åbne exceptions");
  });

  // #246: the footer must not dump a raw commit hash + rule-version on the
  // calm cockpit surface — they are tucked into a collapsed <details>.
  test("footer does not dump raw commit hash / rule-version in the visible row", () => {
    const footerMatch = /<footer class="footer">[\s\S]*?<\/footer>/.exec(html);
    expect(footerMatch).not.toBeNull();
    const footer = footerMatch![0];
    const visibleRow = /<div class="row">[\s\S]*?<\/div>\s*<\/div>/.exec(footer)?.[0] ?? "";
    // The visible row is just "Genereret ..." — no commit/rules dump.
    expect(visibleRow).not.toContain("abc1234");
    expect(visibleRow).not.toContain("2026-05");
    expect(visibleRow).toContain("Genereret");
    // Build provenance is still available, but de-emphasised in <details>.
    expect(footer).toContain("<details");
    expect(footer).toContain("abc1234");
  });
});

// --------------------------------------------------------------------------
// Recurring-feature cards (#islands → control surfaces)
// --------------------------------------------------------------------------

/** A fixture extended with the recurring-feature inputs. */
function fixtureWithRecurringFeatures(): DashboardInput {
  return {
    ...buildFixture(),
    payables: {
      ok: true,
      count: 2,
      status: "open",
      asOfDate: "2026-05-17",
      totalOpenBalance: 6750,
      overdueOpenBalance: 2500,
      notYetDueOpenBalance: 4250,
      errors: [],
      rows: [
        {
          payableId: 1,
          documentId: 501,
          billNo: "V-2026-0007",
          billDate: "2026-03-01",
          dueDate: "2026-03-31",
          supplierName: "Leverandør A ApS",
          vendorId: null,
          grossAmount: 2500,
          currency: "DKK",
          paidAmount: 0,
          openBalance: 2500,
          status: "open",
          isOverdue: true,
          overdueDays: 47,
          agingBucket: "31-60",
        },
        {
          payableId: 2,
          documentId: 502,
          billNo: "V-2026-0009",
          billDate: "2026-05-01",
          dueDate: "2026-06-15",
          supplierName: "Leverandør B I/S",
          vendorId: null,
          grossAmount: 4250,
          currency: "DKK",
          paidAmount: 0,
          openBalance: 4250,
          status: "open",
          isOverdue: false,
          overdueDays: 0,
          agingBucket: "not-due",
        },
      ],
    },
    accrualRegister: {
      ok: true,
      accruals: [
        {
          accrualId: 1,
          accrualType: "prepaid_expense",
          description: "Forsikring helår",
          totalAmount: 12000,
          recognitionPeriods: 12,
          recognizedPeriods: 4,
          recognizedAmount: 4000,
          remainingAmount: 8000,
          fullyRecognized: false,
          balanceAccountNo: "1300",
          resultAccountNo: "3150",
          firstRecognitionDate: "2026-01-31",
          periodStepMonths: 1,
        },
      ],
      totals: { totalAmount: 12000, recognizedAmount: 4000, remainingAmount: 8000 },
      errors: [],
    },
    accrualsDue: {
      ok: true,
      asOfDate: "2026-05-17",
      totalDueAmount: 1000,
      errors: [],
      periods: [
        {
          accrualId: 1,
          accrualType: "prepaid_expense",
          description: "Forsikring helår",
          periodIndex: 5,
          totalPeriods: 12,
          recognitionDate: "2026-05-31",
          amount: 1000,
          overdueDays: 0,
        },
      ],
    },
    // An UNDER-budget expense month, consistent with real buildBudgetVsActual
    // output: for an expense line variance = budget − actual = +800
    // (favourable), while the aggregate totalVariance = totalActual −
    // totalBudget = −800. The pill must be derived from the per-line variance,
    // not the raw aggregate (whose sign depends on account mix).
    budgetVsActual: {
      ok: true,
      appliedRules: ["budget:vs-actual"],
      periodStart: "2026-05",
      periodEnd: "2026-05",
      lines: [
        {
          accountNo: "3000",
          accountName: "Software",
          accountType: "expense",
          period: "2026-05",
          budget: 5000,
          actual: 4200,
          variance: 800,
        },
      ],
      totalBudget: 5000,
      totalActual: 4200,
      totalVariance: -800,
      errors: [],
    },
    liquidity: {
      ok: true,
      appliedRules: ["liquidity-forecast"],
      startDate: "2026-06-01",
      months: 3,
      openingBalance: 25000,
      closingBalance: 31000,
      errors: [],
      periods: [
        { period: "2026-06", openingBalance: 25000, invoiceInflow: 12500, recurringInflow: 0, budgetedCostOutflow: 5000, netChange: 7500, closingBalance: 32500 },
        { period: "2026-07", openingBalance: 32500, invoiceInflow: 0, recurringInflow: 0, budgetedCostOutflow: 5000, netChange: -5000, closingBalance: 27500 },
        { period: "2026-08", openingBalance: 27500, invoiceInflow: 8500, recurringInflow: 0, budgetedCostOutflow: 5000, netChange: 3500, closingBalance: 31000 },
      ],
    },
    tax: {
      fiscalYearLabel: "2025",
      available: true,
      corporateTax: 13200,
      bookkeptResult: 60000,
      needsReviewCount: 1,
    },
    euSalesOss: {
      euSalesValue: 18000,
      euCustomerCount: 2,
      ossConsumerSalesBase: 4500,
    },
  };
}

describe("renderDashboard — recurring-feature cards", () => {
  test("the historical fixture (no new fields) renders identically", () => {
    // Additive guarantee: a DashboardInput without the new fields is the
    // unchanged historical dashboard — same snapshot as before.
    const html = renderDashboard(buildFixture());
    const expected = readFileSync(SNAPSHOT_PATH, "utf8");
    expect(html).toBe(expected);
  });

  test("renders a creditor (payables) card with open and overdue balances", () => {
    const html = renderDashboard(fixtureWithRecurringFeatures());
    expect(html).toContain("Åbne kreditorposter");
    expect(html).toContain("Leverandør A ApS");
    expect(html).toContain("V-2026-0007");
    // Overdue creditor item shows a danger pill with the overdue days.
    expect(html).toContain("forfalden (47 d)");
    // The summary line carries the total + overdue split.
    expect(html).toContain("Åben kreditorgæld i alt");
  });

  test("renders an accruals card with exposure and due recognition periods", () => {
    const html = renderDashboard(fixtureWithRecurringFeatures());
    expect(html).toContain("Periodeafgrænsningsposter");
    expect(html).toContain("Resterende balanceeksponering");
    // One due recognition period of 1.000,00 kr.
    expect(html).toContain("Recognition-perioder der skal bogføres");
    expect(html).toContain("1 forfalden");
  });

  test("renders a budget & liquidity card", () => {
    const html = renderDashboard(fixtureWithRecurringFeatures());
    expect(html).toContain("Budget &amp; likviditet");
    expect(html).toContain("Budget vs. faktisk");
    expect(html).toContain("Likviditetsprognose");
    // 3-month forecast, final projected balance 31.000,00 kr.
    expect(html).toContain("31.000,00 kr.");
  });

  // #cockpit-wiring-review-4: the under-budget expense month must read calm.
  test("the budget pill reads favourable for an under-budget expense month", () => {
    // The fixture is an under-budget expense: per-line variance +800 (good),
    // raw totalVariance −800. The pill must reflect the favourable per-line
    // variance, not the negative aggregate.
    const html = renderDashboard(fixtureWithRecurringFeatures());
    expect(html).toContain("budget overholdt");
    expect(html).not.toContain("budgetafvigelse");
  });

  // #cockpit-wiring-review-4: the bug. An OVER-budget expense month has a
  // positive raw totalVariance (totalActual − totalBudget > 0) — keying the
  // pill off that shows a green "budget overholdt" pill for overspending, the
  // opposite of reality. The pill must come from the per-line signed variance.
  test("the budget pill reads UNFAVOURABLE for an over-budget expense month", () => {
    const overBudget: DashboardInput = {
      ...fixtureWithRecurringFeatures(),
      budgetVsActual: {
        ok: true,
        appliedRules: ["budget:vs-actual"],
        periodStart: "2026-05",
        periodEnd: "2026-05",
        lines: [
          {
            accountNo: "3000",
            accountName: "Software",
            accountType: "expense",
            period: "2026-05",
            budget: 5000,
            // Overspent: actual 7000 > budget 5000. For an expense line the
            // real buildBudgetVsActual yields variance = budget − actual =
            // −2000 (unfavourable). The raw totalVariance = totalActual −
            // totalBudget = +2000 (which naively looks "good").
            actual: 7000,
            variance: -2000,
          },
        ],
        totalBudget: 5000,
        totalActual: 7000,
        totalVariance: 2000,
        errors: [],
      },
    };
    const html = renderDashboard(overBudget);
    // The pill must NOT be the green "budget overholdt" — overspending is
    // unfavourable.
    expect(html).toContain("budgetafvigelse");
    expect(html).not.toContain("budget overholdt");
  });

  test("renders the tax card with estimated corporate tax for a closed year", () => {
    const html = renderDashboard(fixtureWithRecurringFeatures());
    expect(html).toContain("Estimeret selskabsskat");
    expect(html).toContain("regnskabsår 2025");
    expect(html).toContain("13.200,00 kr.");
    // One needs-review item is surfaced.
    expect(html).toContain("1 til gennemgang");
  });

  test("the tax card shows the awaiting-year-end state for an open year", () => {
    const input: DashboardInput = {
      ...fixtureWithRecurringFeatures(),
      tax: { fiscalYearLabel: "2026", available: false },
    };
    const html = renderDashboard(input);
    expect(html).toContain("Forberedelse er klar, når regnskabsåret er lukket");
    expect(html).toContain("afventer årsafslutning");
    // No corporate-tax figure is shown for an open year.
    expect(html).not.toContain("Estimeret selskabsskat");
  });

  test("the EU sales / OSS indicator surfaces only when there is activity", () => {
    const withActivity = renderDashboard(fixtureWithRecurringFeatures());
    expect(withActivity).toContain("EU-salg &amp; OSS");
    expect(withActivity).toContain("EU-salg uden moms (VIES)");
    expect(withActivity).toContain("OSS — salg til EU-forbrugere");

    // Zero cross-border activity ⇒ the section is omitted entirely.
    const noActivity: DashboardInput = {
      ...fixtureWithRecurringFeatures(),
      euSalesOss: { euSalesValue: 0, euCustomerCount: 0, ossConsumerSalesBase: 0 },
    };
    const html = renderDashboard(noActivity);
    expect(html).not.toContain("EU-salg &amp; OSS");
  });

  test("the new cards keep the document valid and deterministic", () => {
    const a = renderDashboard(fixtureWithRecurringFeatures());
    const b = renderDashboard(fixtureWithRecurringFeatures());
    expect(a).toBe(b);
    expect(a.startsWith("<!DOCTYPE html>")).toBe(true);
    expect(a.trimEnd().endsWith("</html>")).toBe(true);
    // Section open/close tags stay balanced with the extra sections.
    for (const tag of ["section", "table", "thead", "tbody", "tr"]) {
      const open = (a.match(new RegExp(`<${tag}(\\s|>)`, "g")) ?? []).length;
      const close = (a.match(new RegExp(`</${tag}>`, "g")) ?? []).length;
      expect(open, `unbalanced <${tag}>`).toBe(close);
    }
  });

  test("an empty payables/accruals input renders empty-state text, not a crash", () => {
    const input: DashboardInput = {
      ...buildFixture(),
      payables: { ok: true, count: 0, status: "open", asOfDate: "2026-05-17", totalOpenBalance: 0, overdueOpenBalance: 0, notYetDueOpenBalance: 0, rows: [], errors: [] },
      accrualRegister: { ok: true, accruals: [], totals: { totalAmount: 0, recognizedAmount: 0, remainingAmount: 0 }, errors: [] },
      accrualsDue: { ok: true, asOfDate: "2026-05-17", totalDueAmount: 0, periods: [], errors: [] },
    };
    const html = renderDashboard(input);
    expect(html).toContain("Ingen åbne kreditorposter");
    expect(html).toContain("Ingen periodeafgrænsningsposter");
  });
});

// --------------------------------------------------------------------------
// Determinism
// --------------------------------------------------------------------------

describe("renderDashboard — determinism", () => {
  test("two calls with the same input produce identical bytes", () => {
    const a = renderDashboard(buildFixture());
    const b = renderDashboard(buildFixture());
    expect(a).toBe(b);
    expect(Buffer.byteLength(a, "utf8")).toBe(Buffer.byteLength(b, "utf8"));
  });

  test("source code does not call Date.now, Math.random or process.uptime", () => {
    const source = readFileSync(DASHBOARD_SOURCE_PATH, "utf8");
    // Strip comments before grep so docstrings can mention forbidden APIs.
    const stripped = source.replace(/\/\/[^\n]*/g, "").replace(/\/\*[\s\S]*?\*\//g, "");
    expect(stripped).not.toMatch(/Date\.now\s*\(/);
    expect(stripped).not.toMatch(/Math\.random\s*\(/);
    expect(stripped).not.toMatch(/process\.uptime\s*\(/);
    expect(stripped).not.toMatch(/process\.hrtime\s*\(/);
    expect(stripped).not.toMatch(/performance\.now\s*\(/);
    expect(stripped).not.toMatch(/new Date\s*\(\s*\)/);
  });

  test("renders empty-state HTML for empty database", () => {
    const empty: DashboardInput = {
      ...buildFixture(),
      invoices: { ok: true, count: 0, status: "open", asOfDate: "2026-05-17", errors: [], rows: [] },
      overdueInvoices: { ok: true, count: 0, status: "overdue", asOfDate: "2026-05-17", errors: [], rows: [] },
      unlinkedBank: { ok: true, count: 0, rows: [], errors: [] },
      exceptions: { ok: true, count: 0, rows: [], errors: [] },
      recentActivity: [],
      audit: { ok: true, entryCount: 0 },
    };
    const html = renderDashboard(empty);
    expect(html).toContain("Ingen åbne fakturaer");
    expect(html).toContain("Ingen aktivitet endnu");
    expect(html.startsWith("<!DOCTYPE html>")).toBe(true);
  });
});

// --------------------------------------------------------------------------
// Format helpers
// --------------------------------------------------------------------------

describe("formatDkk", () => {
  test("formats positive numbers with Danish locale", () => {
    // #314: canonical " kr." suffix from `formatKronerDa`, not " DKK".
    expect(formatDkk(1234.56)).toBe("1.234,56 kr.");
    expect(formatDkk(0)).toBe("0,00 kr.");
    expect(formatDkk(1)).toBe("1,00 kr.");
    expect(formatDkk(999)).toBe("999,00 kr.");
    expect(formatDkk(1000)).toBe("1.000,00 kr.");
    expect(formatDkk(1000000)).toBe("1.000.000,00 kr.");
  });

  test("formats negative numbers with minus prefix", () => {
    expect(formatDkk(-1234.56)).toBe("-1.234,56 kr.");
    expect(formatDkk(-0.01)).toBe("-0,01 kr.");
  });

  test("returns em-dash for non-finite numbers", () => {
    expect(formatDkk(Number.NaN)).toBe("—");
    expect(formatDkk(Number.POSITIVE_INFINITY)).toBe("—");
  });
});

// --------------------------------------------------------------------------
// Components in isolation
// --------------------------------------------------------------------------

describe("components", () => {
  test("metricCard escapes input", () => {
    const html = metricCard("LABEL <x>", "value &", "secondary \"q\"");
    expect(html).toContain("LABEL &lt;x&gt;");
    expect(html).toContain("value &amp;");
    expect(html).toContain("secondary &quot;q&quot;");
  });

  test("auditStatusPill — ok shows success", () => {
    const html = auditStatusPill(true, 142);
    expect(html).toContain("pill success");
    expect(html).toContain("142");
  });

  test("auditStatusPill — failure shows danger and truncated error", () => {
    const longError = "x".repeat(200);
    const html = auditStatusPill(false, 0, longError);
    expect(html).toContain("pill danger");
    expect(html).toContain("…"); // truncated marker
  });
});

// --------------------------------------------------------------------------
// WCAG AA contrast for key token combinations
// --------------------------------------------------------------------------

describe("DESIGN.md token contrast (WCAG AA)", () => {
  test("ink on paper meets AA (>= 4.5)", () => {
    expect(contrastRatio("#1B1A17", "#F4F1EB")).toBeGreaterThanOrEqual(4.5);
  });
  test("on-accent on accent meets AA (>= 4.5)", () => {
    expect(contrastRatio("#F4F1EB", "#A6332A")).toBeGreaterThanOrEqual(4.5);
  });
  test("danger on paper meets AA (>= 4.5)", () => {
    expect(contrastRatio("#8F2A22", "#F4F1EB")).toBeGreaterThanOrEqual(4.5);
  });
  test("ink-muted on paper meets AA (>= 4.5)", () => {
    expect(contrastRatio("#4C4740", "#F4F1EB")).toBeGreaterThanOrEqual(4.5);
  });
});

function contrastRatio(foreground: string, background: string): number {
  const fg = relativeLuminance(foreground);
  const bg = relativeLuminance(background);
  const lighter = Math.max(fg, bg);
  const darker = Math.min(fg, bg);
  return (lighter + 0.05) / (darker + 0.05);
}

function relativeLuminance(hex: string): number {
  const channels = hex.replace("#", "").match(/.{2}/g)!.map((chunk) => parseInt(chunk, 16) / 255);
  const [r, g, b] = channels.map((v) => v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}
