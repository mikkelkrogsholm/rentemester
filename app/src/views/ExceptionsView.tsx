// Exceptions queue view (#332) — per-virksomhed kø af undtagelser (unmatched
// bank-rows, blokerede write-flows, dokumenter uden bilag-pligt-link osv.).
// Listen kommer fra det nye GET /api/companies/:slug/exceptions endpoint;
// POST .../exceptions/:id/resolve er allerede implementeret i kernen (#213,
// slice 1) og bruges af 'Marker som løst'-knappen.

import { useState } from "react";
import { Link, useParams, useSearchParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type { CompanyExceptions, ExceptionRow } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";

const STATUS_TABS: Array<{ value: "open" | "resolved" | "all"; label: string }> = [
  { value: "open", label: "Åbne" },
  { value: "resolved", label: "Løste" },
  { value: "all", label: "Alle" },
];

const SEVERITY_LABEL: Record<ExceptionRow["severity"], string> = {
  high: "Høj",
  medium: "Medium",
  low: "Lav",
};

/**
 * Human-readable Danish labels for the raw `exception.type` codes the core
 * emits. Falls back to the raw code (in a <code>-tag) when an unknown type
 * shows up, so the screen never silently drops information. (Round-2 review:
 * "UNMATCHED_BANK_TRANSACTION raw code is shown to the user — should be
 * mapped to a Danish label.")
 */
const TYPE_LABEL: Record<string, string> = {
  UNMATCHED_BANK_TRANSACTION: "Ubehandlet banktransaktion",
  AGENT_NEEDS_REVIEW: "Agenten har brug for godkendelse",
  AGENT_PAYABLE_OVERDUE: "Forfalden leverandørpost (agent)",
  AGENT_ACCRUAL_READY: "Periodisering klar til bogføring (agent)",
  AGENT_ASSET_CANDIDATE: "Muligt anlæg over kapitaliseringsgrænsen (agent)",
  AGENT_TAX_NEEDS_REVIEW: "Oplysningsskema-felt skal kontrolleres (agent)",
  DOCUMENT_NO_BILAG: "Bilag mangler",
  PERIOD_LOCKED_WRITE: "Bogføring blokeret af periodelås",
  BACKUP_LOCKED_WRITE: "Bogføring blokeret af backup-lås",
};

export function ExceptionsView() {
  const { slug = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const statusRaw = params.get("status") ?? "open";
  const status: "open" | "resolved" | "all" =
    statusRaw === "resolved" || statusRaw === "all" ? statusRaw : "open";
  const [refresh, setRefresh] = useState(0);
  // Resolve-tilstand: undgå at klikke flere gange på samme række.
  const [resolving, setResolving] = useState<Set<number>>(new Set());
  const [resolveError, setResolveError] = useState<string | null>(null);

  const state = useAsync<CompanyExceptions>(
    () => api.exceptions(slug, status),
    [slug, status, refresh],
  );

  const setStatus = (next: "open" | "resolved" | "all") => {
    const updated = new URLSearchParams(params);
    if (next === "open") updated.delete("status");
    else updated.set("status", next);
    setParams(updated, { replace: true });
  };

  const resolve = async (row: ExceptionRow) => {
    if (resolving.has(row.id)) return;
    setResolveError(null);
    setResolving((s) => new Set([...s, row.id]));
    try {
      await api.resolveException(slug, row.id, {
        note: "Markeret som løst fra cockpittet",
      });
      setRefresh((n) => n + 1);
    } catch (err) {
      setResolveError(
        err instanceof ApiError ? err.message : "Kunne ikke markere som løst.",
      );
    } finally {
      setResolving((s) => {
        const next = new Set(s);
        next.delete(row.id);
        return next;
      });
    }
  };

  // Keep stale data visible during a reload (matches DashboardView/InvoicesView/
  // BankView) — only show the spinner on the FIRST load, never on a refresh.
  if (state.loading && !state.data) return <Loading />;
  // `onRetry` so a failed load is not a dead end — the owner can re-run it.
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;
  const data = state.data!;
  const rows = data.rows;

  return (
    <section className="exceptions-view">
      <header className="page-head">
        <div>
          <h2>{data.company.name}</h2>
          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · Undtagelser
          </p>
        </div>
        <div className="row-actions">
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </header>

      <section className="card">
        <h3>Status</h3>
        <div className="filter-bar">
          {STATUS_TABS.map((t) => (
            <button
              key={t.value}
              type="button"
              className={`btn small ${status === t.value ? "primary" : "secondary"}`}
              onClick={() => setStatus(t.value)}
            >
              {t.label}
            </button>
          ))}
        </div>
        <p className="muted">
          {data.count} undtagelse{data.count === 1 ? "" : "r"}
          {status === "open" && (
            <>
              {" "}
              · Høj: {data.bySeverity.high} · Medium: {data.bySeverity.medium} ·
              Lav: {data.bySeverity.low}
            </>
          )}
        </p>
      </section>

      {resolveError && (
        <div className="callout danger" role="alert">
          {resolveError}
        </div>
      )}

      {rows.length === 0 ? (
        <div className="card">
          <p className="muted">Ingen undtagelser i denne status.</p>
        </div>
      ) : (
        <table className="table">
          <thead>
            <tr>
              <th>ID</th>
              <th>Type</th>
              <th>Alvor</th>
              <th>Status</th>
              <th>Besked</th>
              <th>Næste skridt</th>
              <th>Oprettet</th>
              <th>Handlinger</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.id}>
                <td>#{row.id}</td>
                <td>
                  {TYPE_LABEL[row.type] ?? <code>{row.type}</code>}
                </td>
                <td className={`severity-${row.severity}`}>
                  {SEVERITY_LABEL[row.severity]}
                </td>
                <td>{row.status === "open" ? "Åben" : "Løst"}</td>
                <td>{row.message}</td>
                <td>{row.requiredAction ?? "—"}</td>
                <td className="muted">{row.createdAt}</td>
                <td>
                  {row.status === "open" ? (
                    <button
                      type="button"
                      className="btn small secondary"
                      onClick={() => resolve(row)}
                      disabled={resolving.has(row.id)}
                    >
                      {resolving.has(row.id) ? "Markerer …" : "Markér som løst"}
                    </button>
                  ) : (
                    <span className="muted">
                      Løst{row.resolvedAt ? ` ${row.resolvedAt}` : ""}
                      {row.resolvedBy ? ` af ${row.resolvedBy}` : ""}
                    </span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
