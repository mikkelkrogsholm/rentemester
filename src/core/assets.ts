import { decodeTransactionRejection, TransactionRejectionError } from "./transaction-rejection";
/**
 * Fixed-asset workflows (#124 depreciation, #125 immediate write-off).
 *
 * #124 capitalises a purchased asset and posts deterministic linear
 * depreciation entries (debit depreciation expense, credit accumulated
 * depreciation) via `postJournalEntry`. Duplicate-period posting is blocked.
 *
 * #125 books an eligible small purchase as an immediate write-off
 * (straksafskrivning): a single balanced journal entry that expenses the
 * purchase. It requires explicit confirmation plus source-backed
 * threshold/rule metadata, and queues an exception when eligibility is
 * uncertain or documentation is missing.
 *
 * Money is integer øre (DKK with 2 decimals). Rentemester assists the
 * bookkeeping workflow; the user/advisor remains responsible for the tax
 * treatment of capitalisation, depreciation plans and straksafskrivning
 * eligibility.
 */

import type { Database } from "bun:sqlite";
import { postJournalEntry, type JournalPostResult } from "./ledger";
import { insertAuditLog } from "./actor";
import { recordException } from "./exceptions";
import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { fromOre, roundDkk, toOre } from "./money";
import { resolveAccountRole } from "./account-roles";

const DEPR_RULE_ID = "DK-ASSET-DEPR-001";
const WRITEOFF_RULE_ID = "DK-ASSET-WRITEOFF-001";

// Default chart-of-accounts numbers for the fixed-asset domain (seeded by
// seedAccounts() in the 5800-5899 range).
const DEFAULT_ASSET_ACCOUNT = "5800";
const DEFAULT_ACCUMULATED_DEPRECIATION_ACCOUNT = "5810";
const DEFAULT_DEPRECIATION_EXPENSE_ACCOUNT = "5820";

/**
 * Year-based small-asset (straksafskrivning) threshold table, keyed by income
 * year (indkomstår). The grænse is set in afskrivningsloven § 6, stk. 1, nr. 2
 * and adjusted yearly. These are conservative workflow guardrails — the
 * user/advisor owns the actual tax-law determination and must supply a source
 * for the rule applied.
 *
 * Sources (verified 2026-06):
 *   - 2024: 33.100 kr
 *   - 2025: 34.400 kr
 *   - 2026: 36.000 kr
 *   skm.dk: https://skm.dk/tal-og-metode/satser/satser-og-beloebsgraenser-i-lovgivningen/afskrivningsloven
 *   SKAT C.C.2.4.2.4.2.5: https://info.skat.dk/data.aspx?oid=2060787
 */
export const STRAKSAFSKRIVNING_THRESHOLDS_BY_YEAR: ReadonlyArray<{ year: number; thresholdDkk: number }> = [
  { year: 2024, thresholdDkk: 33100 },
  { year: 2025, thresholdDkk: 34400 },
  { year: 2026, thresholdDkk: 36000 },
];

/**
 * Resolve the straksafskrivning threshold for an income year. Years before the
 * earliest table entry clamp to the earliest known sats; years after the latest
 * entry (no published sats yet) clamp to the latest known — both conservative:
 * an unknown future year never silently relaxes the guardrail beyond the last
 * confirmed value.
 */
export function straksafskrivningThresholdForYear(year: number): number {
  const table = STRAKSAFSKRIVNING_THRESHOLDS_BY_YEAR;
  const first = table[0]!;
  const last = table[table.length - 1]!;
  if (year <= first.year) return first.thresholdDkk;
  if (year >= last.year) return last.thresholdDkk;
  // Pick the entry for the exact year, or the most recent entry on/before it.
  let chosen = first.thresholdDkk;
  for (const row of table) {
    if (row.year <= year) chosen = row.thresholdDkk;
    else break;
  }
  return chosen;
}

/**
 * Backwards-compatible default threshold (current income year 2026 = 36.000 kr).
 * Kept exported for callers that reference a single representative grænse;
 * year-sensitive logic must use {@link straksafskrivningThresholdForYear}.
 */
export const STRAKSAFSKRIVNING_THRESHOLD_DKK = straksafskrivningThresholdForYear(
  STRAKSAFSKRIVNING_THRESHOLDS_BY_YEAR[STRAKSAFSKRIVNING_THRESHOLDS_BY_YEAR.length - 1]!.year,
);

export type DepreciationMethod = "linear";

