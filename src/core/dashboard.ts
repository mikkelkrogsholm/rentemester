// Dashboard render-engine: deterministisk HTML fra DESIGN.md-tokens.
//
// Kontrakt:
//   renderDashboard(input: DashboardInput): string
//
// Givet samme input skal output være byte-for-byte identisk. Render-engine
// kalder ikke Date.now(), Math.random(), filsystem eller env. Alle disse
// felter samles af CLI'en (#85) og leveres ind via DashboardInput.
//
// Output er én selvstændig HTML-fil med inline CSS. Source Serif 4 + IBM
// Plex Sans + IBM Plex Mono refereres via Google Fonts <link>, men HTML'en
// rendreres korrekt offline med deterministiske system-fallbacks.

import type { CompanySettings } from "./company";
import type { InvoiceListResult, InvoiceListRow } from "./invoice-list";
import type { BankTransactionListResult } from "./reconciliation";
import type { VatPeriodReport } from "./vat";
import type { BackupComplianceStatus } from "./system-backups";
import type { AuditLogRow } from "./audit-log";

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

export type DashboardExceptionRow = {
  id: number;
  type: string;
  severity: string;
  status: string;
  message: string;
};

export type DashboardExceptionsResult = {
  ok: boolean;
  count: number;
  rows: DashboardExceptionRow[];
  errors: string[];
};

export type DashboardAuditStatus = {
  ok: boolean;
  entryCount: number;
  firstError?: string;
};

export type DashboardInput = {
  asOfDate: string;            // YYYY-MM-DD
  generatedAt: string;         // ISO 8601 UTC
  commitSha: string;           // 7-char short SHA or 'unknown'
  ruleBundleVersion: string;
  company: CompanySettings;
  invoices: InvoiceListResult;
  overdueInvoices: InvoiceListResult;
  unlinkedBank: BankTransactionListResult;
  exceptions: DashboardExceptionsResult;
  vatPeriod: VatPeriodReport;
  vatDaysRemaining: number;
  recentActivity: AuditLogRow[];
  backup: BackupComplianceStatus;
  audit: DashboardAuditStatus;
};

export type RenderOptions = {
  // Currently only present for forward-compat. Render is deterministic from
  // input alone; the test suite still passes {} or no options at all.
  // (Kept as an explicit shape so future flags can be added without churn.)
};

// --------------------------------------------------------------------------
// Constants — DESIGN.md tokens duplicated inline for deterministic rendering
// --------------------------------------------------------------------------
//
// These MUST stay in sync with DESIGN.md. tests/unit/design-tokens.test.ts
// covers the source file; the dashboard-render snapshot test catches drift
// here. If you change DESIGN.md, update both.

const TOKENS = {
  paper: "#F4F1EB",
  paperRaised: "#FBF8F3",
  ink: "#1B1A17",
  inkMuted: "#4C4740",
  accent: "#A6332A",
  onAccent: "#F4F1EB",
  danger: "#8F2A22",
  success: "#2E5E4E",
  warning: "#8A5A12",
  info: "#2D5673",
  accentSoft: "#E8D7D3",
  dangerSoft: "#EED9D6",
  successSoft: "#DCE8E1",
  warningSoft: "#EEE3D1",
  infoSoft: "#D9E4EB",
  headlineFamily: "Source Serif 4",
  bodyFamily: "IBM Plex Sans",
  monoFamily: "IBM Plex Mono",
  bodySize: "16px",
  bodyLineHeight: "1.5",
  spaceXxs: "4px",
  spaceXs: "8px",
  spaceSm: "12px",
  spaceMd: "16px",
  spaceLg: "24px",
  spaceXl: "32px",
  roundedSm: "2px",
  roundedMd: "4px",
  roundedLg: "8px",
};

const MONTH_NAMES_DK = [
  "januar", "februar", "marts", "april", "maj", "juni",
  "juli", "august", "september", "oktober", "november", "december",
];

// --------------------------------------------------------------------------
// HTML escape (deterministic, no externals)
// --------------------------------------------------------------------------

function escapeHtml(value: string | number | null | undefined): string {
  if (value == null) return "";
  const str = String(value);
  let out = "";
  for (let i = 0; i < str.length; i++) {
    const ch = str.charCodeAt(i);
    switch (ch) {
      case 38: out += "&amp;"; break;
      case 60: out += "&lt;"; break;
      case 62: out += "&gt;"; break;
      case 34: out += "&quot;"; break;
      case 39: out += "&#39;"; break;
      default: out += str[i];
    }
  }
  return out;
}

