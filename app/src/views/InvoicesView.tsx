// Fakturaer — the per-company issued invoices (cockpit-redesign iteration 5;
// human write-actions added in #213, slice 4).
//
// Renders `/api/companies/:slug/invoices?year=`: the sales invoices issued in
// the selected fiscal year, each with its settlement status (kladde / bogført
// / betalt / forfalden …). Summary cards above the table give the year's gross
// total, the outstanding total and the overdue count. A company with no issued
// invoices shows a graceful empty state. All money fields are kroner —
// `formatKroner` is used throughout.
//
// Slice 4 makes the view write-capable for the human-mode invoice actions:
//   - "Udsted faktura" (page action) opens the multi-line InvoiceIssueModal;
//   - per row, "Afstem" settles an issued invoice against a bank payment via
//     a ConfirmDialog because the posting is write-irreversible.
// Every write action is hidden for an archived (read-only) year.
//
// Issue #385: a per-row "Bogfør" action used to live here too. Every row in
// this list is already posted (the `InvoiceStatus` union has no "draft" and
// the empty state copy reads "Udstedte fakturaer vises her, så snart de er
// bogført"), so re-offering "Bogfør" only tempted the owner into a
// double-post. The action was removed from the cockpit; ledger reposting
// remains available via `invoice post` in the CLI for the rare repair case.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type {
  CompanyInvoiceRow,
  CompanyInvoices,
  InvoiceStatus,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { InvoiceIssueModal } from "../components/InvoiceIssueModal";

/** Human label + flag tone for each settlement status. */
const STATUS_META: Record<
  InvoiceStatus,
  { label: string; tone: "ok" | "warning" | "critical" | "neutral" }
> = {
  open: { label: "Bogført", tone: "neutral" },
  paid: { label: "Betalt", tone: "ok" },
  credited: { label: "Krediteret", tone: "warning" },
  refunded: { label: "Refunderet", tone: "warning" },
  overpaid: { label: "Overbetalt", tone: "warning" },
  written_off: { label: "Afskrevet", tone: "critical" },
  overdue: { label: "Forfalden", tone: "critical" },
};

