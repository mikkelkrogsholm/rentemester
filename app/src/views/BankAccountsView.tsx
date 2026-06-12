// Bankkonti + CSV-mapping-profiler (#345).
//
// Per-virksomhed liste over registrerede bankkonti + de indbyggede
// CSV-mapping-profiler (Lunar, Danske Bank, Sydbank, …). 'Opret konto'-
// modal kalder POST /api/companies/:slug/bank-accounts.
//
// Note: pr.-konto mapping-override + sample-CSV-preview er parkeret som
// follow-up — read-side + create-flow er nu fuld. CSV-import ad hoc
// foregår fortsat via BankImportModal som genbruger profilerne.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type { BankAccount, CompanyBankAccounts } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";

export function BankAccountsView() {
  const { slug = "" } = useParams();
  const [refresh, setRefresh] = useState(0);
  const [openCreate, setOpenCreate] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const state = useAsync<CompanyBankAccounts>(
    () => api.bankAccounts(slug),
    [slug, refresh],
  );

  if (state.loading) return <Loading />;
  if (state.error) return <ErrorState message={state.error} />;
  const data = state.data!;

  return (
    <section className="bank-accounts-view">
      <header className="page-head">
        <div>
          <h2>{data.company.name}</h2>
          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · Bankkonti
          </p>
        </div>
        <div className="row-actions">
          <button
            type="button"
            className="btn primary"
            onClick={() => {
              setError(null);
              setOpenCreate(true);
            }}
          >
            Opret bankkonto …
          </button>
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </header>

      {error && (
        <div className="callout danger" role="alert">
          {error}
        </div>
      )}

      <section className="card">
        <h3>Registrerede bankkonti ({data.accounts.length})</h3>
        {data.accounts.length === 0 ? (
          <p className="muted">
            Ingen bankkonti endnu. Opret én for at kunne importere bank-CSV via{" "}
            <code>BankImportModal</code> eller CLI'ens <code>bank import</code>.
          </p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Navn</th>
                <th>Bank</th>
                <th>Reg.nr.</th>
                <th>Konto-nr.</th>
                <th>IBAN</th>
                <th>Valuta</th>
                <th>Ledger-konto</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {data.accounts.map((a) => (
                <BankAccountRow key={a.id} account={a} />
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="card">
        <h3>Indbyggede CSV-mapping-profiler ({data.profiles.length})</h3>
        <p className="muted">
          BankImportModal og CLI'ens <code>bank import --profile &lt;navn&gt;</code>
          genbruger disse hard-kodede mapping-profiler. Pr.-konto mapping-
          override er en follow-up.
        </p>
        <table className="table">
          <thead>
            <tr>
              <th>Profil-navn</th>
              <th>Bank</th>
              <th>Separator</th>
              <th>Encoding</th>
              <th>Dato-format</th>
            </tr>
          </thead>
          <tbody>
            {data.profiles.map((p) => (
              <tr key={p.name}>
                <td>
                  <code>{p.name}</code>
                </td>
                <td>{p.bankName ?? "—"}</td>
                <td>
                  <code>{p.separator ?? ";"}</code>
                </td>
                <td>{p.encoding ?? "utf-8"}</td>
                <td>{p.dateOrder ?? "dmy"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      {openCreate && (
        <CreateBankAccountModal
          slug={slug}
          onClose={() => setOpenCreate(false)}
          onDone={() => {
            setOpenCreate(false);
            setRefresh((n) => n + 1);
          }}
          onError={(msg) => setError(msg)}
        />
      )}
    </section>
  );
}

function BankAccountRow({ account }: { account: BankAccount }) {
  return (
    <tr>
      <td>{account.name}</td>
      <td>{account.bankName ?? "—"}</td>
      <td>{account.registrationNo ?? "—"}</td>
      <td>{account.accountNo ?? "—"}</td>
      <td>{account.iban ?? "—"}</td>
      <td>{account.currency}</td>
      <td>{account.ledgerAccountNo ?? "—"}</td>
      <td>
        <span className={`pill ${account.active ? "ok" : "warn"}`}>
          {account.active ? "Aktiv" : "Inaktiv"}
        </span>
      </td>
    </tr>
  );
}

function CreateBankAccountModal({
  slug,
  onClose,
  onDone,
  onError,
}: {
  slug: string;
  onClose: () => void;
  onDone: () => void;
  onError: (msg: string) => void;
}) {
  const [name, setName] = useState("");
  const [bankName, setBankName] = useState("");
  const [registrationNo, setRegistrationNo] = useState("");
  const [accountNo, setAccountNo] = useState("");
  const [iban, setIban] = useState("");
  const [currency, setCurrency] = useState("DKK");
  const [ledgerAccountNo, setLedgerAccountNo] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setSubmitting(true);
    try {
      await api.createBankAccount(slug, {
        name,
        ...(bankName ? { bankName } : {}),
        ...(registrationNo ? { registrationNo } : {}),
        ...(accountNo ? { accountNo } : {}),
        ...(iban ? { iban } : {}),
        ...(currency ? { currency } : {}),
        ...(ledgerAccountNo ? { ledgerAccountNo } : {}),
      });
      onDone();
    } catch (err) {
      onError(
        err instanceof ApiError ? err.message : "Oprettelse fejlede.",
      );
      setSubmitting(false);
    }
  };

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="modal">
        <h3>Opret bankkonto</h3>
        <form onSubmit={submit}>
          <label>
            Navn (påkrævet)
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              placeholder="fx 'Lunar driftskonto'"
            />
          </label>
          <label>
            Bank-navn
            <input
              type="text"
              value={bankName}
              onChange={(e) => setBankName(e.target.value)}
              placeholder="fx 'Lunar Bank'"
            />
          </label>
          <label>
            Reg.nr.
            <input
              type="text"
              value={registrationNo}
              onChange={(e) => setRegistrationNo(e.target.value)}
              placeholder="4-cifret"
            />
          </label>
          <label>
            Konto-nr.
            <input
              type="text"
              value={accountNo}
              onChange={(e) => setAccountNo(e.target.value)}
            />
          </label>
          <label>
            IBAN
            <input
              type="text"
              value={iban}
              onChange={(e) => setIban(e.target.value)}
              placeholder="DK…"
            />
          </label>
          <label>
            Valuta
            <input
              type="text"
              value={currency}
              onChange={(e) => setCurrency(e.target.value.toUpperCase())}
              maxLength={3}
            />
          </label>
          <label>
            Ledger-konto (valgfri)
            <input
              type="text"
              value={ledgerAccountNo}
              onChange={(e) => setLedgerAccountNo(e.target.value)}
              placeholder="fx 2000 (Bank)"
            />
          </label>
          <div className="row-actions">
            <button
              type="submit"
              className="btn primary"
              disabled={submitting || !name.trim()}
            >
              {submitting ? "Opretter …" : "Opret bankkonto"}
            </button>
            <button type="button" className="btn secondary" onClick={onClose}>
              Annullér
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
