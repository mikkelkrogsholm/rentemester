/**
 * Runtime bookkeeper agent — the periodic loop (#183).
 *
 * `runAgentLoop` is the deterministic, replayable entrypoint that drives one
 * bookkeeping run for one company. It is the packaged form of the demo loop
 * in `examples/agent-demo` — same fixture + same `--as-of` date produce an
 * identical, asserted run result.
 *
 * It calls EXISTING exported core functions to do the actual bookkeeping; it
 * never reimplements a feature and never writes to the ledger or rules
 * directly. The hard boundary (see `docs/runtime-agent-contract.md`):
 *
 *   - the agent acts only within the guardrails;
 *   - anything it cannot decide deterministically becomes an exception,
 *     never a guess at a posting.
 *
 * Determinism: there is no wall-clock dependence. The only "now" the agent
 * knows is the explicit `asOf` date. Inbox files are processed in a stable
 * sorted order.
 */

import { readFileSync, readdirSync, existsSync } from "node:fs";
import { basename, extname, join } from "node:path";
import type { Database } from "bun:sqlite";
import { openDb, migrate } from "../core/db";
import { companyPaths } from "../core/paths";
import { getCompanySettings } from "../core/company";
import { ingestDocument, type DocumentMetadata } from "../core/documents";
import { importBankCsv } from "../core/bank";
import { suggestBankMatches } from "../core/bank-suggest-matches";
import { bookExpenseFromBank } from "../core/expense-booking";
import {
  recordException,
  listExceptions,
  syncUnmatchedBankTransactionExceptions,
} from "../core/exceptions";
import { buildVatReport, vatFilingDeadline } from "../core/vat";
import { buildVatFiling } from "../core/vat-filing";
import { fiscalYearForDate } from "../core/fiscal-year";
import { vatPeriodWindowFor, type VatPeriodWindow } from "../core/periods";
import { isValidIsoDate, diffDays, addDays } from "../core/dates";
import { formatKroner } from "../cli-format";
import {
  AGENT_ACTOR_ID,
  AGENT_PROGRAM,
  AGENT_RULE_ID,
  AUTO_BOOK_CONFIDENCE_THRESHOLD,
  DEADLINE_HORIZON_DAYS,
  FIXED_ASSET_REVIEW_THRESHOLD_DKK,
  looksLikeFixedAsset,
  resolveSupplierRule,
  type SupplierRule,
} from "./contract";
import { STRAKSAFSKRIVNING_THRESHOLD_DKK } from "../core/assets";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export type AgentRunInput = {
  /** Absolute path to an initialised company directory. */
  companyRoot: string;
  /** The explicit "now" for the run. YYYY-MM-DD. The only clock the agent has. */
  asOf: string;
  /**
   * Directory holding bilag (one document file per bilag). Each file must
   * have a sibling `<stem>.json` in `metadataDir`. This is the maildrop the
   * agent ingests from. Optional — omit it for a deadline-only run.
   */
  inboxDir?: string;
  /** Directory holding the parallel metadata JSON files. Defaults to inboxDir. */
  metadataDir?: string;
  /** Absolute path to a bank-statement CSV to import. Optional. */
  bankCsvPath?: string;
};

export type BookedExpense = {
  documentNo: string;
  documentId: number;
  bankTransactionId: number;
  supplier: string;
  amount: number;
  currency: string;
  expenseAccount: string;
  vatTreatment: SupplierRule["vatTreatment"];
  label: string;
  journalEntryNo: string | null;
};

export type RoutedException = {
  exceptionId: number;
  type: string;
  severity: string;
  message: string;
  requiredAction: string | null;
};

export type DeadlineNotice = {
  kind: "vat_quarter" | "fiscal_year";
  periodStart: string;
  periodEnd: string;
  /** Statutory filing/finalisation deadline. */
  dueDate: string;
  daysRemaining: number;
  /** Whether the period is already closed/reported and ready to file. */
  ready: boolean;
  note: string;
};

export type AgentRunReport = {
  ok: boolean;
  /** Canonical actor the agent booked under. */
  actor: string;
  asOf: string;
  company: string;
  /** Phases the loop executed, in order. */
  phases: string[];
  documentsIngested: number;
  documentsRejected: number;
  bankTransactionsImported: number;
  expensesBooked: BookedExpense[];
  /** Exceptions left open at end of run — the human's work list. */
  openExceptions: RoutedException[];
  upcomingDeadlines: DeadlineNotice[];
  /** Plain-language lines describing what was done and what needs the human. */
  summary: string[];
  errors: string[];
};