export type DepreciationPeriod = {
  periodIndex: number;
  amount: number;
};

export type ComputeDepreciationScheduleInput = {
  cost: number;
  acquisitionDate: string;
  usefulLifeMonths: number;
  method?: DepreciationMethod;
};

/**
 * Deterministic linear depreciation schedule.
 *
 * The cost is split as evenly as integer-øre arithmetic allows; the final
 * period carries the rounding remainder so the schedule sums exactly to the
 * capitalised cost. Pure function — identical inputs always yield identical
 * output.
 */
export function computeDepreciationSchedule(input: ComputeDepreciationScheduleInput): DepreciationPeriod[] {
  const method = input.method ?? "linear";
  if (method !== "linear") throw new Error(`unsupported depreciation method: ${method}`);
  const months = input.usefulLifeMonths;
  if (!Number.isInteger(months) || months <= 0) throw new Error("usefulLifeMonths must be a positive integer");
  const costOre = toOre(roundDkk(input.cost));
  if (costOre <= 0n) throw new Error("cost must be positive");

  const monthsBig = BigInt(months);
  const baseOre = costOre / monthsBig;
  const remainderOre = costOre - baseOre * monthsBig;

  const schedule: DepreciationPeriod[] = [];
  for (let i = 0; i < months; i += 1) {
    // The remainder is concentrated in the final period so every period before
    // it gets an identical amount and the total reconciles to cost exactly.
    const periodOre = i === months - 1 ? baseOre + remainderOre : baseOre;
    schedule.push({ periodIndex: i + 1, amount: fromOre(periodOre) });
  }
  return schedule;
}

