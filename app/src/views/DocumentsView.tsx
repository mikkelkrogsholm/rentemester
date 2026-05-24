// Bilag — the per-company ingested documents (cockpit-redesign iteration 3).
//
// Renders `/api/companies/:slug/documents`: the ingested documents/receipts,
// each showing the voucher and posted journal entry it is linked to (#196)
// where one exists. Documents are not year-scoped, but the company sub-nav
// still carries the selected `?year=` so it follows the user across views —
// the fiscal years for the selector are fetched separately.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { CompanyDocuments, FiscalYearEntry } from "../lib/types";
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

export function DocumentsView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
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

      <p className="statement-asof muted">
        {d.documents.length} bilag · {d.linkedCount} bogført ·{" "}
        {d.unlinkedCount} ubehandlet
      </p>

      <div className="card statement-card table-scroll">
        <table className="data statement-table">
          <thead>
            <tr>
              <th>Bilagsnr.</th>
              <th>Type</th>
              <th>Leverandør</th>
              <th>Faktura</th>
              <th>Dato</th>
              <th className="num">Beløb inkl. moms</th>
              <th>Postering</th>
              <th>Bilagsfil</th>
            </tr>
          </thead>
          <tbody>
            {d.documents.length === 0 ? (
              <tr>
                <td colSpan={8} className="empty-inline">
                  Ingen bilag ingested endnu.
                </td>
              </tr>
            ) : (
              d.documents.map((doc) => (
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
