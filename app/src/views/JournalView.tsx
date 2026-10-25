import { VirtualJournalList, journalEntryKey, type VirtualJournalRowProps } from "../components/VirtualJournalList";
import { ReportSourceNotice } from "../components/ReportSourceNotice";
import * as stylex from "@stylexjs/stylex";
import { ButtonLink, Button, Input, PageHeader } from "../components/ui";
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

import { JournalReversalForm } from "../components/JournalReversalForm";
import { ManualJournalForm } from "../components/ManualJournalForm";
import { useCapabilities } from "../lib/useCapabilities";
import { useMemo, useState } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner, compareMoney, moneyZero } from "../lib/format";
// #379 — the EntryRow needs the slug to build the bilag-file URL on the fly.
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import type { CompanyJournal } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ArchivedBanner } from "../components/ArchivedBanner";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";

const FILTER_PARAM_KEYS = ["q", "from", "to", "amountMin", "amountMax", "journalEntryId", "journalLineId"] as const;

export function JournalView() {
  const { slug = "" } = useParams();
  const [posting, setPosting] = useState(false);
  const { can } = useCapabilities();
  const { year, source, setYear } = useCompanyYear();
  // An optional account drill-down: `?account=<accountNo>` filters the journal
  // to the entries that touch that account (set by the statement views).
  const [params, setParams] = useSearchParams();
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
    (signal) => api.journal(slug, year, account, { signal, ...(source ? {source} : {}) }),
    [slug, year, account, source],
  );

  const filteredEntries = useMemo(() => {
    const entries = state.data?.entries ?? [];
    if (!hasActiveFilter) return entries;
    const needle = q.trim().toLowerCase();

    return entries.filter((entry) => {
      if (journalEntryId !== null && entry.id !== journalEntryId) return false;
      if (journalLineId !== null && !entry.lines.some((line) => line.journalLineId === journalLineId)) return false;
      if (needle !== "" && !entryMatchesText(entry, needle)) return false;
      if (fromDate !== "" && entry.date < fromDate) return false;
      if (toDate !== "" && entry.date > toDate) return false;
      if (amountMin !== "" && compareMoney(entry.total, amountMin) === -1)
        return false;
      if (amountMax !== "" && compareMoney(entry.total, amountMax) === 1)
        return false;
      return true;
    });
  }, [state.data, hasActiveFilter, q, fromDate, toDate, amountMin, amountMax, journalEntryId, journalLineId]);

  if (state.loading && !state.data)
    return <Loading label="Henter posteringer…" />;
  if (state.error && (!state.data || state.data.archived || source === "archive"))
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const j = state.data!;
  const currency = j.declaredCurrency ?? j.company.currency ?? "DKK";
  const selectedSource = j.selectedSource ?? source;
  const totalCount = j.entries.length;
  const matchCount = filteredEntries.length;

  return (
    <section className="statement">
      {state.error && <div className="banner warning" role="alert">Status kunne ikke opdateres. De tidligere hentede oplysninger vises fortsat.</div>}
      <PageHeader title="Posteringer" actions={<><div className="row-actions">
          {!j.archived && j.nativePosting && can("company.ledger.post") && <Button onClick={() => setPosting(true)}>Ny postering</Button>}
          <ButtonLink className="btn secondary" to={`/companies/${slug}/saldobalance${j.selectedYear ? `?year=${encodeURIComponent(j.selectedYear)}${selectedSource ? `&source=${selectedSource}` : ""}` : ""}`}>Saldobalance</ButtonLink>
          {/* #465 — revisor-anmodning: hele kassekladden som CSV. URL'en
              bærer den aktive konto-drilldown med, så ejeren kan eksportere
              "kun denne konto"-udsnittet direkte. */}
          <a
            className="btn secondary"
            href={api.journalCsvUrl(slug, j.selectedYear, account ?? null, selectedSource)}
            download
          >
            Hent CSV
          </a>
          <a className="btn secondary" href={api.journalPdfUrl(slug,j.selectedYear,account??null,selectedSource)} download>Hent PDF</a>
          <ButtonLink className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div>

          <p className="muted">
            {j.company.cvr ? `CVR ${j.company.cvr} · ` : ""}
            {j.company.country} · {currency} · Posteringer
          </p>
        </div>

      </PageHeader>

      {!j.archived && posting && j.nativePosting && <ManualJournalForm slug={slug} currency={currency} options={j.nativePosting} onSaved={state.reload} onClose={() => setPosting(false)} />}

      <CompanyNav
        slug={slug}
        years={j.fiscalYears}
        selectedYear={j.selectedYear}
        selectedSource={selectedSource}
        onYearChange={setYear}
      />

      <ReportSourceNotice report={j} slug={slug}/>
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
          <Button requiredPermission="company.review" variant="secondary"
            type="button"
            className="btn secondary"
            onClick={clearAccount}
          >
            Vis alle posteringer
          </Button>
        </div>
      )}

      <div className="journal-filter-bar card" role="search">
        <label className="journal-filter-field journal-filter-field--search">
          <span className="muted">Søg</span>
          <Input
            type="search"
            value={q}
            placeholder="Søg på tekst, bilagsnummer eller konto…"
            onChange={(e) => setFilter("q", e.target.value)}
          />
        </label>
        <label className="journal-filter-field">
          <span className="muted">Fra</span>
          <Input
            type="date"
            value={fromDate}
            onChange={(e) => setFilter("from", e.target.value)}
          />
        </label>
        <label className="journal-filter-field">
          <span className="muted">Til</span>
          <Input
            type="date"
            value={toDate}
            onChange={(e) => setFilter("to", e.target.value)}
          />
        </label>
        <label className="journal-filter-field">
          <span className="muted">Beløb min</span>
          <Input
            type="number"
            inputMode="decimal"
            value={amountMin}
            placeholder="0"
            onChange={(e) => setFilter("amountMin", e.target.value)}
          />
        </label>
        <label className="journal-filter-field">
          <span className="muted">Beløb maks</span>
          <Input
            type="number"
            inputMode="decimal"
            value={amountMax}
            placeholder="∞"
            onChange={(e) => setFilter("amountMax", e.target.value)}
          />
        </label>
        {hasActiveFilter && (
          <Button requiredPermission="company.review" variant="secondary"
            type="button"
            className="btn secondary"
            onClick={clearAllFilters}
          >
            Ryd filtre
          </Button>
        )}
      </div>

      <p className="statement-asof muted">
        {j.periodStart && j.periodEnd ? `${j.periodStart} – ${j.periodEnd}` : "Regnskabsperiodens grænser er ikke oplyst"} ·{" "}
        {hasActiveFilter
          ? `${matchCount} af ${totalCount} posteringer matcher`
          : `${totalCount} posteringer`}
      </p>
      {filteredEntries.length === 0 ? (
        <div className="card statement-card">
          <p className={["empty-inline", stylex.props(viewStyles.site0).className].filter(Boolean).join(" ")} >
            {hasActiveFilter
              ? "Ingen posteringer matcher filtrene."
              : j.accountFilter
                ? "Ingen posteringer på kontoen i året."
                : "Ingen posteringer i året."}
          </p>
        </div>
      ) : j.entries.length > 200 ? (
        <VirtualJournalList
          key={JSON.stringify([slug, year, source, account])}
          entries={filteredEntries}
          renderEntry={(entry, virtualProps, onOpenChange) => <EntryRow
            key={journalEntryKey(entry)} entry={entry} currency={currency} slug={slug}
            archived={j.archived} reversalRuleVersion={j.nativePosting?.ruleVersion}
            onChanged={state.reload} virtualProps={virtualProps} onOpenChange={onOpenChange}
          />}
        />
      ) : (
        <ul className="entry-list">
          {filteredEntries.map((entry) => (
            <EntryRow
              key={entry.id ?? entry.sourceId}
              entry={entry}
              currency={currency}
              slug={slug}
              archived={j.archived}
              reversalRuleVersion={j.nativePosting?.ruleVersion}
              onChanged={state.reload}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function entryMatchesText(entry: CompanyJournal["entries"][number], needle: string): boolean {
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
  reversalRuleVersion,
  onChanged,
  virtualProps,
  onOpenChange,
}: {
  virtualProps?: VirtualJournalRowProps;
  onOpenChange?: (open: boolean) => void;
  reversalRuleVersion?: string;
  onChanged?: () => void;
  entry: CompanyJournal["entries"][number];
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
  const [reversing, setReversing] = useState(false);
  const { can } = useCapabilities();
  // #379 — en post har et bilag når både linkage og fil-route er meningsfulde.
  // Arkiverede år vises altid som "Intet bilag" (filen er ikke i `documents`).
  const hasDocument = !archived && entry.documentId !== null;
  return (
    <li {...virtualProps} className={`entry-item${open ? " open" : ""}`}>
      <Button
        type="button"
        className="entry-summary"
        aria-expanded={open}
        onClick={() => { setOpen(!open); onOpenChange?.(!open || reversing); }}
      >
        <span className="entry-caret" aria-hidden="true">
          {open ? "▾" : "▸"}
        </span>
        <span className="entry-no">{entry.entryNo}</span>
        <span className="entry-date">{entry.date}</span>
        <span {...stylex.props(viewStyles.entryText)}>{entry.text}</span>
        <span className="entry-total num">
          {formatKroner(entry.total, currency)}
        </span>
      </Button>
      {open && (
        <div className="entry-lines table-scroll">
          {entry.reversalOfEntryNo && <p>Tilbageførsel af {entry.reversalOfEntryNo}</p>}
          {entry.reversedByEntryNo && <p>Tilbageført ved {entry.reversedByEntryNo}</p>}
          {reversalRuleVersion && !archived && entry.reversalOfEntryId == null && entry.reversedByEntryId == null && can("company.ledger.post") && <Button variant="secondary" onClick={() => setReversing(true)}>Tilbagefør postering</Button>}
          {reversing && reversalRuleVersion && entry.id !== null && <JournalReversalForm slug={slug} entry={entry} ruleVersion={reversalRuleVersion} onSaved={() => onChanged?.()} onClose={() => setReversing(false)} />}
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
                    {moneyZero(line.debit) ? "—" : formatKroner(line.debit, currency)}
                  </td>
                  <td className="num">
                    {moneyZero(line.credit) ? "—" : formatKroner(line.credit, currency)}
                  </td>
                  <td>
                    {line.sourceAmount && <details><summary>Original kildepost</summary><p>Kilde-ID: {line.sourceId}</p><p>Originalbeløb: {line.sourceAmount} · rapportsaldo: {formatKroner(line.balance ?? null,currency)}</p><p>Original dato: {line.date} · bilagstype: {line.voucherType} · momstype: {line.vatType}</p>{line.sourceRunningBalance !== null && line.sourceRunningBalance !== undefined && <p>Original løbende saldo: {line.sourceRunningBalance}</p>}</details>}
                    {archived || line.journalLineId === null || typeof line.debit !== "number" || typeof line.credit !== "number" ? (
                      <span className="muted">Ingen dimensionshistorik</span>
                    ) : (
                      <DimensionAssignments slug={slug} journalLineId={line.journalLineId} currency={currency} amountMinor={Math.round(Math.abs(line.debit - line.credit) * 100)} />
                    )}
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

type Allocation = { dimensionId: string; memberId: string; amountMinor: number; currency: string };
type AssignmentEvent = {
  id: number; allocations_json: string; source: string; source_ref: string | null; plan_hash: string;
  event_type: "assigned" | "superseded"; supersedes_assignment_id: number | null; reason?: string | null;
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
function currentAssignment(events: AssignmentEvent[]): AssignmentEvent | null {
  const retired = new Set(events.map(e => e.supersedes_assignment_id).filter((id): id is number => id !== null));
  return events.find(e => e.event_type === "assigned" && !retired.has(e.id)) ?? null;
}
function DimensionAssignments({ slug, journalLineId, currency, amountMinor }: { slug: string; journalLineId: number; currency: string; amountMinor: number }) {
  const state = useAsync<AssignmentEvent[]>((signal) => api.dimensionAssignments(slug, journalLineId, { signal }) as Promise<AssignmentEvent[]>, [slug, journalLineId]);
  if (state.loading && !state.data) return <span className="muted">Henter dimensioner…</span>;
  if (state.error && !state.data) return <span className="muted">Dimensionshistorik kunne ikke hentes</span>;
  const events = state.data ?? [], current = currentAssignment(events);
  const allocations = current ? parseAllocations(current.allocations_json) : [];
  return <details className="dimension-assignment">
    <summary>{current ? allocations.map(a => `${a.dimensionId}: ${a.memberId}`).join(", ") || "Godkendt dimension" : "Ingen godkendt dimension"}</summary>
    {state.error && <p role="alert">Dimensionshistorik kunne ikke opdateres. Den tidligere hentede historik vises fortsat.</p>}
    {current && <p className="muted">Kilde: {current.source} · plan-hash <code>{current.plan_hash}</code><br />
      {current.source_ref && <>Kildereference: {current.source_ref}<br /></>}
      Godkendt af {current.actor} via {current.principal} · {current.created_at}</p>}
    <DimensionReview key={current?.id ?? "new"} slug={slug} journalLineId={journalLineId} current={current} currency={currency} amountMinor={amountMinor} onChanged={state.reload} />
    {events.length > 0 && <details><summary>Tildelingshistorik ({events.length})</summary><ol>{events.map(e => <li key={e.id}>
      #{e.id} · {e.event_type} · {parseAllocations(e.allocations_json).map(a => `${a.dimensionId}: ${a.memberId} — ${a.amountMinor} øre ${a.currency}`).join(", ")}<br />
      {e.supersedes_assignment_id !== null && <>Erstatter hændelse #{e.supersedes_assignment_id}. </>}{e.reason && <>Begrundelse: {e.reason}<br /></>}
      {e.actor} via {e.principal} · {e.created_at} · plan-hash <code>{e.plan_hash}</code>
    </li>)}</ol></details>}
  </details>;
}
/** All writes use an explicitly reviewed plan; replacements append retirement and assignment atomically. */
function DimensionReview({ slug, journalLineId, current, currency, amountMinor, onChanged }: { slug: string; journalLineId: number; current: AssignmentEvent | null; currency: string; amountMinor: number; onChanged: () => void }) {
  const initial = () => current ? parseAllocations(current.allocations_json) : [{ dimensionId: "", memberId: "", amountMinor, currency }];
  const [editing, setEditing] = useState(false);
  const [allocations, setAllocations] = useState<Allocation[]>(initial);
  const [plan, setPlan] = useState<{ planHash: string; journalEntryHash?: string; lineAmountMinor?: number; lineCurrency?: string } | null>(null);
  const [reviewed, setReviewed] = useState(false), [error, setError] = useState<string | null>(null), [confirming, setConfirming] = useState(false);
  const outcome = useMutationOutcome(onChanged);
  const guard = useDiscardGuard(editing && JSON.stringify(allocations) !== JSON.stringify(initial()), () => {
    setEditing(false); setPlan(null); setReviewed(false); setConfirming(false); setAllocations(initial());
  });
  const invalidate = () => { setPlan(null); setReviewed(false); };
  function change(index: number, key: keyof Allocation, value: string) {
    invalidate(); setAllocations(rows => rows.map((row, i) => i === index ? { ...row, [key]: key === "amountMinor" ? Number(value) : value } : row));
  }
  async function makePlan() {
    if (outcome.isBlocked()) return;
    setError(null); invalidate();
    try {
      const result = await api.planDimensionAssignment(slug, { journalLineId, allocations, source: "reviewed" });
      if (!result.ok || !result.plan) throw new Error(result.errors?.join(", ") || "Planen kunne ikke valideres.");
      setPlan(result.plan);
    } catch (cause) { setError(cause instanceof Error ? cause.message : String(cause)); }
  }
  async function apply(reason: string) {
    if (outcome.isBlocked()) return;
    if (!plan || !reviewed) throw new Error("Gennemgå først den præcise plan.");
    if (current && !reason.trim()) throw new Error("Skriv en begrundelse for korrektionen.");
    const input = { journalLineId, allocations, source: "reviewed" as const, planHash: plan.planHash, idempotencyKey: `cockpit-dimension-${current?.id ?? "new"}-${plan.planHash}` };
    await (current ? api.replaceDimensionAssignment(slug, { ...input, expectedAssignmentId: current.id, reason }) : api.applyDimensionAssignment(slug, input)).catch(outcome.reject);
    guard.dismiss(); onChanged();
  }
  if (!editing) return <>{outcome.feedback}<Button requiredPermission={current ? "company.review" : "company.draft.write"} disabled={outcome.blocked} variant="secondary" type="button" className="btn secondary" onClick={() => setEditing(true)}>{current ? "Gennemgå og ret" : "Fordel på dimensioner"}</Button></>;
  return <div className="dimension-review card">
    {outcome.feedback}{guard.confirmation}<p><strong>{current ? "Ret dimensionsklassifikation" : "Fordel på dimensioner"}</strong></p>
    <p className="muted">Posteringen ændres aldrig. Fordelingen gennemgås som en præcis, hash-bundet plan. En rettelse bevarer den tidligere tildeling med begrundelse.</p>
    {allocations.map((allocation, index) => <div className="row-actions" key={index}>
      <label>Dimension<Input disabled={outcome.blocked} aria-label={`Dimension ${index + 1}`} value={allocation.dimensionId} onChange={e => change(index, "dimensionId", e.target.value)} /></label>
      <label>Medlem<Input disabled={outcome.blocked} aria-label={`Medlem ${index + 1}`} value={allocation.memberId} onChange={e => change(index, "memberId", e.target.value)} /></label>
      <label>Øre ({allocation.currency})<Input disabled={outcome.blocked} aria-label={`Øre ${index + 1}`} type="number" min="1" step="1" value={allocation.amountMinor} onChange={e => change(index, "amountMinor", e.target.value)} /></label>
      {allocations.length > 1 && <Button variant="secondary" disabled={outcome.blocked} onClick={() => { invalidate(); setAllocations(rows => rows.filter((_, i) => i !== index)); }}>Fjern fordeling {index + 1}</Button>}
    </div>)}
    <div className="row-actions"><Button variant="secondary" disabled={outcome.blocked} onClick={() => { invalidate(); setAllocations(rows => [...rows, { dimensionId: "", memberId: "", amountMinor: 0, currency }]); }}>Tilføj fordeling</Button><Button variant="secondary" type="button" disabled={outcome.blocked} onClick={() => void makePlan()}>Validér revideret plan</Button><Button variant="secondary" type="button" onClick={guard.onClose}>Annullér</Button></div>
    {plan && <div className="card"><p>Plan-hash: <code>{plan.planHash}</code></p>{plan.journalEntryHash && <p>Journal-hash: <code>{plan.journalEntryHash}</code> · {plan.lineAmountMinor} øre {plan.lineCurrency}</p>}<label><Input disabled={outcome.blocked} type="checkbox" checked={reviewed} onChange={e => setReviewed(e.target.checked)} /> Jeg har gennemgået den præcise plan.</label><Button variant="secondary" type="button" requiredPermission={current ? "company.review" : "company.draft.write"} disabled={!reviewed || outcome.blocked} onClick={() => setConfirming(true)}>{current ? "Erstat nuværende tildeling atomisk" : "Anvend godkendt fordeling"}</Button></div>}
    {error && <p role="alert">{error}</p>}
    {confirming && <ConfirmDialog title={current ? "Erstat dimensionsklassifikation" : "Anvend dimensionsklassifikation"} body={<p>Den præcise, hash-bundne fordeling gemmes i revisionssporet. Journalens beløb og identitet ændres aldrig.</p>} confirmLabel={current ? "Erstat tildeling" : "Anvend tildeling"} confirmKind={current ? "danger" : "primary"} noteLabel={current ? "Begrundelse" : undefined} onConfirm={apply} onClose={() => setConfirming(false)} onRefresh={onChanged} />}
  </div>;
}

const viewStyles = stylex.create({
  entryText: { minWidth: 0, flexGrow: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
site0: { padding: "var(--space-md)" }
});
