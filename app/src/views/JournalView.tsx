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
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ArchivedBanner } from "../components/ArchivedBanner";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { PartyLink } from "../components/PartyLink";
import { FilterBar, FormField, PageHeaderActions, PageState } from "../components/CockpitPrimitives";

const FILTER_PARAM_KEYS = ["q", "from", "to", "amountMin", "amountMax", "journalEntryId", "journalLineId"] as const;

export function JournalView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  // An optional account drill-down: `?account=<accountNo>` filters the journal
  // to the entries that touch that account (set by the statement views).
  const [params, setParams] = useSearchParams();
  const [page, setPage] = useState(0);
  const [selectedEntryId, setSelectedEntryId] = useState<number | null>(null);
  const account = params.get("account") ?? undefined;
  const journalEntryId = Number(params.get("journalEntryId")) || null;
  const journalLineId = Number(params.get("journalLineId")) || null;
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
    amountMax !== "" ||
    journalEntryId !== null ||
    journalLineId !== null;

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
      if (journalEntryId !== null && entry.id !== journalEntryId) return false;
      if (journalLineId !== null && !entry.lines.some((line) => line.journalLineId === journalLineId)) return false;
      if (needle !== "" && !entryMatchesText(entry, needle)) return false;
      if (fromDate !== "" && entry.date < fromDate) return false;
      if (toDate !== "" && entry.date > toDate) return false;
      if (minN !== null && !Number.isNaN(minN) && entry.total < minN)
        return false;
      if (maxN !== null && !Number.isNaN(maxN) && entry.total > maxN)
        return false;
      return true;
    });
  }, [state.data, hasActiveFilter, q, fromDate, toDate, amountMin, amountMax, journalEntryId, journalLineId]);

  if (state.loading && !state.data)
    return <PageState kind="loading" title="Henter posteringer" />;
  if (state.error)
    return <PageState kind="error" title="Posteringer kunne ikke hentes" onRetry={state.reload}>{state.error}</PageState>;

  const j = state.data!;
  const currency = j.company.currency || "DKK";
  const totalCount = j.entries.length;
  const matchCount = filteredEntries.length;
  const pageSize = 25;
  const pageEntries = filteredEntries.slice(page * pageSize, page * pageSize + pageSize);

  return (
    <section className="statement" data-cockpit-page="journal" data-evidence-issue="652">
      <div className="page-head">
        <div>
          <h2>{j.company.name}</h2>
          <h3 data-evidence-heading>Posteringer</h3>
          <p className="muted" data-evidence-status={filteredEntries.length ? "normal" : "empty"}>{filteredEntries.length ? "Posteringer klar" : "Ingen posteringer i perioden"}</p>
          <p className="muted">
            {j.company.cvr ? `CVR ${j.company.cvr} · ` : ""}
            {j.company.country} · {currency} · Posteringer
          </p>
        </div>
        <PageHeaderActions>
          {/* #465 — revisor-anmodning: hele kassekladden som CSV. URL'en
              bærer den aktive konto-drilldown med, så ejeren kan eksportere
              "kun denne konto"-udsnittet direkte. */}
          <a
            className="btn secondary"
            href={api.journalCsvUrl(slug, j.selectedYear, account ?? null)}
            download
          >
            Hent CSV
          </a>
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </PageHeaderActions>
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

      <FilterBar activeFilters={[
        q && `Søgning: ${q}`, fromDate && `Fra: ${fromDate}`, toDate && `Til: ${toDate}`,
        amountMin && `Beløb fra: ${amountMin}`, amountMax && `Beløb til: ${amountMax}`,
      ].filter(Boolean) as string[]} onReset={clearAllFilters} resetLabel="Ryd filtre" advanced={<><FormField label="Beløb min">
          <input
            type="number"
            inputMode="decimal" value={amountMin} placeholder="0" onChange={(e) => setFilter("amountMin", e.target.value)}
          />
        </FormField><FormField label="Beløb maks">
          <input
            type="number"
            inputMode="decimal" value={amountMax} placeholder="∞" onChange={(e) => setFilter("amountMax", e.target.value)}
          />
        </FormField></>}>
        <FormField label="Søg"><input type="search" value={q} placeholder="Søg på tekst, bilagsnummer eller konto…" onChange={(e) => setFilter("q", e.target.value)} /></FormField>
        <FormField label="Fra"><input type="date" value={fromDate} onChange={(e) => setFilter("from", e.target.value)} /></FormField>
        <FormField label="Til"><input type="date" value={toDate} onChange={(e) => setFilter("to", e.target.value)} /></FormField>
      </FilterBar>

      <p className="statement-asof muted">
        {j.periodStart} – {j.periodEnd} ·{" "}
        {hasActiveFilter
          ? `${matchCount} af ${totalCount} posteringer matcher`
          : `${totalCount} posteringer`}
      </p>
      {filteredEntries.length === 0 ? (
        <PageState kind="empty" title={hasActiveFilter ? "Ingen posteringer" : "Ingen posteringer i året"}>
          {hasActiveFilter
              ? "Ingen posteringer matcher filtrene."
              : j.accountFilter
                ? "Ingen posteringer på kontoen i året."
                : "Ingen posteringer i året."}
        </PageState>
      ) : (
        <ul className="entry-list" aria-label="Posteringer">
          {pageEntries.map((entry) => (
            <EntryRow
              key={entry.id}
              entry={entry}
              currency={currency}
              slug={slug}
              archived={j.archived}
              open={selectedEntryId === entry.id}
              onSelect={() => setSelectedEntryId((current) => current === entry.id ? null : entry.id)}
            />
          ))}
        </ul>
      )}
      {filteredEntries.length > pageSize && <nav className="row-actions" aria-label="Sider"><button type="button" className="btn secondary" disabled={page === 0} onClick={() => setPage((p) => p - 1)}>Forrige</button><span className="muted">Side {page + 1} af {Math.ceil(filteredEntries.length / pageSize)}</span><button type="button" className="btn secondary" disabled={(page + 1) * pageSize >= filteredEntries.length} onClick={() => setPage((p) => p + 1)}>Næste</button></nav>}
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
  open,
  onSelect,
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
  open: boolean;
  onSelect: () => void;
}) {
  // #379 — en post har et bilag når både linkage og fil-route er meningsfulde.
  // Arkiverede år vises altid som "Intet bilag" (filen er ikke i `documents`).
  const hasDocument = !archived && entry.documentId !== null;
  return (
    <li className={`entry-item${open ? " open" : ""}`}>
      <button
        type="button"
        className="entry-summary"
        aria-expanded={open}
        onClick={onSelect}
      >
        <span className="entry-caret" aria-hidden="true">
          {open ? "▾" : "▸"}
        </span>
        <span className="entry-no">{entry.entryNo}</span>
        <span className="entry-date">{entry.date}</span>
        <span className="entry-text"><PartyLink slug={slug} partyId={entry.partyId}>{entry.text}</PartyLink></span>
        <span className="muted">{entry.documentNo ? "Bilag" : "Bilag mangler"}</span>
        <span className="muted">Bogført</span>
        <span className="entry-total num">
          {formatKroner(entry.total, currency)}
        </span>
      </button>
      {open && (
        <div className="entry-lines table-scroll">
          <table className="data statement-table">
            <thead>
              <tr>
                <th scope="col">Konto</th>
                <th scope="col">Navn</th>
                <th className="num" scope="col">Debet</th>
                <th className="num" scope="col">Kredit</th>
                <th scope="col">Dimensioner</th>
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
                  <td>
                    {line.journalLineId === null ? (
                      <span className="muted">Ingen dimensionshistorik</span>
                    ) : (
                      <DimensionAssignments slug={slug} journalLineId={line.journalLineId} />
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="entry-bilag">
            <ExplanationPanel slug={slug} entryId={entry.id} />
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

function ExplanationPanel({ slug, entryId }: { slug: string; entryId: number }) {
  const state = useAsync<any>(() => api.journalExplanation(slug, entryId), [slug, entryId]);
  if (state.loading && !state.data) return <span className="muted">Henter forklaring…</span>;
  if (state.error) return <span className="muted">Forklaringen kunne ikke hentes.</span>;
  const e = state.data;
  return <details className="dimension-assignment"><summary>Forklar posten</summary><p>Konkrete kilder vises kun, når de er eksplicit knyttet til posteringen.</p>
    <p><strong>Faglig vurdering:</strong> {e.professionalAssessment.text}</p>
    <p><strong>Lovgrundlag:</strong> {e.legalSource.text}</p>
    {e.appliedRule && <p><strong>Anvendt Rentemester-regel:</strong> {e.appliedRule.ruleId} v{e.appliedRule.version} (gælder fra {e.appliedRule.effectiveFrom}).</p>}
    {e.correction.sentence && <p>{e.correction.sentence}</p>}
    <details><summary>Evidens</summary><code>{e.evidence.entryHash}</code></details>
  </details>;
}

type Allocation = { dimensionId: string; memberId: string; amountMinor: number; currency: string };
type AssignmentEvent = {
  id: number; allocations_json: string; source: string; source_ref: string | null; plan_hash: string;
  event_type: "assigned" | "superseded"; supersedes_assignment_id: number | null;
  actor: string; principal: string; created_at: string;
};

function parseAllocations(value: string): Allocation[] {
  try {
    const parsed: unknown = JSON.parse(value);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((item): item is Allocation => Boolean(item) && typeof item === "object" &&
      typeof (item as Allocation).dimensionId === "string" && typeof (item as Allocation).memberId === "string" &&
      Number.isSafeInteger((item as Allocation).amountMinor) && typeof (item as Allocation).currency === "string");
  } catch { return []; }
}

/** Current means an assigned event that no later supersession explicitly retires. */
function currentAssignment(events: AssignmentEvent[]): AssignmentEvent | null {
  const retired = new Set(events.map((event) => event.supersedes_assignment_id).filter((id): id is number => id !== null));
  return events.find((event) => event.event_type === "assigned" && !retired.has(event.id)) ?? null;
}

function DimensionAssignments({ slug, journalLineId }: { slug: string; journalLineId: number }) {
  const state = useAsync<AssignmentEvent[]>(() => api.dimensionAssignments(slug, journalLineId) as Promise<AssignmentEvent[]>, [slug, journalLineId]);
  if (state.loading && !state.data) return <span className="muted">Henter dimensioner…</span>;
  if (state.error) return <span className="muted">Dimensionshistorik kunne ikke hentes</span>;
  const events = state.data ?? [];
  const current = currentAssignment(events);
  if (!current) return <span className="muted">Ingen godkendt dimension</span>;
  const allocations = parseAllocations(current.allocations_json);
  return <details className="dimension-assignment">
    <summary>{allocations.map((item) => `${item.dimensionId}: ${item.memberId}`).join(", ") || "Godkendt dimension"}</summary>
    <p className="muted">
      Kilde: {current.source} · plan-hash <code>{current.plan_hash}</code><br />
      {current.source_ref && <>Kildereference: {current.source_ref}<br /></>}
      Godkendt af {current.actor} via {current.principal} · {current.created_at}
    </p>
    <DimensionReview slug={slug} journalLineId={journalLineId} current={current} onChanged={state.reload} />
  </details>;
}

/**
 * The legal posting is immutable. A correction therefore first retires the
 * current allocation and only then applies the exact hash-bound replacement.
 * This compact form intentionally accepts only explicit allocation ids and
 * amounts — it does not guess a dimension or silently redistribute amounts.
 */
function DimensionReview({ slug, journalLineId, current, onChanged }: { slug: string; journalLineId: number; current: AssignmentEvent; onChanged: () => void }) {
  const [editing, setEditing] = useState(false);
  const [allocations, setAllocations] = useState<Allocation[]>(() => parseAllocations(current.allocations_json));
  const [plan, setPlan] = useState<{ planHash: string } | null>(null);
  const [reviewed, setReviewed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmSupersede, setConfirmSupersede] = useState(false);

  function change(index: number, key: keyof Allocation, value: string) {
    setPlan(null); setReviewed(false);
    setAllocations((rows) => rows.map((row, i) => i === index
      ? { ...row, [key]: key === "amountMinor" ? Number(value) : value } : row));
  }
  async function makePlan() {
    setError(null);
    try {
      const result = await api.planDimensionAssignment(slug, { journalLineId, allocations, source: "reviewed" });
      if (!result.ok || !result.plan) throw new Error(result.errors?.join(", ") || "Planen kunne ikke valideres.");
      setPlan(result.plan);
    } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
  }
  async function replace(reason: string) {
    if (!reason.trim()) throw new Error("Skriv en begrundelse for korrektionen.");
    if (!plan || !reviewed) throw new Error("Gennemgå først den præcise plan.");
    await api.replaceDimensionAssignment(slug, { journalLineId, expectedAssignmentId: current.id, allocations, source: "reviewed", planHash: plan.planHash, reason, idempotencyKey: `cockpit-dimension-replace-${current.id}-${plan.planHash}` });
    setEditing(false); setPlan(null); setReviewed(false); setConfirmSupersede(false); onChanged();
  }
  if (!editing) return <button type="button" className="btn secondary" onClick={() => setEditing(true)}>Gennemgå og ret</button>;
  return <div className="dimension-review card">
    <p><strong>Ret dimensionsklassifikation</strong></p>
    <p className="muted">Posteringen ændres aldrig. Den nuværende tildeling supersederes og den reviewede plan anvendes atomisk med begrundelse.</p>
    {allocations.map((allocation, index) => <div className="row-actions" key={`${allocation.dimensionId}-${index}`}>
      <label>Dimension<input aria-label={`Dimension ${index + 1}`} value={allocation.dimensionId} onChange={(event) => change(index, "dimensionId", event.target.value)} /></label>
      <label>Medlem<input aria-label={`Medlem ${index + 1}`} value={allocation.memberId} onChange={(event) => change(index, "memberId", event.target.value)} /></label>
      <label>Øre<input aria-label={`Øre ${index + 1}`} type="number" value={allocation.amountMinor} onChange={(event) => change(index, "amountMinor", event.target.value)} /></label>
    </div>)}
    <div className="row-actions"><button type="button" className="btn secondary" onClick={() => void makePlan()}>Validér revideret plan</button><button type="button" className="btn secondary" onClick={() => setEditing(false)}>Annullér</button></div>
    {plan && <div className="card"><p>Plan-hash: <code>{plan.planHash}</code></p><label><input type="checkbox" checked={reviewed} onChange={(event) => setReviewed(event.target.checked)} /> Jeg har gennemgået den præcise plan.</label><button type="button" className="btn secondary" disabled={!reviewed} onClick={() => setConfirmSupersede(true)}>Erstat nuværende tildeling atomisk</button></div>}
    {error && <p className="muted" role="alert">{error}</p>}
    {confirmSupersede && <ConfirmDialog title="Erstat dimensionsklassifikation" body={<p>Den nuværende klassifikation bevares i revisionssporet. Supersession og den præcise, hash-bundne erstatning gemmes atomisk, så linjen aldrig står uden en aktuel tildeling.</p>} confirmLabel="Erstat tildeling" confirmKind="danger" noteLabel="Begrundelse" onConfirm={replace} onClose={() => setConfirmSupersede(false)} />}
  </div>;
}
