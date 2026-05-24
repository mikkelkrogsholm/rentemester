// Agent-forslag → menneskelig godkendelse-flow (#346).
//
// Rentemester's narrative is: agent surfaces, human decides, ledger enforces.
// The agent loop and the exception sync functions raise `AGENT_*` exceptions
// whenever a deterministic agent run needs the owner's call — an overdue
// kreditorpost, a periodeafgrænsnings-periode der er klar til bogføring, et
// muligt anlæg over kapitaliseringsgrænsen, et needs-review-punkt på
// oplysningsskemaet. The dashboard already shows them collapsed as a count;
// this view makes them individually visible and gives the owner two explicit
// actions per row:
//
//   * "Godkend" — accepts the suggestion. The view resolves the underlying
//     exception with a "Godkendt af ejer i cockpit"-note, then deep-links to
//     the action-specific view (Anlæg, Leverandørfaktura, Posteringer) where
//     the owner actually books the entry. Approve here NEVER posts on its own.
//
//   * "Afvis" — rejects the suggestion with a free-text reason. The exception
//     is resolved with an "Afvist af ejer i cockpit"-note carrying the reason
//     so the audit trail preserves WHY the owner declined.
//
// All approvals and rejections go through the SAME `resolveException` core
// that the existing "Løs"-button uses, so the audit chain stays intact. The
// cockpit never re-implements the underlying bookkeeping.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type {
  AgentSuggestionRow,
  CompanyAgentSuggestions,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";

const SEVERITY_LABEL: Record<AgentSuggestionRow["severity"], string> = {
  high: "Høj prioritet",
  medium: "Mellem prioritet",
  low: "Lav prioritet",
};