// ---------------------------------------------------------------------------
// Intake helpers
// ---------------------------------------------------------------------------

type InboxBilag = {
  stem: string;
  filePath: string;
  metadata: DocumentMetadata;
};

/**
 * Reads the maildrop in a stable sorted order so the run is replayable.
 * Throws on a missing metadata sibling — a bilag with no metadata is an
 * operator error, not something the agent silently guesses around.
 */
function loadInbox(inboxDir: string, metadataDir: string): InboxBilag[] {
  if (!existsSync(inboxDir)) throw new Error(`inbox directory does not exist: ${inboxDir}`);
  const items: InboxBilag[] = [];
  for (const file of readdirSync(inboxDir).sort()) {
    if (file.startsWith(".")) continue;
    const ext = extname(file).toLowerCase();
    if (ext === ".json") continue; // metadata lives alongside; skip it as a bilag
    if (![".txt", ".pdf", ".png", ".jpg", ".jpeg"].includes(ext)) continue;
    const stem = basename(file, extname(file));
    const metadataPath = join(metadataDir, `${stem}.json`);
    if (!existsSync(metadataPath)) {
      throw new Error(`bilag ${file} has no metadata sibling at ${metadataPath}`);
    }
    items.push({
      stem,
      filePath: join(inboxDir, file),
      metadata: JSON.parse(readFileSync(metadataPath, "utf8")) as DocumentMetadata,
    });
  }
  return items;
}

// ---------------------------------------------------------------------------
// Deadline arithmetic (pure — no Date.now)
// ---------------------------------------------------------------------------

function pad2(n: number) {
  return String(n).padStart(2, "0");
}

// ---------------------------------------------------------------------------
// The loop
// ---------------------------------------------------------------------------

/**
 * Runs one deterministic bookkeeping loop for one company.
 *
 * The caller owns the company directory lifecycle (init/backup). This function
 * opens the ledger, runs the ordered phases, and returns a fully-populated,
 * asserted report. It does not print — formatting belongs to the entrypoint.
 */
export function runAgentLoop(input: AgentRunInput): AgentRunReport {
  const report: AgentRunReport = {
    ok: true,
    actor: AGENT_ACTOR_ID,
    asOf: input.asOf,
    company: input.companyRoot,
    phases: [],
    documentsIngested: 0,
    documentsRejected: 0,
    bankTransactionsImported: 0,
    expensesBooked: [],
    openExceptions: [],
    upcomingDeadlines: [],
    summary: [],
    errors: [],
  };

  if (!isValidIsoDate(input.asOf)) {
    report.ok = false;
    report.errors.push(`--as-of must be a valid YYYY-MM-DD date (got: ${input.asOf})`);
    return report;
  }
  const paths = companyPaths(input.companyRoot);
  if (!existsSync(paths.db)) {
    report.ok = false;
    report.errors.push(`no initialised company at ${input.companyRoot} (run 'rentemester init' first)`);
    return report;
  }

  // The agent attributes every mutation to its canonical actor id by passing
  // `createdBy`/`createdByProgram` explicitly into each booking call (see the
  // `book` phase). It deliberately does NOT mutate process.env: a per-run
  // entrypoint must not leave global actor state behind for the next caller.
  const db = openDb(paths.db);
  try {
    migrate(db);
    runPhases(db, input, report);
  } finally {
    db.close();
  }
  return report;
}

