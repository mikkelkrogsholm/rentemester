// Bank — the per-company bank transactions (cockpit-redesign iteration 3).
//
// Renders `/api/companies/:slug/bank?year=`: the imported bank_transactions
// rows for the year — date, text, amount, running balance — each with its
// reconciliation status (matched vs unmatched to a posted journal entry). The
// registered bank account and its booked ledger balance are shown above the
// table. All money fields are kroner — `formatKroner` is used throughout.
//
// #451 — filter-bar: fritekstsøgning (transaktionstekst og posteringsnr.),
// datointerval (fra/til), status-filter (alle/afstemt/uafstemt) og sortering
// på dato/beløb. Alle filtre er client-side og afspejles i URL-params
// (`q`, `from`, `to`, `status`) så ejeren kan dele linket eller komme tilbage
// til samme udsnit. En "Ryd filtre"-knap dukker op når et filter er aktivt.
// Status-linjen under filter-baren viser hvor mange transaktioner der matcher
// — og hvor mange af dem der er afstemt vs. uafstemte.

import { useMemo, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { BankTransactionRow, CompanyBank } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { BankImportModal } from "../components/BankImportModal";
import {
  BankReconcileModal,
  type BankReconcileTransaction,
} from "../components/BankReconcileModal";

// #451 — the URL keys we own; listed once so "Ryd filtre" can clear them all
// without touching other params (e.g. `?year=`).
const FILTER_PARAM_KEYS = ["q", "from", "to", "status"] as const;

type StatusFilter = "all" | "matched" | "unmatched";
type SortKey = "date" | "amount";
type SortDir = "asc" | "desc";

function isStatusFilter(v: string): v is StatusFilter {
  return v === "all" || v === "matched" || v === "unmatched";
}

function txMatchesText(tx: BankTransactionRow, needle: string): boolean {
  if (tx.text && tx.text.toLowerCase().includes(needle)) return true;
  if (tx.journalEntryNo && tx.journalEntryNo.toLowerCase().includes(needle))
    return true;
  return false;
}

export function BankView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const [params, setParams] = useSearchParams();
  const state = useAsync<CompanyBank>(() => api.bank(slug, year), [slug, year]);
  // True while the bank-CSV-import modal (#213, slice 2) is open.
  const [importing, setImporting] = useState(false);
  // The unmatched row currently being settled from the cockpit (#365). Null
  // while no settle-modal is open.
  const [reconciling, setReconciling] =
    useState<BankReconcileTransaction | null>(null);

  // --- #451 filter-bar params (client-side; reflected in URL) ---------------
  const q = params.get("q") ?? "";
  const fromDate = params.get("from") ?? "";
  const toDate = params.get("to") ?? "";
  const statusRaw = params.get("status") ?? "all";
  const status: StatusFilter = isStatusFilter(statusRaw) ? statusRaw : "all";

  // #451 — sorter for the date/amount columns. Default is the import order
  // (chronological as inserted); only after the owner clicks a column-header
  // do we override that order.
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
    status !== "all";

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

  const allTransactions = state.data?.transactions ?? [];

  const filteredTransactions = useMemo(() => {
    if (!hasActiveFilter) return allTransactions;
    const needle = q.trim().toLowerCase();
    return allTransactions.filter((tx) => {
      if (needle !== "" && !txMatchesText(tx, needle)) return false;
      if (fromDate !== "" && tx.date < fromDate) return false;
      if (toDate !== "" && tx.date > toDate) return false;
      if (status === "matched" && tx.reconciliationStatus !== "matched")
        return false;
      if (status === "unmatched" && tx.reconciliationStatus !== "unmatched")
        return false;
      return true;
    });
  }, [allTransactions, hasActiveFilter, q, fromDate, toDate, status]);

  const sortedTransactions = useMemo(() => {
    if (!sort) return filteredTransactions;
    const out = [...filteredTransactions];
    out.sort((a, b) => {
      let cmp = 0;
      if (sort.key === "date") {
        cmp = a.date < b.date ? -1 : a.date > b.date ? 1 : 0;
      } else {
        cmp = a.amount - b.amount;
      }
      return sort.dir === "asc" ? cmp : -cmp;
    });
    return out;
  }, [filteredTransactions, sort]);

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
        <ArchivedBankView
          bank={b}
          currency={currency}
          q={q}
          fromDate={fromDate}
          toDate={toDate}
          status={status}
          hasActiveFilter={hasActiveFilter}
          setFilter={setFilter}
          clearAllFilters={clearAllFilters}
          sortIndicator={sortIndicator}
          toggleSort={toggleSort}
          sortedTransactions={sortedTransactions}
        />
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

          <BankFilterBar
            q={q}
            fromDate={fromDate}
            toDate={toDate}
            status={status}
            hasActiveFilter={hasActiveFilter}
            setFilter={setFilter}
            clearAllFilters={clearAllFilters}
          />

          <BankFilterSummary
            allTransactions={b.transactions}
            filteredTransactions={sortedTransactions}
            hasActiveFilter={hasActiveFilter}
          />

          <div className="card statement-card table-scroll">
            <table className="data statement-table">
              <thead>
                <tr>
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
                  <th>Tekst</th>
                  <th className="num">
                    <button
                      type="button"
                      className="th-sort"
                      onClick={() => toggleSort("amount")}
                      aria-label="Sortér efter beløb"
                    >
                      Beløb{sortIndicator("amount")}
                    </button>
                  </th>
                  <th className="num">Saldo</th>
                  <th>Afstemning</th>
                </tr>
              </thead>
              <tbody>
                {sortedTransactions.length === 0 ? (
                  <tr>
                    <td colSpan={5} className="empty-inline">
                      {hasActiveFilter
                        ? "Ingen transaktioner matcher filtrene."
                        : "Ingen banktransaktioner i året."}
                    </td>
                  </tr>
                ) : (
                  sortedTransactions.map((tx) => (
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

// #451 — the filter-bar above the bank-transactions table. Same shape and
// CSS classes as DocumentsView's filter-bar so the cockpit reads consistently
// across "tunge tabeller".
function BankFilterBar({
  q,
  fromDate,
  toDate,
  status,
  hasActiveFilter,
  setFilter,
  clearAllFilters,
}: {
  q: string;
  fromDate: string;
  toDate: string;
  status: StatusFilter;
  hasActiveFilter: boolean;
  setFilter: (
    key: (typeof FILTER_PARAM_KEYS)[number],
    value: string,
  ) => void;
  clearAllFilters: () => void;
}) {
  return (
    <div className="journal-filter-bar card" role="search">
      <label className="journal-filter-field journal-filter-field--search">
        <span className="muted">Søg</span>
        <input
          type="search"
          value={q}
          placeholder="Søg på tekst eller posteringsnr…"
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
          <option value="matched">Kun afstemte</option>
          <option value="unmatched">Kun uafstemte</option>
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
  );
}

// #451 — the "X af Y matcher · Z afstemt · W uafstemte" status line under
// the filter bar. Always counts AGAINST the filtered set so the owner sees
// how the current filter slices reconciliation status.
function BankFilterSummary({
  allTransactions,
  filteredTransactions,
  hasActiveFilter,
}: {
  allTransactions: BankTransactionRow[];
  filteredTransactions: BankTransactionRow[];
  hasActiveFilter: boolean;
}) {
  const total = allTransactions.length;
  const matchCount = filteredTransactions.length;
  const matched = filteredTransactions.filter(
    (tx) => tx.reconciliationStatus === "matched",
  ).length;
  const unmatched = matchCount - matched;
  return (
    <p className="statement-asof muted">
      {hasActiveFilter
        ? `${matchCount} af ${total} transaktioner matcher`
        : `${total} transaktioner`}
      {" · "}
      {matched} afstemt · {unmatched} uafstemte
    </p>
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
//
// #451 — the filter-bar is also shown for archived years (read-only-egnet) so
// the owner can search historical bank movements with the same affordances.
function ArchivedBankView({
  bank,
  currency,
  q,
  fromDate,
  toDate,
  status,
  hasActiveFilter,
  setFilter,
  clearAllFilters,
  sortIndicator,
  toggleSort,
  sortedTransactions,
}: {
  bank: CompanyBank;
  currency: string;
  q: string;
  fromDate: string;
  toDate: string;
  status: StatusFilter;
  hasActiveFilter: boolean;
  setFilter: (
    key: (typeof FILTER_PARAM_KEYS)[number],
    value: string,
  ) => void;
  clearAllFilters: () => void;
  sortIndicator: (key: SortKey) => string;
  toggleSort: (key: SortKey) => void;
  sortedTransactions: BankTransactionRow[];
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

      <BankFilterBar
        q={q}
        fromDate={fromDate}
        toDate={toDate}
        status={status}
        hasActiveFilter={hasActiveFilter}
        setFilter={setFilter}
        clearAllFilters={clearAllFilters}
      />

      <BankFilterSummary
        allTransactions={bank.transactions}
        filteredTransactions={sortedTransactions}
        hasActiveFilter={hasActiveFilter}
      />

      <div className="card statement-card table-scroll">
        <table className="data statement-table">
          <thead>
            <tr>
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
              <th>Tekst</th>
              <th className="num">
                <button
                  type="button"
                  className="th-sort"
                  onClick={() => toggleSort("amount")}
                  aria-label="Sortér efter beløb"
                >
                  Beløb{sortIndicator("amount")}
                </button>
              </th>
              <th className="num">Saldo</th>
            </tr>
          </thead>
          <tbody>
            {sortedTransactions.length === 0 ? (
              <tr>
                <td colSpan={4} className="empty-inline">
                  {hasActiveFilter
                    ? "Ingen transaktioner matcher filtrene."
                    : `Ingen banktransaktioner importeret for ${bank.selectedYear}.`}
                </td>
              </tr>
            ) : (
              sortedTransactions.map((tx) => (
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
