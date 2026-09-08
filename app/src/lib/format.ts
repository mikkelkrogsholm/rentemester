// Pure presentation helpers — no React, easy to unit-test.

import type { CompanySummary } from "./types";

/**
 * The single canonical Danish display formatter for a kroner amount — a thin
 * browser-local copy of `core/money.ts#formatKronerDa` (browser code cannot
 * import from `src/core`). For every realistic amount (any finite value JS
 * renders in fixed, non-exponential notation) it emits the byte-identical
 * string: period thousands separator, comma decimal separator, exactly two
 * decimals, a regular-space `" kr."` suffix and a minus prefix for negatives,
 * e.g. `1234.5` → `"1.234,50 kr."`. Non-finite / null / undefined / empty
 * input yields `"—"`.
 *
 * #314: this replaces the divergent `Intl.NumberFormat({style:"currency"})`
 * rendering, which used a non-breaking space before the suffix and so drifted
 * from every server-rendered surface.
 */
function formatKronerDa(value: unknown): string {
  const num = typeof value === "number" ? value : Number(value);
  if (value == null || value === "" || !Number.isFinite(num)) return "—";
  const abs = Math.abs(num);
  const s = abs.toString();
  if (s.includes("e") || s.includes("E")) {
    // Exponential notation: only astronomically large (≥ 1e21) or vanishingly
    // small (< 1e-6) magnitudes, neither of which occurs in real bookkeeping.
    // Render without a BigInt-parse crash; byte-identity with the server
    // formatter is not promised at this (impossible) scale.
    if (abs < 0.005) return "0,00 kr.";
    const whole = BigInt(Math.round(abs))
      .toString()
      .replace(/\B(?=(\d{3})+(?!\d))/g, ".");
    return `${num < 0 ? "-" : ""}${whole},00 kr.`;
  }
  // Single half-up round to øre from the FULL decimal string — mirrors core's
  // toOre/scaledInt over String(num). NOT a toFixed(3) pre-round, which would
  // double-round inputs carrying >2 decimals (1.0049 → "1,01" vs core "1,00").
  // "first dropped digit ≥ 5 ⇒ round up" is exactly round-half-up. The sign is
  // taken from the ROUNDED øre, so a sub-øre negative renders "0,00 kr.".
  const dot = s.indexOf(".");
  const whole = dot === -1 ? s : s.slice(0, dot);
  const frac = dot === -1 ? "" : s.slice(dot + 1);
  let ore = BigInt(whole) * 100n + BigInt(frac.slice(0, 2).padEnd(2, "0"));
  if ((frac[2] ?? "0") >= "5") ore += 1n;
  const negative = num < 0 && ore > 0n;
  const wholeOre = ore / 100n;
  const fraction = (ore % 100n).toString().padStart(2, "0");
  const wholeText = wholeOre.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  return `${negative ? "-" : ""}${wholeText},${fraction} kr.`;
}

/**
 * Normalizes a currency code: a browser-local copy of
 * `core/money.ts#normalizeCurrency` — trims and upper-cases, defaulting to
 * "DKK" for a null/undefined/empty value.
 */
function normalizeCurrency(value?: string | null): string {
  const trimmed = (value ?? "").trim().toUpperCase();
  return trimmed.length > 0 ? trimmed : "DKK";
}

/**
 * Danish-style amount formatting. Ledger amounts are in minor units (øre).
 *
 * #314: a DKK amount renders via the canonical `formatKronerDa` so it is
 * byte-identical to every server-rendered surface; a foreign-currency amount
 * keeps its own code ("1.234,56 EUR") rather than a misleading "kr.".
 */
export function formatCurrency(minorUnits: number, currency = "DKK"): string {
  return formatKroner(minorUnits / 100, currency);
}

/**
 * Danish-style amount formatting for figures already expressed in kroner
 * (DKK with decimals) — e.g. the `/overview` P&L, VAT and bank fields. Use
 * this, not `formatCurrency`, which divides by 100 for minor-unit ledgers.
 *
 * #314: a DKK amount delegates to the canonical `formatKronerDa` (emitting the
 * identical `"1.234,56 kr."` string as the rest of the system); a non-DKK
 * amount keeps its own currency code.
 */
export function formatKroner(kroner: number, currency = "DKK"): string {
  if (normalizeCurrency(currency) === "DKK") return formatKronerDa(kroner);
  return new Intl.NumberFormat("da-DK", {
    style: "currency",
    currency: normalizeCurrency(currency),
    maximumFractionDigits: 2,
  }).format(kroner);
}

/**
 * Danish-style percentage formatting for a ratio expressed as a fraction
 * (0–1) — e.g. the Overblik nøgletal (bruttomargin, egenkapitalandel). Returns
 * "—" when the ratio is null (an undefined figure, never a fabricated 0%).
 */