export function InvoicesView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const state = useAsync<CompanyInvoices>(
    () => api.invoices(slug, year),
    [slug, year],
  );
  // True while the invoice-issue modal (#213, slice 4) is open.
  const [issuing, setIssuing] = useState(false);
  // The invoice row whose "Afstem" ConfirmDialog is open, if any.
  const [settling, setSettling] = useState<CompanyInvoiceRow | null>(null);
  // The invoice row whose "Krediter" ConfirmDialog is open, if any (#412).
  const [crediting, setCrediting] = useState<CompanyInvoiceRow | null>(null);

  if (state.loading && !state.data)
    return <Loading label="Henter fakturaer…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const inv = state.data!;
  const currency = inv.company.currency || "DKK";

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{inv.company.name}</h2>
          <p className="muted">
            {inv.company.cvr ? `CVR ${inv.company.cvr} · ` : ""}
            {inv.company.country} · {currency} · Fakturaer
          </p>
        </div>
        <div className="row-actions">
          {/* The issue write action — hidden for an archived (read-only) year,
              where no live ledger is available to issue into. */}
          {!inv.archived && (
            <button
              type="button"
              className="btn"
              onClick={() => setIssuing(true)}
            >
              Udsted faktura
            </button>
          )}
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

      <CompanyNav
        slug={slug}
        years={inv.fiscalYears}
        selectedYear={inv.selectedYear}
        onYearChange={setYear}
      />

      {issuing && (
        <InvoiceIssueModal
          slug={slug}
          onIssued={state.reload}
          onClose={() => setIssuing(false)}
        />
      )}

      {settling && (
        <ConfirmDialog
          title="Afstem faktura mod bankbetaling"
          body={
            <p>
              Afstem faktura <strong>{settling.invoiceNo}</strong> mod en
              indgående bankbetaling med samme reference. Afstemningen lægger en
              postering og kan ikke fortrydes.
            </p>
          }
          confirmLabel="Afstem faktura"
          confirmKind="danger"
          noteLabel="Bankreference"
          notePlaceholder="Referencen på banktransaktionen"
          onConfirm={async (reference) => {
            if (!reference.trim()) {
              throw {
                code: "bad_request",
                message: "Angiv referencen på bankbetalingen.",
              };
            }
            await api.settleInvoice(slug, {
              invoiceDocumentId: settling.documentId,
              bankTransactionReference: reference.trim(),
            });
            state.reload();
          }}
          onClose={() => setSettling(null)}
        />
      )}

      {/* #412: the Krediter ConfirmDialog. A credit note appends a reversing
          journal entry (and a new credit-note document), so the action is
          write-irreversible. A begrundelse is required for the audit trail —
          a blank value blocks the call before it reaches the server. */}
      {crediting && (
        <ConfirmDialog
          title="Udsted kreditnota"
          body={
            <p>
              Krediter faktura <strong>{crediting.invoiceNo}</strong>. En
              kreditnota bogføres som modgående postering med eget nummer fra
              kreditnota-serien. Handlingen kan ikke fortrydes og kræver en
              begrundelse til revisionssporet.
            </p>
          }
          confirmLabel="Udsted kreditnota"
          confirmKind="danger"
          noteLabel="Begrundelse"
          notePlaceholder="Hvorfor krediteres fakturaen?"
          onConfirm={async (reason) => {
            if (!reason.trim()) {
              throw {
                code: "bad_request",
                message:
                  "Angiv en begrundelse for kreditnotaen — den indgår i revisionssporet.",
              };
            }
            await api.creditInvoice(slug, {
              invoiceDocumentId: crediting.documentId,
              issueDate: new Date().toISOString().slice(0, 10),
              reason: reason.trim(),
            });
            state.reload();
          }}
          onClose={() => setCrediting(null)}
        />
      )}

      {inv.archived ? (
        <ArchivedNotice year={inv.selectedYear} />
      ) : inv.invoices.length === 0 ? (
        <div className="card archived-notice">
          <h3>Ingen fakturaer endnu</h3>
          <p className="muted">
            Der er ikke udstedt salgsfakturaer i regnskabsåret{" "}
            {inv.selectedYear}. Udstedte fakturaer vises her, så snart de er
            bogført. Brug <em>Udsted faktura</em> for at lave en ny.
          </p>
        </div>
      ) : (
        <>
          <div className="status-grid invoices-summary">
            <div className="card status-card">
              <h3>Faktureret i alt</h3>
              <div className="status-figure">
                {formatKroner(inv.totalGross, currency)}
              </div>
              <p className="muted status-note">
                {inv.invoices.length}{" "}
                {inv.invoices.length === 1 ? "faktura" : "fakturaer"} i{" "}
                {inv.selectedYear}
              </p>
            </div>
            <div className="card status-card">
              <h3>Udestående</h3>
              <div
                className={`status-figure${
                  inv.totalOpen > 0 ? " status-alert" : ""
                }`}
              >
                {formatKroner(inv.totalOpen, currency)}
              </div>
              <p className="muted status-note">
                {inv.overdueCount > 0
                  ? `${inv.overdueCount} forfalden${
                      inv.overdueCount === 1 ? "" : "e"
                    }`
                  : "Ingen forfaldne fakturaer"}
              </p>
            </div>
          </div>

          <div className="card statement-card table-scroll">
            <table className="data statement-table">
              <thead>
                <tr>
                  <th>Fakturanr.</th>
                  <th>Kunde</th>
                  <th>Dato</th>
                  <th>Forfald</th>
                  <th className="num">Beløb inkl. moms</th>
                  <th className="num">Udestående</th>
                  <th>Status</th>
                  <th>Handlinger</th>
                </tr>
              </thead>
              <tbody>
                {inv.invoices.map((row) => {
                  const meta = STATUS_META[row.status];
                  // Settlement only makes sense while a balance is open.
                  const canSettle = row.openBalance > 0;
                  // #412: Krediter is offered for any posted invoice that has
                  // not already been written off / refunded / fully credited.
                  // A partial credit reduces the open balance but leaves the
                  // source invoice in its open/paid/overdue state, so those
                  // remain creditable until the core refuses on "already fully
                  // credited" (mapped to a 409 by the mutation pipeline).
                  const canCredit =
                    row.status !== "credited" &&
                    row.status !== "refunded" &&
                    row.status !== "written_off";
                  return (
                    <tr key={row.documentId}>
                      <td className="account-no">{row.invoiceNo}</td>
                      <td>{row.customerName ?? "—"}</td>
                      <td className="entry-date">{row.invoiceDate ?? "—"}</td>
                      <td className="entry-date">
                        {row.effectiveDueDate ?? "—"}
                      </td>
                      <td className="num">
                        {formatKroner(row.grossAmount, currency)}
                      </td>
                      <td className="num">
                        {row.openBalance > 0
                          ? formatKroner(row.openBalance, currency)
                          : "—"}
                      </td>
                      <td>
                        <span className={`flag ${meta.tone}`}>
                          {meta.label}
                          {row.status === "overdue" && row.overdueDays > 0
                            ? ` · ${row.overdueDays} dage`
                            : ""}
                        </span>
                      </td>
                      <td>
                        <div className="row-actions">
                          {/* #378: the PDF link is the primary action — the
                              whole point of issuing an invoice is to send it
                              to the customer. `target="_blank"` so the browser
                              opens it inline without losing the table view. */}
                          <a
                            className="btn secondary"
                            href={api.invoicePdfUrl(slug, row.documentId)}
                            target="_blank"
                            rel="noopener"
                          >
                            Hent PDF
                          </a>
                          {canSettle && (
                            <button
                              type="button"
                              className="btn secondary"
                              onClick={() => setSettling(row)}
                            >
                              Afstem
                            </button>
                          )}
                          {/* #412: per-row Krediter button. The action is
                              hidden for an archived (read-only) year — every
                              write-action in this view is — and for rows
                              already credited/refunded/written off. */}
                          {!inv.archived && canCredit && (
                            <button
                              type="button"
                              className="btn secondary"
                              onClick={() => setCrediting(row)}
                            >
                              Krediter
                            </button>
                          )}
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </>
      )}
    </section>
  );
}

function ArchivedNotice({ year }: { year: string }) {
  return (
    <div className="card archived-notice">
      <h3>Fakturaer er ikke tilgængelige for {year}</h3>
      <p className="muted">
        {year} er et arkiveret regnskabsår. Udstedte fakturaer føres kun i den
        aktive ledger og vises derfor ikke for et arkiveret år.
        Resultatopgørelse, balance, saldobalance og posteringer for {year} er
        tilgængelige.
      </p>
    </div>
  );
}
