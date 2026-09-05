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

import { useMemo, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type {
  CompanyDocuments,
  DocumentRow,
  FiscalYearEntry,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { DocumentIngestModal } from "../components/DocumentIngestModal";
import { DocumentBookExpenseModal } from "../components/DocumentBookExpenseModal";

type DocumentsPage = {
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

function documentAmount(doc: DocumentRow): number | null {
  if (doc.amountIncVat !== null) return doc.amountIncVat;
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

export function DocumentsView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const [params, setParams] = useSearchParams();
  const documentId = Number(params.get("documentId")) || null;
  const state = useAsync<DocumentsPage>(
    async () => {
      const [documents, fiscalYears] = await Promise.all([
        api.documents(slug),
        api.fiscalYears(slug),
      ]);
      return { documents, fiscalYears };
    },
    [slug],
  );
  const partyLinks = useAsync(() => api.documentPartyLinks(slug), [slug]);
  // True while the document-intake modal (#213, slice 3) is open.
  const [ingesting, setIngesting] = useState(false);
  // Holds the bilag id whose Bogfør-modal is open (#407); null when none.
  const [bookingDocumentId, setBookingDocumentId] = useState<number | null>(
    null,
  );
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
  const [contextSourceReference, setContextSourceReference] = useState("");
  const [contextBusinessUseReason, setContextBusinessUseReason] = useState("");
  const [contextConfirmed, setContextConfirmed] = useState(false);
  const [vatEvidenceBankTransactionId, setVatEvidenceBankTransactionId] = useState("");
  const [vatEvidenceReference, setVatEvidenceReference] = useState("");
  const [vatEvidenceSha256, setVatEvidenceSha256] = useState("");
  const [vatEvidenceRationale, setVatEvidenceRationale] = useState("");
  const [vatEvidenceConfirmed, setVatEvidenceConfirmed] = useState(false);

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
  const [sort, setSort] = useState<{ key: SortKey; dir: SortDir } | null>(
    null,
  );

  function setFilter(key: (typeof FILTER_PARAM_KEYS)[number], value: string) {
    const next = new URLSearchParams(params);
    if (value === "" || value === "all") {
      next.delete(key);
    } else {
      next.set(key, value);
    }
    setParams(next, { replace: true });
  }

  function clearAllFilters() {
    const next = new URLSearchParams(params);
    for (const k of FILTER_PARAM_KEYS) next.delete(k);
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
    setSort((prev) => {
      if (!prev || prev.key !== key) return { key, dir: "asc" };
      if (prev.dir === "asc") return { key, dir: "desc" };
      return null;
    });
  }

  function sortIndicator(key: SortKey): string {
    if (!sort || sort.key !== key) return "";
    return sort.dir === "asc" ? " ▲" : " ▼";
  }

  const allDocuments = state.data?.documents.documents ?? [];
  const linkedIds = useMemo(() => new Set((partyLinks.data ?? []).filter((link) => link.linked === 1).map((link) => link.id)), [partyLinks.data]);
  const internalNoPartyIds = useMemo(() => new Set((partyLinks.data ?? []).filter((link) => link.resolution_state === "internal_no_external_party").map((link) => link.id)), [partyLinks.data]);

  const filteredDocuments = useMemo(() => {
    if (!hasActiveFilter) return allDocuments;
    const needle = q.trim().toLowerCase();
    return allDocuments.filter((doc) => {
      if (documentId !== null && doc.id !== documentId) return false;
      if (needle !== "" && !documentMatchesText(doc, needle)) return false;
      if (fromDate !== "") {
        if (!doc.invoiceDate || doc.invoiceDate < fromDate) return false;
      }
      if (toDate !== "") {
        if (!doc.invoiceDate || doc.invoiceDate > toDate) return false;
      }
      if (status === "booked" && doc.journalEntryNo === null) return false;
      if (status === "unbooked" && doc.journalEntryNo !== null) return false;
      if (type !== "all" && doc.documentType !== type) return false;
      if (party === "linked" && !linkedIds.has(doc.id)) return false;
      if (party === "unlinked" && (linkedIds.has(doc.id) || internalNoPartyIds.has(doc.id))) return false;
      if (party === "internal_no_external_party" && !internalNoPartyIds.has(doc.id)) return false;
      // Ambiguity is intentionally not inferred: it needs an explicit reviewed
      // plan conflict, so this view offers the bounded unlinked review queue.
      if (party === "ambiguous") return false;
      return true;
    });
  }, [allDocuments, hasActiveFilter, q, fromDate, toDate, status, type, party, documentId, linkedIds, internalNoPartyIds]);

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
  if (state.error)
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

  const totalCount = d.documents.length;
  const matchCount = sortedDocuments.length;
  const reviewedDocument = partyReviewId === null ? null : allDocuments.find((doc) => doc.id === partyReviewId) ?? null;

  async function beginPartyReview(doc: DocumentRow) {
    setPartyReviewId(doc.id);
    setSelectedPartyId("");
    setPartyRole("vendor");
    setPartyPlan(null);
    setPartyError(null);
    setPartyConfirmed(false);
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
    if (!reviewedDocument || !selectedPartyId) return;
    setPartyBusy(true);
    setPartyError(null);
    setPartyPlan(null);
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
    if (!reviewedDocument || !partyPlan || !partyConfirmed) return;
    setPartyBusy(true);
    setPartyError(null);
    try {
      const result = await api.applyDocumentPartyLink(slug, {
        ...identityInput(reviewedDocument),
        planHash: partyPlan.planHash,
        confirm: true,
        // A UI retry remains safe for this exact reviewed plan.
        idempotencyKey: `document-party-link-${reviewedDocument.id}-${partyPlan.planHash}`,
      });
      if (!result.ok) {
        setPartyError(result.errors?.join(", ") ?? "Koblingen kunne ikke gemmes.");
        return;
      }
      await Promise.all([partyLinks.reload(), state.reload()]);
      // Inspect after the write so the visible status/history is current.
      await api.documentPartyLinkHistory(slug, reviewedDocument.id);
      setPartyReviewId(null);
    } catch (error) {
      setPartyError(error instanceof Error ? error.message : "Koblingen kunne ikke gemmes.");
    } finally {
      setPartyBusy(false);
    }
  }

  async function confirmInternalNoParty() {
    if (!reviewedDocument || reviewedDocument.documentType !== "internal_voucher" || !window.confirm("Bekræft at dette interne bilag bevidst ikke har en ekstern part.")) return;
    setPartyBusy(true); setPartyError(null);
    try { const result = await api.confirmInternalNoExternalParty(slug, { documentId: reviewedDocument.id, reason: "Confirmed in Documents Cockpit", idempotencyKey: `internal-no-party-${reviewedDocument.id}`, confirm: true }); if (!result.ok) { setPartyError(result.errors?.join(", ") ?? "Beslutningen kunne ikke gemmes."); return; } await partyLinks.reload(); setPartyReviewId(null); }
    catch (error) { setPartyError(error instanceof Error ? error.message : "Beslutningen kunne ikke gemmes."); }
    finally { setPartyBusy(false); }
  }

  async function recordCompanyContext() {
    if (!reviewedDocument || reviewedDocument.documentType !== "purchase_sale" || !contextConfirmed || !contextSourceReference.trim() || !contextBusinessUseReason.trim()) return;
    setPartyBusy(true); setPartyError(null);
    try {
      const result = await api.setDocumentCompanyContext(slug, { documentId: reviewedDocument.id, sourceReference: contextSourceReference.trim(), businessUseReason: contextBusinessUseReason.trim() });
      if (!result.ok) { setPartyError(result.errors?.join(", ") ?? "Virksomhedskonteksten kunne ikke gemmes."); return; }
      setContextConfirmed(false);
    } catch (error) { setPartyError(error instanceof Error ? error.message : "Virksomhedskonteksten kunne ikke gemmes."); }
    finally { setPartyBusy(false); }
  }
  async function reviewPurchaseVatEvidence() {
    if (!reviewedDocument || !vatEvidenceConfirmed || !/^\d+$/.test(vatEvidenceBankTransactionId) || !/^[a-fA-F0-9]{64}$/.test(vatEvidenceSha256) || !vatEvidenceReference.trim() || !vatEvidenceRationale.trim()) return;
    setPartyBusy(true); setPartyError(null);
    try { const result=await api.reviewPurchaseVatEvidence(slug,{documentId:reviewedDocument.id,bankTransactionId:Number(vatEvidenceBankTransactionId),businessEvidenceReference:vatEvidenceReference.trim(),businessEvidenceSha256:vatEvidenceSha256.toLowerCase(),rationale:vatEvidenceRationale.trim()}); if(!result.ok){setPartyError(result.errors?.join(", ")??"Momsbeviset kunne ikke gennemgås.");return;} setVatEvidenceConfirmed(false); }
    catch(error){setPartyError(error instanceof Error?error.message:"Momsbeviset kunne ikke gennemgås.");}
    finally{setPartyBusy(false);}
  }

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{d.company.name}</h2>
          <p className="muted">
            {d.company.cvr ? `CVR ${d.company.cvr} · ` : ""}
            {d.company.country} · {currency} · Bilag
          </p>
        </div>
        <div className="row-actions">
          {!selectedYearArchived && (
            <button
              type="button"
              className="btn"
              onClick={() => setIngesting(true)}
            >
              Indlæs bilag
            </button>
          )}
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

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

      {bookingDocumentId !== null && (
        <DocumentBookExpenseModal
          slug={slug}
          documentId={bookingDocumentId}
          onBooked={state.reload}
          onClose={() => setBookingDocumentId(null)}
        />
      )}

      <div className="journal-filter-bar card" role="search">
        <label className="journal-filter-field journal-filter-field--search">
          <span className="muted">Søg</span>
          <input
            type="search"
            value={q}
            placeholder="Søg på leverandør, bilagsnr., faktura eller posteringstekst…"
            onChange={(e) => setFilter("q", e.target.value)}
          />
        </label>
        <label className="journal-filter-field">
          <span className="muted">Fra</span>
          <input
            type="date"
            value={fromDate}
            onChange={(e) => setFilter("from", e.target.value)}
          />
        </label>
        <label className="journal-filter-field">
          <span className="muted">Kanonisk part</span>
          <select value={party} onChange={(e) => setFilter("party", e.target.value)}>
            <option value="all">Alle</option>
            <option value="linked">Koblet</option>
            <option value="unlinked">Mangler review</option>
            <option value="internal_no_external_party">Internt uden ekstern part</option>
            <option value="ambiguous">Tvetydige (kræver review)</option>
          </select>
        </label>
        <label className="journal-filter-field">
          <span className="muted">Til</span>
          <input
            type="date"
            value={toDate}
            onChange={(e) => setFilter("to", e.target.value)}
          />
        </label>
        <label className="journal-filter-field">
          <span className="muted">Status</span>
          <select
            value={status}
            onChange={(e) => setFilter("status", e.target.value)}
          >
            <option value="all">Alle</option>
            <option value="booked">Bogført</option>
            <option value="unbooked">Kun ubehandlede</option>
          </select>
        </label>
        <label className="journal-filter-field">
          <span className="muted">Type</span>
          <select
            value={type}
            onChange={(e) => setFilter("type", e.target.value)}
          >
            <option value="all">Alle</option>
            <option value="purchase_sale">Køb/salg</option>
            <option value="cash_register_receipt">Kassebon</option>
            <option value="internal_voucher">Internt bilag</option>
            <option value="external_accounting_evidence">Eksternt lønbilag</option>
          </select>
        </label>
        {hasActiveFilter && (
          <button
            type="button"
            className="btn secondary"
            onClick={clearAllFilters}
          >
            Ryd filtre
          </button>
        )}
      </div>

      <p className="statement-asof muted">
        {hasActiveFilter
          ? `${matchCount} af ${totalCount} bilag matcher`
          : `${totalCount} bilag`}
        {" · "}
        {d.linkedCount} bogført · {d.unlinkedCount} ubehandlet
      </p>

      {reviewedDocument && (
        <section className="card" aria-label="Gennemgå kanonisk part">
          <div className="page-head">
            <div>
              <h3>Gennemgå kanonisk part</h3>
              <p className="muted">Bilag {reviewedDocument.documentNo ?? `#${reviewedDocument.id}`}. Navne er kun søgehjælp — koblingen kræver den uforanderlige identitet nedenfor.</p>
            </div>
            <button type="button" className="btn secondary" onClick={() => setPartyReviewId(null)}>Luk</button>
          </div>
          <dl className="key-value-list">
            <div><dt>Identitet på bilaget</dt><dd>{reviewedDocument.supplierCountryCode ?? "—"} · {reviewedDocument.supplierIdentifierKind ?? "—"} · {reviewedDocument.supplierVatOrCvr ?? "Ingen verificerbar identifikator"}</dd></div>
            <div><dt>Bevis</dt><dd>Originalfilen og bogføringen ændres ikke. Planen binder bilagets hash til den valgte part.</dd></div>
          </dl>
          <div className="row-actions">
            <label>Rolle <select value={partyRole} onChange={(event) => { setPartyRole(event.target.value as "vendor" | "customer"); setPartyPlan(null); }}><option value="vendor">Leverandør</option><option value="customer">Kunde</option></select></label>
            <label>Vælg kanonisk part <select aria-label="Vælg kanonisk part" value={selectedPartyId} onChange={(event) => { setSelectedPartyId(event.target.value); setPartyPlan(null); }} disabled={partyBusy}><option value="">Vælg en synlig part…</option>{partyCandidates.map((candidate) => <option key={candidate.partyId} value={candidate.partyId}>{candidate.name}</option>)}</select></label>
            <button type="button" className="btn secondary" disabled={partyBusy || !selectedPartyId || !reviewedDocument.supplierVatOrCvr} onClick={planPartyLink}>Vis plan</button>
          </div>
          {!reviewedDocument.supplierVatOrCvr && <p className="flag warning">Bilaget har ingen verificerbar identifikator. Navne alene kan ikke kobles.</p>}
          {partyError && <p className="flag warning" role="alert">{partyError}</p>}
          {partyPlan && <div className="card"><p><strong>Plan klar</strong> — {partyPlan.partySnapshot?.name ?? "Valgt part"}; bevis: {partyPlan.evidence?.kind ?? "exact_identifier"}.</p><p className="muted">Plan-hash: <code>{partyPlan.planHash}</code></p><label><input type="checkbox" checked={partyConfirmed} onChange={(event) => setPartyConfirmed(event.target.checked)} /> Jeg har gennemgået planen og vil oprette den append-only kobling.</label><div className="row-actions"><button type="button" className="btn" disabled={partyBusy || !partyConfirmed} onClick={applyPartyLink}>Bekræft og anvend</button></div></div>}
          {reviewedDocument.documentType === "internal_voucher" && <div className="card"><p className="muted">Interne bilag kan bekræftes uden ekstern part. Beslutningen er append-only og ændrer ikke bilag, moms eller journal.</p><button type="button" className="btn secondary" disabled={partyBusy} onClick={confirmInternalNoParty}>Bekræft ingen ekstern part</button></div>}
          {reviewedDocument.documentType === "purchase_sale" && <div className="card"><h4>Separat virksomhedskontekst</h4><p className="muted">Brug kun når det oprindelige købsbilag faktisk er ufuldstændigt eller et dansk forenklet bilag. Det ændrer aldrig modtageren på fakturaen og godkender ikke moms.</p><label className="modal-field">Kildereference<input value={contextSourceReference} onChange={(event) => setContextSourceReference(event.target.value)} disabled={partyBusy} /></label><label className="modal-field">Forretningsmæssig begrundelse<input value={contextBusinessUseReason} onChange={(event) => setContextBusinessUseReason(event.target.value)} disabled={partyBusy} /></label><label><input type="checkbox" checked={contextConfirmed} onChange={(event) => setContextConfirmed(event.target.checked)} disabled={partyBusy} /> Jeg har gennemgået den uforanderlige kilde og vil gemme denne attribution append-only.</label><div className="row-actions"><button type="button" className="btn secondary" disabled={partyBusy || !contextConfirmed || !contextSourceReference.trim() || !contextBusinessUseReason.trim()} onClick={recordCompanyContext}>Gem virksomhedskontekst</button></div></div>}
          {reviewedDocument.documentType === "purchase_sale" && <div className="card"><h4>Momsbevis ved formel fakturamangel</h4><p className="muted">Kun for et sandfærdigt ufuldstændigt standardbilag. Det er ikke en override: leverandør, 25 % moms, eksakt virksomhedsbetaling og erhvervsbevis skal kunne efterprøves.</p><label className="modal-field">Bankpost-id<input value={vatEvidenceBankTransactionId} onChange={(event)=>setVatEvidenceBankTransactionId(event.target.value)} disabled={partyBusy}/></label><label className="modal-field">Erhvervsbevis – reference<input value={vatEvidenceReference} onChange={(event)=>setVatEvidenceReference(event.target.value)} disabled={partyBusy}/></label><label className="modal-field">Erhvervsbevis – SHA-256<input value={vatEvidenceSha256} onChange={(event)=>setVatEvidenceSha256(event.target.value)} disabled={partyBusy}/></label><label className="modal-field">Review-begrundelse<input value={vatEvidenceRationale} onChange={(event)=>setVatEvidenceRationale(event.target.value)} disabled={partyBusy}/></label><label><input type="checkbox" checked={vatEvidenceConfirmed} onChange={(event)=>setVatEvidenceConfirmed(event.target.checked)} disabled={partyBusy}/> Jeg bekræfter, at dette alene vedrører en formel fakturamangel.</label><div className="row-actions"><button type="button" className="btn secondary" disabled={partyBusy||!vatEvidenceConfirmed||!/^\d+$/.test(vatEvidenceBankTransactionId)||!/^[a-fA-F0-9]{64}$/.test(vatEvidenceSha256)||!vatEvidenceReference.trim()||!vatEvidenceRationale.trim()} onClick={reviewPurchaseVatEvidence}>Gem momsbevis-review</button></div></div>}
        </section>
      )}

      <div className="card statement-card table-scroll">
        <table className="data statement-table">
          <thead>
            <tr>
              <th>Bilagsnr.</th>
              <th>Type</th>
              <th>Modpart / grundlag</th>
              <th>Faktura</th>
              <th>
                <button
                  type="button"
                  className="th-sort"
                  onClick={() => toggleSort("date")}
                  aria-label="Sortér efter dato"
                >
                  Dato{sortIndicator("date")}
                </button>
              </th>
              <th className="num">
                <button
                  type="button"
                  className="th-sort"
                  onClick={() => toggleSort("amount")}
                  aria-label="Sortér efter beløb"
                >
                  Beløb inkl. moms{sortIndicator("amount")}
                </button>
              </th>
              <th>Postering</th>
              <th>Bilagsfil</th>
            </tr>
          </thead>
          <tbody>
            {sortedDocuments.length === 0 ? (
              <tr>
                <td colSpan={8} className="empty-inline">
                  {hasActiveFilter
                    ? "Ingen bilag matcher filtrene."
                    : "Ingen bilag ingested endnu."}
                </td>
              </tr>
            ) : (
              sortedDocuments.map((doc) => (
                <tr key={doc.id}>
                  <td className="account-no">
                    {doc.documentNo ?? `#${doc.id}`}
                  </td>
                  <td>
                    {DOC_TYPE_LABELS[doc.documentType] ?? doc.documentType}
                  </td>
                  <td>
                    <div>
                      {doc.documentType === "internal_voucher"
                        ? doc.internalVoucherKind === "non_cash_balance_correction"
                          ? "Internt balancekorrektionsbilag — ingen bankbevægelse"
                          : `Bankpost #${doc.sourceBankTransactionId ?? "—"}`
                        : doc.supplierName ?? "—"}
                    </div>
                    {doc.documentType === "internal_voucher" && doc.accountingRationale ? (
                      <div className="muted">{doc.accountingRationale}</div>
                    ) : null}
                    {doc.documentType === "internal_voucher" && doc.preparedBy ? <div className="muted">Forberedt af {doc.preparedBy}{doc.preparedByProgram ? ` via ${doc.preparedByProgram}` : ""}{doc.preparedAt ? ` · ${doc.preparedAt}` : ""}</div> : null}
                    {(doc.supplierCountryCode || doc.supplierIdentifierKind || doc.supplierIdentityStatus) && (
                      <div className="muted">
                        {doc.supplierCountryCode ?? "—"} · {doc.supplierIdentifierKind ?? "—"} · {doc.supplierIdentityStatus ?? "—"}
                      </div>
                    )}
                    <div className="muted">{internalNoPartyIds.has(doc.id) ? "Bekræftet internt bilag uden ekstern part" : linkedIds.has(doc.id) ? "Kanonisk part koblet" : "Kanonisk part ikke koblet — gennemgå før anvendelse"}</div>
                    {!linkedIds.has(doc.id) && !internalNoPartyIds.has(doc.id) && <button type="button" className="btn small secondary" onClick={() => beginPartyReview(doc)}>Gennemgå part</button>}
                  </td>
                  <td>{doc.invoiceNo ?? "—"}</td>
                  <td className="entry-date">{doc.invoiceDate ?? "—"}</td>
                  <td className="num">
                    {doc.amountIncVat !== null
                      ? formatKroner(doc.amountIncVat, doc.currency)
                      : doc.journalEntryTotal !== null
                        ? formatKroner(doc.journalEntryTotal, doc.currency)
                        : "—"}
                  </td>
                  <td>
                    {doc.journalEntryNo ? (
                      <div className="doc-posting">
                        <span className="flag ok">
                          {doc.journalEntryNo}
                          {doc.voucherRef ? ` · bilag ${doc.voucherRef}` : ""}
                        </span>
                        {doc.journalEntryText ? (
                          <span className="doc-posting-text muted">
                            {doc.journalEntryText}
                          </span>
                        ) : null}
                      </div>
                    ) : (
                      <div className="doc-posting">
                        <span className="flag warning">Ikke bogført</span>
                        {!selectedYearArchived && <>
                          <button
                            type="button"
                            className="btn small"
                            onClick={() => setBookingDocumentId(doc.id)}
                          >
                            Bogfør bilag
                          </button>
                          <Link className="btn small secondary" to={`/companies/${slug}/koebsoverblik?sourceKind=document&sourceId=${doc.id}`}>Åbn købscase</Link>
                        </>}
                      </div>
                    )}
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
    </section>
  );
}
