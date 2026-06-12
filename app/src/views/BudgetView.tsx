// Budget — the per-company budget vs. faktisk view (#339).
//
// Two faces of the same data, toggled by a single switch:
//
//   1. Plan-mode ("Budget"): an input grid (konto × måned) where the owner
//      types planned amounts. Each saved cell appends a new revision via
//      `POST /api/companies/:slug/budget` — the core is append-only, so
//      re-saving a cell is always safe and the history is fully auditable.
//
//   2. Compare-mode ("Sammenlign med faktisk"): the same grid replaced by
//      the comparison table read from `GET .../budget-vs-actual`. Each row
//      is one (account, month) cell with budget, actual, variance (kr) and
//      variance % alongside.
//
// The sign convention follows core/budget.ts: positive variance = "good"
// (under budget for expense accounts, over target for income accounts).
// Money is kroner throughout — `formatKroner` is used everywhere, `formatPercent`
// for the % column.

import { useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner, formatPercent } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type {
  CompanyBudget,
  CompanyBudgetLine,
  CompanyBudgetVsActual,
  CompanyBudgetVsActualLine,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";

/** Danish month abbreviations, jan→dec — same set the other views use. */
const MONTH_LABELS_DK = [
  "jan", "feb", "mar", "apr", "maj", "jun",
  "jul", "aug", "sep", "okt", "nov", "dec",
];

/** Pretty Danish label for a `YYYY-MM` period, e.g. `2026-06` → `jun 2026`. */
function periodLabel(period: string): string {
  const m = /^(\d{4})-(\d{2})$/.exec(period);
  if (!m) return period;
  const year = m[1]!;
  const month = Number(m[2]);
  if (!(month >= 1 && month <= 12)) return period;
  return `${MONTH_LABELS_DK[month - 1]} ${year}`;
}

export function BudgetView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const [mode, setMode] = useState<"plan" | "compare">("plan");

  const plan = useAsync<CompanyBudget>(() => api.budget(slug, year), [slug, year, mode]);
  const compare = useAsync<CompanyBudgetVsActual>(
    () => api.budgetVsActual(slug, year),
    [slug, year, mode],
  );

  // We always need ONE of the two payloads to render. The plan/compare toggle
  // picks which one drives the body. The other one is also fetched so a
  // toggle is instant after first load.
  const state = mode === "plan" ? plan : compare;

  if (state.loading && !state.data) return <Loading label="Henter budget…" />;
  if (state.error) return <ErrorState message={state.error} onRetry={state.reload} />;

  const data = state.data!;
  const currency = data.company.currency || "DKK";

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{data.company.name}</h2>
          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · {currency} · Budget
          </p>
        </div>
        <div className="row-actions">
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

      <CompanyNav
        slug={slug}
        years={data.fiscalYears}
        selectedYear={data.selectedYear}
        onYearChange={setYear}
      />

      <div className="budget-toolbar" role="tablist" aria-label="Budget-visning">
        <button
          type="button"
          role="tab"
          aria-selected={mode === "plan"}
          className={`btn ${mode === "plan" ? "primary" : "secondary"}`}
          onClick={() => setMode("plan")}
        >
          Budget
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "compare"}
          className={`btn ${mode === "compare" ? "primary" : "secondary"}`}
          onClick={() => setMode("compare")}
        >
          Sammenlign med faktisk
        </button>
      </div>

      {data.archived ? (
        <ArchivedNotice year={data.selectedYear} />
      ) : mode === "plan" ? (
        <BudgetGrid
          slug={slug}
          data={plan.data!}
          currency={currency}
          onSaved={() => {
            plan.reload();
            compare.reload();
          }}
        />
      ) : (
        <BudgetVsActualTable data={compare.data!} currency={currency} />
      )}
    </section>
  );
}