function runPhases(db: Database, input: AgentRunInput, report: AgentRunReport): void {
  // ---- Phase 1: ingest ---------------------------------------------------
  report.phases.push("ingest");
  let inbox: InboxBilag[] = [];
  if (input.inboxDir) {
    try {
      inbox = loadInbox(input.inboxDir, input.metadataDir ?? input.inboxDir);
    } catch (error) {
      report.ok = false;
      report.errors.push(error instanceof Error ? error.message : String(error));
      return;
    }
    for (const bilag of inbox) {
      const res = ingestDocument(db, input.companyRoot, bilag.filePath, bilag.metadata);
      if (res.ok) {
        report.documentsIngested += 1;
      } else {
        report.documentsRejected += 1;
        // A rejected bilag is the rules refusing the agent — record it for the
        // human rather than dropping it silently. Duplicates are common on
        // re-runs and benign, so they are not escalated.
        const isDuplicate = (res.errors ?? []).some((e) => e.includes("duplicate"));
        if (!isDuplicate) {
          recordException(db, {
            type: "AGENT_DOCUMENT_REJECTED",
            severity: "medium",
            message: `Bilag ${bilag.stem} blev afvist af bogføringen: ${(res.errors ?? []).join("; ")}`,
            requiredAction: "Gennemgå bilagets metadata, ret fejlen, og indlæs bilaget igen manuelt.",
            sourceEvidence: { rule: AGENT_RULE_ID, bilag: bilag.stem, errors: res.errors ?? [] },
          });
        }
      }
    }
  }

  // ---- Bank import (feeds the book + reconcile phases) -------------------
  if (input.bankCsvPath) {
    if (!existsSync(input.bankCsvPath)) {
      report.ok = false;
      report.errors.push(`bank CSV does not exist: ${input.bankCsvPath}`);
      return;
    }
    const bank = importBankCsv(db, input.companyRoot, input.bankCsvPath);
    if (!bank.ok) {
      report.ok = false;
      report.errors.push(`bank import failed: ${(bank.errors ?? []).join("; ")}`);
      return;
    }
    report.bankTransactionsImported = bank.imported ?? 0;
  }

  // ---- Phase 2 + 3: book the unambiguous, route the uncertain -----------
  report.phases.push("book", "route");
  const suggestions = suggestBankMatches(db, { max: 5 });
  if (!suggestions.ok) {
    report.ok = false;
    report.errors.push(`bank suggest-matches failed: ${suggestions.errors.join("; ")}`);
    return;
  }
  // Stable order: ascending bank-transaction id, so the run is replayable.
  const rows = [...suggestions.rows].sort((a, b) => a.bankTransactionId - b.bankTransactionId);
  for (const row of rows) {
    // Only outgoing supplier payments are bookable as expenses here. Customer
    // payments and refunds are settled by their own deterministic features —
    // the agent never improvises a posting for them.
    const best = row.suggestions
      .filter((s) => s.kind === "purchase_sale")
      .sort((a, b) => b.confidence - a.confidence)[0];

    if (!best) {
      // No expense candidate at all — leave it for the reconcile phase to
      // surface as an unmatched-bank-transaction exception.
      continue;
    }
    if (best.confidence < AUTO_BOOK_CONFIDENCE_THRESHOLD) {
      routeException(db, report, {
        type: "AGENT_LOW_CONFIDENCE_MATCH",
        severity: "medium",
        message:
          `Banktransaktion ${row.bankTransactionId} "${row.text}" matcher bilag ${best.documentId} ` +
          `med en sikkerhed på ${best.confidence.toFixed(2)} — under grænsen på ${AUTO_BOOK_CONFIDENCE_THRESHOLD} ` +
          `for automatisk bogføring, så agenten bogfører den ikke selv.`,
        requiredAction:
          "Gennemgå det foreslåede match, og bogfør det derefter manuelt med 'expense book' — eller udlign det.",
        relatedBankTransactionId: row.bankTransactionId,
        relatedDocumentId: best.documentId,
        sourceEvidence: {
          rule: AGENT_RULE_ID,
          confidence: best.confidence,
          reasons: best.reasons,
        },
      });
      continue;
    }
    const rule = resolveSupplierRule(best.supplierName);
    if (!rule) {
      // Confident match, but no deterministic account rule — the agent will
      // NOT guess an expense account. Straight to the exception queue.
      routeException(db, report, {
        type: "AGENT_NO_ACCOUNT_RULE",
        severity: "medium",
        message:
          `Banktransaktion ${row.bankTransactionId} matcher sikkert bilag ${best.documentId} ` +
          `(leverandør '${best.supplierName ?? "ukendt"}'), men der findes ingen kontoregel for leverandøren — ` +
          `agenten gætter ikke på en konto.`,
        requiredAction:
          "Opret en kontoregel for leverandøren, eller bogfør udgiften manuelt med 'expense book'.",
        relatedBankTransactionId: row.bankTransactionId,
        relatedDocumentId: best.documentId,
        sourceEvidence: { rule: AGENT_RULE_ID, supplierName: best.supplierName ?? null },
      });
      continue;
    }
    // Fixed-asset judgement (#223): a confident match with a single account
    // rule is normally booked straight away — but a materially sized purchase
    // in an asset-like category (hardware/equipment) may be a capitalisable
    // fixed asset that belongs in the asset register, not a P&L operating
    // expense. The loop cannot deterministically decide capitalise-and-
    // depreciate vs straksafskrivning vs ordinary expense, so it does NOT
    // guess: it routes the purchase to the exception queue for a human/asset
    // decision and never books it silently to an expense account.
    const grossAmount = Math.abs(row.amount);
    if (looksLikeFixedAsset(rule, grossAmount)) {
      const aboveSmallAsset = grossAmount >= STRAKSAFSKRIVNING_THRESHOLD_DKK;
      routeException(db, report, {
        type: "AGENT_POSSIBLE_FIXED_ASSET",
        severity: "high",
        message:
          `Banktransaktion ${row.bankTransactionId} "${row.text}" på ${grossAmount.toLocaleString("da-DK")} kr. ` +
          `matcher bilag ${best.documentId} (leverandør '${best.supplierName ?? "ukendt"}') i kategorien ` +
          `'${rule.label}'. Købet ligner et anlægsaktiv (driftsmiddel) og er ikke automatisk bogført ` +
          `som en driftsudgift — om det skal aktiveres og afskrives eller straksafskrives er en ` +
          `aktiv-/skattevurdering, som agenten ikke gætter på.`,
        requiredAction:
          aboveSmallAsset
            ? `Beløbet er over småaktiv-grænsen på ${STRAKSAFSKRIVNING_THRESHOLD_DKK.toLocaleString("da-DK")} kr. ` +
              `og skal som udgangspunkt aktiveres og afskrives. Opret aktivet med 'asset register' og ` +
              `afskriv det med 'asset depreciate', eller bogfør udgiften manuelt hvis den ikke er et anlægsaktiv.`
            : `Beløbet er under småaktiv-grænsen på ${STRAKSAFSKRIVNING_THRESHOLD_DKK.toLocaleString("da-DK")} kr. ` +
              `Vurder om det skal straksafskrives ('asset write-off'), aktiveres og afskrives ('asset register'), ` +
              `eller bogføres som en almindelig driftsudgift — og bogfør det derefter.`,
        relatedBankTransactionId: row.bankTransactionId,
        relatedDocumentId: best.documentId,
        sourceEvidence: {
          rule: AGENT_RULE_ID,
          supplierName: best.supplierName ?? null,
          category: rule.label,
          expenseAccount: rule.expenseAccount,
          grossAmountDkk: grossAmount,
          fixedAssetReviewThresholdDkk: FIXED_ASSET_REVIEW_THRESHOLD_DKK,
          straksafskrivningThresholdDkk: STRAKSAFSKRIVNING_THRESHOLD_DKK,
          aboveSmallAssetThreshold: aboveSmallAsset,
        },
      });
      continue;
    }
    // Unambiguous: confident match AND a single account rule. Book it via the
    // existing expense-booking feature — the ledger still has the final word.
    const booked = bookExpenseFromBank(db, {
      documentId: best.documentId,
      bankTransactionId: row.bankTransactionId,
      expenseAccountNo: rule.expenseAccount,
      vatTreatment: rule.vatTreatment,
      createdBy: AGENT_ACTOR_ID,
      createdByProgram: AGENT_PROGRAM,
    });
    if (booked.ok) {
      report.expensesBooked.push({
        documentNo: best.invoiceNo ?? `DOC-${best.documentId}`,
        documentId: best.documentId,
        bankTransactionId: row.bankTransactionId,
        supplier: best.supplierName ?? "ukendt",
        amount: row.amount,
        currency: row.currency,
        expenseAccount: rule.expenseAccount,
        vatTreatment: rule.vatTreatment,
        label: rule.label,
        journalEntryNo: booked.entryNo ?? null,
      });
    } else {
      // The ledger refused the posting (a guardrail fired, e.g. a missing
      // VIES validation for a reverse-charge purchase). The agent obeys: the
      // transaction becomes an exception, not a forced posting.
      routeException(db, report, {
        type: "AGENT_BOOKING_BLOCKED",
        severity: "high",
        message:
          `Booking bank transaction ${row.bankTransactionId} against document ${best.documentId} ` +
          `was blocked by the ledger: ${(booked.errors ?? []).join("; ")}`,
        requiredAction:
          "Resolve the guardrail (e.g. validate the supplier VAT id) then re-run the agent.",
        relatedBankTransactionId: row.bankTransactionId,
        relatedDocumentId: best.documentId,
        sourceEvidence: { rule: AGENT_RULE_ID, ledgerErrors: booked.errors ?? [] },
      });
    }
  }

  // ---- Phase 4: reconcile -----------------------------------------------
  // Any bank transaction with no posted journal entry is surfaced as an
  // exception by the shared reconciliation sync — the agent calls the
  // existing core function rather than reimplementing it.
  report.phases.push("reconcile");
  syncUnmatchedBankTransactionExceptions(db);

  // ---- Phase 5: deadlines -----------------------------------------------
  report.phases.push("deadlines");
  checkDeadlines(db, input.asOf, report);

  // ---- Phase 6: report --------------------------------------------------
  report.phases.push("report");
  const open = listExceptions(db, { status: "open" });
  if (open.ok) {
    report.openExceptions = open.rows
      .sort((a, b) => a.id - b.id)
      .map((row) => ({
        exceptionId: row.id,
        type: row.type,
        severity: row.severity,
        message: row.message,
        requiredAction: row.requiredAction,
      }));
  }
  buildSummary(report);
}