// --------------------------------------------------------------------------
// Formatting (Danish locale, deterministic)
// --------------------------------------------------------------------------

/**
 * Format an amount as `1.234,56 DKK` with non-breaking space before currency.
 * Always 2 decimals, period thousand-sep, comma decimal-sep, minus prefix for
 * negatives. Uses scaled integer math via toFixed to avoid float drift on the
 * deterministic input ranges this dashboard operates on (DKK with 2 decimals).
 */
export function formatDkk(amount: number): string {
  if (!Number.isFinite(amount)) return "—";
  const negative = amount < 0;
  const abs = Math.abs(amount);
  // money-allowed: display-only formatting of already-rounded DKK, not currency math.
  const fixed = abs.toFixed(2);
  const [whole, frac] = fixed.split(".");
  let grouped = "";
  for (let i = 0; i < whole!.length; i++) {
    const remaining = whole!.length - i;
    grouped += whole![i];
    if (remaining > 1 && remaining % 3 === 1) grouped += ".";
  }
  const sign = negative ? "-" : "";
  // Non-breaking space U+00A0 between number and currency.
  return `${sign}${grouped},${frac} DKK`;
}

/** YYYY-MM-DD → "17. maj 2026" */
function formatDateLong(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!m) return iso;
  const year = m[1]!;
  const month = parseInt(m[2]!, 10);
  const day = parseInt(m[3]!, 10);
  if (month < 1 || month > 12) return iso;
  return `${day}. ${MONTH_NAMES_DK[month - 1]} ${year}`;
}

/** YYYY-MM-DD → "DD-MM" */
function formatDateShort(iso: string | null | undefined): string {
  if (!iso) return "—";
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!m) return iso;
  return `${m[3]}-${m[2]}`;
}

/** ISO 8601 → "DD-MM HH:mm" (UTC, no timezone math; render-engine is pure). */
function formatTimestampShort(iso: string | null | undefined): string {
  if (!iso) return "—";
  // Accept both "YYYY-MM-DD HH:MM:SS" (SQLite default) and "YYYY-MM-DDTHH:MM:SSZ".
  const m = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2})/.exec(iso);
  if (!m) return iso;
  return `${m[3]}-${m[2]} ${m[4]}:${m[5]}`;
}

function truncate(value: string, max: number): string {
  if (value.length <= max) return value;
  return value.slice(0, max - 1) + "…";
}

/** YYYY-MM-DD → quarter info for the period containing the date. */
function quarterPeriod(asOfDate: string): { label: string; start: string; end: string; quarter: number; year: number } {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(asOfDate);
  if (!m) return { label: "—", start: asOfDate, end: asOfDate, quarter: 0, year: 0 };
  const year = parseInt(m[1]!, 10);
  const month = parseInt(m[2]!, 10);
  const quarter = Math.floor((month - 1) / 3) + 1;
  const startMonth = (quarter - 1) * 3 + 1;
  const endMonth = startMonth + 2;
  const lastDay = new Date(Date.UTC(year, endMonth, 0)).getUTCDate(); // pure math, no Date.now
  const pad = (n: number) => String(n).padStart(2, "0");
  const start = `${year}-${pad(startMonth)}-01`;
  const end = `${year}-${pad(endMonth)}-${pad(lastDay)}`;
  const label = `Q${quarter} ${year} (01-${pad(startMonth)} → ${pad(lastDay)}-${pad(endMonth)})`;
  return { label, start, end, quarter, year };
}

/** Days between two YYYY-MM-DD dates (b - a). Pure UTC math, no env. */
function daysBetween(a: string, b: string): number {
  const pa = /^(\d{4})-(\d{2})-(\d{2})/.exec(a);
  const pb = /^(\d{4})-(\d{2})-(\d{2})/.exec(b);
  if (!pa || !pb) return 0;
  const da = Date.UTC(parseInt(pa[1]!, 10), parseInt(pa[2]!, 10) - 1, parseInt(pa[3]!, 10));
  const db = Date.UTC(parseInt(pb[1]!, 10), parseInt(pb[2]!, 10) - 1, parseInt(pb[3]!, 10));
  return Math.round((db - da) / 86400000);
}

function daysAgoLabel(days: number | null): string {
  if (days == null) return "ingen registreret";
  if (days <= 0) return "i dag";
  if (days === 1) return "1 dag siden";
  return `${days} dage siden`;
}

// --------------------------------------------------------------------------
// CSS (inline, generated from TOKENS)
// --------------------------------------------------------------------------