export function formatPercent(fraction: number | null): string {
  if (fraction === null || !Number.isFinite(fraction)) return "—";
  return new Intl.NumberFormat("da-DK", {
    style: "percent",
    maximumFractionDigits: 1,
  }).format(fraction);
}

/** Today as YYYY-MM-DD (local) — the default `asOf` for the cockpit. */
export function todayIso(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/**
 * Parses a Danish-entered amount string into a number, or `null` if the input
 * is ambiguous or not a number (#UI-5).
 *
 * Danish convention uses `.` as the thousands separator and `,` as the decimal
 * separator — the exact opposite of JS `Number()`. A raw `Number("1.234")`
 * silently reads `1,234`, a 1000× error in a bookkeeping field. This is the one
 * canonical parser every amount input must use.
 *
 * Accepted forms (after trimming, dropping a single leading sign and spaces):
 *   "1234"        → 1234         (plain integer)
 *   "1234,56"     → 1234.56      (comma decimal)
 *   "1.234,56"    → 1234.56      (dot thousands + comma decimal)
 *   "1.234.567"   → 1234567      (dot thousands, no decimals)
 *   "1234.56"     → 1234.56      (a lone dot with 1–2 trailing digits is read
 *                                 as a decimal point — a pragmatic concession to
 *                                 users who type the en-US way)
 *
 * Rejected (returns `null`) — anything genuinely ambiguous or malformed:
 *   ""            → null         (empty)
 *   "abc"         → null         (not a number)
 *   "1,234,56"    → null         (two commas)
 *   "1.23.456"    → null         (dot groups that aren't 3-digit thousands)
 *   "1,2345"      → null         (more than 2 decimals after the comma)
 */
export function parseDanishAmount(raw: string): number | null {
  let s = raw.trim().replace(/\s/g, "");
  if (s === "") return null;
  let sign = "";
  if (s[0] === "+" || s[0] === "-") {
    sign = s[0] === "-" ? "-" : "";
    s = s.slice(1);
  }
  if (s === "" || /[^0-9.,]/.test(s)) return null;

  const commas = (s.match(/,/g) ?? []).length;
  if (commas > 1) return null;

  let normalized: string;
  if (commas === 1) {
    // Comma is the decimal separator. Everything before it may carry `.`
    // thousands separators; the fraction must be 1–2 digits.
    const [intPart, fracPart] = s.split(",");
    if (!/^[0-9.]*$/.test(intPart) || !/^[0-9]{1,2}$/.test(fracPart)) return null;
    const intDigits = stripDanishThousands(intPart);
    if (intDigits === null) return null;
    normalized = `${intDigits === "" ? "0" : intDigits}.${fracPart}`;
  } else if (s.includes(".")) {
    // No comma. A trailing `.dd` (1–2 digits) is treated as a decimal point;
    // otherwise the dots must be valid 3-digit thousands groups.
    const lastDot = s.lastIndexOf(".");
    const tail = s.slice(lastDot + 1);
    const beforeTail = s.slice(0, lastDot);
    if (
      !beforeTail.includes(".") &&
      tail.length >= 1 &&
      tail.length <= 2 &&
      /^[0-9]+$/.test(beforeTail) &&
      /^[0-9]+$/.test(tail)
    ) {
      normalized = `${beforeTail}.${tail}`;
    } else {
      const intDigits = stripDanishThousands(s);
      if (intDigits === null) return null;
      normalized = intDigits === "" ? "0" : intDigits;
    }
  } else {
    normalized = s;
  }

  const num = Number(`${sign}${normalized}`);
  return Number.isFinite(num) ? num : null;
}

/**
 * Collapses a string of digits grouped by `.` thousands separators ("1.234.567")
 * into bare digits ("1234567"). Returns `null` if the grouping is not valid
 * Danish thousands (each group after the first must be exactly 3 digits).
 */
function stripDanishThousands(part: string): string | null {
  if (part === "") return "";
  if (!part.includes(".")) return /^[0-9]+$/.test(part) ? part : null;
  const groups = part.split(".");
  if (!/^[0-9]{1,3}$/.test(groups[0])) return null;
  for (let i = 1; i < groups.length; i += 1) {
    if (!/^[0-9]{3}$/.test(groups[i])) return null;
  }
  return groups.join("");
}

const DA_DATE_FORMAT = new Intl.DateTimeFormat("da-DK", {
  day: "numeric",
  month: "short",
  year: "numeric",
});

/**
 * Formats an ISO date (YYYY-MM-DD, optionally with a time part) as a Danish
 * running-text date — "27. feb. 2026" (#UI-8). Use this in prose, headings and
 * banners where a raw ISO string reads as machine output.
 *
 * Deliberately NOT used in table columns: there, the sortable, fixed-width ISO
 * form (YYYY-MM-DD) carries meaning a localized string would lose. Returns "—"
 * for null/empty/unparseable input rather than an "Invalid Date".
 */
export function formatDateDa(iso: string | null | undefined): string {
  if (iso == null || iso === "") return "—";
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!m) return "—";
  const [, y, mo, d] = m;
  // Build from UTC parts to keep the day stable regardless of the runner's
  // timezone — a local `new Date("2026-02-27")` can roll back a day west of UTC.
  const date = new Date(Date.UTC(Number(y), Number(mo) - 1, Number(d)));
  if (Number.isNaN(date.getTime())) return "—";
  return DA_DATE_FORMAT.format(date);
}