function routeException(
  db: Database,
  report: AgentRunReport,
  input: {
    type: string;
    severity: "low" | "medium" | "high";
    message: string;
    requiredAction: string;
    relatedBankTransactionId?: number;
    relatedDocumentId?: number;
    sourceEvidence?: unknown;
  },
): void {
  const res = recordException(db, {
    type: input.type,
    severity: input.severity,
    message: input.message,
    requiredAction: input.requiredAction,
    relatedBankTransactionId: input.relatedBankTransactionId ?? null,
    relatedDocumentId: input.relatedDocumentId ?? null,
    sourceEvidence: input.sourceEvidence,
  });
  if (!res.ok) {
    report.errors.push(`failed to record exception (${input.type}): ${res.errors.join("; ")}`);
  }
}

/** The VAT period immediately before `window`, on the same cadence. */
function previousVatPeriod(window: VatPeriodWindow): VatPeriodWindow {
  // One day before the period start lands inside the previous period.
  return vatPeriodWindowFor(addDays(window.start, -1), window.vatPeriodType);
}

/**
 * Records one VAT-period deadline notice and, when the period needs the
 * human (not closed) and its filing deadline is within the escalation
 * horizon, also escalates it as an exception. The agent never files — it
 * surfaces the obligation.
 *
 * The period boundaries follow the company's real SKAT cadence
 * (`vatPeriodType`): a monthly filer gets a one-month window, a half-yearly
 * filer a six-month one — the canonical `core/periods.ts` helpers own the
 * window math, so a quarterly company is byte-identical to the historical
 * hardcoded behaviour.
 */