function buildStyle(): string {
  // Body fallback chain stays consistent across browsers without needing the
  // Google Fonts request to succeed. Mono fallback chain keeps tabular figures.
  const bodyStack = `"${TOKENS.bodyFamily}", -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif`;
  const headlineStack = `"${TOKENS.headlineFamily}", Georgia, "Times New Roman", serif`;
  const monoStack = `"${TOKENS.monoFamily}", "SF Mono", Menlo, Consolas, monospace`;

  return `
:root {
  --paper: ${TOKENS.paper};
  --paper-raised: ${TOKENS.paperRaised};
  --ink: ${TOKENS.ink};
  --ink-muted: ${TOKENS.inkMuted};
  --accent: ${TOKENS.accent};
  --on-accent: ${TOKENS.onAccent};
  --danger: ${TOKENS.danger};
  --success: ${TOKENS.success};
  --warning: ${TOKENS.warning};
  --info: ${TOKENS.info};
  --accent-soft: ${TOKENS.accentSoft};
  --danger-soft: ${TOKENS.dangerSoft};
  --success-soft: ${TOKENS.successSoft};
  --warning-soft: ${TOKENS.warningSoft};
  --info-soft: ${TOKENS.infoSoft};
  --space-xxs: ${TOKENS.spaceXxs};
  --space-xs: ${TOKENS.spaceXs};
  --space-sm: ${TOKENS.spaceSm};
  --space-md: ${TOKENS.spaceMd};
  --space-lg: ${TOKENS.spaceLg};
  --space-xl: ${TOKENS.spaceXl};
  --rounded-sm: ${TOKENS.roundedSm};
  --rounded-md: ${TOKENS.roundedMd};
  --rounded-lg: ${TOKENS.roundedLg};
}
* { box-sizing: border-box; }
html, body {
  margin: 0;
  padding: 0;
  background: var(--paper);
  color: var(--ink);
  font-family: ${bodyStack};
  font-size: ${TOKENS.bodySize};
  line-height: ${TOKENS.bodyLineHeight};
}
.page {
  max-width: 960px;
  margin: 0 auto;
  padding: var(--space-xl) var(--space-lg);
}
.headline {
  font-family: ${headlineStack};
  font-weight: 600;
  letter-spacing: -0.01em;
}
.mono {
  font-family: ${monoStack};
  font-variant-numeric: tabular-nums;
  font-feature-settings: "tnum" 1;
}
.amount {
  font-family: ${monoStack};
  font-variant-numeric: tabular-nums;
  font-feature-settings: "tnum" 1;
  text-align: right;
  white-space: nowrap;
}
.amount-lg {
  font-family: ${monoStack};
  font-variant-numeric: tabular-nums;
  font-feature-settings: "tnum" 1;
  font-size: 28px;
  line-height: 1.1;
  color: var(--ink);
}
.label-sm {
  font-family: ${bodyStack};
  font-size: 11px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--ink-muted);
}
.header {
  background: var(--paper-raised);
  border: 1px solid var(--ink-muted);
  border-radius: var(--rounded-md);
  padding: var(--space-lg);
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: var(--space-lg);
}
.header h1 {
  margin: 0 0 var(--space-xs) 0;
  font-family: ${headlineStack};
  font-size: 24px;
  font-weight: 600;
  color: var(--ink);
}
.header .meta {
  color: var(--ink-muted);
  font-size: 14px;
}
.header .cvr {
  font-family: ${monoStack};
  color: var(--ink-muted);
  font-size: 14px;
}
.metrics {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: var(--space-sm);
  margin-bottom: var(--space-lg);
}
.metric-card {
  background: var(--paper-raised);
  border: 1px solid var(--ink-muted);
  border-radius: var(--rounded-md);
  padding: var(--space-md);
}
.metric-card.accent { border-color: var(--accent); }
.metric-card.danger { border-color: var(--danger); }
.metric-card .value { color: var(--ink); }
.metric-card .secondary {
  color: var(--ink-muted);
  font-size: 13px;
  margin-top: var(--space-xxs);
}
.metric-card .label-sm { margin-top: var(--space-sm); }
.section {
  margin-bottom: var(--space-lg);
}
.section h2 {
  font-family: ${headlineStack};
  font-size: 18px;
  font-weight: 600;
  margin: 0 0 var(--space-sm) 0;
  color: var(--ink);
  border-bottom: 1px solid var(--ink-muted);
  padding-bottom: var(--space-xs);
}
.deadline-card {
  background: var(--paper-raised);
  border: 1px solid var(--ink-muted);
  border-radius: var(--rounded-md);
  padding: var(--space-md);
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-md);
}
.deadline-card .label-sm { margin-bottom: var(--space-xxs); }
table.dash-table {
  width: 100%;
  border-collapse: collapse;
  background: var(--paper);
}
table.dash-table th, table.dash-table td {
  padding: var(--space-xs) var(--space-sm);
  text-align: left;
  border-bottom: 1px solid var(--ink-muted);
  font-size: 14px;
}
table.dash-table th {
  color: var(--ink-muted);
  font-weight: 500;
  font-size: 11px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}
table.dash-table tbody tr:nth-child(even) { background: var(--paper-raised); }
table.dash-table td.amount, table.dash-table th.amount { text-align: right; }
table.dash-table td.center, table.dash-table th.center { text-align: center; }
.pill {
  display: inline-block;
  padding: 2px var(--space-xs);
  border-radius: var(--rounded-sm);
  font-size: 12px;
  font-weight: 500;
  letter-spacing: 0.02em;
  color: var(--ink);
}
.pill.success { background: var(--success-soft); }
.pill.warning { background: var(--warning-soft); }
.pill.danger { background: var(--danger-soft); }
.pill.neutral { background: var(--paper-raised); border: 1px solid var(--ink-muted); }
.status-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--space-sm) 0;
  border-bottom: 1px solid var(--ink-muted);
}
.status-row:last-child { border-bottom: none; }
.status-row .label { color: var(--ink); font-weight: 500; }
.status-row .detail { color: var(--ink-muted); font-size: 13px; }
.muted { color: var(--ink-muted); }
.empty-state {
  color: var(--ink-muted);
  font-style: italic;
  padding: var(--space-sm) 0;
}
.footer {
  background: var(--paper-raised);
  border: 1px solid var(--ink-muted);
  border-radius: var(--rounded-md);
  padding: var(--space-md);
  color: var(--ink-muted);
  font-size: 12px;
  margin-top: var(--space-lg);
}
.footer .row { display: flex; justify-content: space-between; gap: var(--space-md); }
.footer .mono { color: var(--ink-muted); }
.activity-log {
  display: grid;
  grid-template-columns: auto auto auto 1fr;
  gap: var(--space-xs) var(--space-md);
  font-size: 14px;
}
.activity-log .time { font-family: ${monoStack}; color: var(--ink); }
.activity-log .actor { color: var(--ink-muted); }
.activity-log .event { font-family: ${monoStack}; color: var(--ink); }
.activity-log .message { color: var(--ink); }
@media print {
  body { background: white; }
  .page { padding: var(--space-md); max-width: 100%; }
}
`.trim();
}

