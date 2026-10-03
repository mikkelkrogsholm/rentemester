// Bilag — the per-company ingested documents (cockpit-redesign iteration 3).
//
// Renders `/api/companies/:slug/documents`: the ingested documents/receipts,
// each showing the voucher and posted journal entry it is linked to (#196)
// where one exists. Documents are not year-scoped, but the company sub-nav
// still carries the selected `?year=` so it follows the user across views —
// the fiscal years for the selector are fetched separately.
//
// #433 — filter-bar: fritekstsøgning (leverandørnavn, bilagsnr., fakturanr.,
// posteringstekst), datointerval på fakturadato, status-filter (alle/bogført/
// ikke bogført) og type-filter (Køb/salg/Kassebon). Alle filtre er client-side
// og afspejles i URL-params (`q`, `from`, `to`, `status`, `type`) så ejeren
// kan dele linket eller komme tilbage til samme udsnit. Dato- og beløbs-
// kolonnerne har sorter-handles og en "Ryd filtre"-knap dukker op når et
// filter er aktivt.

import { useEffect, useMemo, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import { useCapabilities } from "../lib/useCapabilities";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";
import type {
  CompanyDocuments,
  DocumentRow,
  FiscalYearEntry,
} from "../lib/types";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { DocumentIngestModal } from "../components/DocumentIngestModal";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ButtonLink, Button, Input, Select, Amount, PageHeader, Pagination, FilterBar } from "../components/ui";
import { listPagination, listReturnTo, workflowTo } from "./workflow-navigation";
import { PartyLink } from "../components/PartyLink";

export type DocumentsPage = {
  documents: CompanyDocuments;
  fiscalYears: FiscalYearEntry[];
};

const DOC_TYPE_LABELS: Record<string, string> = {
  purchase_sale: "Køb/salg",
  cash_register_receipt: "Kassebon",
  internal_voucher: "Internt bilag",
  external_accounting_evidence: "Eksternt lønbilag",
};

// #433 — the keys we own in the URL. Listed once so "Ryd filtre" can clear
// them all without touching other params (e.g. `?year=`).
const FILTER_PARAM_KEYS = ["q", "from", "to", "status", "type", "party"] as const;

type StatusFilter = "all" | "booked" | "unbooked";
type TypeFilter = "all" | "purchase_sale" | "cash_register_receipt" | "internal_voucher" | "external_accounting_evidence";

type SortKey = "date" | "amount";
type SortDir = "asc" | "desc";
type PartyFilter = "all" | "linked" | "unlinked" | "internal_no_external_party" | "ambiguous";
type PartyCandidate = { partyId: string; name: string };
type PartyPlan = {
  planHash: string;
  documentSha256?: string;
  documentPayloadSha256?: string;
  evidence?: { kind?: string; jurisdiction?: string; identifierKind?: string; identifier?: string };
  partySnapshot?: { name?: string };
};

function isStatusFilter(v: string): v is StatusFilter {
  return v === "all" || v === "booked" || v === "unbooked";
}
function isTypeFilter(v: string): v is TypeFilter {
  return (
    v === "all" ||
    v === "purchase_sale" ||
    v === "cash_register_receipt" ||
    v === "internal_voucher" ||
    v === "external_accounting_evidence"
  );
}

export function documentAmount(doc: DocumentRow & { associations?: DocumentRow[] }): number | null {
  if (doc.amountIncVat !== null) return doc.amountIncVat;
  if (doc.associations && doc.associations.filter((entry) => entry.journalEntryNo).length > 1) return null;
  if (doc.journalEntryTotal !== null) return doc.journalEntryTotal;
  return null;
}

function documentMatchesText(doc: DocumentRow, needle: string): boolean {
  if (doc.supplierName && doc.supplierName.toLowerCase().includes(needle))
    return true;
  if (doc.documentNo && doc.documentNo.toLowerCase().includes(needle))
    return true;
  if (doc.invoiceNo && doc.invoiceNo.toLowerCase().includes(needle))
    return true;
  if (
    doc.accountingRationale &&
    doc.accountingRationale.toLowerCase().includes(needle)
  )
    return true;
  if (
    doc.sourceBankTransactionId !== null &&
    String(doc.sourceBankTransactionId).includes(needle)
  )
    return true;
  if (
    doc.journalEntryText &&
    doc.journalEntryText.toLowerCase().includes(needle)
  )
    return true;
  if (
    doc.journalEntryNo &&
    doc.journalEntryNo.toLowerCase().includes(needle)
  )
    return true;
  return false;
}

export type GroupedDocument = DocumentRow & { associations: DocumentRow[] };

/** One row per original document; each linked posting remains available. */
export function groupDocuments(rows: DocumentRow[]): GroupedDocument[] {
  const groups = new Map<number, GroupedDocument>();
  for (const row of rows) {
    const existing = groups.get(row.id);
    if (!existing) {
      groups.set(row.id, { ...row, associations: [row] });
      continue;
    }
    if (!existing.associations.some((entry) => entry.journalEntryId === row.journalEntryId && entry.journalEntryNo === row.journalEntryNo && entry.voucherRef === row.voucherRef)) existing.associations.push(row);
    // Representative posting identifies booked documents without summing relations.
    if (!existing.journalEntryNo && row.journalEntryNo) {
      existing.journalEntryNo = row.journalEntryNo;
      existing.journalEntryId = row.journalEntryId;
      existing.journalEntryText = row.journalEntryText;
      existing.journalEntryTotal = row.journalEntryTotal;
      existing.voucherRef = row.voucherRef;
    }
    existing.hasFile ||= row.hasFile;
  }
  return [...groups.values()];
}

