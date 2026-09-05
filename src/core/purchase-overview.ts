import type { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { canonicalJson } from "./canonical-json";
import { listPurchaseCases, purchaseCaseNeed, type PurchaseCase, type PurchaseCaseNeed } from "./purchase-cases";
import { listAccountingDrafts } from "./accounting-drafts";
import { buildProfitAndLoss } from "./financial-statements";

export type PurchaseOverviewFilter = { from: string; to: string; includeProvisional?: boolean };
type SourceFact = { date: string | null; amount: number | null; currency: string | null };
type KnownEffect = {
  caseId: string;
  status: "known";
  draftId: string;
  draftVersion: number;
  draftEventHash: string;
  expense: number;
  expectedVat: number;
};
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
  return fact.date === null || (fact.date >= input.from && fact.date <= input.to);
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
    const need = purchaseCaseNeed(db, item.purchaseCase);
    return { ...item, need };
  });
  const profitAndLoss=buildProfitAndLoss(db,input.from,input.to);
  const canonical = {
    sourceCaseCount: current.length,
    postedCaseCount: current.filter(item => item.purchaseCase.accountingProgress === "posted").length,
    unpostedCaseCount: current.filter(item => item.purchaseCase.accountingProgress === "unposted").length,
    financialAggregation: "not_available_without_double_counting" as const,
    economicEffect: { expense: profitAndLoss.totalExpense, result: profitAndLoss.result, basis: "posted_ledger" as const },
  };
  const activeDrafts = listAccountingDrafts(db).filter(draft => (draft.status === "created" || draft.status === "revised" || draft.status === "submitted") && draft.payload.transactionDate >= input.from && draft.payload.transactionDate <= input.to);
  const provisionalEffects = current.map(({ purchaseCase }): KnownEffect | { caseId: string; status: "unknown" | "excluded"; reason: string } => {
    if (input.includeProvisional === false || purchaseCase.accountingProgress === "posted") return { caseId: purchaseCase.caseId, status: "excluded" as const, reason: "canonical_booking_exists" };
    const matches = activeDrafts.filter(draft =>
      (purchaseCase.source.kind === "document" && draft.payload.documentId === purchaseCase.source.id) ||
      (purchaseCase.source.kind === "bank_transaction" && draft.payload.sourceBankTransactionId === purchaseCase.source.id));
    if (matches.length !== 1) return { caseId: purchaseCase.caseId, status: "unknown" as const, reason: matches.length ? "multiple_active_drafts" : "no_active_draft" };
    const draft=matches[0]!;
    const currency = draft.payload.currency ?? "DKK";
    const hasDocumentedConversion = draft.payload.amountForeign != null && draft.payload.amountDkk != null && draft.payload.fxRateToDkk != null;
    if (currency !== "DKK" && !hasDocumentedConversion) return { caseId: purchaseCase.caseId, status: "excluded", reason: "foreign_currency_without_documented_conversion" };
    const amounts = draft.payload.lines.map(line=>{const account=db.query("SELECT type FROM accounts WHERE account_no=?").get(line.accountNo) as {type:string}|null;return {type:account?.type??null,amount:Number(line.debitAmount??0)-Number(line.creditAmount??0)};});
    if (amounts.some(line=>line.type===null)) return { caseId: purchaseCase.caseId, status: "unknown" as const, reason: "unknown_account" };
    const documentId = draft.payload.documentId ?? (purchaseCase.source.kind === "document" ? purchaseCase.source.id : null);
    const documentedVat = documentId == null ? null : (db.query("SELECT vat_amount AS vatAmount FROM documents WHERE id=?").get(documentId) as {vatAmount:number|null}|null)?.vatAmount ?? null;
    const expectedVat = amounts.filter(line=>line.type==="vat").reduce((sum,line)=>sum+line.amount,0);
    if (documentedVat == null && expectedVat === 0 && !draft.payload.lines.some(line => line.vatCode != null)) return { caseId: purchaseCase.caseId, status: "unknown", reason: "vat_classification_missing" };
    return { caseId: purchaseCase.caseId, status: "known", draftId:draft.id, draftVersion:draft.version, draftEventHash:draft.eventHash, expense:amounts.filter(line=>line.type==="expense").reduce((sum,line)=>sum+line.amount,0), expectedVat };
  });
  const known=[...new Map(provisionalEffects.filter((item):item is KnownEffect=>item.status==="known").map(item=>[item.draftId,item])).values()];
  const provisionalExpense = known.reduce((sum,item)=>sum+item.expense,0);
  const expectedVat = known.reduce((sum,item)=>sum+item.expectedVat,0);
  const provisional = {
    included: input.includeProvisional !== false,
    caseCount: input.includeProvisional === false ? 0 : current.length,
    unresolvedDocumentationCount: input.includeProvisional === false ? 0 : current.filter(item => item.purchaseCase.documentationOutcome === "unresolved").length,
    alternativeEvidenceCount: input.includeProvisional === false ? 0 : current.filter(item => item.purchaseCase.documentationOutcome === "alternative_evidence_assessed").length,
    financialAggregation: "not_available_without_double_counting" as const,
    economicEffect: { status: "projection_not_filing_ready" as const, expense: provisionalExpense, expectedVat, effects: provisionalEffects },
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
    basis: {
      canonical,
      provisional,
      difference: {
        basis: "canonical_plus_known_unposted_drafts" as const,
        provisionalCaseCount: provisional.caseCount,
        expenseDelta: provisionalExpense,
        expectedVatDelta: expectedVat,
        combinedEconomicEffect: {
          expense: canonical.economicEffect.expense + provisionalExpense,
          result: canonical.economicEffect.result - provisionalExpense,
        },
        canonicalPostingIsUnchanged: true,
      },
    },
    groups,
    sourceHash: digest({ scope: input, current: current.map(item => ({ caseId: item.purchaseCase.caseId, version: item.purchaseCase.version, sourceFingerprint: item.purchaseCase.sourceFingerprint, need: item.need?.key ?? null, fact: item.fact })) }),
  };
}