// --------------------------------------------------------------------------
// Components
// --------------------------------------------------------------------------

function fontLink(): string {
  // Single deterministic Google Fonts URL. HTML still renders if blocked.
  const families = [
    "family=Source+Serif+4:wght@400;600",
    "family=IBM+Plex+Sans:wght@400;500",
    "family=IBM+Plex+Mono:wght@400;500",
  ].join("&");
  return `<link rel="preconnect" href="https://fonts.googleapis.com">\n<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>\n<link rel="stylesheet" href="https://fonts.googleapis.com/css2?${families}&display=swap">`;
}

export function metricCard(label: string, value: string, secondary?: string, accent?: "accent" | "danger" | null): string {
  const cls = accent ? `metric-card ${accent}` : "metric-card";
  const secondaryHtml = secondary
    ? `<div class="secondary">${escapeHtml(secondary)}</div>`
    : `<div class="secondary muted">&nbsp;</div>`;
  return `<div class="${cls}">
  <div class="value amount-lg">${escapeHtml(value)}</div>
  ${secondaryHtml}
  <div class="label-sm">${escapeHtml(label)}</div>
</div>`;
}

export function auditStatusPill(ok: boolean, entryCount: number, firstError?: string): string {
  if (ok) {
    return `<span class="pill success">✔ OK</span> <span class="muted">${escapeHtml(entryCount)} entries</span>`;
  }
  const detail = firstError ? truncate(firstError, 80) : "ukendt fejl";
  return `<span class="pill danger">✘ FEJL</span> <span class="muted">${escapeHtml(detail)}</span>`;
}

