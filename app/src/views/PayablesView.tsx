// Leverandørfaktura-arbejdsbordet (#340) — the cockpit's payable workbench.
//
// Renders `/api/companies/:slug/payables`: the kreditorliste from
// `core/payables.ts#buildPayablesList` plus the picker rows the
// "Registrér leverandørfaktura"-modal needs. Summary cards above the table
// give the total open balance, the overdue portion and the not-yet-due
// portion; status filter pills switch between Åbne / Forfaldne / Betalte /
// Alle.
//
// Two write actions live on this view:
//   - "Registrér leverandørfaktura" (page action) opens the
//     `PayableRegisterModal`, turning an ingested bilag into a kreditorpost.
//   - per row, "Markér betalt" opens a `ConfirmDialog` that takes a bank
//     transaction id and runs `api.payPayable` — the same `payPayableFromBank`
//     core function the CLI's `payable pay` command uses.
//
// All write actions are write-irreversible and go through the same
// `withCompanyMutation` pipeline as every other cockpit write — confirm gate,
// backup lock and actor attribution included.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type {
  CompanyPayables,
  CompanyPayableRow,
  PayableListStatusFilter,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { PayableRegisterModal } from "../components/PayableRegisterModal";

const FILTERS: { value: PayableListStatusFilter; label: string }[] = [
  { value: "open", label: "Åbne" },
  { value: "overdue", label: "Forfaldne" },
  { value: "paid", label: "Betalte" },
  { value: "all", label: "Alle" },
];

const AGING_LABELS: Record<CompanyPayableRow["agingBucket"], string> = {
  "not-due": "Ikke forfalden",
  "0-30": "Forfalden 1–30 dage",
  "31-60": "Forfalden 31–60 dage",
  "61-90": "Forfalden 61–90 dage",
  "90+": "Forfalden > 90 dage",
};

