// Kontakter — the per-company customers and vendors (cockpit-redesign it. 5).
//
// Renders `/api/companies/:slug/contacts`: the master data — customers (kunder)
// and vendors (leverandører) — each in its own table with the key figures the
// ledger keys off (CVR, betalingsbetingelser, standardkonto). Contacts are not
// year-scoped, but the company sub-nav still carries the selected `?year=` so
// it follows the user across views — the fiscal years for the selector are
// fetched from the response. A company with no contacts shows a graceful
// empty state.
//
// #390: the page is now ALSO the daily-maintenance surface. The page-head
// exposes a primary "Tilføj kunde" + "Tilføj leverandør" action; each row in
// either table is clickable and opens the same modal in edit-mode. The
// Importér button remains for one-off CSV migrations.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type {
  CompanyContacts,
  ContactCustomerRow,
  ContactVendorRow,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ImportModal } from "../components/ImportModal";
import {
  ContactFormModal,
  type ContactKind,
} from "../components/ContactFormModal";

/** VAT-treatment codes from the ledger, mapped to a Danish label. */
const VAT_TREATMENT_LABELS: Record<string, string> = {
  standard: "Standardmoms",
  domestic_reverse_charge: "Omvendt betalingspligt (DK)",
  foreign_reverse_charge: "Omvendt betalingspligt (udland)",
  exempt: "Momsfritaget",
};

/** Local UI state when the create/edit modal is open. */
type ModalState =
  | { kind: "customer"; row?: ContactCustomerRow }
  | { kind: "vendor"; row?: ContactVendorRow };

export function ContactsView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const state = useAsync<CompanyContacts>(
    () => api.contacts(slug),
    [slug],
  );
  // True while the generic file-import modal is open.
  const [importing, setImporting] = useState(false);
  // The create/edit modal — undefined when closed.
  const [modal, setModal] = useState<ModalState | undefined>(undefined);

  if (state.loading && !state.data)
    return <Loading label="Henter kontakter…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const c = state.data!;
  const currency = c.company.currency || "DKK";
  const selectedYear =
    year ??
    c.fiscalYears.find((y) => y.source === "live")?.label ??
    c.fiscalYears[0]?.label ??
    String(new Date().getFullYear());
  const total = c.customers.length + c.vendors.length;

  function openCreate(kind: ContactKind) {
    setModal({ kind } as ModalState);
  }

  function openEditCustomer(row: ContactCustomerRow) {
    setModal({ kind: "customer", row });
  }

  function openEditVendor(row: ContactVendorRow) {
    setModal({ kind: "vendor", row });
  }

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{c.company.name}</h2>
          <p className="muted">
            {c.company.cvr ? `CVR ${c.company.cvr} · ` : ""}
            {c.company.country} · {currency} · Kontakter
          </p>
        </div>
        <div className="row-actions">
          <button
            type="button"
            className="btn"
            onClick={() => openCreate("customer")}
          >
            Tilføj kunde
          </button>
          <button
            type="button"
            className="btn"
            onClick={() => openCreate("vendor")}
          >
            Tilføj leverandør
          </button>
          <button
            type="button"
            className="btn secondary"
            onClick={() => setImporting(true)}
          >
            Importér
          </button>
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

      <CompanyNav
        slug={slug}
        years={c.fiscalYears}
        selectedYear={selectedYear}
        onYearChange={setYear}
      />

      {importing && (
        <ImportModal
          slug={slug}
          onImported={state.reload}
          onClose={() => setImporting(false)}
        />
      )}

      {modal && (
        <ContactFormModal
          slug={slug}
          kind={modal.kind}
          customer={modal.kind === "customer" ? modal.row : undefined}
          vendor={modal.kind === "vendor" ? modal.row : undefined}
          onSaved={state.reload}
          onClose={() => setModal(undefined)}
        />
      )}

      {total === 0 ? (
        <div className="card archived-notice">
          <h3>Ingen kontakter endnu</h3>
          <p className="muted">
            Der er ingen registrerede kunder eller leverandører for denne
            virksomhed. Brug «Tilføj kunde» eller «Tilføj leverandør» ovenfor
            for at oprette stamdata — eller «Importér» til at hente kontakter
            fra et tidligere bogføringssystem.
          </p>
          <div className="row-actions" style={{ marginTop: "1rem" }}>
            <button
              type="button"
              className="btn"
              onClick={() => openCreate("customer")}
            >
              Tilføj kunde
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => openCreate("vendor")}
            >
              Tilføj leverandør
            </button>
          </div>
        </div>
      ) : (
        <>
          <p className="statement-asof muted">
            {c.customers.length}{" "}
            {c.customers.length === 1 ? "kunde" : "kunder"} ·{" "}
            {c.vendors.length}{" "}
            {c.vendors.length === 1 ? "leverandør" : "leverandører"}
          </p>

          <div className="section">
            <h3>Kunder</h3>
            <CustomerTable
              customers={c.customers}
              onEdit={openEditCustomer}
            />
          </div>

          <div className="section">
            <h3>Leverandører</h3>
            <VendorTable vendors={c.vendors} onEdit={openEditVendor} />
          </div>
        </>
      )}
    </section>
  );
}