/**
 * Formats a kroner amount as a raw SKAT TastSelv-compatible number string: NO
 * thousand separator, NO currency suffix. The standard VAT form uses whole
 * kroner, so this deliberately emits a bare signed integer.
 *
 * Shared because the display formatter's `.`-grouped output ("52.317,00 kr.")
 * must NEVER reach the clipboard — the thousand separator corrupts the field.
 */
export function tastSelvNumber(kroner: number): string {
  return String(Number.isFinite(kroner) ? Math.trunc(kroner) : 0);
}

export type AttentionLevel = "critical" | "warning" | "ok";

export type AttentionFlag = {
  level: Exclude<AttentionLevel, "ok">;
  label: string;
  /**
   * Optional cockpit-route the owner can click through to (#420). When set,
   * the CompanyCard renders the flag as a link so a critical warning is
   * never a dead-end — the owner has a concrete next step.
   */
  to?: string;
};

/** A VAT deadline within this many days counts as "soon" — a warning. */
const VAT_DEADLINE_SOON_DAYS = 30;

/**
 * Derives the "needs attention" flags for a company. An owner judges a company
 * on its headline health — these flags surface what needs a hand: a broken
 * audit chain, a negative result, an upcoming/overdue VAT deadline, an
 * unreconciled bank statement, and open tasks. Keeping the rules here means
 * they are tested once and reused by the sort and the card.
 */
export function attentionFlags(c: CompanySummary): AttentionFlag[] {
  const flags: AttentionFlag[] = [];
  if (c.ledgerMissing) {
    flags.push({ level: "critical", label: "Mangler regnskab" });
    return flags;
  }
  if (!c.auditChainOk) {
    // #420 — flaget skal ikke være en blind alarm. Klikket fører til
    // Integritet-viewet (#333) hvor brudet er forklaret med entry-nr og
    // ejeren får et konkret næste skridt (kontakt revisor / genskab backup).
    flags.push({
      level: "critical",
      label: "Revisionskæde brudt",
      to: `/companies/${c.slug}/integritet`,
    });
  }
  if (c.resultat < 0) {
    flags.push({ level: "critical", label: "Negativt resultat" });
  }
  if (c.vat && c.vat.payable > 0) {
    // The countdown targets the SKAT filing/payment deadline — NOT the end of
    // the VAT period, which is an earlier date. The flag says "Momsfrist" so
    // an owner does not read it as the current period ending.
    if (c.vat.daysRemaining < 0) {
      flags.push({ level: "critical", label: "Momsfrist overskredet" });
    } else if (c.vat.daysRemaining <= VAT_DEADLINE_SOON_DAYS) {
      // Reuse `formatDeadline` for correct Danish inflection — a bare
      // "om N dage" reads "om 1 dage" / "om 0 dage", which is wrong grammar.
      flags.push({
        level: "warning",
        label: `Momsfrist ${formatDeadline(c.vat.daysRemaining)}`,
      });
    }
  }
  if (c.attentionStatus === "requires-attention") {
    flags.push({
      level: "warning",
      label: `${c.openTaskCount} åbne opgaver`,
    });
  }
  return flags;
}

/** The overall level for a company — the worst of its flags. */
export function attentionLevel(c: CompanySummary): AttentionLevel {
  const flags = attentionFlags(c);
  if (flags.some((f) => f.level === "critical")) return "critical";
  if (flags.length > 0) return "warning";
  return "ok";
}

const RANK: Record<AttentionLevel, number> = { critical: 0, warning: 1, ok: 2 };

/**
 * Sorts a portfolio "needs attention" first: critical, then warning, then ok;
 * within a level, archived companies sink and ties break by display name.
 */
export function sortByAttention(companies: CompanySummary[]): CompanySummary[] {
  return [...companies].sort((a, b) => {
    const byLevel = RANK[attentionLevel(a)] - RANK[attentionLevel(b)];
    if (byLevel !== 0) return byLevel;
    if (a.archived !== b.archived) return a.archived ? 1 : -1;
    return a.name.localeCompare(b.name, "da");
  });
}

/** A short Danish day-relative phrase for a deadline, e.g. "om 12 dage". */
export function formatDeadline(daysRemaining: number): string {
  if (daysRemaining < 0) {
    const n = Math.abs(daysRemaining);
    return `overskredet ${n} ${n === 1 ? "dag" : "dage"}`;
  }
  if (daysRemaining === 0) return "i dag";
  if (daysRemaining === 1) return "i morgen";
  return `om ${daysRemaining} dage`;
}