export function SuggestionsView() {
  const { slug = "" } = useParams();
  const state = useAsync<CompanyAgentSuggestions>(
    () => api.agentSuggestions(slug),
    [slug],
  );
  const [actionError, setActionError] = useState<string | null>(null);

  if (state.loading && !state.data)
    return <Loading label="Henter agent-forslag…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const data = state.data!;
  const currency = data.company.currency || "DKK";

  return (
    <section className="statement">
      <div className="page-head">
        <div>
          <h2>{data.company.name}</h2>
          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · {currency} · Agent-forslag
          </p>
        </div>
        <div className="row-actions">
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </div>

      <p className="statement-asof muted">
        Agenten foreslår — du beslutter. Hvert forslag er deterministisk
        afledt af en regel i <code>rules/dk/*.yaml</code> og bogføres aldrig
        uden et eksplicit klik fra dig. Godkendelse løser forslaget; den
        konkrete postering laver du på den linkede side.
      </p>

      <div className="status-grid invoices-summary">
        <div className="card status-card">
          <h3>Forslag i kø</h3>
          <div className="status-figure">{data.count}</div>
          <p className="muted status-note">
            {data.count === 0
              ? "Agenten har intet, der venter på en beslutning."
              : data.count === 1
                ? "Ét forslag venter på din beslutning."
                : `${data.count} forslag venter på din beslutning.`}
          </p>
        </div>
        <div className="card status-card">
          <h3>Høj prioritet</h3>
          <div className="status-figure">{data.bySeverity.high}</div>
          <p className="muted status-note">
            Bør afgøres først — kan udløse rentepåkrav eller manglende
            bogføring.
          </p>
        </div>
        <div className="card status-card">
          <h3>Mellem / Lav</h3>
          <div className="status-figure">
            {data.bySeverity.medium + data.bySeverity.low}
          </div>
          <p className="muted status-note">
            {data.bySeverity.medium} mellem · {data.bySeverity.low} lav
          </p>
        </div>
      </div>

      {actionError ? (
        <div className="card archived-notice" role="alert">
          <p className="muted">{actionError}</p>
        </div>
      ) : null}

      <h3 style={{ marginTop: "1.5rem" }}>Forslag</h3>
      {data.rows.length === 0 ? (
        <div className="card archived-notice">
          <p className="muted">
            Agenten har ingen åbne forslag. Hver gang en automatisk kørsel
            løber ind i en sag, der kræver din vurdering — fx en overforfalden
            kreditorpost, et muligt anlæg, eller en periodeafgrænsning klar
            til bogføring — dukker forslaget op her.
          </p>
        </div>
      ) : (
        <div className="card statement-card table-scroll">
          <table className="data statement-table">
            <thead>
              <tr>
                <th>Prioritet</th>
                <th>Type</th>
                <th>Agentens vurdering</th>
                <th>Hjemmel</th>
                <th>Handling</th>
              </tr>
            </thead>
            <tbody>
              {data.rows.map((row) => (
                <SuggestionRowView
                  key={row.exceptionId}
                  row={row}
                  slug={slug}
                  onChanged={() => state.reload()}
                  onError={setActionError}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}

      <p className="statement-check ok">
        Hvert forslag er en åben undtagelse i ledger'en — godkendelse og
        afvisning løser undtagelsen via samme kerne (
        <code>resolveException</code>) som CLI'en og MCP-kald bruger, så
        beslutningen står på audit-sporet med din actor og en
        beslutningstekst.
      </p>
    </section>
  );
}

function SuggestionRowView({
  row,
  slug,
  onChanged,
  onError,
}: {
  row: AgentSuggestionRow;
  slug: string;
  onChanged: () => void;
  onError: (msg: string) => void;
}) {
  const [busy, setBusy] = useState<"approve" | "reject" | null>(null);

  async function handleApprove() {
    if (
      !window.confirm(
        `Godkend agent-forslaget "${row.kindLabel}"? Forslaget løses som godkendt — du bogfører selv den konkrete handling bagefter.`,
      )
    )
      return;
    setBusy("approve");
    try {
      await api.approveAgentSuggestion(slug, row.exceptionId);
      onChanged();
    } catch (err) {
      onError(
        err instanceof ApiError
          ? err.message
          : "Kunne ikke godkende forslaget.",
      );
    } finally {
      setBusy(null);
    }
  }

  async function handleReject() {
    const reason = window.prompt(
      `Afvis agent-forslaget "${row.kindLabel}"? Skriv en kort begrundelse (gemmes på audit-sporet, så agenten kan lære af afvisningen):`,
      "",
    );
    // window.prompt returns null on cancel, "" when the owner left it blank.
    // Both are valid — null cancels, "" still rejects (without a reason).
    if (reason === null) return;
    setBusy("reject");
    try {
      await api.rejectAgentSuggestion(
        slug,
        row.exceptionId,
        reason.trim().length > 0 ? reason.trim() : undefined,
      );
      onChanged();
    } catch (err) {
      onError(
        err instanceof ApiError ? err.message : "Kunne ikke afvise forslaget.",
      );
    } finally {
      setBusy(null);
    }
  }

  const severityLabel = SEVERITY_LABEL[row.severity];
  const flagClass =
    row.severity === "high"
      ? "warn"
      : row.severity === "medium"
        ? "neutral"
        : "ok";

  return (
    <tr>
      <td>
        <span className={`flag ${flagClass}`}>{severityLabel}</span>
      </td>
      <td>
        <strong>{row.kindLabel}</strong>
        <div className="muted" style={{ fontSize: "0.85em" }}>
          {row.type}
          {row.agentActor ? ` · ${row.agentActor}` : ""}
        </div>
      </td>
      <td>
        <p style={{ margin: 0 }}>{row.rationale}</p>
        {row.requiredAction ? (
          <p
            className="muted"
            style={{ margin: "0.25rem 0 0 0", fontSize: "0.9em" }}
          >
            <strong>Foreslået handling:</strong> {row.requiredAction}
          </p>
        ) : null}
        {row.link ? (
          <p style={{ margin: "0.25rem 0 0 0", fontSize: "0.9em" }}>
            <Link to={`/companies/${slug}/${row.link}`}>
              Åbn relateret side →
            </Link>
          </p>
        ) : null}
      </td>
      <td>
        {row.ruleId ? (
          <code>{row.ruleId}</code>
        ) : (
          <span className="muted">—</span>
        )}
      </td>
      <td>
        <div className="row-actions">
          <button
            type="button"
            className="btn"
            onClick={handleApprove}
            disabled={busy !== null}
            aria-label={`Godkend ${row.kindLabel}`}
          >
            {busy === "approve" ? "Godkender…" : "Godkend"}
          </button>
          <button
            type="button"
            className="btn secondary"
            onClick={handleReject}
            disabled={busy !== null}
            aria-label={`Afvis ${row.kindLabel}`}
          >
            {busy === "reject" ? "Afviser…" : "Afvis"}
          </button>
        </div>
      </td>
    </tr>
  );
}
