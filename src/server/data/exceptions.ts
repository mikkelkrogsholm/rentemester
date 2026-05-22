// Exception grouping for the cockpit "Opgaver" card (#320).
//
// Collapses a flat list of open exceptions into one Danish, actionable summary
// line per `type` — so the cockpit renders "362 banktransaktioner mangler
// afstemning" instead of 362 individual English exception messages. Split out
// of `server/data.ts` by #320; behaviour is unchanged.

/**
 * One grouped exception line for the Overblik "Opgaver" card — every open
 * exception of one `type` collapsed into a single Danish, actionable summary
 * with a count and a deep-link target.
 */
export type ExceptionGroup = {
  /** The shared `exceptions.type`. */
  type: string;
  /** Open exceptions of this type. */
  count: number;
  /** The highest severity among the grouped exceptions. */
  severity: "low" | "medium" | "high";
  /** A Danish one-liner, e.g. "362 banktransaktioner mangler afstemning". */
  label: string;
  /** The cockpit sub-view this group links to, e.g. "bank"; null when none. */
  link: string | null;
};

const SEVERITY_RANK: Record<string, number> = { low: 0, medium: 1, high: 2 };

/**
 * Builds the Danish summary line for a group of `count` same-type exceptions.
 * Known types get a tailored, pluralised sentence and a deep-link target;
 * unknown types fall back to a generic count so nothing is ever dropped.
 */
function describeExceptionGroup(
  type: string,
  count: number,
): { label: string; link: string | null } {
  const n = count;
  switch (type) {
    case "UNMATCHED_BANK_TRANSACTION":
      return {
        label: `${n} ${
          n === 1 ? "banktransaktion mangler" : "banktransaktioner mangler"
        } afstemning`,
        link: "bank",
      };
    case "BANK_BALANCE_GAP":
      return {
        label: `${n} ${n === 1 ? "afvigelse" : "afvigelser"} mellem bogført og faktisk banksaldo`,
        link: "bank",
      };
    case "MAIL_INTAKE_NO_ATTACHMENT":
      return {
        label: `${n} ${n === 1 ? "indkommen mail" : "indkomne mails"} uden vedhæftet bilag`,
        link: null,
      };
    case "MAIL_INTAKE_AMBIGUOUS_METADATA":
      return {
        label: `${n} ${n === 1 ? "bilag" : "bilag"} med uklare oplysninger fra mail`,
        link: null,
      };
    case "MAIL_INTAKE_INGEST_BLOCKED":
      return {
        label: `${n} ${n === 1 ? "mail kunne" : "mails kunne"} ikke indlæses`,
        link: null,
      };
    case "ASSET_WRITEOFF_MISSING_DOCUMENTATION":
      return {
        label: `${n} ${n === 1 ? "aktiv-afskrivning mangler" : "aktiv-afskrivninger mangler"} dokumentation`,
        link: null,
      };
    case "ASSET_WRITEOFF_ELIGIBILITY_UNCERTAIN":
      return {
        label: `${n} ${n === 1 ? "aktiv-afskrivning" : "aktiv-afskrivninger"} med usikker fradragsret`,
        link: null,
      };
    default:
      return {
        label: `${n} ${n === 1 ? "undtagelse" : "undtagelser"} kræver gennemgang`,
        link: null,
      };
  }
}

/**
 * Collapses a list of open exceptions into one summary line per `type`. Each
 * group carries a Danish, actionable label and the highest severity seen, so
 * the cockpit renders "362 banktransaktioner mangler afstemning" instead of
 * 362 individual English lines. Deterministic: groups are ordered by severity
 * (high first), then by descending count, then by type.
 */
export function groupExceptions(
  rows: Array<{ type: string; severity: string }>,
): ExceptionGroup[] {
  const byType = new Map<string, { count: number; severity: string }>();
  for (const row of rows) {
    const existing = byType.get(row.type);
    if (existing) {
      existing.count += 1;
      if ((SEVERITY_RANK[row.severity] ?? 0) > (SEVERITY_RANK[existing.severity] ?? 0)) {
        existing.severity = row.severity;
      }
    } else {
      byType.set(row.type, { count: 1, severity: row.severity });
    }
  }
  const groups: ExceptionGroup[] = [];
  for (const [type, agg] of byType) {
    const { label, link } = describeExceptionGroup(type, agg.count);
    const severity =
      agg.severity === "high" || agg.severity === "medium" ? agg.severity : "low";
    groups.push({ type, count: agg.count, severity, label, link });
  }
  groups.sort(
    (a, b) =>
      (SEVERITY_RANK[b.severity] ?? 0) - (SEVERITY_RANK[a.severity] ?? 0) ||
      b.count - a.count ||
      a.type.localeCompare(b.type),
  );
  return groups;
}
