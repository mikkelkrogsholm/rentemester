import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { canonicalJson } from "./canonical-json";
import { listPurchaseCases, purchaseCaseNeed, purchaseCaseSourceFingerprint, type PurchaseCase, type PurchaseCaseNeed } from "./purchase-cases";

export type PurchaseOverviewFilter = { from: string; to: string; includeProvisional?: boolean };
type SourceFact = { date: string | null; amount: number | null; currency: string | null };
const iso = (value: string) => /^\d{4}-\d{2}-\d{2}$/.test(value);
const digest = (value: unknown) => createHash("sha256").update(canonicalJson(value)).digest("hex");

function sourceFact(db: Database, purchaseCase: PurchaseCase): SourceFact {
  if (purchaseCase.source.kind === "bank_transaction") {
    return (db.query("SELECT transaction_date AS date,amount,currency FROM bank_transactions WHERE id=?").get(purchaseCase.source.id) as SourceFact | null) ?? { date: null, amount: null, currency: null };
  }
  if (purchaseCase.source.kind === "document") {
    return (db.query("SELECT invoice_date AS date,amount_inc_vat AS amount,currency FROM documents WHERE id=?").get(purchaseCase.source.id) as SourceFact | null) ?? { date: null, amount: null, currency: null };
  }
  return (db.query("SELECT bill_date AS date,gross_amount AS amount,currency FROM payables WHERE id=?").get(purchaseCase.source.id) as SourceFact | null) ?? { date: null, amount: null, currency: null };
}

function inScope(fact: SourceFact, input: PurchaseOverviewFilter): boolean {
  return fact.date !== null && fact.date >= input.from && fact.date <= input.to;
}

function group(purchaseCase: PurchaseCase, need: PurchaseCaseNeed) {
  return {
    need,
    case: { caseId: purchaseCase.caseId, version: purchaseCase.version, source: purchaseCase.source, sourceFingerprint: purchaseCase.sourceFingerprint, documentationOutcome: purchaseCase.documentationOutcome, accountingProgress: purchaseCase.accountingProgress, vatEvidence: purchaseCase.vatEvidence },
  };
}

/** Read-only operational projection. It intentionally does not aggregate money:
 * a document, bank transaction and payable can describe the same economic fact.
 * Canonical reporting remains the ledger/reporting surface. */
export function buildPurchaseOverview(db: Database, input: PurchaseOverviewFilter) {
  if (!iso(input.from) || !iso(input.to) || input.from > input.to) throw new Error("ordered ISO from and to dates are required");
  const all = listPurchaseCases(db);
  const scoped = all.map(purchaseCase => ({ purchaseCase, fact: sourceFact(db, purchaseCase) })).filter(item => inScope(item.fact, input));
  const current = scoped.map(item => {
    const actualFingerprint = purchaseCaseSourceFingerprint(db, item.purchaseCase.source);
    const need = actualFingerprint === item.purchaseCase.sourceFingerprint ? purchaseCaseNeed(db, item.purchaseCase) : { key: "source:stale", question: "The source changed; inspect its current evidence before review." } as PurchaseCaseNeed;
    return { ...item, need };
  });
  const canonical = {
    sourceCaseCount: current.length,
    postedCaseCount: current.filter(item => item.purchaseCase.accountingProgress === "posted").length,
    unpostedCaseCount: current.filter(item => item.purchaseCase.accountingProgress === "unposted").length,
    financialAggregation: "not_available_without_double_counting" as const,
  };
  const provisional = {
    included: input.includeProvisional !== false,
    caseCount: input.includeProvisional === false ? 0 : current.length,
    unresolvedDocumentationCount: input.includeProvisional === false ? 0 : current.filter(item => item.purchaseCase.documentationOutcome === "unresolved").length,
    alternativeEvidenceCount: input.includeProvisional === false ? 0 : current.filter(item => item.purchaseCase.documentationOutcome === "alternative_evidence_assessed").length,
    financialAggregation: "not_available_without_double_counting" as const,
  };
  const byNeed = new Map<string, Array<ReturnType<typeof group>>>();
  if (input.includeProvisional !== false) for (const item of current) if (item.need) {
    const items = byNeed.get(item.need.key) ?? [];
    items.push(group(item.purchaseCase, item.need));
    byNeed.set(item.need.key, items);
  }
  const groups = [...byNeed.entries()].map(([needKey, members]) => ({
    need: members[0]!.need,
    caseCount: members.length,
    members: members.map(member => member.case).sort((a, b) => a.caseId.localeCompare(b.caseId)),
    selectionHash: digest({ needKey, members: members.map(member => ({ caseId: member.case.caseId, version: member.case.version, sourceFingerprint: member.case.sourceFingerprint })).sort((a, b) => a.caseId.localeCompare(b.caseId)) }),
  })).sort((a, b) => a.need.key.localeCompare(b.need.key));
  return {
    scope: { from: input.from, to: input.to },
    basis: { canonical, provisional, difference: { provisionalCaseCount: provisional.caseCount, canonicalPostingIsUnchanged: true } },
    groups,
    sourceHash: digest({ scope: input, current: current.map(item => ({ caseId: item.purchaseCase.caseId, version: item.purchaseCase.version, sourceFingerprint: item.purchaseCase.sourceFingerprint, need: item.need?.key ?? null, fact: item.fact })) }),
  };
}