export function backupStatusPill(backup: BackupComplianceStatus): { pill: string; detail: string } {
  const days = backup.daysSinceLatestBackup;
  if (backup.backupsFound === 0) {
    return { pill: `<span class="pill danger">Ingen backup</span>`, detail: "ingen registreret" };
  }
  if (days === null || days > 7) {
    return { pill: `<span class="pill danger">Forfalden</span>`, detail: daysAgoLabel(days) };
  }
  if (days >= 5) {
    return { pill: `<span class="pill warning">Snart due</span>`, detail: daysAgoLabel(days) };
  }
  return { pill: `<span class="pill success">✔ OK</span>`, detail: daysAgoLabel(days) };
}

function invoiceStatusPill(row: InvoiceListRow): string {
  if (row.isOverdue) return `<span class="pill danger">overdue${row.overdueDays > 0 ? ` (${row.overdueDays} d)` : ""}</span>`;
  return `<span class="pill success">open</span>`;
}

export function invoiceTable(result: InvoiceListResult, maxRows = 10): string {
  if (result.rows.length === 0) {
    return `<div class="empty-state">Ingen åbne fakturaer</div>`;
  }
  const sorted = [...result.rows].sort((a, b) => {
    const ad = a.effectiveDueDate ?? "9999-99-99";
    const bd = b.effectiveDueDate ?? "9999-99-99";
    if (ad < bd) return -1;
    if (ad > bd) return 1;
    return a.invoiceNumber.localeCompare(b.invoiceNumber);
  });
  const visible = sorted.slice(0, maxRows);
  const overflow = sorted.length - visible.length;
  const rows = visible.map((row) => {
    const customer = row.customerName ?? row.customerCvr ?? "—";
    return `<tr>
  <td class="mono">${escapeHtml(row.invoiceNumber)}</td>
  <td>${escapeHtml(customer)}</td>
  <td class="amount">${escapeHtml(formatDkk(row.openBalance))}</td>
  <td class="amount mono">${escapeHtml(formatDateShort(row.effectiveDueDate))}</td>
  <td class="center">${invoiceStatusPill(row)}</td>
</tr>`;
  }).join("\n");
  const overflowRow = overflow > 0
    ? `<div class="muted" style="margin-top: var(--space-xs); font-size: 13px;">… og ${overflow} yderligere</div>`
    : "";
  return `<table class="dash-table">
  <thead>
    <tr>
      <th>Fakturanr.</th>
      <th>Kunde</th>
      <th class="amount">Beløb</th>
      <th class="amount">Forfald</th>
      <th class="center">Status</th>
    </tr>
  </thead>
  <tbody>
${rows}
  </tbody>
</table>
${overflowRow}`;
}

function activityList(rows: AuditLogRow[]): string {
  if (rows.length === 0) {
    return `<div class="empty-state">Ingen aktivitet endnu</div>`;
  }
  const items = rows.slice(0, 10).map((row) =>
    `  <div class="time">${escapeHtml(formatTimestampShort(row.createdAt))}</div>
  <div class="actor">${escapeHtml(row.actor)}</div>
  <div class="event">${escapeHtml(row.eventType)}</div>
  <div class="message">${escapeHtml(truncate(row.message, 60))}</div>`
  ).join("\n");
  return `<div class="activity-log">
${items}
</div>`;
}

function deadlineSection(input: DashboardInput): string {
  const period = quarterPeriod(input.asOfDate);
  const daysRemaining = input.vatDaysRemaining;
  const errors = input.vatPeriod.errors ?? [];
  let pill: string;
  let detail: string;
  if (errors.length > 0) {
    pill = `<span class="pill warning">Kan ikke beregne</span>`;
    detail = truncate(errors[0]!, 80);
  } else if (daysRemaining < 0) {
    pill = `<span class="pill danger">Forfalden</span>`;
    detail = `${Math.abs(daysRemaining)} dage over`;
  } else if (daysRemaining <= 14) {
    pill = `<span class="pill warning">${daysRemaining} dage tilbage</span>`;
    detail = "";
  } else {
    pill = `<span class="pill success">${daysRemaining} dage tilbage</span>`;
    detail = "";
  }
  const net = input.vatPeriod.netVatPayable;
  const netLabel = net < 0 ? "Til gode" : "Est. nettomoms";
  const netValue = formatDkk(net);
  return `<div class="deadline-card">
  <div>
    <div class="label-sm">Næste momsperiode</div>
    <div class="headline" style="font-size: 18px;">${escapeHtml(period.label)}</div>
    <div class="muted" style="font-size: 13px; margin-top: var(--space-xxs);">${pill} ${escapeHtml(detail)}</div>
  </div>
  <div style="text-align: right;">
    <div class="label-sm">${escapeHtml(netLabel)}</div>
    <div class="amount-lg">${escapeHtml(netValue)}</div>
  </div>
</div>`;
}

