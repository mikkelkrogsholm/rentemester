import { formatKronerDa } from "../core/money";

export type OutputFormat = "json" | "human";

export function resolveOutputFormat(flags: Map<string, string | true>): OutputFormat | null {
  if (flags.has("--json")) return "json";
  const format = flags.get("--format");
  if (format === undefined) return process.stdout.isTTY ? "human" : "json";
  if (format === "json" || format === "human") return format;
  return null;
}

export function printStructuredResult(commandLabel: string, result: Record<string, unknown>, format: OutputFormat) {
  const ok = result.ok !== false;
  if (format === "json") {
    console.log(JSON.stringify(result, null, 2));
    return;
  }

  const lines = ok ? buildHumanSuccess(commandLabel, result) : buildHumanError(commandLabel, result);
  const text = lines.join("\n");
  if (ok) console.log(text);
  else console.error(text);
}

/**
 * A command's `description` in `cli-meta.ts` is a full sentence (often two)
 * meant for help text — never a heading. #268: a write command that falls
 * through to the generic human renderer used to print that whole description
 * as the `✔` heading. `headingFromLabel` collapses such a label to a short
 * heading: the first clause before an em dash / period, capped in length.
 */
function headingFromLabel(commandLabel: string): string {
  const firstClause = commandLabel.split(/\s+[—–-]\s+|\.\s/)[0]?.trim() ?? commandLabel;
  const short = firstClause.length > 0 ? firstClause : commandLabel;
  return short.length > 60 ? `${short.slice(0, 57).trimEnd()}…` : short;
}

function buildHumanSuccess(commandLabel: string, result: Record<string, unknown>) {
  const lines = [`✔ ${headingFromLabel(commandLabel)}`];
  for (const line of collectSummaryLines(result)) lines.push(`  ${line}`);
  const warnings = asStringArray(result.warnings);
  if (warnings.length > 0) {
    lines.push("  Advarsler:");
    for (const warning of warnings) lines.push(`    - ${warning}`);
  }
  return lines;
}

export function buildHumanError(commandLabel: string, result: Record<string, unknown>) {
  const lines = [`✘ ${commandLabel} failed`];
  const errors = asStringArray(result.errors);
  if (errors.length > 0) {
    for (const error of errors) lines.push(`  → ${error}`);
  } else {
    lines.push("  → Kommandoen fejlede uden en specifik fejlbesked.");
  }
  const warnings = asStringArray(result.warnings);
  if (warnings.length > 0) {
    lines.push("  Advarsler:");
    for (const warning of warnings) lines.push(`    - ${warning}`);
  }
  appendPeriodCloseGuidance(lines, errors);
  return lines;
}

/**
 * `vat momsangivelse` and `report annual` reject an unfinalised period with a
 * terse "... is not closed ... run 'period close' first". For an owner near a
 * deadline that is cryptic, so when we recognise that failure we append a
 * plain-Danish explanation of what closing a period means and the exact
 * command to run. (#227)
 */
function appendPeriodCloseGuidance(lines: string[], errors: string[]): void {
  const closeError = errors.find(
    (e) => /is not closed/i.test(e) && /period close/i.test(e),
  );
  if (!closeError) return;
  const range = closeError.match(/(\d{4}-\d{2}-\d{2})\.\.(\d{4}-\d{2}-\d{2})/);
  const isFiscalYear = /fiscal|regnskabsår|annual/i.test(closeError);
  const kind = isFiscalYear ? "fiscal_year" : "vat_quarter";
  const fromTo = range ? `--from ${range[1]} --to ${range[2]}` : "--from <YYYY-MM-DD> --to <YYYY-MM-DD>";
  lines.push("");
  lines.push("  Sådan kommer du videre:");
  lines.push(
    "    En momsangivelse/årsrapport kan kun bygges på en LUKKET periode — det sikrer,",
  );
  lines.push(
    "    at tallene ikke ændrer sig efter indberetning. Du lukker perioden med:",
  );
  lines.push(`      rentemester period close --company <path> ${fromTo} --kind ${kind}`);
  lines.push(
    "    Luk først perioden når alle bilag for perioden er bogført — en lukket periode",
  );
  lines.push(
    "    blokerer ny bogføring med dato i perioden. Brug --status reported i stedet for",
  );
  lines.push(
    "    standard 'closed', hvis du allerede har indberettet til SKAT/Erhvervsstyrelsen.",
  );
}

