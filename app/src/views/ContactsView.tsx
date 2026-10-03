import * as stylex from "@stylexjs/stylex";
import { ButtonLink, Button, Input, Select, FilterBar, Pagination, PageHeader } from "../components/ui";
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
import { useParams, useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { formatKroner } from "../lib/format";
import type {
  CompanyContacts,
  ContactCustomerRow,
  ContactVendorRow,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ImportModal } from "../components/ImportModal";
import { ConfirmDialog } from "../components/ConfirmDialog";
import {
  ContactFormModal,
  type ContactKind,
} from "../components/ContactFormModal";

/** VAT-treatment codes from the ledger, mapped to a Danish label. */
const VAT_TREATMENT_LABELS: Record<string, string> = {
  standard: "Standardmoms",
  domestic_reverse_charge: "Omvendt betalingspligt (DK)",
  foreign_reverse_charge: "Omvendt betalingspligt (EU-tjenester)",
  exempt: "Momsfritaget",
};

/** Local UI state when the create/edit modal is open. */
type ModalState =
  | { kind: "customer"; row?: ContactCustomerRow }
  | { kind: "vendor"; row?: ContactVendorRow };

/**
 * #430 — pending delete-bekræftelse. Når den er sat, viser cockpittet en
 * `ConfirmDialog` med en menneske-læselig beskrivelse af konsekvenserne;
 * `onConfirm` kalder `api.deleteCustomer` / `api.deleteVendor` som server-
 * side blokerer hvis kontakten er i brug på en åben faktura/gæld.
 */
type DeleteState =
  | { kind: "customer"; row: ContactCustomerRow }
  | { kind: "vendor"; row: ContactVendorRow };

export function ContactsView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const state = useAsync<CompanyContacts>(
    (signal) => api.contacts(slug, { signal }),
    [slug],
  );
  const [params, setParams] = useSearchParams();
  const query = params.get("q") ?? "";
  const kindFilter = params.get("kind") ?? "all";
  function setContactFilter(key: "q" | "kind", value: string) {
    const next = new URLSearchParams(params);
    if (value && value !== "all") next.set(key, value); else next.delete(key);
    next.delete("page"); setParams(next, { replace: true });
  }
  function changePage(value: number, size: number) {
    const next = new URLSearchParams(params); next.set("page", String(value)); next.set("pageSize", String(size)); setParams(next, { replace: true });
  }
  // True while the generic file-import modal is open.
  const [importing, setImporting] = useState(false);
  // The create/edit modal — undefined when closed.
  const [modal, setModal] = useState<ModalState | undefined>(undefined);
  // #430 — pending delete-bekræftelse (kunde eller leverandør). Undefined når
  // ingen dialog er åben.
  const [pendingDelete, setPendingDelete] = useState<DeleteState | undefined>(
    undefined,
  );

  if (state.loading && !state.data)
    return <Loading label="Henter kontakter…" />;
  if (state.error && !state.data)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const c = state.data!;
  const currency = c.company.currency || "DKK";
  const selectedYear =
    year ??
    c.fiscalYears.find((y) => y.source === "live")?.label ??
    c.fiscalYears[0]?.label ??
    String(new Date().getFullYear());
  const total = c.customers.length + c.vendors.length;
  const needle = query.trim().toLocaleLowerCase("da");
  const matches = (row: { name: string; vatOrCvr: string | null; email?: string | null }) => !needle || [row.name, row.vatOrCvr, row.email].some(value => value?.toLocaleLowerCase("da").includes(needle));
  const customers = kindFilter === "vendors" ? [] : c.customers.filter(matches).sort((a, b) => a.name.localeCompare(b.name, "da"));
  const vendors = kindFilter === "customers" ? [] : c.vendors.filter(matches).sort((a, b) => a.name.localeCompare(b.name, "da"));
  const filteredTotal = customers.length + vendors.length;
  const requestedSize = Number(params.get("pageSize"));
  const pageSize = [25, 50, 100].includes(requestedSize) ? requestedSize : 50;
  const requestedPage = Number(params.get("page"));
  const page = Math.min(Math.max(1, Number.isSafeInteger(requestedPage) ? requestedPage : 1), Math.max(1, Math.ceil(filteredTotal / pageSize)));
  const first = (page - 1) * pageSize;
  const pageCustomers = customers.slice(first, first + pageSize);
  const pageVendors = vendors.slice(Math.max(0, first - customers.length), Math.max(0, first + pageSize - customers.length));


  function openCreate(kind: ContactKind) {
    setModal({ kind } as ModalState);
  }

  function openEditCustomer(row: ContactCustomerRow) {
    setModal({ kind: "customer", row });
  }

  function openEditVendor(row: ContactVendorRow) {
    setModal({ kind: "vendor", row });
  }

  function openDeleteCustomer(row: ContactCustomerRow) {
    setPendingDelete({ kind: "customer", row });
  }

  function openDeleteVendor(row: ContactVendorRow) {
    setPendingDelete({ kind: "vendor", row });
  }

  return (
    <section className="statement">
      {state.error && <div className="banner warning" role="alert">Status kunne ikke opdateres. Din formular er bevaret; oplysningerne bag den er fra den seneste gennemførte læsning.</div>}
      <PageHeader title="Kontakter" actions={<><div className="row-actions">
          <Button requiredPermission="company.master-data"
            type="button"
            className="btn"
            onClick={() => openCreate("customer")}
          >
            Tilføj kunde
          </Button>
          <Button requiredPermission="company.master-data"
            type="button"
            className="btn"
            onClick={() => openCreate("vendor")}
          >
            Tilføj leverandør
          </Button>
          <Button requiredPermission="company.ledger.post" variant="secondary"
            type="button"
            className="btn secondary"
            onClick={() => setImporting(true)}
          >
            Importér
          </Button>
          <ButtonLink className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div>

          <p className="muted">
            {c.company.cvr ? `CVR ${c.company.cvr} · ` : ""}
            {c.company.country} · {currency} · Kontakter
          </p>
        </div>

      </PageHeader>

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

      {pendingDelete && (
        <ConfirmDialog
          title={
            pendingDelete.kind === "customer"
              ? `Slet kunde ${pendingDelete.row.name}?`
              : `Slet leverandør ${pendingDelete.row.name}?`
          }
          body={
            <>
              <p>
                {pendingDelete.kind === "customer"
                  ? "Kunden fjernes fra dine fremtidige fakturaer og dropdowns."
                  : "Leverandøren fjernes fra dine fremtidige bilag og dropdowns."}
              </p>
              <p>
                Allerede bogførte fakturaer og posteringer beholder
                navnet som det var på bogføringstidspunktet — historikken
                og revisor-eksporten er ikke påvirket.
              </p>
              <p className="muted">
                {pendingDelete.kind === "customer"
                  ? "Hvis kunden er i brug på en åben (ikke-betalt) faktura, bliver sletningen blokeret med et henvisning til fakturanummeret."
                  : "Hvis leverandøren har en åben gæld der ikke er betalt endnu, bliver sletningen blokeret med en henvisning til regningen."}
              </p>
            </>
          }
          confirmLabel="Slet"
          confirmKind="danger"
          onConfirm={async () => {
            if (pendingDelete.kind === "customer") {
              await api.deleteCustomer(slug, pendingDelete.row.id);
            } else {
              await api.deleteVendor(slug, pendingDelete.row.id);
            }
            // Reload so the deleted row disappears immediately.
            state.reload();
          }}
          onClose={() => setPendingDelete(undefined)} onRefresh={state.reload}
        />
      )}

      <FilterBar activeCount={Number(Boolean(query)) + Number(kindFilter !== "all")} onReset={() => { const next = new URLSearchParams(params); next.delete("q"); next.delete("kind"); next.delete("page"); setParams(next, { replace: true }); }}>
        <label>Søg kontakter<Input type="search" value={query} onChange={event => setContactFilter("q", event.target.value)} placeholder="Navn, CVR eller e-mail" /></label>
        <label>Kontakttype<Select value={kindFilter} onChange={event => setContactFilter("kind", event.target.value)}><option value="all">Alle kontakter</option><option value="customers">Kunder</option><option value="vendors">Leverandører</option></Select></label>
      </FilterBar>
      {total > 0 && filteredTotal === 0 && <p role="status">Ingen kontakter matcher filtrene.</p>}
      {total === 0 ? (
        <div className="card archived-notice">
          <h3>Ingen kontakter endnu</h3>
          <p className="muted">
            Der er ingen registrerede kunder eller leverandører for denne
            virksomhed. Brug «Tilføj kunde» eller «Tilføj leverandør» ovenfor
            for at oprette stamdata — eller «Importér» til at hente kontakter
            fra et tidligere bogføringssystem.
          </p>
          <div className={["row-actions", stylex.props(viewStyles.site0).className].filter(Boolean).join(" ")} >
            <Button requiredPermission="company.master-data"
              type="button"
              className="btn"
              onClick={() => openCreate("customer")}
            >
              Tilføj kunde
            </Button>
            <Button requiredPermission="company.master-data"
              type="button"
              className="btn"
              onClick={() => openCreate("vendor")}
            >
              Tilføj leverandør
            </Button>
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
            {pageCustomers.length === 0 && customers.length > 0 ? <p className="muted">Kunderne vises på en anden side.</p> : <CustomerTable
              customers={pageCustomers}
              onEdit={openEditCustomer}
              onDelete={openDeleteCustomer}
            />}
          </div>

          <div className="section">
            <h3>Leverandører</h3>
            {pageVendors.length === 0 && vendors.length > 0 ? <p className="muted">Leverandørerne vises på en anden side.</p> : <VendorTable
              vendors={pageVendors}
              onEdit={openEditVendor}
              onDelete={openDeleteVendor}
            />}
          </div>
        </>
      )}
      {total > 0 && <Pagination total={filteredTotal} page={page} pageSize={pageSize} onPageChange={value => changePage(value, pageSize)} onPageSizeChange={size => changePage(1, size)} />}
    </section>
  );
}

function CustomerTable({
  customers,
  onEdit,
  onDelete,
}: {
  customers: ContactCustomerRow[];
  onEdit: (row: ContactCustomerRow) => void;
  onDelete: (row: ContactCustomerRow) => void;
}) {
  return (
    <div className="card statement-card table-scroll daily-list">
      <table role="table" className="data statement-table">
        <thead>
          <tr role="row">
            <th role="columnheader">Navn</th>
            <th role="columnheader">CVR / moms-nr.</th>
            <th role="columnheader">E-mail</th>
            <th role="columnheader">Valuta</th>
            <th role="columnheader" className="num">Betalingsfrist</th>
            <th role="columnheader" className="num">Udestående</th>
            <th aria-label="Handlinger" />
          </tr>
        </thead>
        <tbody>
          {customers.length === 0 ? (
            <tr role="row">
              <td role="cell" colSpan={7} className="empty-inline">
                Ingen kunder registreret.
              </td>
            </tr>
          ) : (
            customers.map((row) => (
              <tr role="row" key={row.id}>
                <td data-label="Navn" role="cell">{row.name}</td>
                <td data-label="CVR / moms-nr." role="cell" className="account-no">{row.vatOrCvr ?? "—"}</td>
                <td data-label="E-mail" role="cell">{row.email ?? "—"}</td>
                <td data-label="Valuta" role="cell">{row.defaultCurrency}</td>
                <td data-label="Betalingsfrist" role="cell" className="num">{row.paymentTermsDays} dage</td>
                <td data-label="Udestående" role="cell" className="num">
                  {row.openInvoiceCount === 0 ? (
                    <span className="muted">—</span>
                  ) : (
                    <>
                      <span
                        className={
                          row.overdueCount > 0 ? "status-alert" : undefined
                        }
                      >
                        {formatKroner(row.openBalance, row.defaultCurrency)}
                      </span>
                      {row.overdueCount > 0 && (
                        <>
                          {" "}
                          <span className="flag critical">
                            {row.overdueCount} forfalden
                            {row.overdueCount === 1 ? "" : "e"}
                          </span>
                        </>
                      )}
                    </>
                  )}
                </td>
                <td data-label="Oplysning" role="cell" className="num row-actions">
                  <Button requiredPermission="company.master-data" variant="secondary"
                    type="button"
                    className="btn secondary"
                    onClick={() => onEdit(row)}
                    aria-label={`Redigér ${row.name}`}
                  >
                    Redigér
                  </Button>
                  <Button requiredPermission="company.master-data" variant="danger"
                    type="button"
                    className="btn danger"
                    onClick={() => onDelete(row)}
                    aria-label={`Slet ${row.name}`}
                  >
                    Slet
                  </Button>
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
  onDelete,
}: {
  vendors: ContactVendorRow[];
  onEdit: (row: ContactVendorRow) => void;
  onDelete: (row: ContactVendorRow) => void;
}) {
  return (
    <div className="card statement-card table-scroll daily-list">
      <table role="table" className="data statement-table">
        <thead>
          <tr role="row">
            <th role="columnheader">Navn</th>
            <th role="columnheader">CVR / moms-nr.</th>
            <th role="columnheader">Standard udgiftskonto</th>
            <th role="columnheader">Momsbehandling</th>
            <th aria-label="Handlinger" />
          </tr>
        </thead>
        <tbody>
          {vendors.length === 0 ? (
            <tr role="row">
              <td role="cell" colSpan={5} className="empty-inline">
                Ingen leverandører registreret.
              </td>
            </tr>
          ) : (
            vendors.map((row) => (
              <tr role="row" key={row.id}>
                <td data-label="Navn" role="cell">{row.name}</td>
                <td data-label="CVR / moms-nr." role="cell" className="account-no">{row.vatOrCvr ?? "—"}</td>
                <td data-label="Standard udgiftskonto" role="cell" className="account-no">
                  {row.defaultExpenseAccount ?? "—"}
                </td>
                <td data-label="Momsbehandling" role="cell">
                  {row.defaultVatTreatment
                    ? VAT_TREATMENT_LABELS[row.defaultVatTreatment] ??
                      row.defaultVatTreatment
                    : "—"}
                </td>
                <td data-label="Oplysning" role="cell" className="num row-actions">
                  <Button requiredPermission="company.master-data" variant="secondary"
                    type="button"
                    className="btn secondary"
                    onClick={() => onEdit(row)}
                    aria-label={`Redigér ${row.name}`}
                  >
                    Redigér
                  </Button>
                  <Button requiredPermission="company.master-data" variant="danger"
                    type="button"
                    className="btn danger"
                    onClick={() => onDelete(row)}
                    aria-label={`Slet ${row.name}`}
                  >
                    Slet
                  </Button>
                </td>
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}

const viewStyles = stylex.create({
site0: { marginTop: "1rem" }
});