export function DocumentsView({ detail = false }: { detail?: boolean } = {}) {
  const { slug = "", documentId: routeDocumentId } = useParams();
  const { can } = useCapabilities(slug);
  const { year, setYear } = useCompanyYear();
  const [params, setParams] = useSearchParams();
  const parsedDocumentId = Number(detail ? routeDocumentId : params.get("documentId"));
  const documentId = Number.isSafeInteger(parsedDocumentId) && parsedDocumentId > 0 ? parsedDocumentId : null;
  const state = useAsync<DocumentsPage>(
    async (signal) => {
      const [documents, fiscalYears] = await Promise.all([
        api.documents(slug, { signal }),
        api.fiscalYears(slug, { signal }),
      ]);
      return { documents, fiscalYears };
    },
    [slug],
  );
  const partyLinks = useAsync((signal) => api.documentPartyLinks(slug, undefined, { signal }), [slug]);
  const partyCoverage = useAsync((signal) => api.partyCoverage(slug, { signal }), [slug]);
  const [coverageBusy,setCoverageBusy]=useState(false);
  const [coverageError,setCoverageError]=useState<string|null>(null);
  // True while the document-intake modal (#213, slice 3) is open.
  const [ingesting, setIngesting] = useState(false);
  const [confirmingInternal, setConfirmingInternal] = useState(false);
  // #588: a deliberately small, reviewed flow. A person selects a document,
  // sees its recorded identity, then selects a visible canonical party. Names
  // only help find a candidate; the server still requires exact evidence.
  const [partyReviewId, setPartyReviewId] = useState<number | null>(null);
  const [partyCandidates, setPartyCandidates] = useState<PartyCandidate[]>([]);
  const [selectedPartyId, setSelectedPartyId] = useState("");
  const [partyRole, setPartyRole] = useState<"vendor" | "customer" | "bank" | "payee" | "establishment" | "location" | "payment_descriptor">("vendor");
  const [partyPlan, setPartyPlan] = useState<PartyPlan | null>(null);
  const [partyError, setPartyError] = useState<string | null>(null);
  const [partyBusy, setPartyBusy] = useState(false);
  const [partyConfirmed, setPartyConfirmed] = useState(false);
  const [sourceReviewEnabled, setSourceReviewEnabled] = useState(false);
  const [observedName, setObservedName] = useState("");
  const [observedAddress, setObservedAddress] = useState("");
  const [observedJurisdiction, setObservedJurisdiction] = useState("");
  const [observedIdentifierKind, setObservedIdentifierKind] = useState<"dk_cvr"|"eu_vat"|"non_eu">("dk_cvr");
  const [observedIdentifier, setObservedIdentifier] = useState("");
  const [sourceReference, setSourceReference] = useState("");
  const [sourceLocation, setSourceLocation] = useState("");
  const [sourceRationale, setSourceRationale] = useState("");
  const [contextSourceReference, setContextSourceReference] = useState("");
  const [contextBusinessUseReason, setContextBusinessUseReason] = useState("");
  const [contextConfirmed, setContextConfirmed] = useState(false);
  const [vatEvidenceBankTransactionId, setVatEvidenceBankTransactionId] = useState("");
  const [vatEvidenceReference, setVatEvidenceReference] = useState("");
  const [vatEvidenceSha256, setVatEvidenceSha256] = useState("");
  const [vatEvidenceRationale, setVatEvidenceRationale] = useState("");
  const [vatEvidenceConfirmed, setVatEvidenceConfirmed] = useState(false);
  const [reviewScope, setReviewScope] = useState<string | null>(null);
  const currentScope = `${slug}:${year ?? "default"}`;
  const partyOutcome = useMutationOutcome(() => { partyLinks.reload(); partyCoverage.reload(); state.reload(); });
  const coverageOutcome = useMutationOutcome(() => { partyCoverage.reload(); partyLinks.reload(); state.reload(); }, "party-coverage");
  const markPartySaved = useUnsavedChanges(partyReviewId !== null && reviewScope === currentScope && Boolean(selectedPartyId || sourceReference || sourceLocation || sourceRationale || contextSourceReference || contextBusinessUseReason || vatEvidenceReference || vatEvidenceSha256 || vatEvidenceRationale || vatEvidenceBankTransactionId));
  useEffect(() => {
    setReviewScope(currentScope);
    setPartyReviewId(null); setPartyPlan(null); setPartyConfirmed(false); setConfirmingInternal(false); setIngesting(false);
    setSourceReviewEnabled(false); setObservedName(""); setObservedAddress(""); setObservedJurisdiction(""); setObservedIdentifier(""); setSourceReference(""); setSourceLocation(""); setSourceRationale("");
    setContextSourceReference(""); setContextBusinessUseReason(""); setContextConfirmed(false);
    setVatEvidenceBankTransactionId(""); setVatEvidenceReference(""); setVatEvidenceSha256(""); setVatEvidenceRationale(""); setVatEvidenceConfirmed(false);
  }, [currentScope]);

  // --- #433 filter-bar params (client-side; reflected in URL) ---------------
  const q = params.get("q") ?? "";
  const fromDate = params.get("from") ?? "";
  const toDate = params.get("to") ?? "";
  const statusRaw = params.get("status") ?? "all";
  const typeRaw = params.get("type") ?? "all";
  const partyRaw = params.get("party") ?? "all";
  const status: StatusFilter = isStatusFilter(statusRaw) ? statusRaw : "all";
  const type: TypeFilter = isTypeFilter(typeRaw) ? typeRaw : "all";
  const party: PartyFilter = partyRaw === "linked" || partyRaw === "unlinked" || partyRaw === "internal_no_external_party" || partyRaw === "ambiguous" ? partyRaw : "all";

  // #433 — sorter for the date/amount columns. Default is the order returned
  // by the server (the document id), which is what the page used to do; only
  // after the owner clicks a column-header do we override that order.
  const sortKey = params.get("sort");
  const sort: { key: SortKey; dir: SortDir } | null = sortKey === "date" || sortKey === "amount" ? { key: sortKey, dir: params.get("dir") === "desc" ? "desc" : "asc" } : null;

  function setFilter(key: (typeof FILTER_PARAM_KEYS)[number], value: string) {
    const next = new URLSearchParams(params);
    if (value === "" || value === "all") {
      next.delete(key);
    } else {
      next.set(key, value);
    }
    next.delete("page");
    setParams(next, { replace: true });
  }

  function clearAllFilters() {
    const next = new URLSearchParams(params);
    for (const k of [...FILTER_PARAM_KEYS, "documentId", "page"]) next.delete(k);
    setParams(next, { replace: true });
  }

  const hasActiveFilter =
    q !== "" ||
    fromDate !== "" ||
    toDate !== "" ||
    status !== "all" ||
    type !== "all" ||
    party !== "all" ||
    documentId !== null;

  function toggleSort(key: SortKey) {
    const next = new URLSearchParams(params);
    if (sort?.key === key && sort.dir === "desc") { next.delete("sort"); next.delete("dir"); }
    else { next.set("sort", key); next.set("dir", sort?.key === key ? "desc" : "asc"); }
    next.delete("page");
    setParams(next, { replace: true });
  }

  function setSorting(key: string, direction?: string) {
    const next = new URLSearchParams(params);
    if (key === "default") { next.delete("sort"); next.delete("dir"); }
    else { next.set("sort", key); next.set("dir", direction ?? sort?.dir ?? "asc"); }
    next.delete("page");
    setParams(next, { replace: true });
  }

  function sortIndicator(key: SortKey): string {
    if (!sort || sort.key !== key) return "";
    return sort.dir === "asc" ? " ▲" : " ▼";
  }

  const allDocuments = useMemo(() => groupDocuments(state.data?.documents.documents ?? []), [state.data?.documents.documents]);
  const linkedIds = useMemo(() => new Set((partyLinks.data ?? []).filter((link) => link.linked === 1).map((link) => link.id)), [partyLinks.data]);
  const internalNoPartyIds = useMemo(() => new Set((partyLinks.data ?? []).filter((link) => link.resolution_state === "internal_no_external_party").map((link) => link.id)), [partyLinks.data]);

  const filteredDocuments = useMemo(() => {
    if (detail && documentId === null) return [];
    if (!hasActiveFilter) return allDocuments;
    const needle = q.trim().toLowerCase();
    return allDocuments.filter((doc) => {
      if (documentId !== null && doc.id !== documentId) return false;
      if (!detail && needle !== "" && !doc.associations.some((association) => documentMatchesText(association, needle))) return false;
      if (!detail && fromDate !== "") {
        if (!doc.invoiceDate || doc.invoiceDate < fromDate) return false;
      }
      if (!detail && toDate !== "") {
        if (!doc.invoiceDate || doc.invoiceDate > toDate) return false;
      }
      if (!detail && status === "booked" && doc.journalEntryNo === null) return false;
      if (!detail && status === "unbooked" && doc.journalEntryNo !== null) return false;
      if (!detail && type !== "all" && doc.documentType !== type) return false;
      if (!detail && party === "linked" && !linkedIds.has(doc.id)) return false;
      if (!detail && party === "unlinked" && (linkedIds.has(doc.id) || internalNoPartyIds.has(doc.id))) return false;
      if (!detail && party === "internal_no_external_party" && !internalNoPartyIds.has(doc.id)) return false;
      // Ambiguity is intentionally not inferred: it needs an explicit reviewed
      // plan conflict, so this view offers the bounded unlinked review queue.
      if (!detail && party === "ambiguous") return false;
      return true;
    });
  }, [allDocuments, hasActiveFilter, q, fromDate, toDate, status, type, party, documentId, linkedIds, internalNoPartyIds, detail]);

  const sortedDocuments = useMemo(() => {
    if (!sort) return filteredDocuments;
    const out = [...filteredDocuments];
    out.sort((a, b) => {
      let cmp = 0;
      if (sort.key === "date") {
        const ad = a.invoiceDate ?? "";
        const bd = b.invoiceDate ?? "";
        cmp = ad < bd ? -1 : ad > bd ? 1 : 0;
      } else {
        const av = documentAmount(a);
        const bv = documentAmount(b);
        if (av === null && bv === null) cmp = 0;
        else if (av === null) cmp = 1;
        else if (bv === null) cmp = -1;
        else cmp = av - bv;
      }
      return sort.dir === "asc" ? cmp : -cmp;
    });
    return out;
  }, [filteredDocuments, sort]);

  if (state.loading && !state.data) return <Loading label="Henter bilag…" />;
  if (state.error && !state.data)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const { documents: d, fiscalYears } = state.data!;
  const currency = d.company.currency || "DKK";
  const selectedYear =
    year ??
    fiscalYears.find((y) => y.source === "live")?.label ??
    fiscalYears[0]?.label ??
    String(new Date().getFullYear());
  // The intake action is hidden when the selected year is an archived
  // (pre-cut-over, read-only) year — there is no live ledger to ingest into.
  const selectedYearArchived =
    fiscalYears.find((y) => y.label === selectedYear)?.source === "archive";

  const totalCount = allDocuments.length;
  const bookedCount = allDocuments.filter((doc) => doc.journalEntryNo !== null).length;
  const pagination = listPagination(params, sortedDocuments.length);
  const visibleDocuments = detail ? sortedDocuments : sortedDocuments.slice(pagination.offset, pagination.offset + pagination.pageSize);
  const returnTo = listReturnTo(slug, "bilag", params.get("returnTo"), year);
  function setPageParam(key: "page" | "pageSize", value: number) {
    const next = new URLSearchParams(params); next.set(key, String(value));
    if (key === "pageSize") next.delete("page");
    setParams(next, { replace: true });
  }
  const matchCount = sortedDocuments.length;
  const reviewedDocument = partyReviewId === null || reviewScope !== currentScope ? null : allDocuments.find((doc) => doc.id === partyReviewId) ?? null;
  const observationRole=partyRole==="establishment"||partyRole==="location"||partyRole==="payment_descriptor";

  async function beginPartyReview(doc: DocumentRow) {
    if (partyOutcome.isBlocked()) return;
    setReviewScope(currentScope);
    setPartyReviewId(doc.id);
    setSelectedPartyId("");
    setPartyRole("vendor");
    setPartyPlan(null);
    setPartyError(null);
    setPartyConfirmed(false);
    setSourceReviewEnabled(!doc.supplierVatOrCvr);
    setObservedName(doc.supplierName ?? ""); setObservedAddress(""); setObservedJurisdiction(""); setObservedIdentifierKind("dk_cvr"); setObservedIdentifier(""); setSourceReference(""); setSourceLocation(""); setSourceRationale("");
    setContextSourceReference("");
    setContextBusinessUseReason("");
    setContextConfirmed(false);
    setPartyBusy(true);
    try {
      // This is a membership-scoped search. It is not a match decision.
      const result = await api.searchCanonicalParties(slug, doc.supplierName ?? "");
      setPartyCandidates(result.rows);
    } catch (error) {
      setPartyCandidates([]);
      setPartyError(error instanceof Error ? error.message : "Kunne ikke hente synlige parter.");
    } finally {
      setPartyBusy(false);
    }
  }

  async function applySafeCoverage(){if(coverageOutcome.isBlocked()||coverageBusy||selectedYearArchived||!can("company.master-data"))return;setCoverageBusy(true);setCoverageError(null);try{const planned=await api.planPartyCoverage(slug);if(!planned.plan.operations.length){await partyCoverage.reload();return;}if(!window.confirm(`Anvend ${planned.plan.operations.length} sikre, hash-bundne modpartskoblinger?`))return;const result=await api.applyPartyCoverage(slug,{planHash:planned.plan.planHash,idempotencyKey:`cockpit-party-coverage-${planned.plan.planHash}`,confirm:true}).catch(coverageOutcome.reject);if(!result.ok)throw new Error(result.errors?.join(", ")??"Coverage-planen blev afvist.");await Promise.all([partyCoverage.reload(),partyLinks.reload(),state.reload()]);}catch(error){setCoverageError(error instanceof Error?error.message:"Coverage-planen kunne ikke anvendes.");}finally{setCoverageBusy(false);}}

  async function markUnresolvedExternal(bankTransactionId:number){if(coverageOutcome.isBlocked()||coverageBusy||selectedYearArchived||!can("company.master-data"))return;const evidenceReference=window.prompt("Kildereference for den ukendte eksterne modpart:")?.trim(),rationale=evidenceReference?window.prompt("Hvorfor kan den juridiske modpart ikke identificeres nu?")?.trim():null,nextAction=rationale?window.prompt("Næste konkrete handling:")?.trim():null;if(!evidenceReference||!rationale||!nextAction)return;const decisions=[{bankTransactionId,unresolvedExternalParty:true,evidenceReference,rationale,nextAction}];setCoverageBusy(true);setCoverageError(null);try{const planned=await api.planPartyCoverage(slug,decisions);if(!window.confirm("Gem den hash-bundne opfølgning append-only?"))return;const result=await api.applyPartyCoverage(slug,{decisions,planHash:planned.plan.planHash,idempotencyKey:`cockpit-unresolved-external-${planned.plan.planHash}`,confirm:true}).catch(coverageOutcome.reject);if(!result.ok)throw new Error(result.errors?.join(", ")??"Beslutningen blev afvist.");await Promise.all([partyCoverage.reload(),partyLinks.reload()]);}catch(error){setCoverageError(error instanceof Error?error.message:"Beslutningen kunne ikke gemmes.");}finally{setCoverageBusy(false);}}

  async function linkBankRowParty(row:{bankTransactionId:number;transactionHash:string;documentHash?:string|null;currentDecision?:{id:number;decisionHash:string}}){
    if(coverageOutcome.isBlocked()||coverageBusy||selectedYearArchived||!can("company.master-data"))return;
    const partyId=window.prompt("Kanonisk party-id for netop denne bankrække:")?.trim();
    const role=partyId?window.prompt("Rolle, fx employee eller authority:")?.trim():null;
    const provenance=role?window.prompt("Provenance for den række-specifikke identifikation:")?.trim():null;
    const evidenceReference=provenance?window.prompt("Præcis kildereference:")?.trim():null;
    const rationale=evidenceReference?window.prompt("Begrundelse for koblingen:")?.trim():null;
    if(!partyId||!role||!provenance||!evidenceReference||!rationale)return;
    const decision={bankTransactionId:row.bankTransactionId,scope:"bank_transaction",transactionHash:row.transactionHash,...(row.documentHash?{documentHash:row.documentHash}:{}),partyId,role,provenance,evidenceReference,rationale,...(row.currentDecision?{supersedesEventId:row.currentDecision.id,supersedesDecisionHash:row.currentDecision.decisionHash}:{})};
    setCoverageBusy(true);setCoverageError(null);
    try{const planned=await api.planPartyCoverage(slug,[decision]);if(!window.confirm(row.currentDecision?"Erstat den eksakte nuværende bankrækkebeslutning append-only?":"Gem koblingen kun på denne eksakte bankrække?"))return;const result=await api.applyPartyCoverage(slug,{decisions:[decision],planHash:planned.plan.planHash,idempotencyKey:`cockpit-bank-row-party-${planned.plan.planHash}`,confirm:true}).catch(coverageOutcome.reject);if(!result.ok)throw new Error(result.errors?.join(", ")??"Bankrækkekoblingen blev afvist.");await partyCoverage.reload();}catch(error){setCoverageError(error instanceof Error?error.message:"Bankrækkekoblingen kunne ikke gemmes.");}finally{setCoverageBusy(false);}
  }

  function identityInput(doc: DocumentRow) {
    return {
      documentId: doc.id,
      partyId: selectedPartyId,
      role: partyRole,
      jurisdiction: doc.supplierCountryCode ?? undefined,
      identifierKind: doc.supplierIdentifierKind ?? undefined,
      identifier: doc.supplierVatOrCvr ?? undefined,
      ...(sourceReviewEnabled ? { sourceReview:{ observedName, observedAddress:observedAddress||undefined, ...(!observationRole?{jurisdiction:observedJurisdiction.toUpperCase(),identifierKind:observedIdentifierKind,identifier:observedIdentifier||undefined}:{}), sourceReference, sourceLocation, rationale:sourceRationale } } : {}),
    };
  }

  async function planPartyLink() {
    if (partyOutcome.isBlocked()) return;
    if (!reviewedDocument || !selectedPartyId) return;
    setPartyBusy(true);
    setPartyError(null);
    setPartyPlan(null);
    setPartyConfirmed(false);
    try {
      const result = await api.planDocumentPartyLink(slug, identityInput(reviewedDocument));
      if (!result.ok || !result.plan) {
        setPartyError(result.errors?.join(", ") ?? "Planen kunne ikke godkendes.");
        return;
      }
      setPartyPlan(result.plan as PartyPlan);
    } catch (error) {
      setPartyError(error instanceof Error ? error.message : "Kunne ikke planlægge koblingen.");
    } finally {
      setPartyBusy(false);
    }
  }

  async function applyPartyLink() {
    if (partyOutcome.isBlocked() || partyBusy || !reviewedDocument || !partyPlan || !partyConfirmed) return;
    setPartyBusy(true);
    setPartyError(null);
    try {
      const result = await api.applyDocumentPartyLink(slug, {
        ...identityInput(reviewedDocument),
        planHash: partyPlan.planHash,
        confirm: true,
        // A UI retry remains safe for this exact reviewed plan.
        idempotencyKey: `document-party-link-${reviewedDocument.id}-${partyPlan.planHash}`,
      }).catch(partyOutcome.reject);
      if (!result.ok) {
        setPartyError(result.errors?.join(", ") ?? "Koblingen kunne ikke gemmes.");
        return;
      }
      await Promise.all([partyLinks.reload(), state.reload()]);
      // Inspect after the write so the visible status/history is current.
      await api.documentPartyLinkHistory(slug, reviewedDocument.id);
      markPartySaved();
      setPartyReviewId(null);
    } catch (error) {
      setPartyError(error instanceof Error ? error.message : "Koblingen kunne ikke gemmes.");
    } finally {
      setPartyBusy(false);
    }
  }

  async function confirmInternalNoParty() {
    if (partyOutcome.isBlocked() || !reviewedDocument || reviewedDocument.documentType !== "internal_voucher") return;
    const result = await api.confirmInternalNoExternalParty(slug, {
      documentId: reviewedDocument.id,
      reason: "Bekræftet via bilagsgennemgang.",
      idempotencyKey: `internal-no-party-${reviewedDocument.id}`,
      confirm: true,
    }).catch(partyOutcome.reject);
    if (!result.ok) throw new Error(result.errors?.join(", ") ?? "Beslutningen kunne ikke gemmes.");
    await partyLinks.reload();
    setPartyReviewId(null);
  }

  async function recordCompanyContext() {
    if (partyOutcome.isBlocked() || partyBusy) return;
    if (!reviewedDocument || reviewedDocument.documentType !== "purchase_sale" || !contextConfirmed || !contextSourceReference.trim() || !contextBusinessUseReason.trim()) return;
    setPartyBusy(true); setPartyError(null);
    try {
      const result = await api.setDocumentCompanyContext(slug, { documentId: reviewedDocument.id, sourceReference: contextSourceReference.trim(), businessUseReason: contextBusinessUseReason.trim() }).catch(partyOutcome.reject);
      if (!result.ok) { setPartyError(result.errors?.join(", ") ?? "Virksomhedskonteksten kunne ikke gemmes."); return; }
      setContextConfirmed(false);
    } catch (error) { setPartyError(error instanceof Error ? error.message : "Virksomhedskonteksten kunne ikke gemmes."); }
    finally { setPartyBusy(false); }
  }
  async function reviewPurchaseVatEvidence() {
    if (partyOutcome.isBlocked() || partyBusy) return;
    if (!reviewedDocument || !vatEvidenceConfirmed || !/^\d+$/.test(vatEvidenceBankTransactionId) || !/^[a-fA-F0-9]{64}$/.test(vatEvidenceSha256) || !vatEvidenceReference.trim() || !vatEvidenceRationale.trim()) return;
    setPartyBusy(true); setPartyError(null);
    try { const result=await api.reviewPurchaseVatEvidence(slug,{documentId:reviewedDocument.id,bankTransactionId:Number(vatEvidenceBankTransactionId),businessEvidenceReference:vatEvidenceReference.trim(),businessEvidenceSha256:vatEvidenceSha256.toLowerCase(),rationale:vatEvidenceRationale.trim()}).catch(partyOutcome.reject); if(!result.ok){setPartyError(result.errors?.join(", ")??"Momsbeviset kunne ikke gennemgås.");return;} setVatEvidenceConfirmed(false); }
    catch(error){setPartyError(error instanceof Error?error.message:"Momsbeviset kunne ikke gennemgås.");}
    finally{setPartyBusy(false);}
  }

  return (
    <section className="statement">
      {state.error && <Banner kind="warning">Status kunne ikke opdateres: {state.error} De tidligere hentede bilag vises fortsat.</Banner>}
      {partyOutcome.feedback}
      {coverageOutcome.feedback}
      <PageHeader title={detail ? `Bilag ${sortedDocuments[0]?.documentNo ?? documentId ?? ""}` : "Bilag"}
        description={`${d.company.name} · ${currency} · Alle bilag på tværs af regnskabsår`}
        actions={detail ? <ButtonLink className="btn secondary" to={returnTo}>Tilbage til bilag</ButtonLink> : !selectedYearArchived && can("company.documents.upload") ? <Button onClick={() => setIngesting(true)}>Indlæs bilag</Button> : undefined} />

      <CompanyNav
        slug={slug}
        years={fiscalYears}
        selectedYear={selectedYear}
        onYearChange={setYear}
      />

      {ingesting && (
        <DocumentIngestModal
          slug={slug}
          onIngested={state.reload}
          onClose={() => setIngesting(false)}
        />
      )}

      {confirmingInternal && reviewedDocument && !selectedYearArchived && can("company.master-data") && <ConfirmDialog
        title="Bekræft internt bilag uden ekstern part"
        body={<p>Bekræft at bilag {reviewedDocument.documentNo ?? reviewedDocument.id} bevidst ikke har en ekstern part. Beslutningen registreres i revisionssporet.</p>}
        confirmLabel="Bekræft ingen ekstern part"
        onConfirm={confirmInternalNoParty}
        onRefresh={() => { partyLinks.reload(); state.reload(); }}
        onClose={() => setConfirmingInternal(false)} />}

      {!detail && <FilterBar activeCount={[q, fromDate, toDate, status === "all" ? "" : status, type === "all" ? "" : type, party === "all" ? "" : party].filter(Boolean).length} onReset={clearAllFilters}>
        <label className="journal-filter-field journal-filter-field--search">
          <span className="muted">Søg</span>
          <Input
            type="search"
            value={q}
            placeholder="Søg på leverandør, bilagsnr., faktura eller posteringstekst…"
            onChange={(e) => setFilter("q", e.target.value)}
          />
        </label>
        <label className="journal-filter-field">
          <span className="muted">Fra</span>
          <Input
            type="date"
            value={fromDate}
            onChange={(e) => setFilter("from", e.target.value)}
          />
        </label>
        <label className="journal-filter-field">
          <span className="muted">Registreret modpart</span>
          <Select value={party} onChange={(e) => setFilter("party", e.target.value)}>
            <option value="all">Alle</option>
            <option value="linked">Koblet</option>
            <option value="unlinked">Mangler review</option>
            <option value="internal_no_external_party">Internt uden ekstern part</option>
            <option value="ambiguous">Tvetydige (kræver review)</option>
          </Select>
        </label>
        <label className="journal-filter-field">
          <span className="muted">Til</span>
          <Input
            type="date"
            value={toDate}
            onChange={(e) => setFilter("to", e.target.value)}
          />
        </label>
        <label className="journal-filter-field">
          <span className="muted">Status</span>
          <Select
            value={status}
            onChange={(e) => setFilter("status", e.target.value)}
          >
            <option value="all">Alle</option>
            <option value="booked">Bogført</option>
            <option value="unbooked">Kun ubehandlede</option>
          </Select>
        </label>
        <label className="journal-filter-field">
          <span className="muted">Type</span>
          <Select
            value={type}
            onChange={(e) => setFilter("type", e.target.value)}
          >
            <option value="all">Alle</option>
            <option value="purchase_sale">Køb/salg</option>
            <option value="cash_register_receipt">Kassebon</option>
            <option value="internal_voucher">Internt bilag</option>
            <option value="external_accounting_evidence">Eksternt lønbilag</option>
          </Select>
        </label>

        <label className="journal-filter-field">Sortering<Select value={sort?.key ?? "default"} onChange={(event) => setSorting(event.target.value)}><option value="default">Nyeste indlæsning</option><option value="date">Fakturadato</option><option value="amount">Beløb</option></Select></label>
        {sort && <label className="journal-filter-field">Rækkefølge<Select value={sort.dir} onChange={(event) => setSorting(sort.key, event.target.value)}><option value="asc">Stigende</option><option value="desc">Faldende</option></Select></label>}
      </FilterBar>}

      <p className="statement-asof muted">
        {hasActiveFilter
          ? `${matchCount} af ${totalCount} bilag matcher`
          : `${totalCount} bilag`}
        {" · "}
        {bookedCount} bogført · {totalCount - bookedCount} ubehandlet
      </p>

      {partyCoverage.data&&<section className="card" aria-label="Modpartsdækning"><div className="page-head"><div><h3>Modpartsdækning</h3><p className="muted">Én kanonisk projektion fra bank til afstemning, bilag og part.</p></div><Button type="button" className="btn secondary" disabled={selectedYearArchived || !can("company.master-data") || coverageOutcome.blocked || coverageBusy||partyCoverage.data.totals.exact_candidate===0} onClick={applySafeCoverage}>Anvend sikre kandidater</Button></div><div className="stats-grid"><div><strong>{partyCoverage.data.totals.linked+partyCoverage.data.totals.resolved_no_external_party}</strong><span>Dækket</span></div><div><strong>{partyCoverage.data.totals.exact_candidate}</strong><span>Sikre kandidater</span></div><div><strong>{partyCoverage.data.totals.source_observed+partyCoverage.data.totals.unresolved_external_party+partyCoverage.data.totals.ambiguous+partyCoverage.data.totals.missing_source}</strong><span>Kræver menneske</span></div></div><details><summary>Se grundlag og rester</summary><p className="muted">Population <code>{partyCoverage.data.populationHash}</code> · plan <code>{partyCoverage.data.planHash}</code></p><ul>{partyCoverage.data.rows.filter((row)=>row.status!=="linked"&&row.status!=="resolved_no_external_party"||Boolean(row.currentDecision)).map((row)=><li key={row.bankTransactionId}>Bankpost #{row.bankTransactionId}{row.documentId?` · bilag #${row.documentId}`:""}: {row.reason}{row.candidate?.provenance?` (${row.candidate.provenance})`:""}{row.nextAction?` ${row.nextAction}`:""}{row.documentHash&&<Button type="button" className="btn secondary" disabled={selectedYearArchived || !can("company.master-data") || coverageOutcome.blocked || coverageBusy} onClick={()=>linkBankRowParty(row)}>{row.currentDecision?"Ret bankrækkens modpart":"Knyt modpart til bankrække"}</Button>}{row.documentId&&(row.status==="missing_source"||row.status==="source_observed")&&<Button type="button" className="btn secondary" disabled={selectedYearArchived || !can("company.master-data") || coverageOutcome.blocked || coverageBusy} onClick={()=>markUnresolvedExternal(row.bankTransactionId)}>Markér ekstern modpart uafklaret</Button>}</li>)}</ul></details>{coverageError&&<p className="flag warning" role="alert">{coverageError}</p>}</section>}

      {reviewedDocument && !selectedYearArchived && can("company.master-data") && (
        <section className="card" aria-label="Gennemgå registreret modpart">
          <div className="page-head">
            <div>
              <h3>Gennemgå registreret modpart</h3>
              <p className="muted">Bilag {reviewedDocument.documentNo ?? `#${reviewedDocument.id}`}. Navne er kun søgehjælp — koblingen kræver den uforanderlige identitet nedenfor.</p>
            </div>
            <Button type="button" className="btn secondary" onClick={() => setPartyReviewId(null)}>Luk</Button>
          </div>
          <dl className="key-value-list">
            <div><dt>Identitet på bilaget</dt><dd>{reviewedDocument.supplierCountryCode ?? "—"} · {reviewedDocument.supplierIdentifierKind ?? "—"} · {reviewedDocument.supplierVatOrCvr ?? "Ingen verificerbar identifikator"}</dd></div>
            <div><dt>Bevis</dt><dd>Originalfilen og bogføringen ændres ikke. Planen binder bilagets hash til den valgte part.</dd></div>
          </dl>
          <div className="row-actions">
            <label>Rolle <Select disabled={partyOutcome.blocked || partyBusy} value={partyRole} onChange={(event) => { const role=event.target.value as typeof partyRole;setPartyRole(role);if(role==="establishment"||role==="location"||role==="payment_descriptor")setSourceReviewEnabled(true);setPartyPlan(null);setPartyConfirmed(false); }}><option value="vendor">Juridisk leverandør</option><option value="customer">Kunde</option><option value="bank">Bank</option><option value="payee">Betalingsmodtager</option><option value="establishment">Observeret forretning</option><option value="location">Observeret sted</option><option value="payment_descriptor">Observeret betalingstekst</option></Select></label>
            <label>Vælg registreret modpart <Select aria-label="Vælg registreret modpart" value={selectedPartyId} onChange={(event) => { setSelectedPartyId(event.target.value); setPartyPlan(null);setPartyConfirmed(false); }} disabled={partyOutcome.blocked || partyBusy}><option value="">Vælg en synlig part…</option>{partyCandidates.map((candidate) => <option key={candidate.partyId} value={candidate.partyId}>{candidate.name}</option>)}</Select></label>
            <Button type="button" className="btn secondary" disabled={partyOutcome.blocked || partyBusy || !selectedPartyId || ((observationRole||!reviewedDocument.supplierVatOrCvr) && !sourceReviewEnabled)} onClick={planPartyLink}>Vis plan</Button>
          </div>
          {(!reviewedDocument.supplierVatOrCvr||observationRole) && <div className="card"><label><Input type="checkbox" checked={sourceReviewEnabled} disabled={partyOutcome.blocked || partyBusy || observationRole} onChange={(event)=>{setSourceReviewEnabled(event.target.checked);setPartyPlan(null);}}/> Observationen er manuelt aflæst i den uforanderlige original</label>{sourceReviewEnabled&&<><p className="muted">Indtast kun det, der faktisk står i kilden. En observeret forretning eller betalingstekst er ikke den juridiske leverandør.</p><label className="modal-field">Observeret navn<Input disabled={partyOutcome.blocked || partyBusy} value={observedName} onChange={(e)=>{setObservedName(e.target.value);setPartyPlan(null);setPartyConfirmed(false);}}/></label><label className="modal-field">Observeret adresse (valgfri)<Input disabled={partyOutcome.blocked || partyBusy} value={observedAddress} onChange={(e)=>{setObservedAddress(e.target.value);setPartyPlan(null);setPartyConfirmed(false);}}/></label>{!observationRole&&<div className="row-actions"><label>Land<Input size={4} maxLength={2} disabled={partyOutcome.blocked || partyBusy} value={observedJurisdiction} onChange={(e)=>{setObservedJurisdiction(e.target.value);setPartyPlan(null);setPartyConfirmed(false);}}/></label><label>ID-type<Select disabled={partyOutcome.blocked || partyBusy} value={observedIdentifierKind} onChange={(e)=>{setObservedIdentifierKind(e.target.value as typeof observedIdentifierKind);setPartyPlan(null);setPartyConfirmed(false);}}><option value="dk_cvr">Dansk CVR</option><option value="eu_vat">EU VAT</option><option value="non_eu">Ikke-EU</option></Select></label><label>Observeret ID<Input disabled={partyOutcome.blocked || partyBusy} value={observedIdentifier} onChange={(e)=>{setObservedIdentifier(e.target.value);setPartyPlan(null);setPartyConfirmed(false);}}/></label></div>}<label className="modal-field">Kildereference<Input disabled={partyOutcome.blocked || partyBusy} value={sourceReference} onChange={(e)=>{setSourceReference(e.target.value);setPartyPlan(null);setPartyConfirmed(false);}}/></label><label className="modal-field">Placering i kilden<Input disabled={partyOutcome.blocked || partyBusy} value={sourceLocation} onChange={(e)=>{setSourceLocation(e.target.value);setPartyPlan(null);setPartyConfirmed(false);}}/></label><label className="modal-field">Review-begrundelse<Input disabled={partyOutcome.blocked || partyBusy} value={sourceRationale} onChange={(e)=>{setSourceRationale(e.target.value);setPartyPlan(null);setPartyConfirmed(false);}}/></label></>}</div>}
          {partyError && <p className="flag warning" role="alert">{partyError}</p>}
          {partyPlan && <div className="card"><p><strong>Plan klar</strong> — {partyPlan.partySnapshot?.name ?? "Valgt part"}; bevis: {partyPlan.evidence?.kind ?? "exact_identifier"}.</p><p className="muted">Plan-hash: <code>{partyPlan.planHash}</code></p><label><Input type="checkbox" disabled={partyOutcome.blocked || partyBusy} checked={partyConfirmed} onChange={(event) => setPartyConfirmed(event.target.checked)} /> Jeg har gennemgået planen og vil oprette den append-only kobling.</label><div className="row-actions"><Button type="button" className="btn" disabled={partyOutcome.blocked || partyBusy || !partyConfirmed} onClick={applyPartyLink}>Bekræft og anvend</Button></div></div>}
          {reviewedDocument.documentType === "internal_voucher" && <div className="card"><p className="muted">Interne bilag kan bekræftes uden ekstern part. Beslutningen er append-only og ændrer ikke bilag, moms eller journal.</p><Button type="button" className="btn secondary" disabled={partyOutcome.blocked || partyBusy} onClick={() => setConfirmingInternal(true)}>Bekræft ingen ekstern part</Button></div>}
          {reviewedDocument.documentType === "purchase_sale" && <div className="card"><h4>Separat virksomhedskontekst</h4><p className="muted">Brug kun når det oprindelige købsbilag faktisk er ufuldstændigt eller et dansk forenklet bilag. Det ændrer aldrig modtageren på fakturaen og godkender ikke moms.</p><label className="modal-field">Kildereference<Input value={contextSourceReference} onChange={(event) => setContextSourceReference(event.target.value)} disabled={partyOutcome.blocked || partyBusy} /></label><label className="modal-field">Forretningsmæssig begrundelse<Input value={contextBusinessUseReason} onChange={(event) => setContextBusinessUseReason(event.target.value)} disabled={partyOutcome.blocked || partyBusy} /></label><label><Input type="checkbox" checked={contextConfirmed} onChange={(event) => setContextConfirmed(event.target.checked)} disabled={partyOutcome.blocked || partyBusy} /> Jeg har gennemgået den uforanderlige kilde og vil gemme denne attribution append-only.</label><div className="row-actions"><Button type="button" className="btn secondary" disabled={partyOutcome.blocked || partyBusy || !contextConfirmed || !contextSourceReference.trim() || !contextBusinessUseReason.trim()} onClick={recordCompanyContext}>Gem virksomhedskontekst</Button></div></div>}
          {reviewedDocument.documentType === "purchase_sale" && <div className="card"><h4>Momsbevis ved formel fakturamangel</h4><p className="muted">Kun for et sandfærdigt ufuldstændigt standardbilag. Det er ikke en override: leverandør, 25 % moms, eksakt virksomhedsbetaling og erhvervsbevis skal kunne efterprøves.</p><label className="modal-field">Bankpost-id<Input value={vatEvidenceBankTransactionId} onChange={(event)=>setVatEvidenceBankTransactionId(event.target.value)} disabled={partyOutcome.blocked || partyBusy}/></label><label className="modal-field">Erhvervsbevis – reference<Input value={vatEvidenceReference} onChange={(event)=>setVatEvidenceReference(event.target.value)} disabled={partyOutcome.blocked || partyBusy}/></label><label className="modal-field">Erhvervsbevis – SHA-256<Input value={vatEvidenceSha256} onChange={(event)=>setVatEvidenceSha256(event.target.value)} disabled={partyOutcome.blocked || partyBusy}/></label><label className="modal-field">Review-begrundelse<Input value={vatEvidenceRationale} onChange={(event)=>setVatEvidenceRationale(event.target.value)} disabled={partyOutcome.blocked || partyBusy}/></label><label><Input type="checkbox" checked={vatEvidenceConfirmed} onChange={(event)=>setVatEvidenceConfirmed(event.target.checked)} disabled={partyOutcome.blocked || partyBusy}/> Jeg bekræfter, at dette alene vedrører en formel fakturamangel.</label><div className="row-actions"><Button type="button" className="btn secondary" disabled={partyOutcome.blocked||partyBusy||!vatEvidenceConfirmed||!/^\d+$/.test(vatEvidenceBankTransactionId)||!/^[a-fA-F0-9]{64}$/.test(vatEvidenceSha256)||!vatEvidenceReference.trim()||!vatEvidenceRationale.trim()} onClick={reviewPurchaseVatEvidence}>Gem momsbevis-review</Button></div></div>}
        </section>
      )}

      {detail && sortedDocuments.length === 0 && <div className="card" role="status"><h2>Bilaget findes ikke</h2><p>Gå tilbage til bilagslisten og vælg et eksisterende bilag.</p></div>}
      <div className="card statement-card table-scroll">
        <table className="data statement-table daily-table" role="table">
          <thead role="rowgroup">
            <tr role="row">
              <th>Bilagsnr.</th>
              <th>Type</th>
              <th>Modpart / grundlag</th>
              <th>Faktura</th>
              <th>
                <Button
                  type="button"
                  className="th-sort"
                  onClick={() => toggleSort("date")}
                  aria-label="Sortér efter dato"
                >
                  Dato{sortIndicator("date")}
                </Button>
              </th>
              <th className="num">
                <Button
                  type="button"
                  className="th-sort"
                  onClick={() => toggleSort("amount")}
                  aria-label="Sortér efter beløb"
                >
                  Beløb inkl. moms{sortIndicator("amount")}
                </Button>
              </th>
              <th>Postering</th>
              <th>Bilagsfil</th>
            </tr>
          </thead>
          <tbody role="rowgroup">
            {sortedDocuments.length === 0 ? (
              <tr role="row">
                <td colSpan={8} className="empty-inline">
                  {hasActiveFilter
                    ? "Ingen bilag matcher filtrene."
                    : "Ingen bilag indlæst endnu."}
                </td>
              </tr>
            ) : (
              visibleDocuments.map((doc) => (
                <tr role="row" key={doc.id}>
                  <td role="cell" data-label="Bilag" className="account-no">
                    <Link to={workflowTo(slug, "bilag", String(doc.id), params, selectedYear)}>{doc.documentNo ?? `#${doc.id}`}</Link>
                  </td>
                  <td role="cell" data-label="Type">
                    {DOC_TYPE_LABELS[doc.documentType] ?? doc.documentType}
                  </td>
                  <td role="cell" data-label="Modpart / grundlag">
                    <div>
                      {doc.documentType === "internal_voucher"
                        ? doc.internalVoucherKind === "non_cash_balance_correction"
                          ? "Internt balancekorrektionsbilag — ingen bankbevægelse"
                          : `Bankpost #${doc.sourceBankTransactionId ?? "—"}`
                        : <PartyLink slug={slug} partyId={doc.partyId}>{doc.supplierName ?? "—"}</PartyLink>}
                    </div>
                    {doc.documentType === "internal_voucher" && doc.accountingRationale ? (
                      <div className="muted">{doc.accountingRationale}</div>
                    ) : null}
                    {doc.documentType === "internal_voucher" && doc.preparedBy ? <div className="muted">Forberedt af {doc.preparedBy}{doc.preparedByProgram ? ` via ${doc.preparedByProgram}` : ""}{doc.preparedAt ? ` · ${doc.preparedAt}` : ""}</div> : null}
                    {(doc.supplierCountryCode || doc.supplierIdentifierKind || doc.supplierIdentityStatus) && (
                      <div className="muted">
                        {doc.supplierCountryCode ?? "—"} · {doc.supplierIdentifierKind ?? "—"} · {doc.supplierIdentityStatus === "resolved" ? "Identitet bekræftet" : doc.supplierIdentityStatus ? "Identitet kræver gennemgang" : "—"}
                      </div>
                    )}
                    <div className="muted">{internalNoPartyIds.has(doc.id) ? "Bekræftet internt bilag uden ekstern part" : linkedIds.has(doc.id) ? "Registreret modpart koblet" : "Registreret modpart ikke koblet — gennemgå før anvendelse"}</div>
                    {!selectedYearArchived && can("company.master-data") && !linkedIds.has(doc.id) && !internalNoPartyIds.has(doc.id) && <Button type="button" className="btn small secondary" disabled={partyOutcome.blocked || partyBusy} onClick={() => beginPartyReview(doc)}>Gennemgå part</Button>}
                  </td>
                  <td role="cell" data-label="Faktura">{doc.invoiceNo ?? "—"}</td>
                  <td role="cell" data-label="Dato" className="entry-date">{doc.invoiceDate ?? "—"}</td>
                  <td role="cell" data-label="Beløb inkl. moms" className="num"><Amount value={documentAmount(doc)} currency={doc.amountIncVat !== null ? doc.currency : "DKK"} />{doc.amountIncVat === null && doc.associations.filter((entry) => entry.journalEntryNo).length > 1 && <span className="muted">Se de tilknyttede posteringer</span>}</td>
                  <td role="cell" data-label="Postering">
                    {doc.journalEntryNo ? doc.associations.filter((association) => association.journalEntryNo).map((association) => (
                      <div className="doc-posting" key={`${association.journalEntryId}-${association.voucherRef}`}>
                        <span className="flag ok">{association.journalEntryNo}{association.voucherRef ? ` · bilag ${association.voucherRef}` : ""}</span>
                        {association.journalEntryText && <span className="doc-posting-text muted">{association.journalEntryText}</span>}
                        {detail && <Amount value={association.journalEntryTotal} currency="DKK" />}
                      </div>
                    )) : <div className="doc-posting"><span className="flag warning">Ikke bogført</span>
                      {!selectedYearArchived && can("company.ledger.post") && <ButtonLink className="btn small" to={workflowTo(slug, "bilag", `${doc.id}/bogfoer`, params, selectedYear)}>Bogfør bilag</ButtonLink>}
                      {!selectedYearArchived && <ButtonLink className="btn small secondary" to={`/companies/${slug}/koebsoverblik?sourceKind=document&sourceId=${doc.id}`}>Åbn købscase</ButtonLink>}
                    </div>}

                  </td>
                  <td>
                    {doc.hasFile ? (
                      <a
                        href={api.documentFileUrl(slug, doc.id)}
                        target="_blank"
                        rel="noreferrer"
                      >
                        Åbn bilag
                      </a>
                    ) : (
                      <span className="muted">—</span>
                    )}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
      {!detail && <Pagination total={matchCount} {...pagination} onPageChange={(page) => setPageParam("page", page)} onPageSizeChange={(size) => setPageParam("pageSize", size)} />}
    </section>
  );
}

export function DocumentDetailView() { return <DocumentsView detail />; }