export function PayablesView() {
  const { slug = "" } = useParams();
  const { setYear } = useCompanyYear();
  const [filter, setFilter] = useState<PayableListStatusFilter>("open");
  const state = useAsync<CompanyPayables>(
    () => api.payables(slug, filter),
    [slug, filter],
  );
  const [registering, setRegistering] = useState(false);
  const [paying, setPaying] = useState<CompanyPayableRow | null>(null);

  if (state.loading && !state.data) {
    return <Loading label="Henter leverandørfakturaer…" />;
  }
  if (state.error) {
    return <ErrorState message={state.error} onRetry={state.reload} />;
  }

  const view = state.data!;
  const currency = view.company.currency || "DKK";

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{view.company.name}</h2>
          <p className="muted">
            {view.company.cvr ? `CVR ${view.company.cvr} · ` : ""}
            {view.company.country} · {currency} · Leverandørfakturaer
          </p>
        </div>
        <div className="row-actions">
          <button
            type="button"
            className="btn"
            onClick={() => setRegistering(true)}
          >
            Registrér leverandørfaktura
          </button>
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

      <CompanyNav
        slug={slug}
        years={view.fiscalYears}
        // Payables are not bound to a fiscal year — show the first available
        // label for context; switching just rewrites the URL `?year=` param.
        selectedYear={view.fiscalYears[0]?.label ?? ""}
        onYearChange={setYear}
      />

      {registering && (
        <PayableRegisterModal
          slug={slug}
          payables={view}
          onRegistered={state.reload}
          onClose={() => setRegistering(false)}
        />
      )}

      {paying && (
        <ConfirmDialog
          title="Markér leverandørfaktura betalt"
          body={
            <p>
              Markér <strong>#{paying.payableId}</strong>{" "}
              {paying.supplierName ? `til ${paying.supplierName}` : ""} betalt
              ved at angive id'et på den udgående banktransaktion. Posten
              lægger en afregningspostering (debet Leverandørgæld, kredit
              bank) og kan ikke fortrydes.
            </p>
          }
          confirmLabel="Markér betalt"
          confirmKind="danger"
          noteLabel="Banktransaktions-id"
          notePlaceholder="Det numeriske id på banklinjen"
          onConfirm={async (raw) => {
            const bankTransactionId = Number(raw.trim());
            if (
              !Number.isInteger(bankTransactionId) ||
              bankTransactionId <= 0
            ) {
              throw {
                code: "bad_request",
                message:
                  "Angiv det numeriske id på den udgående banktransaktion.",
              };
            }
            await api.payPayable(slug, {
              payableId: paying.payableId,
              bankTransactionId,
            });
            state.reload();
          }}
          onClose={() => setPaying(null)}
        />
      )}

      <div className="status-grid invoices-summary">
        <div className="card status-card">
          <h3>Skyldig i alt</h3>
          <div
            className={`status-figure${
              view.totalOpenBalance > 0 ? " status-alert" : ""
            }`}
          >
            {formatKroner(view.totalOpenBalance, currency)}
          </div>
          <p className="muted status-note">
            Pr. {view.asOfDate} · {view.count}{" "}
            {view.count === 1 ? "post" : "poster"}
          </p>
        </div>
        <div className="card status-card">
          <h3>Forfaldne</h3>
          <div
            className={`status-figure${
              view.overdueOpenBalance > 0 ? " status-alert" : ""
            }`}
          >
            {formatKroner(view.overdueOpenBalance, currency)}
          </div>
          <p className="muted status-note">
            {view.overdueOpenBalance > 0
              ? "Betal eller indgå aftale snart"
              : "Ingen forfaldne kreditorposter"}
          </p>
        </div>
        <div className="card status-card">
          <h3>Ikke forfaldne</h3>
          <div className="status-figure">
            {formatKroner(view.notYetDueOpenBalance, currency)}
          </div>
          <p className="muted status-note">Skyldig, men ikke endnu forfalden</p>
        </div>
      </div>

      <nav
        className="filter-pills"
        aria-label="Filtrér leverandørfakturaer på status"
      >
        {FILTERS.map((f) => (
          <button
            key={f.value}
            type="button"
            className={`btn pill${filter === f.value ? " active" : ""}`}
            onClick={() => setFilter(f.value)}
            aria-pressed={filter === f.value}
          >
            {f.label}
          </button>
        ))}
      </nav>

      {view.rows.length === 0 ? (
        <div className="card statement-card empty-state">
          <h3>Ingen leverandørfakturaer i visningen</h3>
          <p className="muted">
            {filter === "open"
              ? "Der er ingen åbne kreditorposter at handle på. Brug \"Registrér leverandørfaktura\" når en ny købsfaktura er læst ind på Bilag-siden."
              : filter === "overdue"
                ? "Ingen forfaldne kreditorposter — godt arbejde."
                : filter === "paid"
                  ? "Ingen betalte kreditorposter endnu."
                  : "Ingen kreditorposter er registreret endnu."}
          </p>
        </div>
      ) : (
        <div className="card statement-card table-scroll">
          <table className="data statement-table">
            <thead>
              <tr>
                <th>Leverandør</th>
                <th>Bilag</th>
                <th>Bilagsdato</th>
                <th>Forfald</th>
                <th className="num">Brutto</th>
                <th className="num">Betalt</th>
                <th className="num">Åben</th>
                <th>Status</th>
                <th>Handlinger</th>
              </tr>
            </thead>
            <tbody>
              {view.rows.map((row) => {
                const tone = row.status === "paid"
                  ? "ok"
                  : row.isOverdue
                    ? "critical"
                    : "neutral";
                const label = row.status === "paid"
                  ? "Betalt"
                  : row.isOverdue
                    ? `Forfalden · ${row.overdueDays} dage`
                    : "Bogført";
                const canPay = row.status === "open" && row.openBalance > 0;
                return (
                  <tr key={row.payableId}>
                    <td>{row.supplierName ?? "—"}</td>
                    <td className="account-no">
                      {row.billNo ?? `#${row.documentId}`}
                    </td>
                    <td className="entry-date">{row.billDate}</td>
                    <td className="entry-date">{row.dueDate}</td>
                    <td className="num">
                      {formatKroner(row.grossAmount, currency)}
                    </td>
                    <td className="num">
                      {row.paidAmount > 0
                        ? formatKroner(row.paidAmount, currency)
                        : "—"}
                    </td>
                    <td className="num">
                      {row.openBalance > 0
                        ? formatKroner(row.openBalance, currency)
                        : "—"}
                    </td>
                    <td>
                      <span className={`flag ${tone}`} title={AGING_LABELS[row.agingBucket]}>
                        {label}
                      </span>
                    </td>
                    <td>
                      <div className="row-actions">
                        {canPay && (
                          <button
                            type="button"
                            className="btn secondary"
                            onClick={() => setPaying(row)}
                          >
                            Markér betalt
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
      )}
    </section>
  );
}