function header(input: DashboardInput): string {
  const company = input.company;
  const dateLong = formatDateLong(input.asOfDate);
  const backupDays = input.backup.daysSinceLatestBackup;
  const backupLabel = input.backup.backupsFound === 0
    ? "Backup: ingen registreret"
    : `Backup: ${daysAgoLabel(backupDays)}`;
  const cvrLine = company.cvr
    ? `<div class="cvr">CVR DK${escapeHtml(company.cvr)}</div>`
    : "";
  return `<header class="header">
  <div>
    <h1>${escapeHtml(company.name)}</h1>
    <div class="meta">Dashboard · ${escapeHtml(dateLong)} · ${escapeHtml(backupLabel)}</div>
  </div>
  ${cvrLine}
</header>`;
}

function metricsSection(input: DashboardInput): string {
  const openSum = input.invoices.rows.reduce((acc, r) => acc + r.openBalance, 0);
  const overdueOldest = input.overdueInvoices.rows.reduce((acc, r) => Math.max(acc, r.overdueDays), 0);
  return `<section class="metrics">
${metricCard("ÅBNE FAKTURAER", String(input.invoices.count), `${formatDkk(openSum)}`, null)}
${metricCard("OVERFORFALDNE", String(input.overdueInvoices.count), input.overdueInvoices.count > 0 ? `ældste ${overdueOldest} d` : "0 dage", input.overdueInvoices.count > 0 ? "accent" : null)}
${metricCard("ULINKEDE BANK-TX", String(input.unlinkedBank.count), undefined, null)}
${metricCard("ÅBNE EXCEPTIONS", String(input.exceptions.count), undefined, input.exceptions.count > 0 ? "danger" : null)}
</section>`;
}

function statusSection(input: DashboardInput): string {
  const backupPill = backupStatusPill(input.backup);
  const backupSub = input.backup.latestBackupAt
    ? formatTimestampShort(input.backup.latestBackupAt)
    : "—";
  const activityNote = input.backup.hasActivitySinceBackup && (input.backup.daysSinceLatestBackup ?? 0) > 0
    ? " (ændringer siden seneste backup)"
    : "";
  const audit = input.audit;
  return `<section class="section">
  <h2>System-status</h2>
  <div class="status-row">
    <div>
      <div class="label">Backup-status</div>
      <div class="detail">${escapeHtml(backupSub)}${escapeHtml(activityNote)}</div>
    </div>
    <div>${backupPill.pill} <span class="muted">${escapeHtml(backupPill.detail)}</span></div>
  </div>
  <div class="status-row">
    <div>
      <div class="label">Audit-chain</div>
      <div class="detail">verificeret ved render</div>
    </div>
    <div>${auditStatusPill(audit.ok, audit.entryCount, audit.firstError)}</div>
  </div>
</section>`;
}

function footer(input: DashboardInput): string {
  const generated = formatTimestampShort(input.generatedAt);
  return `<footer class="footer">
  <div class="row">
    <div>Commit <span class="mono">${escapeHtml(input.commitSha)}</span> · rules <span class="mono">${escapeHtml(input.ruleBundleVersion)}</span> · genereret <span class="mono">${escapeHtml(generated)}</span></div>
    <div class="mono">github.com/mikkelkrogsholm/rentemester</div>
  </div>
</footer>`;
}

// --------------------------------------------------------------------------
// Main render
// --------------------------------------------------------------------------

export function renderDashboard(input: DashboardInput, _options: RenderOptions = {}): string {
  const company = input.company;
  const title = `Rentemester — ${company.name} — ${input.asOfDate}`;

  const sections = [
    header(input),
    metricsSection(input),
    `<section class="section"><h2>Næste deadline</h2>${deadlineSection(input)}</section>`,
    `<section class="section"><h2>Åbne fakturaer</h2>${invoiceTable(input.invoices)}</section>`,
    `<section class="section"><h2>Seneste aktivitet</h2>${activityList(input.recentActivity)}</section>`,
    statusSection(input),
    footer(input),
  ].join("\n");

  return `<!DOCTYPE html>
<html lang="da">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(title)}</title>
${fontLink()}
<style>
${buildStyle()}
</style>
</head>
<body>
<main class="page">
${sections}
</main>
</body>
</html>
`;
}
