// Undtagelser — the cockpit exception queue (#332).
//
// The agent + human-in-the-loop discipline (jf. `feedback_determinism_human_loop.md`)
// rests on the exception queue: usikre regelfortolkninger afgøres af et
// menneske ved bogføring. Renders `/api/companies/:slug/exceptions`: every
// open / resolved / arkiveret exception with its trigger-rule type, severity,
// human-facing message and the concrete `requiredAction` text the agent loop
// recorded. A "Løs"-knap opens a confirm-modal that POSTs to the existing
// `POST .../exceptions/:id/resolve` endpoint — write-irreversible through
// `withCompanyMutation`, so the audit-log captures the cockpit's `resolvedBy`.
//
// Three status pills (Åbne / Løste / Alle) drive `?status=`. The Overblik
// "Opgaver" card uses the SAME grouped summary lines this view shows above
// the table — both surfaces come from `groupExceptions`, so they cannot
// diverge on counts or wording.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type {
  CompanyExceptionGroup,
  CompanyExceptionRow,
  CompanyExceptionStatusFilter,
  CompanyExceptions,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";

const FILTERS: { value: CompanyExceptionStatusFilter; label: string }[] = [
  { value: "open", label: "Åbne" },
  { value: "resolved", label: "Løste" },
  { value: "all", label: "Alle" },
];

/**
 * Human-readable severity flag. The colour tone is set by `flag` + a kind
 * class to match the rest of the cockpit (`ok` / `neutral` / `critical`).
 */
const SEVERITY_LABEL: Record<CompanyExceptionRow["severity"], string> = {
  high: "Høj",
  medium: "Medium",
  low: "Lav",
};

const SEVERITY_TONE: Record<CompanyExceptionRow["severity"], string> = {
  high: "critical",
  medium: "neutral",
  low: "ok",
};

/**
 * The cockpit's `?status=` query value, narrowed to a valid filter (defaults
 * to "open" for any unknown value). Carrying the filter in the URL means a
 * deep-link from the Overblik "Opgaver" card lands on the right sub-list.
 */
function readStatusParam(): CompanyExceptionStatusFilter {
  const params = new URLSearchParams(window.location.search);
  const raw = params.get("status");
  if (raw === "resolved" || raw === "all" || raw === "open") return raw;
  return "open";
}

