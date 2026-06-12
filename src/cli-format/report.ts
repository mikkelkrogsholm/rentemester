import { type OutputFormat, buildHumanError, printStructuredResult } from "./common";
import { renderVatReport, renderVatFiling, renderAnnualReport, renderTaxReturn } from "./vat";
import {
  renderInvoiceStatus,
  renderInvoiceInterest,
  renderInvoiceCompensation,
  renderInvoiceValidate,
  renderInvoiceCreate,
} from "./invoice";
import {
  renderBackupStatus,
  renderExceptionsList,
  renderTrialBalance,
  renderProfitAndLoss,
  renderBalanceSheet,
  renderRetentionStatus,
} from "./dashboard";
import { renderBankReconciliation } from "./bank";

// ===========================================================================
// HUMAN-READABLE REPORT RENDERING — DISPATCH (#211)
// ---------------------------------------------------------------------------
// Read/report commands emit deterministic JSON with English field names for
// agents. When a human runs them (`--format human` / a TTY) that JSON is
// unreadable. `renderHumanReport` renders the same payloads in clear Danish
// with money in kroner-og-øre. The `--format json` path is untouched: handlers
// keep calling `ctx.emitResult` for JSON and only branch into this renderer
// for the human format.
// ===========================================================================

/** Report kinds with a dedicated Danish human renderer. */
export type HumanReportKind =
  | "vat-report"
  | "vat-filing"
  | "report-annual"
  | "report-tax"
  | "invoice-status"
  | "invoice-interest"
  | "invoice-compensation"
  | "invoice-validate"
  | "exceptions-list"
  | "report-trial-balance"
  | "report-profit-loss"
  | "report-balance"
  | "retention-status"
  | "reconcile-bank"
  | "backup-status"
  // #177: `report annual --ixbrl-taxonomy` prints the bounded iXBRL subset.
  // No dedicated renderer — it falls back to the generic structured output.
  | "report-annual-ixbrl-taxonomy";

/**
 * Render a read/report `result` payload as Danish human-readable text.
 *
 * Returns `null` when there is no dedicated renderer for `kind` or the payload
 * does not look renderable, so callers can fall back to the generic
 * `printStructuredResult` rendering.
 */
export function renderHumanReport(
  kind: HumanReportKind,
  result: Record<string, unknown>,
): string | null {
  // `backup-status` reports `ok: false` when the weekly backup duty is NOT met
  // — that is a valid compliance status to render, not a command failure. Its
  // renderer handles both states, so it must run before the generic
  // ok-false-as-error path. (#240)
  if (kind === "backup-status") {
    return renderBackupStatus(result);
  }
  // `invoice validate` reports `ok: false` when the payload is INVALID — that
  // is a valid validation verdict to render in full (the figures and every
  // rejection reason), not a command crash. Its renderer covers both the
  // valid and invalid verdict, so it must run before the generic
  // ok-false-as-error path. (#250)
  if (kind === "invoice-validate") {
    return renderInvoiceValidate(result);
  }
  if (result.ok === false) {
    return buildHumanError(humanReportTitle(kind), result).join("\n");
  }
  switch (kind) {
    case "vat-report":
      return renderVatReport(result);
    case "vat-filing":
      return renderVatFiling(result);
    case "report-annual":
      return renderAnnualReport(result);
    case "report-tax":
      return renderTaxReturn(result);
    case "invoice-status":
      return renderInvoiceStatus(result);
    case "invoice-interest":
      return renderInvoiceInterest(result);
    case "invoice-compensation":
      return renderInvoiceCompensation(result);
    case "exceptions-list":
      return renderExceptionsList(result);
    case "report-trial-balance":
      return renderTrialBalance(result);
    case "report-profit-loss":
      return renderProfitAndLoss(result);
    case "report-balance":
      return renderBalanceSheet(result);
    case "retention-status":
      return renderRetentionStatus(result);
    case "reconcile-bank":
      return renderBankReconciliation(result);
    default:
      // `backup-status` and `invoice-validate` are handled before this switch
      // (their renderers cover the ok:false state too), so they never reach
      // here.
      return null;
  }
}

/**
 * Emit a read/report `result` either as the byte-stable agent JSON or as the
 * Danish human rendering, depending on `format`. The `--format json` path is
 * exactly `JSON.stringify(result, null, 2)` — unchanged from `emitResult` —
 * so agents see no difference. Only the human rendering is new.
 *
 * Returns the process exit code to apply (1 when `result.ok === false`).
 */
export function emitHumanReport(
  kind: HumanReportKind,
  result: Record<string, unknown>,
  format: OutputFormat,
): void {
  if (format === "json") {
    console.log(JSON.stringify(result, null, 2));
  } else {
    const human = renderHumanReport(kind, result);
    if (human !== null) {
      if (result.ok === false) console.error(human);
      else console.log(human);
    } else {
      printStructuredResult(humanReportTitle(kind), result, format);
    }
  }
  if (result.ok === false) process.exitCode = 1;
}

function humanReportTitle(kind: HumanReportKind): string {
  switch (kind) {
    case "vat-report":
      return "Momsrapport";
    case "vat-filing":
      return "Momsangivelse";
    case "report-annual":
      return "Årsrapport";
    case "report-tax":
      return "Skattemæssig indkomst";
    case "invoice-status":
      return "Fakturastatus";
    case "invoice-interest":
      return "Morarente";
    case "invoice-compensation":
      return "Kompensation for sen betaling";
    case "invoice-validate":
      return "Fakturavalidering";
    case "exceptions-list":
      return "Exceptions-kø";
    case "report-trial-balance":
      return "Saldobalance";
    case "report-profit-loss":
      return "Resultatopgørelse";
    case "report-balance":
      return "Balance";
    case "retention-status":
      return "Opbevaringsstatus";
    case "reconcile-bank":
      return "Bankafstemning";
    case "backup-status":
      return "Backup-status";
    case "report-annual-ixbrl-taxonomy":
      return "iXBRL-taksonomi (afgrænset udsnit)";
  }
}

/** Write-command kinds with a dedicated Danish human renderer. (#266, #268) */
export type HumanWriteKind = "invoice-create";

/**
 * Emit a write-command `result` either as the byte-stable agent JSON or as a
 * Danish human rendering, depending on `format`. The `--format json` path is
 * exactly `JSON.stringify(result, null, 2)` — unchanged from `emitResult` — so
 * agents see no difference. Sets `process.exitCode = 1` on `ok: false`. (#268)
 */
export function emitHumanWrite(
  kind: HumanWriteKind,
  result: Record<string, unknown>,
  format: OutputFormat,
): void {
  if (format === "json") {
    console.log(JSON.stringify(result, null, 2));
  } else {
    const human = kind === "invoice-create" ? renderInvoiceCreate(result) : null;
    if (human !== null) {
      if (result.ok === false) console.error(human);
      else console.log(human);
    } else {
      printStructuredResult(kind, result, format);
    }
  }
  if (result.ok === false) process.exitCode = 1;
}
