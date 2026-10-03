import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { ButtonLink, Dialog, Button, Input, PageHeader } from "../components/ui";
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
import { useParams } from "react-router-dom";
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
    (signal) => api.bankAccounts(slug, { signal }),
    [slug, refresh],
  );

  if (state.loading && !state.data) return <Loading />;
  if (state.error && !state.data) return <ErrorState message={state.error} onRetry={state.reload} />;
  const data = state.data!;

  return (
    <section className="bank-accounts-view">
      {state.error && <div className="banner warning" role="alert">Status kunne ikke opdateres. Din formular er bevaret; oplysningerne bag den er fra den seneste gennemførte læsning.</div>}
      <PageHeader title="Bankkonti" actions={<><div className="row-actions">
          <Button requiredPermission="company.admin"
            type="button"
            className="btn primary"
            onClick={() => {
              setError(null);
              setOpenCreate(true);
            }}
          >
            Opret bankkonto …
          </Button>
          <ButtonLink className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div>

          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · Bankkonti
          </p>
        </div>

      </PageHeader>

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
          <div className="table-scroll"><table className="table">
            <thead>
              <tr>
                <th>Navn</th>
                <th>Bank</th>
                <th>Reg.nr.</th>
                <th>Konto-nr.</th>
                <th>IBAN</th>
                <th>SWIFT/BIC</th>
                <th>Kontoejer / kundenr.</th>
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
          </table></div>
        )}
      </section>

      <section className="card">
        <h3>Indbyggede CSV-mapping-profiler ({data.profiles.length})</h3>
        <p className="muted">
          BankImportModal og CLI'ens <code>bank import --profile &lt;navn&gt;</code>
          genbruger disse hard-kodede mapping-profiler. Pr.-konto mapping-
          override er en follow-up.
        </p>
        <div className="table-scroll"><table className="table">
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
        </table></div>
      </section>

      {openCreate && (
        <CreateBankAccountModal
          slug={slug}
          onClose={() => setOpenCreate(false)}
          onRefresh={state.reload}
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
      <td>{account.bic ?? "—"}</td>
      <td>{[account.accountOwner, account.customerNo].filter(Boolean).join(" / ") || "—"}</td>
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
  onClose: onDismiss,
  onRefresh,
  onDone,
  onError,
}: {
  slug: string;
  onClose: () => void;
  onRefresh: () => void;
  onDone: () => void;
  onError: (msg: string) => void;
}) {
  const [name, setName] = useState("");
  const [bankName, setBankName] = useState("");
  const [registrationNo, setRegistrationNo] = useState("");
  const [accountNo, setAccountNo] = useState("");
  const [iban, setIban] = useState("");
  const [bic, setBic] = useState("");
  const [accountOwner, setAccountOwner] = useState("");
  const [customerNo, setCustomerNo] = useState("");
  const [currency, setCurrency] = useState("DKK");
  const [ledgerAccountNo, setLedgerAccountNo] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const outcome = useMutationOutcome(onRefresh);
  const guard = useDiscardGuard(Boolean(name || bankName || registrationNo || accountNo || iban || bic || accountOwner || customerNo || ledgerAccountNo), onDismiss);
  const { onClose } = guard;

  const submit = async (e: React.FormEvent) => {e.preventDefault(); if (outcome.isBlocked()) return;
    setSubmitting(true);
    try {
      await api.createBankAccount(slug, {
        name,
        ...(bankName ? { bankName } : {}),
        ...(registrationNo ? { registrationNo } : {}),
        ...(accountNo ? { accountNo } : {}),
        ...(iban ? { iban } : {}),
        ...(bic ? { bic } : {}),
        ...(accountOwner ? { accountOwner } : {}),
        ...(customerNo ? { customerNo } : {}),
        ...(currency ? { currency } : {}),
        ...(ledgerAccountNo ? { ledgerAccountNo } : {}),
      }).catch(outcome.reject);
      guard.dismiss();
      onDone();
    } catch (err) {
      onError(
        err instanceof ApiError ? err.message : "Oprettelse fejlede.",
      );
      setSubmitting(false);
    }
  };

  return (
    <Dialog title="Opret bankkonto" onClose={onClose} busy={submitting}>
    {outcome.feedback}
      {guard.confirmation}


        <form onSubmit={submit}>
          <label>
            Navn (påkrævet)
            <Input disabled={outcome.blocked}
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              placeholder="fx 'Lunar driftskonto'"
            />
          </label>
          <label>
            Bank-navn
            <Input disabled={outcome.blocked}
              type="text"
              value={bankName}
              onChange={(e) => setBankName(e.target.value)}
              placeholder="fx 'Lunar Bank'"
            />
          </label>
          <label>
            Reg.nr.
            <Input disabled={outcome.blocked}
              type="text"
              value={registrationNo}
              onChange={(e) => setRegistrationNo(e.target.value)}
              placeholder="4-cifret"
            />
          </label>
          <label>
            Konto-nr.
            <Input disabled={outcome.blocked}
              type="text"
              value={accountNo}
              onChange={(e) => setAccountNo(e.target.value)}
            />
          </label>
          <label>
            IBAN
            <Input disabled={outcome.blocked}
              type="text"
              value={iban}
              onChange={(e) => setIban(e.target.value)}
              placeholder="DK…"
            />
          </label>
          <label>SWIFT/BIC<Input disabled={outcome.blocked} type="text" value={bic} onChange={(e) => setBic(e.target.value)} /></label>
          <label>Kontoejer<Input disabled={outcome.blocked} type="text" value={accountOwner} onChange={(e) => setAccountOwner(e.target.value)} /></label>
          <label>Bank-kundenr.<Input disabled={outcome.blocked} type="text" value={customerNo} onChange={(e) => setCustomerNo(e.target.value)} /></label>
          <label>
            Valuta
            <Input disabled={outcome.blocked}
              type="text"
              value={currency}
              onChange={(e) => setCurrency(e.target.value.toUpperCase())}
              maxLength={3}
            />
          </label>
          <label>
            Ledger-konto (valgfri)
            <Input disabled={outcome.blocked}
              type="text"
              value={ledgerAccountNo}
              onChange={(e) => setLedgerAccountNo(e.target.value)}
              placeholder="fx 2000 (Bank)"
            />
          </label>
          <div className="row-actions">
            <Button requiredPermission="company.admin"
              type="submit"
              className="btn primary"
              disabled={outcome.blocked || (submitting || !name.trim())}
            >
              {submitting ? "Opretter …" : "Opret bankkonto"}
            </Button>
            <Button variant="secondary" type="button" className="btn secondary" onClick={onClose}>
              Annullér
            </Button>
          </div>
        </form>

    </Dialog>
  );
}