/**
 * Danish labels for the result keys a write command commonly returns. The
 * generic human fallback (`collectSummaryLines`) used to English-ify keys via
 * `humanizeKey` ("Invoice Number", "Document Id"); for a Danish-facing tool
 * that is wrong. Any key without an entry here keeps the humanized form, but
 * the common write-result fields are now translated. (#268)
 */
const WRITE_RESULT_LABEL_DA: Record<string, string> = {
  message: "Besked",
  invoiceNumber: "Fakturanummer",
  invoiceNo: "Fakturanummer",
  entryNo: "Posteringsnummer",
  entryNumber: "Posteringsnummer",
  documentId: "Bilags-id",
  journalEntryId: "Finanspostering-id",
  importBatchId: "Import-batch-id",
  imported: "Importeret",
  skippedDuplicates: "Sprunget over (dubletter)",
  backupId: "Backup-id",
  exportDir: "Eksportmappe",
  latestBackupId: "Seneste backup-id",
  expiredCount: "Udløbne",
  totalCount: "I alt",
  storedPath: "Gemt fil",
  pdfStoredPath: "PDF-fil",
};

function collectSummaryLines(result: Record<string, unknown>) {
  const preferredKeys = [
    "message",
    "invoiceNumber",
    "invoiceNo",
    "entryNo",
    "entryNumber",
    "documentId",
    "journalEntryId",
    "importBatchId",
    "imported",
    "skippedDuplicates",
    "backupId",
    "exportDir",
    "latestBackupId",
    "expiredCount",
    "totalCount",
  ];
  const lines: string[] = [];
  for (const key of preferredKeys) {
    const value = result[key];
    const rendered = renderScalar(value);
    if (rendered === null) continue;
    // #268: never emit an English key on a Danish-facing tool — use the Danish
    // label when one exists, falling back to the humanized form otherwise.
    lines.push(`${WRITE_RESULT_LABEL_DA[key] ?? humanizeKey(key)}: ${rendered}`);
  }
  return lines;
}

function renderScalar(value: unknown): string | null {
  if (typeof value === "string" && value.length > 0) return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return null;
}

function humanizeKey(key: string) {
  return key
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .replace(/_/g, " ")
    .replace(/^./, (c) => c.toUpperCase());
}

export function asStringArray(value: unknown) {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

/**
 * Format a DKK amount as Danish kroner-og-øre, e.g. 38.3 -> "38,30 kr.",
 * -1234.5 -> "-1.234,50 kr.". Non-finite/missing input yields "—".
 *
 * Thin re-export of the single canonical formatter `formatKronerDa` in
 * `core/money.ts` — kept under the historical name so the many call sites in
 * this file stay unchanged. (#314)
 */
export function formatKroner(value: unknown): string {
  return formatKronerDa(value);
}

/** Render an integer count, falling back to 0 for missing values. */
export function asCount(value: unknown): number {
  const num = typeof value === "number" ? value : Number(value);
  return Number.isFinite(num) ? num : 0;
}

export function asText(value: unknown, fallback = "—"): string {
  if (typeof value === "string" && value.length > 0) return value;
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  return fallback;
}

export function asArray(value: unknown): Array<Record<string, unknown>> {
  return Array.isArray(value)
    ? value.filter((item): item is Record<string, unknown> => typeof item === "object" && item !== null)
    : [];
}

/**
 * Format a percentage with Danish decimal comma, e.g. 8.05 -> "8,05 %".
 * Non-finite/missing input yields "—".
 */
export function formatPercent(value: unknown): string {
  const num = typeof value === "number" ? value : Number(value);
  if (value == null || value === "" || !Number.isFinite(num)) return "—";
  const text = (Math.round(num * 100) / 100)
    .toString()
    .replace(".", ",");
  return `${text} %`;
}

export function appendWarnings(lines: string[], result: Record<string, unknown>): void {
  const warnings = asStringArray(result.warnings);
  if (warnings.length === 0) return;
  lines.push("");
  lines.push("Advarsler:");
  for (const warning of warnings) lines.push(`  - ${warning}`);
}
