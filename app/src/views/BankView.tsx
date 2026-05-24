// Bank — the per-company bank transactions (cockpit-redesign iteration 3).
//
// Renders `/api/companies/:slug/bank?year=`: the imported bank_transactions
// rows for the year — date, text, amount, running balance — each with its
// reconciliation status (matched vs unmatched to a posted journal entry). The
// registered bank account and its booked ledger balance are shown above the
// table. All money fields are kroner — `formatKroner` is used throughout.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { CompanyBank } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { BankImportModal } from "../components/BankImportModal";
import {
  BankReconcileModal,
  type BankReconcileTransaction,
} from "../components/BankReconcileModal";

export function BankView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const state = useAsync<CompanyBank>(() => api.bank(slug, year), [slug, year]);
  // True while the bank-CSV-import modal (#213, slice 2) is open.
  const [importing, setImporting] = useState(false);
  // The unmatched row currently being settled from the cockpit (#365). Null
  // while no settle-modal is open.
  const [reconciling, setReconciling] =
    useState<BankReconcileTransaction | null>(null);

  if (state.loading && !state.data) return <Loading label="Henter bank…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const b = state.data!;
  const currency = b.company.currency || "DKK";

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{b.company.name}</h2>
          <p className="muted">
            {b.company.cvr ? `CVR ${b.company.cvr} · ` : ""}
            {b.company.country} · {currency} · Bank
          </p>
        </div>
        <div className="row-actions">
          {/* The bank-import write action — hidden for an archived (read-only)
              year, where no live ledger is available to import into. */}
          {!b.archived && (
            <button
              type="button"
              className="btn"
              onClick={() => setImporting(true)}
            >
              Importér kontoudtog
            </button>
          )}
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

      <CompanyNav
        slug={slug}
        years={b.fiscalYears}
        selectedYear={b.selectedYear}
        onYearChange={setYear}
      />

      {importing && (
        <BankImportModal
          slug={slug}
          onImported={state.reload}
          onClose={() => setImporting(false)}
        />
      )}

      {reconciling && (
        <BankReconcileModal
          slug={slug}
          transaction={reconciling}
          onReconciled={state.reload}
          onClose={() => setReconciling(null)}
        />
      )}

      {b.archived ? (
        <ArchivedBankView bank={b} currency={currency} />
      ) : (
        <>
          <BankDifferenceBanner bank={b} currency={currency} />

          <div className="status-grid bank-summary">
            <div className="card status-card">
              <h3>Faktisk saldo</h3>
              <div className="status-figure">
                {b.actualBalance === null
                  ? "—"
                  : formatKroner(b.actualBalance, currency)}
              </div>
              <p className="muted status-note">
                {/* #305: distinguish "no statement imported" from "a
                    statement was imported but its CSV had no balance column".
                    Saying "intet kontoudtog importeret" for the second case
                    would wrongly suggest the import failed. */}
                {b.actualBalance !== null
                  ? "Seneste saldo fra kontoudtoget"
                  : b.bankStatementStatus === "no-balance-column"
                    ? "Banksaldo ukendt — kontoudtoget havde ingen saldo-kolonne"
                    : "Intet kontoudtog importeret"}
              </p>
            </div>
            <div className="card status-card">
              <h3>Bogført saldo</h3>
              <div className="status-figure">
                {formatKroner(b.bookedBalance, currency)}
              </div>
              <p className="muted status-note">
                {b.accounts.length > 0
                  ? b.accounts
                      .map((a) =>
                        [a.bankName, a.name].filter(Boolean).join(" · "),
                      )
                      .join(", ")
                  : "Bank- og kassekonti"}
              </p>
            </div>
            <div className="card status-card">
              <h3>Afstemning</h3>
              <div className="status-figure">
                {b.matchedCount} / {b.transactions.length}
              </div>
              <p className="muted status-note">
                {b.matchedCount} afstemt ·{" "}
                {b.unmatchedCount} uafstemte transaktioner
              </p>
            </div>
          </div>

          <div className="card statement-card table-scroll">
            <table className="data statement-table">
              <thead>
                <tr>
                  <th>Dato</th>
                  <th>Tekst</th>
                  <th className="num">Beløb</th>
                  <th className="num">Saldo</th>
                  <th>Afstemning</th>
                </tr>
              </thead>
              <tbody>
                {b.transactions.length === 0 ? (
                  <tr>
                    <td colSpan={5} className="empty-inline">
                      Ingen banktransaktioner i året.
                    </td>
                  </tr>
                ) : (
                  b.transactions.map((tx) => (
                    <tr key={tx.id}>
                      <td className="entry-date">{tx.date}</td>
                      <td>{tx.text}</td>
                      <td className="num">
                        {formatKroner(tx.amount, currency)}
                      </td>
                      <td className="num">
                        {tx.runningBalance === null
                          ? "—"
                          : formatKroner(tx.runningBalance, currency)}
                      </td>
                      <td>
                        {tx.reconciliationStatus === "matched" ? (
                          <span className="flag ok">
                            Afstemt
                            {tx.journalEntryNo
                              ? ` · ${tx.journalEntryNo}`
                              : ""}
                          </span>
                        ) : (
                          <div className="row-actions">
                            <span className="flag warning">Uafstemt</span>
                            <button
                              type="button"
                              className="btn secondary"
                              onClick={() =>
                                setReconciling({
                                  id: tx.id,
                                  date: tx.date,
                                  text: tx.text,
                                  amount: tx.amount,
                                  currency,
                                })
                              }
                            >
                              Bogfør
                            </button>
                          </div>
                        )}
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </>
      )}
    </section>
  );
}

// The booked-vs-actual gap is the headline of a bank page — shown prominently
// at the top. When the gap is zero (or no statement is imported) the banner
// reassures rather than alarms.
function BankDifferenceBanner({
  bank,
  currency,
}: {
  bank: CompanyBank;
  currency: string;
}) {
  if (bank.actualBalance === null || bank.difference === null) {
    // #305: a statement WITH transactions but no balance column is not the
    // same as no statement at all — the wording must reflect which it is.
    const noBalanceColumn = bank.bankStatementStatus === "no-balance-column";
    return (
      <div className="card bank-diff-banner neutral">
        <span className="flag">Bank</span>
        <p>
          {noBalanceColumn ? (
            <>
              Kontoudtoget for {bank.selectedYear} indeholder ingen
              saldo-kolonne, så den faktiske banksaldo er ukendt — kun den
              bogførte saldo {formatKroner(bank.bookedBalance, currency)} kan
              vises.
            </>
          ) : (
            <>
              Intet kontoudtog importeret for {bank.selectedYear} — kun den
              bogførte saldo {formatKroner(bank.bookedBalance, currency)} kan
              vises.
            </>
          )}
        </p>
      </div>
    );
  }

  const reconciled = Math.abs(bank.difference) < 0.005;
  if (reconciled) {
    return (
      <div className="card bank-diff-banner ok">
        <span className="flag ok">Afstemt</span>
        <p>
          Kontoudtog og bogført saldo stemmer:{" "}
          {formatKroner(bank.actualBalance, currency)}.
        </p>
      </div>
    );
  }

  return (
    <div className="card bank-diff-banner alert">
      <span className="flag warning">Difference</span>
      <div>
        <div className="bank-diff-figure">
          {formatKroner(bank.difference, currency)}
        </div>
        <p>
          Bogført saldo {formatKroner(bank.bookedBalance, currency)} mod faktisk
          saldo på kontoudtoget {formatKroner(bank.actualBalance, currency)} —
          {bank.unmatchedCount > 0
            ? ` ${bank.unmatchedCount} uafstemte transaktioner.`
            : " endnu ikke afstemt."}
        </p>
      </div>
    </div>
  );
}

// An archived fiscal year (#197): the ledger lives in the read-only archive,
// but the imported bank statement is live, append-only data that legitimately
// spans archived years too. The transactions are shown; the booked-balance and
// reconciliation comparison is not — there is no live ledger for that year to
// reconcile against, so a "difference" banner would be misleading.
function ArchivedBankView({
  bank,
  currency,
}: {
  bank: CompanyBank;
  currency: string;
}) {
  // The statement's closing balance for an archived year is the running
  // balance after its last imported transaction — exact, unlike a cross-year
  // "as of year-end" figure that could borrow a balance from another year.
  const lastTx =
    bank.transactions.length > 0
      ? bank.transactions[bank.transactions.length - 1]!
      : null;
  const closingBalance = lastTx?.runningBalance ?? null;
  return (
    <>
      <div className="card bank-diff-banner neutral">
        <span className="flag">Arkiveret år</span>
        <p>
          {bank.selectedYear} er et arkiveret regnskabsår fra et tidligere
          bogføringssystem. Banktransaktionerne fra det importerede kontoudtog
          vises herunder. Selve bogføringen og afstemningen for arkiverede år
          ligger i regnskabet fra dengang — ikke i Rentemesters aktive ledger.
        </p>
      </div>

      {lastTx && closingBalance !== null && (
        <div className="status-grid bank-summary">
          <div className="card status-card">
            <h3>Saldo på kontoudtoget</h3>
            <div className="status-figure">
              {formatKroner(closingBalance, currency)}
            </div>
            <p className="muted status-note">
              Efter seneste postering den {lastTx.date}
            </p>
          </div>
        </div>
      )}

      <div className="card statement-card table-scroll">
        <table className="data statement-table">
          <thead>
            <tr>
              <th>Dato</th>
              <th>Tekst</th>
              <th className="num">Beløb</th>
              <th className="num">Saldo</th>
            </tr>
          </thead>
          <tbody>
            {bank.transactions.length === 0 ? (
              <tr>
                <td colSpan={4} className="empty-inline">
                  Ingen banktransaktioner importeret for {bank.selectedYear}.
                </td>
              </tr>
            ) : (
              bank.transactions.map((tx) => (
                <tr key={tx.id}>
                  <td className="entry-date">{tx.date}</td>
                  <td>{tx.text}</td>
                  <td className="num">{formatKroner(tx.amount, currency)}</td>
                  <td className="num">
                    {tx.runningBalance === null
                      ? "—"
                      : formatKroner(tx.runningBalance, currency)}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </>
  );
}