export function ExceptionsView() {
  const { slug = "" } = useParams();
  const { setYear } = useCompanyYear();
  const [filter, setFilter] = useState<CompanyExceptionStatusFilter>(
    readStatusParam(),
  );
  const [resolving, setResolving] = useState<CompanyExceptionRow | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const state = useAsync<CompanyExceptions>(
    () => api.exceptions(slug, filter),
    [slug, filter],
  );

  if (state.loading && !state.data) {
    return <Loading label="Henter undtagelseskø…" />;
  }
  if (state.error) {
    return <ErrorState message={state.error} onRetry={state.reload} />;
  }

  const view = state.data!;
  const currency = view.company.currency || "DKK";

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{view.company.name}</h2>
          <p className="muted">
            {view.company.cvr ? `CVR ${view.company.cvr} · ` : ""}
            {view.company.country} · {currency} · Undtagelser
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
        years={view.fiscalYears}
        selectedYear={view.fiscalYears[0]?.label ?? ""}
        onYearChange={setYear}
      />

      <p className="statement-asof muted">
        Undtagelseskøen er hjertet i agent + menneske-i-løkken: usikre
        regelfortolkninger afgøres her, ikke automatisk. Hver post viser
        trigger-reglen, agentens forslag og hvad du skal gøre, før den kan
        løses.
      </p>

      <div className="status-grid invoices-summary">
        <div className="card status-card">
          <h3>Åbne</h3>
          <div
            className={`status-figure${
              view.openCount > 0 ? " status-alert" : ""
            }`}
          >
            {view.openCount}
          </div>
          <p className="muted status-note">
            {view.openCount === 0
              ? "Intet kræver gennemgang lige nu."
              : view.openCount === 1
                ? "1 sag venter på din afgørelse."
                : `${view.openCount} sager venter på din afgørelse.`}
          </p>
        </div>
        <div className="card status-card">
          <h3>Løste</h3>
          <div className="status-figure">{view.resolvedCount}</div>
          <p className="muted status-note">
            Audit-sporet bevarer alle løste sager.
          </p>
        </div>
        <div className="card status-card">
          <h3>I alt</h3>
          <div className="status-figure">
            {view.openCount + view.resolvedCount}
          </div>
          <p className="muted status-note">
            Append-only — sletning er ikke mulig.
          </p>
        </div>
      </div>

      {view.openGroups.length > 0 && (
        <section
          className="card statement-card"
          aria-label="Åbne undtagelser pr. type"
        >
          <h3>Pr. type</h3>
          <ul className="exceptions-groups">
            {view.openGroups.map((g) => (
              <ExceptionGroupLine key={g.type} group={g} />
            ))}
          </ul>
        </section>
      )}

      <nav
        className="filter-pills"
        aria-label="Filtrér undtagelser på status"
      >
        {FILTERS.map((f) => (
          <button
            key={f.value}
            type="button"
            className={`btn pill${filter === f.value ? " active" : ""}`}
            onClick={() => {
              setFilter(f.value);
              setActionError(null);
            }}
            aria-pressed={filter === f.value}
          >
            {f.label}
          </button>
        ))}
      </nav>

      {actionError ? (
        <div className="card archived-notice" role="alert">
          <p className="muted">{actionError}</p>
        </div>
      ) : null}

      {view.rows.length === 0 ? (
        <div className="card statement-card empty-state">
          <h3>Ingen undtagelser i visningen</h3>
          <p className="muted">
            {filter === "open"
              ? "Intet venter på din afgørelse — godt arbejde."
              : filter === "resolved"
                ? "Ingen løste undtagelser i køen endnu."
                : "Der er ingen undtagelser registreret endnu."}
          </p>
        </div>
      ) : (
        <div className="card statement-card table-scroll">
          <table className="data statement-table">
            <thead>
              <tr>
                <th>Type</th>
                <th>Sværhed</th>
                <th>Hvad agenten ser</th>
                <th>Hvad du skal gøre</th>
                <th>Oprettet</th>
                <th>Status</th>
                <th>Handling</th>
              </tr>
            </thead>
            <tbody>
              {view.rows.map((row) => (
                <ExceptionRowView
                  key={row.id}
                  row={row}
                  onResolve={() => {
                    setActionError(null);
                    setResolving(row);
                  }}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}

      {resolving ? (
        <ResolveExceptionModal
          slug={slug}
          row={resolving}
          onClose={() => setResolving(null)}
          onResolved={() => {
            setResolving(null);
            setActionError(null);
            state.reload();
          }}
          onError={setActionError}
        />
      ) : null}
    </section>
  );
}

function ExceptionGroupLine({ group }: { group: CompanyExceptionGroup }) {
  return (
    <li className={`exceptions-group exceptions-group-${group.severity}`}>
      <span className={`flag ${SEVERITY_TONE[group.severity]}`}>
        {SEVERITY_LABEL[group.severity]}
      </span>
      <span className="exceptions-group-label">{group.label}</span>
    </li>
  );
}

function ExceptionRowView({
  row,
  onResolve,
}: {
  row: CompanyExceptionRow;
  onResolve: () => void;
}) {
  const tone = SEVERITY_TONE[row.severity];
  return (
    <tr>
      <td className="account-no">{row.type}</td>
      <td>
        <span className={`flag ${tone}`}>{SEVERITY_LABEL[row.severity]}</span>
      </td>
      <td>{row.message}</td>
      <td>{row.requiredAction ?? "—"}</td>
      <td className="entry-date">{row.createdAt}</td>
      <td>
        {row.status === "open" ? (
          <span className="flag neutral">Åben</span>
        ) : (
          <span
            className="flag ok"
            title={
              row.resolvedBy
                ? `Løst af ${row.resolvedBy}${
                    row.resolvedAt ? ` (${row.resolvedAt})` : ""
                  }`
                : "Løst"
            }
          >
            Løst
          </span>
        )}
      </td>
      <td>
        {row.status === "open" ? (
          <button
            type="button"
            className="btn secondary"
            onClick={onResolve}
            aria-label={`Løs undtagelse ${row.id}`}
          >
            Løs
          </button>
        ) : (
          row.resolutionNote && (
            <span className="muted" title={row.resolutionNote}>
              Note registreret
            </span>
          )
        )}
      </td>
    </tr>
  );
}

function ResolveExceptionModal({
  slug,
  row,
  onClose,
  onResolved,
  onError,
}: {
  slug: string;
  row: CompanyExceptionRow;
  onClose: () => void;
  onResolved: () => void;
  onError: (msg: string) => void;
}) {
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    setBusy(true);
    try {
      await api.resolveException(slug, {
        id: row.id,
        ...(note.trim() ? { note: note.trim() } : {}),
      });
      onResolved();
    } catch (err) {
      onError(
        err instanceof ApiError
          ? err.message
          : err instanceof Error
            ? err.message
            : "Kunne ikke løse undtagelsen.",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="card modal-card">
        <h3>Løs undtagelse #{row.id}</h3>
        <p className="muted">
          <strong>{row.type}</strong> · {SEVERITY_LABEL[row.severity]} sværhed.
          Sagen flyttes fra åben til løst — ledger-rækken bevares (append-only),
          og din afgørelse audit-logges med dit cockpit-navn som <em>resolvedBy</em>.
        </p>
        <p>{row.message}</p>
        {row.requiredAction ? (
          <p>
            <strong>Foreslået handling:</strong> {row.requiredAction}
          </p>
        ) : null}
        <form onSubmit={handleSubmit}>
          <label>
            <span>Note (valgfri) — hvad gjorde du og hvorfor</span>
            <input
              type="text"
              value={note}
              onChange={(e) => setNote(e.target.value)}
              placeholder="Fx: Bilag fundet og bogført manuelt"
              autoFocus
            />
          </label>
          <div className="row-actions">
            <button type="submit" className="btn" disabled={busy}>
              {busy ? "Løser…" : "Løs undtagelse"}
            </button>
            <button
              type="button"
              className="btn secondary"
              onClick={onClose}
              disabled={busy}
            >
              Annullér
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
