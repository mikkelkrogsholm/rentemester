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
};

// #433 — the keys we own in the URL. Listed once so "Ryd filtre" can clear
// them all without touching other params (e.g. `?year=`).
const FILTER_PARAM_KEYS = ["q", "from", "to", "status", "type"] as const;

type StatusFilter = "all" | "booked" | "unbooked";
type TypeFilter = "all" | "purchase_sale" | "cash_register_receipt";

type SortKey = "date" | "amount";
type SortDir = "asc" | "desc";

function isStatusFilter(v: string): v is StatusFilter {
  return v === "all" || v === "booked" || v === "unbooked";
}
function isTypeFilter(v: string): v is TypeFilter {
  return (
    v === "all" || v === "purchase_sale" || v === "cash_register_receipt"
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
  // True while the document-intake modal (#213, slice 3) is open.
  const [ingesting, setIngesting] = useState(false);
  // Holds the bilag id whose Bogfør-modal is open (#407); null when none.
  const [bookingDocumentId, setBookingDocumentId] = useState<number | null>(
    null,
  );

  // --- #433 filter-bar params (client-side; reflected in URL) ---------------
  const q = params.get("q") ?? "";
  const fromDate = params.get("from") ?? "";
  const toDate = params.get("to") ?? "";
  const statusRaw = params.get("status") ?? "all";
  const typeRaw = params.get("type") ?? "all";
  const status: StatusFilter = isStatusFilter(statusRaw) ? statusRaw : "all";
  const type: TypeFilter = isTypeFilter(typeRaw) ? typeRaw : "all";

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
    type !== "all";

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

  const filteredDocuments = useMemo(() => {
    if (!hasActiveFilter) return allDocuments;
    const needle = q.trim().toLowerCase();
    return allDocuments.filter((doc) => {
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
      return true;
    });
  }, [allDocuments, hasActiveFilter, q, fromDate, toDate, status, type]);

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

      <div className="card statement-card table-scroll">
        <table className="data statement-table">
          <thead>
            <tr>
              <th>Bilagsnr.</th>
              <th>Type</th>
              <th>Leverandør</th>
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
                  <td>{doc.supplierName ?? "—"}</td>
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
                        {!selectedYearArchived && (
                          <button
                            type="button"
                            className="btn small"
                            onClick={() => setBookingDocumentId(doc.id)}
                          >
                            Bogfør bilag
                          </button>
                        )}
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