export type RegisterAssetInput = {
  name: string;
  category: string;
  acquisitionDate: string;
  cost: number;
  usefulLifeMonths: number;
  purchaseDocumentId: number;
  method?: DepreciationMethod;
  assetAccountNo?: string;
  depreciationExpenseAccountNo?: string;
  accumulatedDepreciationAccountNo?: string;
  note?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type RegisterAssetResult = {
  ok: boolean;
  assetId?: number;
  totalPeriods?: number;
  periodAmount?: number;
  appliedRules: string[];
  errors: string[];
};

function accountExists(db: Database, accountNo: string): boolean {
  return db.query("SELECT 1 FROM accounts WHERE account_no = ? AND active = 1").get(accountNo) != null;
}

/**
 * Register a capitalised asset with a deterministic linear depreciation plan.
 *
 * The asset is append-only and must reference an existing purchase document so
 * every later depreciation entry can preserve an audit-trail link back to the
 * original purchase/documentation.
 */
export function registerAsset(db: Database, input: RegisterAssetInput): RegisterAssetResult {
  const errors: string[] = [];
  if (typeof input.name !== "string" || input.name.trim().length === 0) errors.push("name is required");
  if (typeof input.category !== "string" || input.category.trim().length === 0) errors.push("category is required");
  if (!looksLikeIsoDate(input.acquisitionDate)) errors.push("acquisitionDate must be YYYY-MM-DD");
  if (!Number.isFinite(input.cost) || input.cost <= 0) errors.push("cost must be a positive number");
  if (!Number.isInteger(input.usefulLifeMonths) || input.usefulLifeMonths <= 0) errors.push("usefulLifeMonths must be a positive integer");
  if (!Number.isInteger(input.purchaseDocumentId) || input.purchaseDocumentId <= 0) errors.push("purchaseDocumentId must be a positive integer");
  const method = input.method ?? "linear";
  if (method !== "linear") errors.push("only the linear depreciation method is supported");
  if (errors.length > 0) return { ok: false, appliedRules: [DEPR_RULE_ID], errors };

  const document = db.query("SELECT id FROM documents WHERE id = ?").get(input.purchaseDocumentId) as { id: number } | null;
  if (!document) {
    return { ok: false, appliedRules: [DEPR_RULE_ID], errors: [`purchase document ${input.purchaseDocumentId} does not exist`] };
  }

  const assetAccountNo = input.assetAccountNo ?? DEFAULT_ASSET_ACCOUNT;
  const depreciationExpenseAccountNo = input.depreciationExpenseAccountNo ?? DEFAULT_DEPRECIATION_EXPENSE_ACCOUNT;
  const accumulatedDepreciationAccountNo = input.accumulatedDepreciationAccountNo ?? DEFAULT_ACCUMULATED_DEPRECIATION_ACCOUNT;
  for (const [label, accountNo] of [
    ["assetAccountNo", assetAccountNo],
    ["depreciationExpenseAccountNo", depreciationExpenseAccountNo],
    ["accumulatedDepreciationAccountNo", accumulatedDepreciationAccountNo],
  ] as const) {
    if (!accountExists(db, accountNo)) errors.push(`${label} ${accountNo} does not exist or is inactive`);
  }
  if (errors.length > 0) return { ok: false, appliedRules: [DEPR_RULE_ID], errors };

  const cost = roundDkk(input.cost);
  const schedule = computeDepreciationSchedule({
    cost,
    acquisitionDate: input.acquisitionDate,
    usefulLifeMonths: input.usefulLifeMonths,
    method: "linear",
  });

  const row = db.query(
    `INSERT INTO assets (
       name, category, acquisition_date, cost, depreciation_method, useful_life_months,
       asset_account_no, depreciation_expense_account_no, accumulated_depreciation_account_no,
       purchase_document_id, note
     ) VALUES (?, ?, ?, ?, 'linear', ?, ?, ?, ?, ?, ?)
     RETURNING id`,
  ).get(
    input.name.trim(),
    input.category.trim(),
    input.acquisitionDate,
    cost,
    input.usefulLifeMonths,
    assetAccountNo,
    depreciationExpenseAccountNo,
    accumulatedDepreciationAccountNo,
    input.purchaseDocumentId,
    input.note?.trim() || null,
  ) as { id: number };

  insertAuditLog(db, {
    eventType: "asset_register",
    entityType: "asset",
    entityId: row.id,
    message: `Registered asset ${input.name.trim()} (cost ${cost}, ${input.usefulLifeMonths} months linear)`,
    createdBy: input.createdBy,
    createdByProgram: input.createdByProgram,
  });

  return {
    ok: true,
    assetId: row.id,
    totalPeriods: schedule.length,
    periodAmount: schedule[0]?.amount,
    appliedRules: [DEPR_RULE_ID],
    errors: [],
  };
}

export type PostDepreciationPeriodInput = {
  assetId: number;
  periodIndex: number;
  transactionDate: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type PostDepreciationPeriodResult = JournalPostResult & {
  assetId?: number;
  periodIndex?: number;
  periodAmount?: number;
};

type AssetRow = {
  id: number;
  name: string;
  cost: number;
  useful_life_months: number;
  depreciation_method: string;
  acquisition_date: string;
  depreciation_expense_account_no: string;
  accumulated_depreciation_account_no: string;
  purchase_document_id: number;
};

function loadAsset(db: Database, assetId: number): AssetRow | null {
  return db.query(
    `SELECT id, name, cost, useful_life_months, depreciation_method, acquisition_date,
            depreciation_expense_account_no, accumulated_depreciation_account_no, purchase_document_id
     FROM assets WHERE id = ?`,
  ).get(assetId) as AssetRow | null;
}

/**
 * Post one period of an asset's depreciation schedule.
 *
 * Debits the depreciation-expense account and credits accumulated
 * depreciation. The entry references the asset's purchase document so the
 * ledger keeps an audit-trail link to the original purchase. Re-posting an
 * already-posted period is blocked deterministically.
 */
export function postDepreciationPeriod(db: Database, input: PostDepreciationPeriodInput): PostDepreciationPeriodResult {
  const errors: string[] = [];
  if (!Number.isInteger(input.assetId) || input.assetId <= 0) errors.push("assetId must be a positive integer");
  if (!Number.isInteger(input.periodIndex) || input.periodIndex <= 0) errors.push("periodIndex must be a positive integer");
  if (!looksLikeIsoDate(input.transactionDate)) errors.push("transactionDate must be YYYY-MM-DD");
  if (errors.length > 0) return { ok: false, appliedRules: [DEPR_RULE_ID], errors };

  const asset = loadAsset(db, input.assetId);
  if (!asset) return { ok: false, appliedRules: [DEPR_RULE_ID], errors: [`asset ${input.assetId} does not exist`] };

  const schedule = computeDepreciationSchedule({
    cost: Number(asset.cost),
    acquisitionDate: asset.acquisition_date,
    usefulLifeMonths: asset.useful_life_months,
    method: "linear",
  });
  if (input.periodIndex > schedule.length) {
    return {
      ok: false,
      appliedRules: [DEPR_RULE_ID],
      errors: [`periodIndex ${input.periodIndex} is outside the depreciation schedule (1..${schedule.length})`],
    };
  }

  const existing = db.query(
    "SELECT id FROM asset_depreciation_entries WHERE asset_id = ? AND period_index = ? LIMIT 1",
  ).get(input.assetId, input.periodIndex) as { id: number } | null;
  if (existing) {
    return {
      ok: false,
      appliedRules: [DEPR_RULE_ID],
      errors: [`depreciation period ${input.periodIndex} for asset ${input.assetId} is already posted`],
    };
  }

  const periodAmount = schedule[input.periodIndex - 1]!.amount;

  try {
    const result = db.transaction(() => {
      const journal = postJournalEntry(db, {
        transactionDate: input.transactionDate,
        text: `Depreciation period ${input.periodIndex}/${schedule.length} for asset ${asset.name}`,
        documentId: asset.purchase_document_id,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: [
          { accountNo: asset.depreciation_expense_account_no, debitAmount: periodAmount, text: `Depreciation ${asset.name}` },
          { accountNo: asset.accumulated_depreciation_account_no, creditAmount: periodAmount, text: `Accumulated depreciation ${asset.name}` },
        ],
      });
      if (!journal.ok) throw new TransactionRejectionError({ appliedRules: journal.appliedRules, errors: journal.errors });

      const entry = db.query(
        `INSERT INTO asset_depreciation_entries (asset_id, period_index, transaction_date, amount, journal_entry_id)
         VALUES (?, ?, ?, ?, ?)
         RETURNING id`,
      ).get(input.assetId, input.periodIndex, input.transactionDate, periodAmount, journal.entryId!) as { id: number };

      insertAuditLog(db, {
        eventType: "asset_depreciation_post",
        entityType: "asset_depreciation_entry",
        entityId: entry.id,
        message: `Posted depreciation period ${input.periodIndex} (${periodAmount}) for asset ${asset.name}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      return {
        ...journal,
        assetId: input.assetId,
        periodIndex: input.periodIndex,
        periodAmount,
        appliedRules: [...new Set([DEPR_RULE_ID, ...(journal.appliedRules ?? [])])],
      };
    })();
    return result;
  } catch (error) {
    const parsed = decodeTransactionRejection(error);
    return {
      ok: false,
      appliedRules: [...new Set([DEPR_RULE_ID, ...(parsed?.appliedRules ?? [])])],
      errors: parsed?.errors ?? [String(error)],
    };
  }
}

export type AssetRegisterRow = {
  assetId: number;
  name: string;
  category: string;
  acquisitionDate: string;
  cost: number;
  usefulLifeMonths: number;
  postedPeriods: number;
  accumulatedDepreciation: number;
  netBookValue: number;
};

export type AssetRegisterReport = {
  ok: boolean;
  assets: AssetRegisterRow[];
  totals: { cost: number; accumulatedDepreciation: number; netBookValue: number };
  errors: string[];
};

/**
 * Asset-register report: every registered asset with its posted accumulated
 * depreciation and net book value, plus portfolio totals.
 */
export function buildAssetRegisterReport(db: Database): AssetRegisterReport {
  const rows = db.query(
    `SELECT a.id, a.name, a.category, a.acquisition_date, a.cost, a.useful_life_months,
            COUNT(d.id) AS posted_periods,
            COALESCE(SUM(d.amount), 0) AS accumulated
     FROM assets a
     LEFT JOIN asset_depreciation_entries d ON d.asset_id = a.id
     GROUP BY a.id
     ORDER BY a.id ASC`,
  ).all() as Array<{
    id: number;
    name: string;
    category: string;
    acquisition_date: string;
    cost: number;
    useful_life_months: number;
    posted_periods: number;
    accumulated: number;
  }>;

  let totalCostOre = 0n;
  let totalAccumulatedOre = 0n;
  const assets: AssetRegisterRow[] = rows.map((row) => {
    const cost = roundDkk(Number(row.cost));
    const accumulated = roundDkk(Number(row.accumulated));
    const netBookValue = fromOre(toOre(cost) - toOre(accumulated));
    totalCostOre += toOre(cost);
    totalAccumulatedOre += toOre(accumulated);
    return {
      assetId: row.id,
      name: row.name,
      category: row.category,
      acquisitionDate: row.acquisition_date,
      cost,
      usefulLifeMonths: row.useful_life_months,
      postedPeriods: Number(row.posted_periods),
      accumulatedDepreciation: accumulated,
      netBookValue,
    };
  });

  return {
    ok: true,
    assets,
    totals: {
      cost: fromOre(totalCostOre),
      accumulatedDepreciation: fromOre(totalAccumulatedOre),
      netBookValue: fromOre(totalCostOre - totalAccumulatedOre),
    },
    errors: [],
  };
}

export type ImmediateWriteOffInput = {
  name: string;
  category: string;
  acquisitionDate: string;
  cost: number;
  purchaseDocumentId: number;
  expenseAccountNo: string;
  transactionDate: string;
  confirmImmediateWriteOff: boolean;
  thresholdRuleSource: string;
  paymentAccountNo?: string;
  note?: string;
  createdBy?: string;
  createdByProgram?: string;
};

export type ImmediateWriteOffResult = JournalPostResult & {
  writeOffId?: number;
  cost?: number;
  thresholdDkk?: number;
};

/**
 * Book an eligible small purchase as an immediate write-off
 * (straksafskrivning): a single balanced journal entry that expenses the
 * purchase instead of capitalising and depreciating it.
 *
 * Guardrails: an explicit confirmation flag and a source-backed
 * threshold/rule reference are mandatory and stored on the record. If the
 * documentation is missing or the cost exceeds the small-asset threshold
 * (uncertain eligibility), the write-off is blocked and an exception is queued
 * for advisor review. Rentemester assists the workflow; the user/advisor
 * remains responsible for the tax treatment.
 */
export function postImmediateWriteOff(db: Database, input: ImmediateWriteOffInput): ImmediateWriteOffResult {
  const errors: string[] = [];
  if (typeof input.name !== "string" || input.name.trim().length === 0) errors.push("name is required");
  if (typeof input.category !== "string" || input.category.trim().length === 0) errors.push("category is required");
  if (!looksLikeIsoDate(input.acquisitionDate)) errors.push("acquisitionDate must be YYYY-MM-DD");
  if (!looksLikeIsoDate(input.transactionDate)) errors.push("transactionDate must be YYYY-MM-DD");
  if (!Number.isFinite(input.cost) || input.cost <= 0) errors.push("cost must be a positive number");
  if (!Number.isInteger(input.purchaseDocumentId) || input.purchaseDocumentId <= 0) errors.push("purchaseDocumentId must be a positive integer");
  if (typeof input.expenseAccountNo !== "string" || input.expenseAccountNo.trim().length === 0) errors.push("expenseAccountNo is required");
  if (errors.length > 0) return { ok: false, appliedRules: [WRITEOFF_RULE_ID], errors };

  // Explicit confirmation is mandatory — straksafskrivning is a tax-treatment
  // choice the user/advisor must own deliberately.
  if (input.confirmImmediateWriteOff !== true) {
    return {
      ok: false,
      appliedRules: [WRITEOFF_RULE_ID],
      errors: ["immediate write-off requires confirmImmediateWriteOff: true — the user/advisor must confirm the straksafskrivning treatment"],
    };
  }

  // Source-backed threshold/rule metadata is mandatory and stored on the
  // record so the applied rule is auditable.
  const thresholdRuleSource = typeof input.thresholdRuleSource === "string" ? input.thresholdRuleSource.trim() : "";
  if (thresholdRuleSource.length === 0) {
    return {
      ok: false,
      appliedRules: [WRITEOFF_RULE_ID],
      errors: ["immediate write-off requires a source-backed threshold/rule reference (thresholdRuleSource)"],
    };
  }

  const cost = roundDkk(input.cost);

  // The straksafskrivning grænse is set per income year (afskrivningsloven
  // § 6). Use the acquisition year's sats so a 2026 acquisition is measured
  // against the 2026 grænse, not a stale prior-year value.
  const acquisitionYear = Number(input.acquisitionDate.slice(0, 4));
  const thresholdDkk = straksafskrivningThresholdForYear(acquisitionYear);

  // Missing documentation: queue an exception for advisor review and block.
  const document = db.query("SELECT id FROM documents WHERE id = ?").get(input.purchaseDocumentId) as { id: number } | null;
  if (!document) {
    recordException(db, {
      type: "ASSET_WRITEOFF_MISSING_DOCUMENTATION",
      severity: "high",
      message: `Immediate write-off for "${input.name.trim()}" references missing purchase document ${input.purchaseDocumentId}`,
      requiredAction: "Attach the purchase document, then re-run the immediate write-off.",
      sourceEvidence: { name: input.name.trim(), cost, purchaseDocumentId: input.purchaseDocumentId },
    });
    return {
      ok: false,
      appliedRules: [WRITEOFF_RULE_ID],
      errors: [`purchase document ${input.purchaseDocumentId} does not exist — write-off blocked, exception queued`],
    };
  }

  // Uncertain eligibility: cost above the small-asset threshold. Queue an
  // exception so an advisor decides whether to capitalise/depreciate instead.
  if (cost > thresholdDkk) {
    recordException(db, {
      type: "ASSET_WRITEOFF_ELIGIBILITY_UNCERTAIN",
      severity: "high",
      relatedDocumentId: input.purchaseDocumentId,
      message: `Immediate write-off for "${input.name.trim()}" (cost ${cost}) exceeds the small-asset threshold ${thresholdDkk} for income year ${acquisitionYear}`,
      requiredAction: "Advisor must confirm straksafskrivning eligibility or capitalise and depreciate the asset instead.",
      sourceEvidence: { name: input.name.trim(), cost, thresholdDkk, acquisitionYear, thresholdRuleSource },
    });
    return {
      ok: false,
      appliedRules: [WRITEOFF_RULE_ID],
      errors: [`cost ${cost} exceeds the small-asset threshold ${thresholdDkk} for income year ${acquisitionYear} — eligibility uncertain, exception queued for advisor review`],
    };
  }

  if (!accountExists(db, input.expenseAccountNo.trim())) {
    return { ok: false, appliedRules: [WRITEOFF_RULE_ID], errors: [`expense account ${input.expenseAccountNo} does not exist or is inactive`] };
  }

  const existing = db.query("SELECT id FROM asset_writeoffs WHERE purchase_document_id = ? LIMIT 1").get(input.purchaseDocumentId) as { id: number } | null;
  if (existing) {
    return { ok: false, appliedRules: [WRITEOFF_RULE_ID], errors: [`purchase document ${input.purchaseDocumentId} already has immediate write-off ${existing.id}`] };
  }

  const payment = input.paymentAccountNo
    ? { ok: true as const, accountNo: input.paymentAccountNo }
    : resolveAccountRole(db, "bank");
  if (!payment.ok) return { ok: false, appliedRules: [WRITEOFF_RULE_ID], errors: [payment.error] };
  const paymentAccountNo = payment.accountNo;

  try {
    const result = db.transaction(() => {
      const journal = postJournalEntry(db, {
        transactionDate: input.transactionDate,
        text: `Immediate write-off (straksafskrivning) for ${input.name.trim()}`,
        documentId: input.purchaseDocumentId,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
        lines: [
          { accountNo: input.expenseAccountNo.trim(), debitAmount: cost, text: `Straksafskrivning ${input.name.trim()}` },
          { accountNo: paymentAccountNo, creditAmount: cost, text: `Write-off settlement ${input.name.trim()}` },
        ],
      });
      if (!journal.ok) throw new TransactionRejectionError({ appliedRules: journal.appliedRules, errors: journal.errors });

      const writeOff = db.query(
        `INSERT INTO asset_writeoffs (
           name, category, acquisition_date, writeoff_date, cost, purchase_document_id,
           expense_account_no, confirmed, threshold_dkk, threshold_rule_source, note, journal_entry_id
         ) VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?)
         RETURNING id`,
      ).get(
        input.name.trim(),
        input.category.trim(),
        input.acquisitionDate,
        input.transactionDate,
        cost,
        input.purchaseDocumentId,
        input.expenseAccountNo.trim(),
        thresholdDkk,
        thresholdRuleSource,
        input.note?.trim() || null,
        journal.entryId!,
      ) as { id: number };

      insertAuditLog(db, {
        eventType: "asset_immediate_writeoff",
        entityType: "asset_writeoff",
        entityId: writeOff.id,
        message: `Immediate write-off ${cost} for ${input.name.trim()}`,
        createdBy: input.createdBy,
        createdByProgram: input.createdByProgram,
      });

      return {
        ...journal,
        writeOffId: writeOff.id,
        cost,
        thresholdDkk,
        appliedRules: [...new Set([WRITEOFF_RULE_ID, ...(journal.appliedRules ?? [])])],
      };
    })();
    return result;
  } catch (error) {
    const parsed = decodeTransactionRejection(error);
    return {
      ok: false,
      appliedRules: [...new Set([WRITEOFF_RULE_ID, ...(parsed?.appliedRules ?? [])])],
      errors: parsed?.errors ?? [String(error)],
    };
  }
}
