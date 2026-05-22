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
import {
  vatPeriodWindowFor,
  vatPeriodLabel,
  DEFAULT_VAT_PERIOD_TYPE,
  type VatPeriodType,
} from "./periods";
import { formatKronerDa } from "./money";
import { exceptionTypeDa, severityDa } from "./messages";

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

export type DashboardExceptionRow = {
  id: number;
  type: string;
  severity: string;
  status: string;
  message: string;
  /**
   * The concrete "what the owner must do" guidance — the most useful part of
   * an exception. Optional: rendered as "Sådan løser du den" when present, so
   * the static dashboard reaches parity with the Cockpit SPA (#270). The CLI
   * passes it through from `core/exceptions`' `required_action` column.
   */
  requiredAction?: string | null;
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
 * Format an amount as canonical Danish kroner-og-øre, e.g. `1.234,56 kr.`.
 * Always 2 decimals, period thousand-sep, comma decimal-sep, minus prefix for
 * negatives.
 *
 * #314: this used to emit `1.234,56 DKK` (NBSP + "DKK" suffix); it now
 * delegates to the single canonical formatter `formatKronerDa` in
 * `core/money.ts`, so the dashboard emits the identical `" kr."` string as
 * every other human-facing surface. The `formatDkk` name is kept because the
 * dashboard render-engine and its tests reference it.
 */
export function formatDkk(amount: number): string {
  return formatKronerDa(amount);
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

function daysAgoLabel(days: number | null): string {
  if (days == null) return "ingen registreret";
  if (days <= 0) return "i dag";
  if (days === 1) return "1 dag siden";
  return `${days} dage siden`;
}

/** Signed day difference `toDate - fromDate` between two YYYY-MM-DD dates, UTC-based, pure. */
function signedDaysBetween(fromDate: string, toDate: string): number {
  const pf = /^(\d{4})-(\d{2})-(\d{2})/.exec(fromDate);
  const pt = /^(\d{4})-(\d{2})-(\d{2})/.exec(toDate);
  if (!pf || !pt) return 0;
  const from = Date.UTC(parseInt(pf[1]!, 10), parseInt(pf[2]!, 10) - 1, parseInt(pf[3]!, 10));
  const to = Date.UTC(parseInt(pt[1]!, 10), parseInt(pt[2]!, 10) - 1, parseInt(pt[3]!, 10));
  return Math.round((to - from) / 86400000);
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
.footer .provenance {
  margin-top: var(--space-xs);
  color: var(--ink-muted);
  font-size: 11px;
}
.footer .provenance summary {
  cursor: pointer;
  letter-spacing: 0.04em;
}
.activity-log {
  display: grid;
  grid-template-columns: auto auto auto 1fr;
  gap: var(--space-xs) var(--space-md);
  font-size: 14px;
}
.activity-log .time { font-family: ${monoStack}; color: var(--ink); white-space: nowrap; }
.activity-log .actor { color: var(--ink-muted); }
.activity-log .event { color: var(--ink); font-weight: 500; }
.activity-log .message { color: var(--ink-muted); }
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

// The audit log records events under terse machine codes (`journal_reverse`,
// `document_ingest`, ...). On the human-facing dashboard those read as the
// system's internals, not an overview of the business — so the "Seneste
// aktivitet" strip translates each event code to a plain-Danish label. (#233)
const ACTIVITY_EVENT_DA: Record<string, string> = {
  asset_depreciation_post: "Afskrivning bogført",
  asset_immediate_writeoff: "Straksafskrivning bogført",
  asset_register: "Aktiv registreret",
  authority_export: "Eksport til myndighed",
  backup_archive_created: "Backup-arkiv oprettet",
  backup_destination_added: "Backup-destination tilføjet",
  backup_destination_removed: "Backup-destination fjernet",
  backup_lock_configured: "Bogføringslås konfigureret",
  backup_placed: "Backup placeret eksternt",
  bank_account_add: "Bankkonto oprettet",
  bank_import: "Banktransaktioner importeret",
  company_cvr_sync: "Stamdata hentet fra CVR",
  credit_note_issue: "Kreditnota udstedt",
  customer_create: "Kunde oprettet",
  document_ingest: "Bilag indlæst",
  gdpr_erasure: "Persondata slettet (GDPR)",
  import_chart_reconcile: "Kontoplan afstemt ved import",
  import_company_reconcile: "Virksomhed afstemt ved import",
  invoice_bad_debt_writeoff: "Tab på debitor bogført",
  invoice_claim_payment_apply: "Betaling af krav registreret",
  invoice_compensation_post: "Kompensation bogført",
  invoice_compensation_register: "Kompensationskrav registreret",
  invoice_email_send: "Faktura sendt på email",
  invoice_interest_post: "Morarente bogført",
  invoice_interest_register: "Morarentekrav registreret",
  invoice_issue: "Faktura udstedt",
  invoice_payment_apply: "Fakturabetaling registreret",
  invoice_refund_apply: "Refundering til kunde bogført",
  invoice_reminder_post: "Rykker bogført",
  invoice_reminder_register: "Rykker registreret",
  invoice_render_pdf: "Faktura-PDF genereret",
  journal_post: "Finanspostering bogført",
  journal_reverse: "Finanspostering tilbageført",
  mileage_entry_create: "Kørselspost registreret",
  mileage_log_export: "Kørselsregnskab eksporteret",
  opening_balance_post: "Primobalance bogført",
  period_close: "Regnskabsperiode lukket",
  period_report: "Regnskabsperiode markeret indberettet",
  public_einvoice_oioubl_export: "OIOUBL e-faktura eksporteret",
  public_einvoice_peppol_submission: "PEPPOL e-faktura afsendt",
  recurring_invoice_generate: "Gentagende faktura genereret",
  recurring_invoice_template_create: "Fakturaskabelon oprettet",
  saft_export: "SAF-T-eksport",
  system_backup: "Backup oprettet",
  system_restore: "Backup gendannet",
  vendor_create: "Leverandør oprettet",
};

/** Translate an audit event code to a plain-Danish label, never an internal code. (#233) */
function activityEventLabel(eventType: string): string {
  const known = ACTIVITY_EVENT_DA[eventType];
  if (known) return known;
  // Unknown code: humanise it (replace underscores, capitalise) rather than
  // showing the raw snake_case identifier.
  const words = eventType.replace(/_/g, " ").trim();
  return words.length > 0 ? words.charAt(0).toUpperCase() + words.slice(1) : "Aktivitet";
}

// The audit log persists its detail messages in English ("Created customer
// ...", "Rendered invoice PDF ...", "Company volume initialized"). The event
// headings are already translated (#233), but the detail text below each one
// still leaked English onto the Danish-facing dashboard. The patterns below
// translate each known message template to plain Danish, preserving the
// variable part (customer name, invoice number, ...) verbatim. An unknown
// message falls through untouched so no information is ever lost. (#286)
const ACTIVITY_MESSAGE_PATTERNS: Array<{ re: RegExp; da: (m: RegExpMatchArray) => string }> = [
  { re: /^Company volume initialized$/, da: () => "Virksomhed oprettet" },
  { re: /^Created customer (.+)$/s, da: (m) => `Kunde oprettet: ${m[1]}` },
  { re: /^Created vendor (.+)$/s, da: (m) => `Leverandør oprettet: ${m[1]}` },
  { re: /^Created full backup (.+)$/s, da: (m) => `Fuld backup oprettet: ${m[1]}` },
  { re: /^Created recurring invoice template (.+)$/s, da: (m) => `Fakturaskabelon oprettet: ${m[1]}` },
  { re: /^Re-rendered invoice PDF (.+)$/s, da: (m) => `Faktura-PDF gendannet: ${m[1]}` },
  { re: /^Rendered invoice PDF (.+)$/s, da: (m) => `Faktura-PDF genereret: ${m[1]}` },
  { re: /^Ingested supporting document (\S+) \((.+)\)$/s, da: (m) => `Bilag ${m[1]} indlæst (${m[2]})` },
  { re: /^Ingested supporting document (.+)$/s, da: (m) => `Bilag ${m[1]} indlæst` },
  { re: /^Issued invoice (.+)$/s, da: (m) => `Faktura udstedt: ${m[1]}` },
  { re: /^Issued credit note (.+?) for (.+)$/s, da: (m) => `Kreditnota ${m[1]} udstedt for ${m[2]}` },
  { re: /^Posted journal entry (.+)$/s, da: (m) => `Finanspostering bogført: ${m[1]}` },
  { re: /^Reversed journal entry (.+?) with (.+)$/s, da: (m) => `Finanspostering ${m[1]} tilbageført med ${m[2]}` },
  { re: /^Added bank account (.+)$/s, da: (m) => `Bankkonto oprettet: ${m[1]}` },
  { re: /^Imported (\d+) bank transactions from (.+)$/s, da: (m) => `${m[1]} banktransaktioner importeret fra ${m[2]}` },
  { re: /^Applied payment (.+?) to invoice (.+)$/s, da: (m) => `Betaling ${m[1]} registreret på faktura ${m[2]}` },
  { re: /^Applied refund (.+?) to invoice (.+)$/s, da: (m) => `Refundering ${m[1]} bogført på faktura ${m[2]}` },
  { re: /^Applied claim receipt (.+?) to invoice (.+?) via combined settlement$/s, da: (m) => `Indbetaling på krav ${m[1]} registreret på faktura ${m[2]} via samlet afregning` },
  { re: /^Applied claim receipt (.+?) to invoice (.+)$/s, da: (m) => `Indbetaling på krav ${m[1]} registreret på faktura ${m[2]}` },
  { re: /^Wrote off bad debt (.+?) on invoice (.+)$/s, da: (m) => `Tab på debitor ${m[1]} bogført på faktura ${m[2]}` },
  { re: /^Registered asset (.+)$/s, da: (m) => `Aktiv registreret: ${m[1]}` },
  { re: /^Posted opening balance \(primobalance\) pr\. (.+?) as (.+)$/s, da: (m) => `Primobalance pr. ${m[1]} bogført som ${m[2]}` },
  { re: /^Restored from backup (.+)$/s, da: (m) => `Gendannet fra backup ${m[1]}` },
];

/**
 * Render an audit-log detail message in plain Danish. The audit log itself
 * stores English templates (immutable history); the dashboard translates them
 * for display only. Unknown messages pass through unchanged. (#286)
 */
function activityMessageDanish(message: string): string {
  const text = message ?? "";
  for (const { re, da } of ACTIVITY_MESSAGE_PATTERNS) {
    const m = re.exec(text);
    if (m) return da(m);
  }
  return text;
}

function activityList(rows: AuditLogRow[]): string {
  if (rows.length === 0) {
    return `<div class="empty-state">Ingen aktivitet endnu</div>`;
  }
  const items = rows.slice(0, 10).map((row) =>
    `  <div class="time">${escapeHtml(formatTimestampShort(row.createdAt))}</div>
  <div class="actor">${escapeHtml(row.actor)}</div>
  <div class="event">${escapeHtml(activityEventLabel(row.eventType))}</div>
  <div class="message">${escapeHtml(activityMessageDanish(row.message))}</div>`
  ).join("\n");
  return `<div class="activity-log">
${items}
</div>`;
}

function deadlineSection(input: DashboardInput): string {
  // The "Næste momsfrist" box must describe the VAT period the CLI selected —
  // the earliest unreported period that carries activity — NOT the calendar
  // period today falls in. The CLI delivers that period as `vatPeriod`; the
  // render-engine keys the label/deadline off `vatPeriod.periodStart` so the
  // box always agrees with the figure shown beside it. (#281)
  //
  // #299: the period window + label + filing deadline follow the company's
  // real VAT cadence (`vatPeriodType`) — a half-yearly filer sees "1. halvår
  // 2026" with the half-year deadline, not a quarter. For a `quarter` company
  // the window/label/deadline are byte-identical to the historical behaviour.
  const vatType: VatPeriodType =
    input.company.vatPeriodType ?? DEFAULT_VAT_PERIOD_TYPE;
  const validStart = /^(\d{4})-(\d{2})-(\d{2})/.test(input.vatPeriod.periodStart);
  const window = validStart
    ? vatPeriodWindowFor(input.vatPeriod.periodStart, vatType)
    : null;
  const period = {
    label: window ? vatPeriodLabel(window) : "—",
  };
  // The countdown targets the real SKAT filing/payment deadline — the 1st of
  // the third month after the period ends — for the company's actual cadence.
  const deadline = window ? window.filingDeadline : null;
  const daysRemaining = deadline ? signedDaysBetween(input.asOfDate, deadline) : 0;
  const errors = input.vatPeriod.errors ?? [];
  let pill: string;
  let detail: string;
  if (errors.length > 0) {
    pill = `<span class="pill warning">Kan ikke beregne</span>`;
    detail = truncate(errors[0]!, 80);
  } else if (!deadline) {
    pill = `<span class="pill warning">Kan ikke beregne</span>`;
    detail = "";
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
  const deadlineLine = deadline
    ? `<div class="muted" style="font-size: 13px; margin-top: var(--space-xxs);">SKAT-frist: <span class="mono">${escapeHtml(deadline)}</span></div>`
    : "";
  const net = input.vatPeriod.netVatPayable;
  const netLabel = net < 0 ? "Til gode" : "Est. nettomoms";
  const netValue = formatDkk(net);
  return `<div class="deadline-card">
  <div>
    <div class="label-sm">Næste momsfrist</div>
    <div class="headline" style="font-size: 18px;">${escapeHtml(period.label)}</div>
    ${deadlineLine}
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
    ? `<div class="cvr">CVR ${escapeHtml(company.cvr)}</div>`
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
${metricCard("BANKPOSTER UDEN BILAG", String(input.unlinkedBank.count), undefined, null)}
${metricCard("ÅBNE EXCEPTIONS", String(input.exceptions.count), undefined, input.exceptions.count > 0 ? "danger" : null)}
</section>`;
}

// A bare exception count tells the owner *that* something needs attention but
// not *what* — which only creates unease and forces a trip to the terminal.
// The dashboard therefore lists each open exception as a short line: severity,
// type, and a (truncated) message. (#263)
const EXCEPTION_SEVERITY_PILL: Record<string, "danger" | "warning" | "neutral"> = {
  high: "danger",
  critical: "danger",
  medium: "warning",
  low: "neutral",
};

// The owner faces the dashboard, not the developer. An exception heading like
// `UNMATCHED_BANK_TRANSACTION` and an English severity pill "medium" read as
// the system's internals — so the static dashboard renders a plain-Danish
// label for the type and the severity, matching what the Cockpit SPA shows.
// The Danish labels live in core/messages.ts: `exceptionTypeDa` plain-Danish
// heading (humanises unknown codes), `severityDa(.., "title")` capitalised
// severity. (#270, #316)

function exceptionsSection(input: DashboardInput): string {
  const result = input.exceptions;
  if (result.count === 0 || result.rows.length === 0) {
    return `<div class="empty-state">Ingen åbne exceptions</div>`;
  }
  // Stable order: highest severity first, then by id so the render is
  // deterministic regardless of the row order the CLI passes in.
  const severityRank: Record<string, number> = { critical: 0, high: 1, medium: 2, low: 3 };
  const sorted = [...result.rows].sort((a, b) => {
    const ra = severityRank[a.severity] ?? 9;
    const rb = severityRank[b.severity] ?? 9;
    if (ra !== rb) return ra - rb;
    return a.id - b.id;
  });
  const maxRows = 12;
  const visible = sorted.slice(0, maxRows);
  const overflow = sorted.length - visible.length;
  const items = visible.map((row) => {
    const pillClass = EXCEPTION_SEVERITY_PILL[row.severity] ?? "neutral";
    // The full message — not a mid-sentence-truncated fragment. The Cockpit
    // SPA shows the whole message; the static dashboard must too. (#270)
    const action = (row.requiredAction ?? "").trim();
    const actionHtml = action
      ? `\n      <div class="detail"><strong>Sådan løser du den:</strong> ${escapeHtml(action)}</div>`
      : "";
    return `  <div class="status-row">
    <div>
      <div class="label">${escapeHtml(exceptionTypeDa(row.type))}</div>
      <div class="detail">${escapeHtml(row.message)}</div>${actionHtml}
    </div>
    <div><span class="pill ${pillClass}">${escapeHtml(severityDa(row.severity, "title"))}</span></div>
  </div>`;
  }).join("\n");
  const overflowRow = overflow > 0
    ? `<div class="muted" style="margin-top: var(--space-xs); font-size: 13px;">… og ${overflow} yderligere</div>`
    : "";
  return `<div class="section">
${items}
</div>
${overflowRow}`;
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
  // The footer faces the owner, not a developer. The raw commit hash and the
  // long rule-bundle-version string are build provenance — kept for support
  // traceability but tucked into a small <details>, never dumped on the calm
  // cockpit surface. The visible line is just "genereret <tid>". (#246)
  const provenance =
    `<details class="provenance"><summary>Teknisk version</summary>` +
    `<span class="mono">commit ${escapeHtml(input.commitSha)}</span> · ` +
    `<span class="mono">regelsæt ${escapeHtml(input.ruleBundleVersion)}</span></details>`;
  return `<footer class="footer">
  <div class="row">
    <div>Genereret <span class="mono">${escapeHtml(generated)}</span> · Rentemester</div>
    <div class="mono">github.com/mikkelkrogsholm/rentemester</div>
  </div>
  ${provenance}
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
    `<section class="section"><h2>Åbne exceptions</h2>${exceptionsSection(input)}</section>`,
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