function reportVatPeriod(
  db: Database,
  asOf: string,
  window: VatPeriodWindow,
  report: AgentRunReport,
): void {
  // The filing deadline comes from the canonical `core/vat.ts` helper — the
  // same date every other VAT surface uses. The window end is always a valid
  // ISO date, so the `null` branch is unreachable in practice.
  const dueDate = vatFilingDeadline(window.end) ?? window.filingDeadline;
  const daysRemaining = diffDays(asOf, dueDate);
  const filing = buildVatFiling(db, window.start, window.end);
  const periodReady = filing.periodStatus === "closed" || filing.periodStatus === "reported";
  const vatReport = buildVatReport(db, window.start, window.end);
  const net = vatReport.ok ? vatReport.netVatPayable : 0;

  report.upcomingDeadlines.push({
    kind: "vat_quarter",
    periodStart: window.start,
    periodEnd: window.end,
    dueDate,
    daysRemaining,
    ready: periodReady,
    note: periodReady
      ? `Momsperioden er lukket — momsangivelse klar (momstilsvar ${formatKroner(net)}).`
      : `Momsperioden er endnu ikke lukket — luk den med 'period close' før momsangivelse.`,
  });

  // Escalate only when the human still has to act AND the deadline is near
  // (or already past) — a period still in progress is not yet actionable.
  if (!periodReady && daysRemaining <= DEADLINE_HORIZON_DAYS) {
    // A quarterly company keeps the exact "Momskvartalet" wording (byte-
    // identical); other cadences read the grammatically-correct definite form
    // for the cadence noun ("måneden" is an en-word, "halvåret" is an et-word).
    const periodWord =
      window.vatPeriodType === "month"
        ? "Momsmåneden"
        : window.vatPeriodType === "half-year"
          ? "Momshalvåret"
          : "Momskvartalet";
    routeException(db, report, {
      type: "AGENT_VAT_DEADLINE_OPEN",
      severity: daysRemaining < 0 ? "high" : "medium",
      message:
        `${periodWord} ${window.start}..${window.end} er endnu ikke lukket, og momsangivelsen ` +
        (daysRemaining < 0
          ? `skulle have været indberettet ${dueDate} (fristen er overskredet med ${Math.abs(daysRemaining)} dage pr. ${asOf}).`
          : `skal indberettes senest ${dueDate} (${daysRemaining} dage fra ${asOf}).`),
      requiredAction:
        "Luk momsperioden med 'period close --kind vat_quarter', og indberet derefter momsangivelsen.",
      sourceEvidence: { rule: AGENT_RULE_ID, dueDate, daysRemaining, periodStatus: filing.periodStatus },
    });
  }
}

