export function isValidIsoDate(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const trimmed = value.trim();
  if (!/^\d{4}-\d{2}-\d{2}$/.test(trimmed)) return false;
  const date = new Date(`${trimmed}T00:00:00Z`);
  return !Number.isNaN(date.getTime()) && date.toISOString().slice(0, 10) === trimmed;
}

/**
 * Today's date as YYYY-MM-DD. Honours the RENTEMESTER_TODAY override for
 * deterministic tests; otherwise reads the wall clock in UTC.
 */
export function todayIsoDate(): string {
  const override = process.env.RENTEMESTER_TODAY;
  if (isValidIsoDate(override)) return override.trim();
  return new Date().toISOString().slice(0, 10);
}

/**
 * Add `days` (may be negative) to a YYYY-MM-DD date and return the resulting
 * YYYY-MM-DD date. UTC-based, so leap years and year boundaries are handled
 * by the calendar without timezone drift.
 */
export function addDays(isoDate: string, days: number): string {
  const date = new Date(`${isoDate}T00:00:00.000Z`);
  date.setUTCDate(date.getUTCDate() + days);
  return date.toISOString().slice(0, 10);
}

/**
 * Signed day difference `toDate - fromDate` between two YYYY-MM-DD dates.
 * Positive when toDate is later, negative when earlier, zero when equal.
 * UTC-based.
 */
export function diffDays(fromDate: string, toDate: string): number {
  const from = new Date(`${fromDate}T00:00:00.000Z`).getTime();
  const to = new Date(`${toDate}T00:00:00.000Z`).getTime();
  return Math.round((to - from) / 86400000);
}

/**
 * Absolute day distance between two YYYY-MM-DD dates, order-independent.
 * UTC-based.
 */
export function daysBetween(a: string, b: string): number {
  return Math.abs(diffDays(a, b));
}

/**
 * Null-safe signed day difference `to - from` for two strings that *start*
 * with a `YYYY-MM-DD` date (a bare date or an ISO timestamp). Returns 0 when
 * either input does not match that prefix, so callers shaping read-side JSON
 * never propagate a NaN. The day math is identical to {@link diffDays}.
 */
export function diffDaysSafe(from: string, to: string): number {
  const pf = /^(\d{4})-(\d{2})-(\d{2})/.exec(from);
  const pt = /^(\d{4})-(\d{2})-(\d{2})/.exec(to);
  if (!pf || !pt) return 0;
  const f = Date.UTC(parseInt(pf[1]!, 10), parseInt(pf[2]!, 10) - 1, parseInt(pf[3]!, 10));
  const t = Date.UTC(parseInt(pt[1]!, 10), parseInt(pt[2]!, 10) - 1, parseInt(pt[3]!, 10));
  return Math.round((t - f) / 86400000);
}

/**
 * Add `days` (may be negative) to a full ISO-8601 *timestamp* and return the
 * resulting timestamp as a full ISO string. Unlike {@link addDays}, which is
 * date-only in and date-only out, this preserves the time-of-day component —
 * used for cache TTLs (`expiresAt`) computed off a `new Date().toISOString()`
 * instant. UTC-based.
 */
export function addDaysToTimestamp(isoTimestamp: string, days: number): string {
  const date = new Date(isoTimestamp);
  date.setUTCDate(date.getUTCDate() + days);
  return date.toISOString();
}

/**
 * Danish month names in calendar order, capitalised (index 0 = "Januar").
 * The single source of truth for month labels — consumers that need lowercase
 * apply {@link decapitalize}, so casing differences are a display choice and
 * never a second copy of the list.
 */
export const MONTH_NAMES_DA = [
  "Januar", "Februar", "Marts", "April", "Maj", "Juni",
  "Juli", "August", "September", "Oktober", "November", "December",
] as const;

/** Lowercases the first character of `value` (e.g. "Januar" -> "januar"). */
export function decapitalize(value: string): string {
  return value.length === 0 ? value : value[0]!.toLowerCase() + value.slice(1);
}
