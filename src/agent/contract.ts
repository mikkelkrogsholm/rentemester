/**
 * Runtime bookkeeper agent — operating contract (#183).
 *
 * This module is the *code* side of the operating contract documented in
 * `docs/runtime-agent-contract.md`. It declares the agent's identity, the
 * ordered loop phases, the hard guardrails, and the supplier rule base the
 * agent uses to classify unambiguous expenses.
 *
 * The contract has one non-negotiable invariant: **the agent never guesses.**
 * Anything it cannot decide deterministically from the rules becomes an
 * exception for a human — it never becomes a posting.
 */

/** Canonical actor id the agent books under. Mutations are attributed here. */
export const AGENT_ACTOR_ID = "agent:rentemester-bookkeeper";

/** Program string recorded on journal entries the agent produces. */
export const AGENT_PROGRAM = "rentemester-runtime-agent";

/** Rule id stamped on agent-run exceptions and report lines. */
export const AGENT_RULE_ID = "DK-RUNTIME-AGENT-001";

/**
 * Ordered phases of the periodic bookkeeping loop. The agent always runs
 * them in this order; a later phase may depend on an earlier one's output
 * (e.g. reconcile depends on documents being ingested first).
 */
export const AGENT_LOOP_PHASES = [
  "ingest", // bilagsmail / maildrop -> documents
  "book", // book the unambiguous expenses
  "route", // route the uncertain to the exception queue
  "payables", // settle the unambiguous creditor payments, surface overdue ones
  "reconcile", // sync unmatched bank transactions into exceptions
  "deadlines", // check VAT / accounting-period / year-end deadlines + accruals
  "report", // produce the end-of-run report
] as const;

export type AgentLoopPhase = (typeof AGENT_LOOP_PHASES)[number];

/**
 * Confidence threshold above which a bank-match suggestion is eligible for
 * automatic booking. Below it, the transaction is routed to the exception
 * queue. Deliberately a single tunable constant — the contract forbids
 * scattered ad-hoc thresholds.
 */
export const AUTO_BOOK_CONFIDENCE_THRESHOLD = 0.65;

/**
 * Days before a VAT-quarter / year-end deadline at which the agent starts
 * surfacing it in the end-of-run report. A deadline already in the window
 * is reported even when its period is not yet closed.
 */
export const DEADLINE_HORIZON_DAYS = 45;

/**
 * Gross-amount (DKK, incl. moms) at or above which a purchase in an asset-like
 * category is treated as a *possible capitalisable fixed asset* and routed to
 * the exception queue for a human/asset decision instead of being booked
 * straight to a P&L expense account (#223).
 *
 * The sourced reference point is the Danish small-asset (straksafskrivning)
 * threshold `STRAKSAFSKRIVNING_THRESHOLD_DKK` (33.100 DKK, see
 * `src/core/assets.ts`, rule `DK-ASSET-WRITEOFF-001`): a purchase below it may
 * be eligible for immediate write-off, one above it generally has to be
 * capitalised and depreciated. Either way — small-asset write-off or
 * capitalisation — is a tax/asset judgement the deterministic loop cannot make,
 * so the loop never books such a purchase silently as an operating expense.
 *
 * The review floor is set well below that threshold so that a materially
 * sized asset-like purchase (e.g. a 12.000 DKK arbejdscomputer) is always
 * surfaced for the asset decision, while trivial accessories are not.
 */
export const FIXED_ASSET_REVIEW_THRESHOLD_DKK = 5000;

/**
 * Supplier classification rule. A rule maps a token found in the (lower-cased)
 * supplier name to a deterministic expense account + VAT treatment. The agent
 * only auto-books a transaction when exactly one rule matches; an unknown
 * supplier is never guessed — it becomes an exception.
 */
export type SupplierRule = {
  token: string;
  expenseAccount: string;
  vatTreatment: "standard" | "reverse_charge" | "representation" | "exempt";
  label: string;
  /**
   * True when the rule's category buys *physical equipment* — hardware,
   * machinery, inventory — i.e. the kind of purchase that may be a
   * capitalisable fixed asset rather than an operating expense (#223). A
   * materially sized purchase in an asset-like category is never auto-booked:
   * it is routed to the exception queue for a human/asset decision.
   */
  assetLike?: boolean;
};

/**
 * The agent's account-mapping rule base. Held intentionally small and explicit:
 * the contract is that the *rules* decide, and a rule must be auditable. A real
 * deployment would extend this from the company's kontoplan — but it stays a
 * deterministic lookup, never an LLM guess at posting time.
 */
export const SUPPLIER_RULES: readonly SupplierRule[] = [
  { token: "google", expenseAccount: "3000", vatTreatment: "standard", label: "Software og SaaS" },
  { token: "microsoft", expenseAccount: "3000", vatTreatment: "standard", label: "Software og SaaS" },
  { token: "openai", expenseAccount: "3010", vatTreatment: "reverse_charge", label: "AI-værktøjer" },
  { token: "anthropic", expenseAccount: "3010", vatTreatment: "reverse_charge", label: "AI-værktøjer" },
  { token: "amazon", expenseAccount: "3020", vatTreatment: "reverse_charge", label: "Hosting og cloud" },
  { token: "aws", expenseAccount: "3020", vatTreatment: "reverse_charge", label: "Hosting og cloud" },
  { token: "dsb", expenseAccount: "3050", vatTreatment: "standard", label: "Rejse og transport" },
  // Hardware is an asset-like category: a purchase here may be a capitalisable
  // fixed asset, so a materially sized one is routed for an asset decision (#223).
  { token: "elgiganten", expenseAccount: "3120", vatTreatment: "standard", label: "Hardware og udstyr", assetLike: true },
];

/**
 * Resolves the single rule for a supplier name, or `null` when no rule — or
 * more than one rule — matches. The "more than one" case is treated as
 * ambiguous on purpose: ambiguity is never resolved by guessing.
 */
export function resolveSupplierRule(supplierName: string | null | undefined): SupplierRule | null {
  if (!supplierName) return null;
  const lower = supplierName.toLowerCase();
  const matches = SUPPLIER_RULES.filter((rule) => lower.includes(rule.token));
  return matches.length === 1 ? matches[0]! : null;
}

/**
 * Whether a confidently-matched purchase looks like a *capitalisable fixed
 * asset* and must therefore go to a human/asset decision instead of being
 * booked straight to a P&L expense account (#223).
 *
 * Deterministic and conservative: a purchase is flagged only when its rule's
 * category is asset-like (physical equipment) AND its gross amount (incl. moms)
 * is at or above `FIXED_ASSET_REVIEW_THRESHOLD_DKK`. A subscription/service
 * category is never flagged regardless of amount; a trivially small equipment
 * purchase is not flagged either. The loop does not decide capitalisation vs
 * straksafskrivning — it only refuses to guess and routes the decision.
 */
export function looksLikeFixedAsset(rule: SupplierRule, grossAmountDkk: number): boolean {
  if (!rule.assetLike) return false;
  return Math.abs(grossAmountDkk) >= FIXED_ASSET_REVIEW_THRESHOLD_DKK;
}