/**
 * Checks the VAT-period and fiscal-year deadlines relative to `asOf` and
 * records each as an upcoming-deadline notice. A deadline that needs the
 * human (period not yet closed, near or past due) is escalated as an
 * exception. The agent always surfaces the obligation; it never files.
 *
 * VAT periods follow the company's `vatPeriodType` — the same canonical
 * `core/periods.ts` cadence the dashboard and cockpit use — so a monthly or
 * half-yearly filer gets correct windows and deadlines.
 */
function checkDeadlines(db: Database, asOf: string, report: AgentRunReport): void {
  const settings = getCompanySettings(db);
  // The VAT period the company is currently accruing in is always relevant.
  const current = vatPeriodWindowFor(asOf, settings.vatPeriodType);
  // The previous period's momsangivelse is the one with a live deadline; it
  // is reported whenever its filing deadline has not yet passed.
  const previous = previousVatPeriod(current);
  const previousDue = vatFilingDeadline(previous.end) ?? previous.filingDeadline;
  if (diffDays(asOf, previousDue) >= 0) {
    reportVatPeriod(db, asOf, previous, report);
  }
  reportVatPeriod(db, asOf, current, report);

  // --- Fiscal year (årsrapport) ---
  const fy = fiscalYearForDate(asOf, settings.fiscalYearStartMonth, settings.fiscalYearLabelStrategy);
  // Årsrapport for a class-B company is due ~5 months after the fiscal year
  // ends; the agent surfaces it conservatively, the human finalises it.
  const fyDueYear = Number(fy.end.slice(0, 4));
  const fyDueMonth = Number(fy.end.slice(5, 7));
  const fyDue = new Date(Date.UTC(fyDueYear, fyDueMonth + 4, 1));
  const fyDueDate = `${fyDue.getUTCFullYear()}-${pad2(fyDue.getUTCMonth() + 1)}-${pad2(fyDue.getUTCDate())}`;
  const fyDaysRemaining = diffDays(asOf, fyDueDate);
  report.upcomingDeadlines.push({
    kind: "fiscal_year",
    periodStart: fy.start,
    periodEnd: fy.end,
    dueDate: fyDueDate,
    daysRemaining: fyDaysRemaining,
    ready: false,
    note:
      fyDaysRemaining <= DEADLINE_HORIZON_DAYS
        ? `Årsrapport for regnskabsår ${fy.displayLabel} nærmer sig — forbered med 'report annual'.`
        : `Regnskabsår ${fy.displayLabel} løber — årsrapport forfalder ${fyDueDate}.`,
  });
}

/** Builds the plain-language end-of-run summary lines. */
function buildSummary(report: AgentRunReport): void {
  const lines: string[] = [];
  lines.push(
    `${report.documentsIngested} bilag ingested` +
      (report.documentsRejected ? `, ${report.documentsRejected} afvist` : ""),
  );
  lines.push(`${report.bankTransactionsImported} banktransaktioner importeret`);
  lines.push(`${report.expensesBooked.length} udgifter bogført automatisk af agenten`);
  lines.push(
    `${report.openExceptions.length} i exception-kø — kræver et menneske` +
      (report.openExceptions.length === 0 ? " (intet)" : ""),
  );
  if (report.upcomingDeadlines.length === 0) {
    lines.push("Ingen deadlines inden for horisonten");
  } else {
    for (const d of report.upcomingDeadlines) {
      const label = d.kind === "vat_quarter" ? "Momsangivelse" : "Årsrapport";
      lines.push(
        `${label} ${d.periodStart}..${d.periodEnd} forfalder ${d.dueDate} ` +
          `(${d.daysRemaining} dage) — ${d.ready ? "klar" : "kræver handling"}`,
      );
    }
  }
  report.summary = lines;
}
