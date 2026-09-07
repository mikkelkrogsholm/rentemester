// Kontoplan view (#344) — read-only per-virksomhed liste over alle konti
// (nummer, navn, type, normal-saldo, evt. moms-mapping) med søg + filter pr.
// type, og en kort summary pr. type. Genbruger eksisterende
// /api/companies/:slug/accounts uden duplikeret core-logik.
//
// Note: åbningsbalance-flowet (CSV-import / pr. konto-indtastning) er ikke
// inkluderet i denne PR — det er et write-flow med actor + audit-event og
// følger som follow-up. Read-side er nu fuld.

import { useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type { AccountRole, AccountRoleResolution, AccountRow, CompanyAccounts } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";

const TYPE_LABELS: Record<string, string> = {
  asset: "Aktiv",
  liability: "Passiv",
  equity: "Egenkapital",
  income: "Indtægt",
  expense: "Omkostning",
  vat: "Moms",
};

const ACCOUNT_ROLE_LABELS: Record<AccountRole, string> = {
  bank: "Bankkonto",
  debtors: "Debitorer",
  creditors: "Kreditorer",
  output_vat: "Salgsmoms",
  input_vat: "Købsmoms",
  reverse_charge_vat: "Omvendt betalingspligt-moms",
  vat_settlement: "Momsafregning",
  operational_default: "Standard driftskonto",
};

const ACCOUNT_ROLE_STATUS_LABELS = {
  complete: "Komplet",
  incomplete: "Ufuldstændig",
  ambiguous: "Kræver menneskelig afklaring",
} as const;

export function AccountsView() {
  const { slug = "" } = useParams();
  const state = useAsync<CompanyAccounts>(() => api.accounts(slug), [slug]);
  const [typeFilter, setTypeFilter] = useState<string>("");
  const [search, setSearch] = useState("");

  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return (state.data?.accounts ?? []).filter((a) => {
      if (typeFilter !== "" && a.type !== typeFilter) return false;
      if (needle === "") return true;
      return (
        a.accountNo.toLowerCase().includes(needle) ||
        a.name.toLowerCase().includes(needle) ||
        (a.defaultVatCode ?? "").toLowerCase().includes(needle)
      );
    });
  }, [state.data, typeFilter, search]);

  if (state.loading) return <Loading />;
  if (state.error) return <ErrorState message={state.error} />;
  const data = state.data!;

  return (
    <section className="accounts-view">
      <header className="page-head">
        <div>
          <h2>{data.company.name}</h2>
          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · {data.company.currency} · Kontoplan
          </p>
        </div>
        <div className="row-actions">
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </header>

      <p className="muted">
        Kontoplanen hjælper dig med at vælge den rigtige konto, når du bogfører.
      </p>

      <section className="card">
        <h3>Gennemgå kontoplan</h3>
        <p>{data.accounts.length} konti er klar til at blive søgt og filtreret.</p>
      </section>

      <section className="card">
        <h3>Sammentælling pr. type</h3>
        <div className="filter-bar">
          {Object.entries(data.byType)
            .sort((a, b) => a[0].localeCompare(b[0]))
            .map(([type, count]) => (
              <button
                key={type}
                type="button"
                className={`btn small ${typeFilter === type ? "primary" : "secondary"}`}
                onClick={() =>
                  setTypeFilter(typeFilter === type ? "" : type)
                }
              >
                {TYPE_LABELS[type] ?? type}: {count}
              </button>
            ))}
          {typeFilter !== "" && (
            <button
              type="button"
              className="btn small secondary"
              onClick={() => setTypeFilter("")}
            >
              Nulstil filter
            </button>
          )}
        </div>
      </section>

      <details className="card">
        <summary>Avanceret: kontoroller og importgrundlag</summary>
        <AccountRolesCard accountRoles={data.accountRoles} />
      </details>

      <section className="card">
        <h3>
          Kontoplan ({filtered.length} af {data.accounts.length})
        </h3>
        <div className="filter-bar">
          <label>
            Søg{" "}
            <input
              type="search"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="kontonummer, navn, vat-kode …"
            />
          </label>
        </div>

        <table className="table">
          <thead>
            <tr>
              <th>Kontonr.</th>
              <th>Navn</th>
              <th>Type</th>
              <th>Normal saldo</th>
              <th>Default moms-kode</th>
              <th>Bogføringslinjer</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((a) => (
              <AccountListRow key={a.accountNo} row={a} />
            ))}
            {filtered.length === 0 && (
              <tr>
                <td colSpan={6} className="muted">
                  Ingen konti matcher filtret.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </section>
    </section>
  );
}

function AccountRolesCard({ accountRoles }: Pick<CompanyAccounts, "accountRoles">) {
  const needsHumanResolution = accountRoles.status !== "complete";
  return (
    <section className="card" aria-labelledby="account-role-heading">
      <h3 id="account-role-heading">Kontoroller</h3>
      <p className={needsHumanResolution ? "warning" : "ok"}>
        Status: {ACCOUNT_ROLE_STATUS_LABELS[accountRoles.status]}
      </p>
      {needsHumanResolution && (
        <p className="muted">
          {accountRoles.status === "ambiguous"
            ? "Flere mulige konti skal bekræftes af et menneske, før bogføring fortsætter."
            : "Manglende roller skal bekræftes af et menneske, før bogføring fortsætter."}
        </p>
      )}
      <table className="table">
        <thead>
          <tr>
            <th>Rolle</th>
            <th>Dry-run opløsning</th>
          </tr>
        </thead>
        <tbody>
          {accountRoles.resolutions.map((resolution) => (
            <AccountRoleResolutionRow key={resolution.role} resolution={resolution} />
          ))}
        </tbody>
      </table>
      {accountRoles.proposals.length > 0 && <p className="muted">Importforslag: {accountRoles.proposals.map((proposal) => `${ACCOUNT_ROLE_LABELS[proposal.role]} → ${proposal.accountNo} (${proposal.source})`).join(", ")}</p>}
      {accountRoles.reasons.length > 0 && <p className="warning">{accountRoles.reasons.map((reason) => `${ACCOUNT_ROLE_LABELS[reason.role]}: ${reason.reason}`).join(" · ")}</p>}
    </section>
  );
}

function AccountRoleResolutionRow({ resolution }: { resolution: AccountRoleResolution }) {
  return (
    <tr>
      <td>{ACCOUNT_ROLE_LABELS[resolution.role]}</td>
      <td className={resolution.ok ? "ok" : "warning"}>
        {resolution.ok
          ? <>Konto <code>{resolution.accountNo}</code> (version {resolution.version})</>
          : <>Kræver menneskelig afklaring: {resolution.error}</>}
      </td>
    </tr>
  );
}

function AccountListRow({ row }: { row: AccountRow }) {
  return (
    <tr>
      <td>
        <code>{row.accountNo}</code>
      </td>
      <td>{row.name}</td>
      <td>{TYPE_LABELS[row.type] ?? row.type}</td>
      <td>{row.normalBalance === "debit" ? "Debet" : "Kredit"}</td>
      <td>
        {row.defaultVatCode ? <code>{row.defaultVatCode}</code> : "—"}
      </td>
      <td className={row.hasPostings ? "ok" : "muted"}>
        {row.hasPostings ? "Ja" : "Nej"}
      </td>
    </tr>
  );
}