function CustomerTable({
  customers,
  onEdit,
}: {
  customers: ContactCustomerRow[];
  onEdit: (row: ContactCustomerRow) => void;
}) {
  return (
    <div className="card statement-card table-scroll">
      <table className="data statement-table">
        <thead>
          <tr>
            <th>Navn</th>
            <th>CVR / moms-nr.</th>
            <th>E-mail</th>
            <th>Valuta</th>
            <th className="num">Betalingsfrist</th>
            <th aria-label="Handlinger" />
          </tr>
        </thead>
        <tbody>
          {customers.length === 0 ? (
            <tr>
              <td colSpan={6} className="empty-inline">
                Ingen kunder registreret.
              </td>
            </tr>
          ) : (
            customers.map((row) => (
              <tr key={row.id}>
                <td>{row.name}</td>
                <td className="account-no">{row.vatOrCvr ?? "—"}</td>
                <td>{row.email ?? "—"}</td>
                <td>{row.defaultCurrency}</td>
                <td className="num">{row.paymentTermsDays} dage</td>
                <td className="num">
                  <button
                    type="button"
                    className="btn secondary"
                    onClick={() => onEdit(row)}
                    aria-label={`Redigér ${row.name}`}
                  >
                    Redigér
                  </button>
                </td>
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}

function VendorTable({
  vendors,
  onEdit,
}: {
  vendors: ContactVendorRow[];
  onEdit: (row: ContactVendorRow) => void;
}) {
  return (
    <div className="card statement-card table-scroll">
      <table className="data statement-table">
        <thead>
          <tr>
            <th>Navn</th>
            <th>CVR / moms-nr.</th>
            <th>Standard udgiftskonto</th>
            <th>Momsbehandling</th>
            <th aria-label="Handlinger" />
          </tr>
        </thead>
        <tbody>
          {vendors.length === 0 ? (
            <tr>
              <td colSpan={5} className="empty-inline">
                Ingen leverandører registreret.
              </td>
            </tr>
          ) : (
            vendors.map((row) => (
              <tr key={row.id}>
                <td>{row.name}</td>
                <td className="account-no">{row.vatOrCvr ?? "—"}</td>
                <td className="account-no">
                  {row.defaultExpenseAccount ?? "—"}
                </td>
                <td>
                  {row.defaultVatTreatment
                    ? VAT_TREATMENT_LABELS[row.defaultVatTreatment] ??
                      row.defaultVatTreatment
                    : "—"}
                </td>
                <td className="num">
                  <button
                    type="button"
                    className="btn secondary"
                    onClick={() => onEdit(row)}
                    aria-label={`Redigér ${row.name}`}
                  >
                    Redigér
                  </button>
                </td>
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}
