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
        { id: 1, type: "bank.unmatched", severity: "medium", status: "open", message: "Bank tx without matching invoice" },
        { id: 2, type: "vat.missing-evidence", severity: "high", status: "open", message: "Missing evidence for VAT deduction" },
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
      { id: 200, eventType: "backup.create", entityType: "system", entityId: null, message: "Backup created", actor: "system", createdAt: "2026-05-17 02:09:00" },
      { id: 199, eventType: "invoice.post", entityType: "document", entityId: "102", message: "Posted 2026-0002", actor: "cli", createdAt: "2026-05-17 01:55:00" },
      { id: 198, eventType: "invoice.issue", entityType: "document", entityId: "102", message: "Issued 2026-0002", actor: "cli", createdAt: "2026-05-16 14:21:00" },
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

  test("amounts are formatted with Danish locale (NBSP before DKK)", () => {
    const NBSP = " ";
    expect(html).toContain(`1.250,00${NBSP}DKK`);
    expect(html).toContain(`8.750,00${NBSP}DKK`);
    expect(html).toContain(`6.250,00${NBSP}DKK`);
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
    expect(formatDkk(1234.56)).toBe("1.234,56 DKK");
    expect(formatDkk(0)).toBe("0,00 DKK");
    expect(formatDkk(1)).toBe("1,00 DKK");
    expect(formatDkk(999)).toBe("999,00 DKK");
    expect(formatDkk(1000)).toBe("1.000,00 DKK");
    expect(formatDkk(1000000)).toBe("1.000.000,00 DKK");
  });

  test("formats negative numbers with minus prefix", () => {
    expect(formatDkk(-1234.56)).toBe("-1.234,56 DKK");
    expect(formatDkk(-0.01)).toBe("-0,01 DKK");
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
