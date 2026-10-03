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

export type DocumentsPage = {
  documents: CompanyDocuments;
  fiscalYears: FiscalYearEntry[];
};

const DOC_TYPE_LABELS: Record<string, string> = {
  purchase_sale: "Køb/salg",
  cash_register_receipt: "Kassebon",
  internal_voucher: "Internt bilag",
};

// #433 — the keys we own in the URL. Listed once so "Ryd filtre" can clear
// them all without touching other params (e.g. `?year=`).
const FILTER_PARAM_KEYS = ["q", "from", "to", "status", "type", "party"] as const;

type StatusFilter = "all" | "booked" | "unbooked";
type TypeFilter = "all" | "purchase_sale" | "cash_register_receipt" | "internal_voucher";

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
    v === "internal_voucher"
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
  // True while the document-intake modal (#213, slice 3) is open.
  const [ingesting, setIngesting] = useState(false);
  const [confirmingInternal, setConfirmingInternal] = useState(false);
  // #588: a deliberately small, reviewed flow. A person selects a document,
  // sees its recorded identity, then selects a visible canonical party. Names
  // only help find a candidate; the server still requires exact evidence.
  const [partyReviewId, setPartyReviewId] = useState<number | null>(null);
  const [partyCandidates, setPartyCandidates] = useState<PartyCandidate[]>([]);
  const [selectedPartyId, setSelectedPartyId] = useState("");
  const [partyRole, setPartyRole] = useState<"vendor" | "customer">("vendor");
  const [partyPlan, setPartyPlan] = useState<PartyPlan | null>(null);
  const [partyError, setPartyError] = useState<string | null>(null);
  const [partyBusy, setPartyBusy] = useState(false);
  const [partyConfirmed, setPartyConfirmed] = useState(false);
  const [reviewScope, setReviewScope] = useState<string | null>(null);
  const currentScope = `${slug}:${year ?? "default"}`;
  const partyOutcome = useMutationOutcome(() => { partyLinks.reload(); state.reload(); });
  const markPartySaved = useUnsavedChanges(partyReviewId !== null && selectedPartyId !== "" && reviewScope === currentScope);
  useEffect(() => {
    setReviewScope(currentScope);
    setPartyReviewId(null); setPartyPlan(null); setPartyConfirmed(false); setConfirmingInternal(false); setIngesting(false);
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

  async function beginPartyReview(doc: DocumentRow) {
    if (partyOutcome.isBlocked()) return;
    setReviewScope(currentScope);
    setPartyReviewId(doc.id);
    setSelectedPartyId("");
    setPartyRole("vendor");
    setPartyPlan(null);
    setPartyError(null);
    setPartyConfirmed(false);
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

  function identityInput(doc: DocumentRow) {
    return {
      documentId: doc.id,
      partyId: selectedPartyId,
      role: partyRole,
      jurisdiction: doc.supplierCountryCode ?? undefined,
      identifierKind: doc.supplierIdentifierKind ?? undefined,
      identifier: doc.supplierVatOrCvr ?? undefined,
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

  return (
    <section className="statement">
      {state.error && <Banner kind="warning">Status kunne ikke opdateres: {state.error} De tidligere hentede bilag vises fortsat.</Banner>}
      {partyOutcome.feedback}
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
            <label>Rolle <Select value={partyRole} onChange={(event) => { setPartyRole(event.target.value as "vendor" | "customer"); setPartyPlan(null); setPartyConfirmed(false); }}><option value="vendor">Leverandør</option><option value="customer">Kunde</option></Select></label>
            <label>Vælg registreret modpart <Select aria-label="Vælg registreret modpart" value={selectedPartyId} onChange={(event) => { setSelectedPartyId(event.target.value); setPartyPlan(null); setPartyConfirmed(false); }} disabled={partyBusy || partyOutcome.blocked}><option value="">Vælg en synlig part…</option>{partyCandidates.map((candidate) => <option key={candidate.partyId} value={candidate.partyId}>{candidate.name}</option>)}</Select></label>
            <Button type="button" className="btn secondary" disabled={partyOutcome.blocked || partyBusy || !selectedPartyId || !reviewedDocument.supplierVatOrCvr} onClick={planPartyLink}>Vis plan</Button>
          </div>
          {!reviewedDocument.supplierVatOrCvr && <p className="flag warning">Bilaget har ingen verificerbar identifikator. Navne alene kan ikke kobles.</p>}
          {partyError && <p className="flag warning" role="alert">{partyError}</p>}
          {partyPlan && <div className="card"><p><strong>Plan klar</strong> — {partyPlan.partySnapshot?.name ?? "Valgt part"}; bevis: {partyPlan.evidence?.kind ?? "exact_identifier"}.</p><p className="muted">Plan-hash: <code>{partyPlan.planHash}</code></p><label><Input type="checkbox" disabled={partyBusy || partyOutcome.blocked} checked={partyConfirmed} onChange={(event) => setPartyConfirmed(event.target.checked)} /> Jeg har gennemgået planen og vil oprette den append-only kobling.</label><div className="row-actions"><Button type="button" className="btn" disabled={partyOutcome.blocked || partyBusy || !partyConfirmed} onClick={applyPartyLink}>Bekræft og anvend</Button></div></div>}
          {reviewedDocument.documentType === "internal_voucher" && <div className="card"><p className="muted">Interne bilag kan bekræftes uden ekstern part. Beslutningen er append-only og ændrer ikke bilag, moms eller journal.</p><Button type="button" className="btn secondary" disabled={partyBusy || partyOutcome.blocked} onClick={() => setConfirmingInternal(true)}>Bekræft ingen ekstern part</Button></div>}
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
                        ? `Bankpost #${doc.sourceBankTransactionId ?? "—"}`
                        : doc.supplierName ?? "—"}
                    </div>
                    {doc.documentType === "internal_voucher" && doc.accountingRationale ? (
                      <div className="muted">{doc.accountingRationale}</div>
                    ) : null}
                    {(doc.supplierCountryCode || doc.supplierIdentifierKind || doc.supplierIdentityStatus) && (
                      <div className="muted">
                        {doc.supplierCountryCode ?? "—"} · {doc.supplierIdentifierKind ?? "—"} · {doc.supplierIdentityStatus === "resolved" ? "Identitet bekræftet" : doc.supplierIdentityStatus ? "Identitet kræver gennemgang" : "—"}
                      </div>
                    )}
                    <div className="muted">{internalNoPartyIds.has(doc.id) ? "Bekræftet internt bilag uden ekstern part" : linkedIds.has(doc.id) ? "Registreret modpart koblet" : "Registreret modpart ikke koblet — gennemgå før anvendelse"}</div>
                    {!selectedYearArchived && can("company.master-data") && !linkedIds.has(doc.id) && !internalNoPartyIds.has(doc.id) && <Button type="button" className="btn small secondary" disabled={partyBusy || partyOutcome.blocked} onClick={() => beginPartyReview(doc)}>Gennemgå part</Button>}
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
