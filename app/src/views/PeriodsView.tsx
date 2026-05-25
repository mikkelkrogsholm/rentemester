// Periodelås view (#342) — per-virksomhed regnskabsperioder med
// effective status (åben/lukket/indberettet), 'Luk periode'-knap +
// 'Genåbn periode'-knap. Cockpittet er en tynd skal over de
// eksisterende CLI-ækvivalente POST .../periods/close og /reopen
// endpoints — ingen ny core-logik introduceres.

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import type {
  AccountingPeriodKind,
  AccountingPeriodRow,
  CompanyPeriods,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";

const KIND_LABEL: Record<AccountingPeriodKind, string> = {
  vat_quarter: "Momsperiode",
  fiscal_year: "Regnskabsår",
  custom: "Andet",
};

const STATUS_LABEL: Record<string, string> = {
  open: "Åben",
  closed: "Lukket",
  reported: "Indberettet",
};

export function PeriodsView() {
  const { slug = "" } = useParams();
  const [refresh, setRefresh] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [openClose, setOpenClose] = useState(false);
  const [reopenTarget, setReopenTarget] =
    useState<AccountingPeriodRow | null>(null);

  const state = useAsync<CompanyPeriods>(
    () => api.periods(slug),
    [slug, refresh],
  );

  const doneRefresh = () => setRefresh((n) => n + 1);

  if (state.loading) return <Loading />;
  if (state.error) return <ErrorState message={state.error} />;
  const data = state.data!;

  return (
    <section className="periods-view">
      <header className="page-head">
        <div>
          <h2>{data.company.name}</h2>
          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · Periodelås
          </p>
        </div>
        <div className="row-actions">
          <button
            type="button"
            className="btn primary"
            onClick={() => {
              setError(null);
              setOpenClose(true);
            }}
          >
            Luk periode …
          </button>
          <Link className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </Link>
        </div>
      </header>

      <p className="muted">
        En lukket periode kan ikke modtage nye posteringer. En indberettet
        periode (sendt til SKAT / Erhvervsstyrelsen) kan ikke genåbnes.
        Genåbning af en lukket periode appendes til audit-log'en med en
        begrundelse.
      </p>

      <section className="card">
        <h3>Sammentælling</h3>
        <div className="filter-bar">
          <span className="pill">Åbne: {data.byStatus.open}</span>
          <span className="pill warn">Lukkede: {data.byStatus.closed}</span>
          <span className="pill">
            Indberettede: {data.byStatus.reported}
          </span>
        </div>
      </section>

      {error && (
        <div className="callout danger" role="alert">
          {error}
        </div>
      )}

      <section className="card">
        <h3>Perioder ({data.periods.length})</h3>
        {data.periods.length === 0 ? (
          <p className="muted">
            Ingen lukkede perioder endnu. Lukninger fra CLI eller fra denne side
            vises her sammen med deres effective status.
          </p>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>Start</th>
                <th>Slut</th>
                <th>Type</th>
                <th>Status</th>
                <th>Lukket af</th>
                <th>Reference</th>
                <th>Handlinger</th>
              </tr>
            </thead>
            <tbody>
              {data.periods.map((p) => (
                <tr key={p.id}>
                  <td className="entry-date">{p.periodStart}</td>
                  <td className="entry-date">{p.periodEnd}</td>
                  <td>{KIND_LABEL[p.kind] ?? p.kind}</td>
                  <td>
                    <span className={`pill status-${p.effectiveStatus}`}>
                      {STATUS_LABEL[p.effectiveStatus] ?? p.effectiveStatus}
                    </span>
                  </td>
                  <td className="muted">{p.closedBy ?? "—"}</td>
                  <td className="muted">{p.reference ?? "—"}</td>
                  <td>
                    {p.effectiveStatus === "closed" && (
                      <button
                        type="button"
                        className="btn small secondary"
                        onClick={() => {
                          setError(null);
                          setReopenTarget(p);
                        }}
                      >
                        Genåbn …
                      </button>
                    )}
                    {p.effectiveStatus === "reported" && (
                      <span className="muted">
                        Indberettet — kan ikke genåbnes
                      </span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      {openClose && (
        <ClosePeriodModal
          slug={slug}
          onClose={() => setOpenClose(false)}
          onDone={() => {
            setOpenClose(false);
            doneRefresh();
          }}
          onError={(msg) => setError(msg)}
        />
      )}
      {reopenTarget && (
        <ReopenPeriodModal
          slug={slug}
          target={reopenTarget}
          onClose={() => setReopenTarget(null)}
          onDone={() => {
            setReopenTarget(null);
            doneRefresh();
          }}
          onError={(msg) => setError(msg)}
        />
      )}
    </section>
  );
}

function ClosePeriodModal({
  slug,
  onClose,
  onDone,
  onError,
}: {
  slug: string;
  onClose: () => void;
  onDone: () => void;
  onError: (msg: string) => void;
}) {
  const [periodStart, setPeriodStart] = useState("");
  const [periodEnd, setPeriodEnd] = useState("");
  const [kind, setKind] = useState<AccountingPeriodKind>("vat_quarter");
  const [reference, setReference] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setSubmitting(true);
    try {
      await api.closePeriod(slug, {
        periodStart,
        periodEnd,
        kind,
        ...(reference ? { reference } : {}),
      });
      onDone();
    } catch (err) {
      onError(
        err instanceof ApiError ? err.message : "Periodelukning fejlede.",
      );
      setSubmitting(false);
    }
  };

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="modal">
        <h3>Luk periode</h3>
        <form onSubmit={submit}>
          <label>
            Start (YYYY-MM-DD)
            <input
              type="date"
              value={periodStart}
              onChange={(e) => setPeriodStart(e.target.value)}
              required
            />
          </label>
          <label>
            Slut (YYYY-MM-DD)
            <input
              type="date"
              value={periodEnd}
              onChange={(e) => setPeriodEnd(e.target.value)}
              required
            />
          </label>
          <label>
            Type
            <select
              value={kind}
              onChange={(e) => setKind(e.target.value as AccountingPeriodKind)}
            >
              <option value="vat_quarter">Momsperiode</option>
              <option value="fiscal_year">Regnskabsår</option>
              <option value="custom">Andet</option>
            </select>
          </label>
          <label>
            Reference (valgfri)
            <input
              type="text"
              value={reference}
              onChange={(e) => setReference(e.target.value)}
              placeholder="fx Q1 2026 momsangivelse"
            />
          </label>
          <div className="row-actions">
            <button
              type="submit"
              className="btn primary"
              disabled={submitting}
            >
              {submitting ? "Lukker …" : "Luk periode"}
            </button>
            <button type="button" className="btn secondary" onClick={onClose}>
              Annullér
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

function ReopenPeriodModal({
  slug,
  target,
  onClose,
  onDone,
  onError,
}: {
  slug: string;
  target: AccountingPeriodRow;
  onClose: () => void;
  onDone: () => void;
  onError: (msg: string) => void;
}) {
  const [reason, setReason] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setSubmitting(true);
    try {
      await api.reopenPeriod(slug, {
        periodStart: target.periodStart,
        periodEnd: target.periodEnd,
        kind: target.kind,
        reason,
      });
      onDone();
    } catch (err) {
      onError(
        err instanceof ApiError ? err.message : "Genåbning fejlede.",
      );
      setSubmitting(false);
    }
  };

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="modal">
        <h3>Genåbn periode</h3>
        <p className="muted">
          {target.periodStart} – {target.periodEnd} ({KIND_LABEL[target.kind]})
          . En begrundelse logges verbatim i audit-log'en.
        </p>
        <form onSubmit={submit}>
          <label>
            Begrundelse (påkrævet)
            <textarea
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              rows={3}
              required
              placeholder="fx 'Bilag indlæst for sent — postering skal korrigeres'"
            />
          </label>
          <div className="row-actions">
            <button
              type="submit"
              className="btn primary"
              disabled={submitting || !reason.trim()}
            >
              {submitting ? "Genåbner …" : "Genåbn periode"}
            </button>
            <button type="button" className="btn secondary" onClick={onClose}>
              Annullér
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
