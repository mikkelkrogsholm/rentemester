// Shared types, tokens, and pure helpers for the dashboard render-engine.
//
// Everything here is consumed by 2+ section files. The render-engine stays
// deterministic: no Date.now(), no Math.random(), no filesystem, no env.

import type { CompanySettings } from "../company";
import type { InvoiceListResult } from "../invoice-list";
import type { BankTransactionListResult } from "../reconciliation";
import type { VatPeriodReport } from "../vat";
import type { BackupComplianceStatus } from "../system-backups";
import type { AuditLogRow } from "../audit-log";
import type { PayablesListResult } from "../payables";
import type { DueAccrualRecognitionResult, AccrualRegisterReport } from "../accruals";
import type { BudgetVsActualReport } from "../budget";
import type { LiquidityForecastResult } from "../liquidity-forecast";
import { formatKronerDa } from "../money";

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

/**
 * Corporate-tax status for the dashboard's Tax card.
 *
 * The render-engine stays pure: the CLI decides which state applies and the
 * card just renders it. There are two states, by fiscal-year lock status:
 *  - `available: false` — the open fiscal year. No final result to file from
 *    yet; the card shows "preparation available once the year is closed".
 *  - `available: true` — a closed fiscal year. The card shows the estimated
 *    selskabsskat and the count of needs-review items the slice did not
 *    compute deterministically.
 */
export type DashboardTaxStatus = {
  /** Fiscal-year label, e.g. "2025". */
  fiscalYearLabel: string;
  /** True once the fiscal year is closed and the tax return can be prepared. */
  available: boolean;
  /** Estimated corporate tax (selskabsskat), DKK — only when `available`. */
  corporateTax?: number | null;
  /** Bookkept årets resultat, DKK — only when `available`. */
  bookkeptResult?: number;
  /** Count of needs-review items — only when `available`. */
  needsReviewCount?: number;
};

/**
 * EU sales-list / OSS indicator for the dashboard.
 *
 * A LIGHT indicator: it only ever surfaces a card when there is cross-border
 * B2B sales activity (the EU-salg uden moms-liste) or OSS-classified consumer
 * sales in the period — i.e. something that needs a separate filing. When both
 * are zero the dashboard renders nothing for it.
 */
export type DashboardEuSalesOssStatus = {
  /** Net value (DKK) of cross-border B2B reverse-charge sales in the period. */
  euSalesValue: number;
  /** Number of EU customers on the recapitulative statement. */
  euCustomerCount: number;
  /** Net value (DKK) of OSS-classified EU consumer sales in the period. */
  ossConsumerSalesBase: number;
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
  /**
   * The recurring-feature inputs (#islands → control surfaces). All optional
   * so the render-engine and its existing fixtures stay backward-compatible:
   * a `DashboardInput` without them renders exactly the historical dashboard.
   * The CLI always supplies them.
   */
  payables?: PayablesListResult;
  accrualsDue?: DueAccrualRecognitionResult;
  accrualRegister?: AccrualRegisterReport;
  budgetVsActual?: BudgetVsActualReport;
  liquidity?: LiquidityForecastResult;
  tax?: DashboardTaxStatus;
  euSalesOss?: DashboardEuSalesOssStatus;
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

export const TOKENS = {
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

export const MONTH_NAMES_DK = [
  "januar", "februar", "marts", "april", "maj", "juni",
  "juli", "august", "september", "oktober", "november", "december",
];

// --------------------------------------------------------------------------
// HTML escape (deterministic, no externals)
// --------------------------------------------------------------------------

export function escapeHtml(value: string | number | null | undefined): string {
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
export function formatDateLong(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!m) return iso;
  const year = m[1]!;
  const month = parseInt(m[2]!, 10);
  const day = parseInt(m[3]!, 10);
  if (month < 1 || month > 12) return iso;
  return `${day}. ${MONTH_NAMES_DK[month - 1]} ${year}`;
}

/** YYYY-MM-DD → "DD-MM" */
export function formatDateShort(iso: string | null | undefined): string {
  if (!iso) return "—";
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!m) return iso;
  return `${m[3]}-${m[2]}`;
}

/** ISO 8601 → "DD-MM HH:mm" (UTC, no timezone math; render-engine is pure). */
export function formatTimestampShort(iso: string | null | undefined): string {
  if (!iso) return "—";
  // Accept both "YYYY-MM-DD HH:MM:SS" (SQLite default) and "YYYY-MM-DDTHH:MM:SSZ".
  const m = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2})/.exec(iso);
  if (!m) return iso;
  return `${m[3]}-${m[2]} ${m[4]}:${m[5]}`;
}

export function truncate(value: string, max: number): string {
  if (value.length <= max) return value;
  return value.slice(0, max - 1) + "…";
}

export function daysAgoLabel(days: number | null): string {
  if (days == null) return "ingen registreret";
  if (days <= 0) return "i dag";
  if (days === 1) return "1 dag siden";
  return `${days} dage siden`;
}

/** Signed day difference `toDate - fromDate` between two YYYY-MM-DD dates, UTC-based, pure. */
export function signedDaysBetween(fromDate: string, toDate: string): number {
  const pf = /^(\d{4})-(\d{2})-(\d{2})/.exec(fromDate);
  const pt = /^(\d{4})-(\d{2})-(\d{2})/.exec(toDate);
  if (!pf || !pt) return 0;
  const from = Date.UTC(parseInt(pf[1]!, 10), parseInt(pf[2]!, 10) - 1, parseInt(pf[3]!, 10));
  const to = Date.UTC(parseInt(pt[1]!, 10), parseInt(pt[2]!, 10) - 1, parseInt(pt[3]!, 10));
  return Math.round((to - from) / 86400000);
}