/**
 * The Budget input grid — one row per account that already carries a budget
 * line, one input per calendar month. Saving a cell appends a new revision
 * via `POST /api/companies/:slug/budget`; the core's append-only schema
 * collapses to the latest revision on the next `GET .../budget`.
 *
 * A "Tilføj konto"-form below the grid lets the owner introduce a new
 * (account, period) cell that no budget line existed for yet.
 */
function BudgetGrid({
  slug,
  data,
  currency,
  onSaved,
}: {
  slug: string;
  data: CompanyBudget;
  currency: string;
  onSaved: () => void;
}) {
  // Bucket the existing lines into a Map<accountNo, Map<period, line>> so we
  // can render the grid with one row per account and one cell per period.
  const grouped = useMemo(() => groupByAccount(data.lines), [data.lines]);
  const accountKeys = useMemo(() => [...grouped.keys()].sort(), [grouped]);

  return (
    <>
      <div className="status-grid invoices-summary">
        <div className="card status-card">
          <h3>Samlet budget</h3>
          <div className="status-figure">
            {formatKroner(data.totalBudget, currency)}
          </div>
          <p className="muted status-note">
            {data.lines.length}{" "}
            {data.lines.length === 1 ? "budgetlinje" : "budgetlinjer"} ·
            regnskabsår {data.selectedYear}
          </p>
        </div>
      </div>

      <div className="card statement-card table-scroll">
        <table className="data statement-table budget-grid">
          <thead>
            <tr>
              <th>Konto</th>
              {data.periods.map((p) => (
                <th key={p} className="num" scope="col">
                  {periodLabel(p)}
                </th>
              ))}
              <th className="num">Total</th>
            </tr>
          </thead>
          <tbody>
            {accountKeys.length === 0 ? (
              <tr>
                <td colSpan={data.periods.length + 2}>
                  <p className="muted">
                    Ingen budgetlinjer endnu — tilføj en linje nedenfor for at
                    komme i gang.
                  </p>
                </td>
              </tr>
            ) : (
              accountKeys.map((accountNo) => {
                const rowMap = grouped.get(accountNo)!;
                const accountName =
                  [...rowMap.values()][0]?.accountName ?? null;
                const rowTotal = [...rowMap.values()].reduce(
                  (sum, l) => sum + l.amount,
                  0,
                );
                return (
                  <tr key={accountNo}>
                    <td className="account-no">
                      {accountNo}
                      {accountName ? (
                        <span className="muted"> · {accountName}</span>
                      ) : null}
                    </td>
                    {data.periods.map((period) => {
                      const existing = rowMap.get(period);
                      return (
                        <td key={period} className="num budget-cell">
                          <BudgetAmountInput
                            slug={slug}
                            accountNo={accountNo}
                            period={period}
                            initialAmount={existing?.amount ?? null}
                            onSaved={onSaved}
                          />
                        </td>
                      );
                    })}
                    <td className="num">
                      {formatKroner(rowTotal, currency)}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      <AddBudgetLineForm
        slug={slug}
        periods={data.periods}
        onSaved={onSaved}
      />
    </>
  );
}

/**
 * One editable cell of the Budget grid: a numeric input that POSTs to the
 * server on blur or Enter. A change persists when the value differs from
 * the last saved amount; an unchanged blur is a no-op. The append-only
 * core guarantees re-saving an unchanged value is safe — we just avoid the
 * round-trip.
 */
function BudgetAmountInput({
  slug,
  accountNo,
  period,
  initialAmount,
  onSaved,
}: {
  slug: string;
  accountNo: string;
  period: string;
  initialAmount: number | null;
  onSaved: () => void;
}) {
  const [value, setValue] = useState<string>(
    initialAmount === null ? "" : String(initialAmount),
  );
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Keep the displayed value in sync when the parent reloads — a save returns
  // fresh data and the cell must reflect it, not a stale initial render.
  useEffect(() => {
    setValue(initialAmount === null ? "" : String(initialAmount));
  }, [initialAmount]);

  const lastSaved = initialAmount === null ? "" : String(initialAmount);

  async function commit() {
    if (value.trim() === lastSaved.trim()) return;
    const trimmed = value.trim();
    if (trimmed.length === 0) {
      // Empty input is "leave the previous revision alone" — clearing a
      // budget is not part of the append-only model, so just snap back.
      setValue(lastSaved);
      return;
    }
    const parsed = Number(trimmed.replace(",", "."));
    if (!Number.isFinite(parsed) || parsed < 0) {
      setError("Beløb skal være et tal ≥ 0");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.setBudget(slug, { accountNo, period, amount: parsed });
      onSaved();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <span className="budget-input-wrap">
      <input
        type="text"
        inputMode="decimal"
        className="budget-input"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        }}
        aria-label={`Budget for konto ${accountNo} ${period}`}
        disabled={saving}
      />
      {error ? (
        <span className="muted budget-cell-error" role="alert">
          {error}
        </span>
      ) : null}
    </span>
  );
}

/**
 * A small form below the grid to seed a new (account, period) cell when no
 * budget line existed yet. Once saved, the grid re-renders with the new row
 * — subsequent edits happen inline.
 */
function AddBudgetLineForm({
  slug,
  periods,
  onSaved,
}: {
  slug: string;
  periods: string[];
  onSaved: () => void;
}) {
  const [accountNo, setAccountNo] = useState("");
  const [period, setPeriod] = useState(periods[0] ?? "");
  const [amount, setAmount] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    const trimmedAcc = accountNo.trim();
    const trimmedAmt = amount.trim().replace(",", ".");
    if (!trimmedAcc) {
      setError("Kontonr. er påkrævet");
      return;
    }
    const parsed = Number(trimmedAmt);
    if (!Number.isFinite(parsed) || parsed < 0) {
      setError("Beløb skal være et tal ≥ 0");
      return;
    }
    setSaving(true);
    try {
      await api.setBudget(slug, {
        accountNo: trimmedAcc,
        period,
        amount: parsed,
      });
      setAccountNo("");
      setAmount("");
      onSaved();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <form className="card budget-add-form" onSubmit={submit}>
      <h3>Tilføj budgetlinje</h3>
      <p className="muted">
        Vælg en konto fra kontoplanen og en måned, og angiv det planlagte
        beløb. En ny linje med samme konto+periode tilføjer en ny revision —
        den nyeste vinder.
      </p>
      <div className="form-row">
        <label>
          Konto
          <input
            type="text"
            value={accountNo}
            onChange={(e) => setAccountNo(e.target.value)}
            placeholder="fx 2200"
            aria-label="Kontonr."
          />
        </label>
        <label>
          Måned
          <select
            value={period}
            onChange={(e) => setPeriod(e.target.value)}
            aria-label="Måned"
          >
            {periods.map((p) => (
              <option key={p} value={p}>
                {periodLabel(p)}
              </option>
            ))}
          </select>
        </label>
        <label>
          Beløb (kr)
          <input
            type="text"
            inputMode="decimal"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            placeholder="fx 5000"
            aria-label="Beløb"
          />
        </label>
        <button type="submit" className="btn primary" disabled={saving}>
          {saving ? "Gemmer…" : "Tilføj budgetlinje"}
        </button>
      </div>
      {error ? (
        <p className="muted budget-form-error" role="alert">
          {error}
        </p>
      ) : null}
    </form>
  );
}

/**
 * The Sammenlign-med-faktisk table — one row per (account, month) cell, with
 * budget, faktisk, variance (kr) and variance (%). A positive variance is
 * "good" (formatted as a positive figure with a check); negative is "bad".
 */
function BudgetVsActualTable({
  data,
  currency,
}: {
  data: CompanyBudgetVsActual;
  currency: string;
}) {
  const summaryTone = data.totalVariance >= 0 ? "ok" : "alert";
  return (
    <>
      <div className="status-grid invoices-summary">
        <div className="card status-card">
          <h3>Samlet budget</h3>
          <div className="status-figure">
            {formatKroner(data.totalBudget, currency)}
          </div>
        </div>
        <div className="card status-card">
          <h3>Samlet faktisk</h3>
          <div className="status-figure">
            {formatKroner(data.totalActual, currency)}
          </div>
        </div>
        <div className="card status-card">
          <h3>Samlet afvigelse</h3>
          <div className={`status-figure status-${summaryTone}`}>
            {formatKroner(data.totalVariance, currency)}
          </div>
        </div>
      </div>

      <div className="card statement-card table-scroll">
        <table className="data statement-table">
          <thead>
            <tr>
              <th>Konto</th>
              <th>Måned</th>
              <th className="num">Budget</th>
              <th className="num">Faktisk</th>
              <th className="num">Afvigelse</th>
              <th className="num">Afvigelse %</th>
            </tr>
          </thead>
          <tbody>
            {data.lines.length === 0 ? (
              <tr>
                <td colSpan={6}>
                  <p className="muted">
                    Ingen budget- eller faktisk-bevægelser i {data.selectedYear}.
                  </p>
                </td>
              </tr>
            ) : (
              data.lines.map((row) => (
                <ComparisonRow
                  key={`${row.accountNo}-${row.period}`}
                  row={row}
                  currency={currency}
                />
              ))
            )}
            {data.lines.length > 0 ? (
              <tr className={`statement-result ${summaryTone === "ok" ? "positive" : "negative"}`}>
                <td colSpan={2}>I alt</td>
                <td className="num">
                  {formatKroner(data.totalBudget, currency)}
                </td>
                <td className="num">
                  {formatKroner(data.totalActual, currency)}
                </td>
                <td className="num">
                  {formatKroner(data.totalVariance, currency)}
                </td>
                <td className="num">—</td>
              </tr>
            ) : null}
          </tbody>
        </table>
      </div>
      <p className="statement-check ok">
        Positiv afvigelse betyder "godt": for udgiftskonti = under budget,
        for indtægtskonti = over mål. Tallene er læst direkte fra ledgeren
        og budget-linjer — samme funktion som CLI-rapporten kalder.
      </p>
    </>
  );
}

function ComparisonRow({
  row,
  currency,
}: {
  row: CompanyBudgetVsActualLine;
  currency: string;
}) {
  const tone = row.variance >= 0 ? "ok" : "alert";
  return (
    <tr>
      <td className="account-no">
        {row.accountNo}
        {row.accountName ? (
          <span className="muted"> · {row.accountName}</span>
        ) : null}
      </td>
      <td>{periodLabel(row.period)}</td>
      <td className="num">{formatKroner(row.budget, currency)}</td>
      <td className="num">{formatKroner(row.actual, currency)}</td>
      <td className={`num status-${tone}`}>
        {formatKroner(row.variance, currency)}
      </td>
      <td className="num">
        {row.variancePercent === null ? "—" : formatPercent(row.variancePercent)}
      </td>
    </tr>
  );
}

function groupByAccount(
  lines: CompanyBudgetLine[],
): Map<string, Map<string, CompanyBudgetLine>> {
  const out = new Map<string, Map<string, CompanyBudgetLine>>();
  for (const line of lines) {
    let bucket = out.get(line.accountNo);
    if (!bucket) {
      bucket = new Map();
      out.set(line.accountNo, bucket);
    }
    bucket.set(line.period, line);
  }
  return out;
}

function ArchivedNotice({ year }: { year: string }) {
  return (
    <div className="card archived-notice">
      <h3>Budget er ikke tilgængeligt for {year}</h3>
      <p className="muted">
        {year} er et arkiveret regnskabsår. Budget vs. faktisk opgøres kun for
        den aktive ledger og vises derfor ikke for et arkiveret år.
      </p>
    </div>
  );
}
