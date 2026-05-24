// Posteringer — the per-company journal (cockpit-redesign iteration 3).
//
// Renders `/api/companies/:slug/journal?year=`: the posted journal entries for
// the year (entry no, date, text, total). Clicking an entry expands it to show
// its debit/credit lines (account no + name, debit, credit). All money fields
// are kroner — `formatKroner` is used throughout.
//
// #396 — filter-bar: fritekstsøgning (entry-tekst, linje-tekst, bilagsnummer,
// modkonto), datointerval og beløbsspand. Alle filtre er client-side og
// afspejles i URL-params (`q`, `from`, `to`, `amountMin`, `amountMax`) så
// ejeren kan dele linket eller komme tilbage til samme udsnit.

import { useMemo, useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
// #379 — the EntryRow needs the slug to build the bilag-file URL on the fly.
import { useAsync } from "../lib/useAsync";
import type { CompanyJournal, JournalEntry } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { ArchivedBanner } from "../components/ArchivedBanner";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";

const FILTER_PARAM_KEYS = ["q", "from", "to", "amountMin", "amountMax"] as const;

export function JournalView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  // An optional account drill-down: `?account=<accountNo>` filters the journal
  // to the entries that touch that account (set by the statement views).
  const [params, setParams] = useSearchParams();
  const account = params.get("account") ?? undefined;
  const clearAccount = () => {
    const next = new URLSearchParams(params);
    next.delete("account");
    setParams(next, { replace: true });
  };

  // --- #396 filter-bar params (client-side; reflected in URL) ---------------
  const q = params.get("q") ?? "";
  const fromDate = params.get("from") ?? "";
  const toDate = params.get("to") ?? "";
  const amountMin = params.get("amountMin") ?? "";
  const amountMax = params.get("amountMax") ?? "";

  function setFilter(key: (typeof FILTER_PARAM_KEYS)[number], value: string) {
    const next = new URLSearchParams(params);
    if (value === "") {
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
    amountMin !== "" ||
    amountMax !== "";

  const state = useAsync<CompanyJournal>(
    () => api.journal(slug, year, account),
    [slug, year, account],
  );

  const filteredEntries = useMemo(() => {
    const entries = state.data?.entries ?? [];
    if (!hasActiveFilter) return entries;
    const needle = q.trim().toLowerCase();
    const minN = amountMin === "" ? null : Number(amountMin);
    const maxN = amountMax === "" ? null : Number(amountMax);
    return entries.filter((entry) => {
      if (needle !== "" && !entryMatchesText(entry, needle)) return false;
      if (fromDate !== "" && entry.date < fromDate) return false;
      if (toDate !== "" && entry.date > toDate) return false;
      if (minN !== null && !Number.isNaN(minN) && entry.total < minN)
        return false;
      if (maxN !== null && !Number.isNaN(maxN) && entry.total > maxN)
        return false;
      return true;
    });
  }, [state.data, hasActiveFilter, q, fromDate, toDate, amountMin, amountMax]);

  if (state.loading && !state.data)
    return <Loading label="Henter posteringer…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const j = state.data!;
  const currency = j.company.currency || "DKK";
  const totalCount = j.entries.length;
  const matchCount = filteredEntries.length;

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{j.company.name}</h2>
          <p className="muted">
            {j.company.cvr ? `CVR ${j.company.cvr} · ` : ""}
            {j.company.country} · {currency} · Posteringer
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
        years={j.fiscalYears}
        selectedYear={j.selectedYear}
        onYearChange={setYear}
      />

      {j.archived && (
        <ArchivedBanner year={j.selectedYear} source={j.archivedSource} />
      )}
      {j.accountFilter && (
        <div className="account-filter">
          <p className="muted">
            Posteringer på konto{" "}
            <span className="account-no">{j.accountFilter.accountNo}</span>{" "}
            {j.accountFilter.name}
          </p>
          <button
            type="button"
            className="btn secondary"
            onClick={clearAccount}
          >
            Vis alle posteringer
          </button>
        </div>
      )}

      <div className="journal-filter-bar card" role="search">
        <label className="journal-filter-field journal-filter-field--search">
          <span className="muted">Søg</span>
          <input
            type="search"
            value={q}
            placeholder="Søg på tekst, bilagsnummer eller konto…"
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
          <span className="muted">Beløb min</span>
          <input
            type="number"
            inputMode="decimal"
            value={amountMin}
            placeholder="0"
            onChange={(e) => setFilter("amountMin", e.target.value)}
          />
        </label>
        <label className="journal-filter-field">
          <span className="muted">Beløb maks</span>
          <input
            type="number"
            inputMode="decimal"
            value={amountMax}
            placeholder="∞"
            onChange={(e) => setFilter("amountMax", e.target.value)}
          />
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

      <p className="statement-asof muted">
        {j.periodStart} – {j.periodEnd} ·{" "}
        {hasActiveFilter
          ? `${matchCount} af ${totalCount} posteringer matcher`
          : `${totalCount} posteringer`}
      </p>
      {filteredEntries.length === 0 ? (
        <div className="card statement-card">
          <p className="empty-inline" style={{ padding: "var(--space-md)" }}>
            {hasActiveFilter
              ? "Ingen posteringer matcher filtrene."
              : j.accountFilter
                ? "Ingen posteringer på kontoen i året."
                : "Ingen posteringer i året."}
          </p>
        </div>
      ) : (
        <ul className="entry-list">
          {filteredEntries.map((entry) => (
            <EntryRow
              key={entry.id}
              entry={entry}
              currency={currency}
              slug={slug}
              archived={j.archived}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function entryMatchesText(entry: JournalEntry, needle: string): boolean {
  if (entry.entryNo.toLowerCase().includes(needle)) return true;
  if (entry.text.toLowerCase().includes(needle)) return true;
  for (const line of entry.lines) {
    if (line.accountNo.toLowerCase().includes(needle)) return true;
    if (line.accountName.toLowerCase().includes(needle)) return true;
    if (line.text && line.text.toLowerCase().includes(needle)) return true;
  }
  return false;
}

function EntryRow({
  entry,
  currency,
  slug,
  archived,
}: {
  entry: JournalEntry;
  currency: string;
  slug: string;
  /**
   * #379 — arkiverede regnskabsår har ingen bilag-linkage; vis "Intet bilag"
   * også når posten i teorien havde en `documentId`, sådan at vi aldrig
   * sender ejeren mod en route der ikke kan resolves.
   */
  archived: boolean;
}) {
  const [open, setOpen] = useState(false);
  // #379 — en post har et bilag når både linkage og fil-route er meningsfulde.
  // Arkiverede år vises altid som "Intet bilag" (filen er ikke i `documents`).
  const hasDocument = !archived && entry.documentId !== null;
  return (
    <li className={`entry-item${open ? " open" : ""}`}>
      <button
        type="button"
        className="entry-summary"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <span className="entry-caret" aria-hidden="true">
          {open ? "▾" : "▸"}
        </span>
        <span className="entry-no">{entry.entryNo}</span>
        <span className="entry-date">{entry.date}</span>
        <span className="entry-text">{entry.text}</span>
        <span className="entry-total num">
          {formatKroner(entry.total, currency)}
        </span>
      </button>
      {open && (
        <div className="entry-lines table-scroll">
          <table className="data statement-table">
            <thead>
              <tr>
                <th>Konto</th>
                <th>Navn</th>
                <th className="num">Debet</th>
                <th className="num">Kredit</th>
              </tr>
            </thead>
            <tbody>
              {entry.lines.map((line, i) => (
                <tr key={i}>
                  <td className="account-no">{line.accountNo}</td>
                  <td>
                    {line.accountName}
                    {line.text ? (
                      <span className="muted"> · {line.text}</span>
                    ) : null}
                  </td>
                  <td className="num">
                    {line.debit ? formatKroner(line.debit, currency) : "—"}
                  </td>
                  <td className="num">
                    {line.credit ? formatKroner(line.credit, currency) : "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="entry-bilag">
            {hasDocument ? (
              <a
                className="entry-bilag-link"
                href={api.documentFileUrl(slug, entry.documentId!)}
                target="_blank"
                rel="noreferrer"
              >
                Åbn bilag
                {entry.documentNo ? (
                  <span className="muted"> · {entry.documentNo}</span>
                ) : null}
              </a>
            ) : (
              <span className="muted entry-bilag-empty">Intet bilag</span>
            )}
          </div>
        </div>
      )}
    </li>
  );
}
