import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useDiscardGuard } from "../lib/useDiscardGuard";
import { useCapabilities } from "../lib/useCapabilities";
import { ButtonLink, Dialog, Button, Input, PageHeader, Select, Textarea } from "../components/ui";
// Periodelås view (#342) — per-virksomhed regnskabsperioder med
// effective status (åben/lukket/indberettet), 'Luk periode'-knap +
// 'Genåbn periode'-knap. Cockpittet er en tynd skal over de
// eksisterende CLI-ækvivalente POST .../periods/close og /reopen
// endpoints — ingen ny core-logik introduceres.

import { useState } from "react";
import { useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { todayIso } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type {
  AccountingPeriodKind,
  AccountingPeriodRow,
  CompanyPeriods,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";

const KIND_LABEL: Record<AccountingPeriodKind, string> = {
  vat_period: "Momsperiode",
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
    (signal) => api.periods(slug, { signal }),
    [slug, refresh],
  );

  const doneRefresh = () => setRefresh((n) => n + 1);

  if (state.loading && !state.data) return <Loading />;
  if (state.error && !state.data) return <ErrorState message={state.error} onRetry={state.reload} />;
  const data = state.data!;

  return (
    <section className="periods-view" data-cockpit-page="period-lock" data-evidence-issue="655">
      {state.error && <div className="banner warning" role="alert">Status kunne ikke opdateres. Din formular er bevaret; oplysningerne bag den er fra den seneste gennemførte læsning.</div>}
      <PageHeader title="Perioder og låsning" actions={<><div className="row-actions">
          <Button requiredPermission="company.review"
            type="button"
            className="btn primary"
            onClick={() => {
              setError(null);
              setOpenClose(true);
            }}
          >
            Luk periode …
          </Button>
          <ButtonLink className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div>

          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · Periodelås
          </p>
        </div>

      </PageHeader>

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
          <div className="table-scroll"><table className="table responsive-table" aria-label="Regnskabsperioder">
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
                      <Button requiredPermission="company.review" variant="secondary"
                        type="button"
                        className="btn small secondary"
                        onClick={() => {
                          setError(null);
                          setReopenTarget(p);
                        }}
                      >
                        Genåbn …
                      </Button>
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
          </table></div>
        )}
      </section>

      {openClose && (
        <ClosePeriodModal
          slug={slug}
          onClose={() => setOpenClose(false)}
          onRefresh={state.reload}
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
          onRefresh={state.reload}
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
  onClose: onDismiss,
  onRefresh,
  onDone,
  onError,
}: {
  slug: string;
  onClose: () => void;
  onRefresh: () => void;
  onDone: () => void;
  onError: (msg: string) => void;
}) {
  const { can } = useCapabilities();
  const [periodStart, setPeriodStart] = useState("");
  const [periodEnd, setPeriodEnd] = useState("");
  const [kind, setKind] = useState<AccountingPeriodKind>("vat_period");
  const [reference, setReference] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [packet, setPacket] = useState<{ hash: string; blockers: number; warnings: number; items: Array<{code:string;status:"passed"|"warning"|"blocked"|"unavailable";waivable:boolean;count:number}> } | null>(null);
  const [force, setForce] = useState(false);
  const [forceReason, setForceReason] = useState("");
  // #301 — a period whose end lies in the future is not over yet. Require a
  // second, explicit acknowledgement before such a close can go through, the
  // same guard VatView's close-modal has.
  const [futureEndAcknowledged, setFutureEndAcknowledged] = useState(false);
  const periodEndsInFuture = periodEnd !== "" && periodEnd > todayIso();

  const outcome = useMutationOutcome(onRefresh);
  const guard = useDiscardGuard(Boolean(periodStart || periodEnd || reference || packet || force || forceReason || futureEndAcknowledged) || kind !== "vat_period", onDismiss);
  const { onClose } = guard;

  const submit = async (e: React.FormEvent) => {e.preventDefault(); if (outcome.isBlocked()) return;
    if (periodEndsInFuture && !futureEndAcknowledged) {
      onError(
        "Bekræft først at du vil lukke en periode der ikke er afsluttet endnu — sæt flueben i feltet nedenfor.",
      );
      return;
    }
    setSubmitting(true);
    try {
      if (!packet) {
        setPacket(await api.closeReadiness(slug, periodStart, periodEnd));
        setSubmitting(false);
        return;
      }
      const review = await api.reviewCloseReadiness(slug, periodStart, periodEnd).catch(outcome.reject);
      if (review.packet.hash !== packet.hash) {
        setPacket(review.packet);
        onError("Grundlaget ændrede sig. Kontrollér den nye packet før lukning.");
        setSubmitting(false);
        return;
      }
      await api.closePeriod(slug, {
        periodStart,
        periodEnd,
        kind,
        ...(reference ? { reference } : {}),
        packetHash: review.packet.hash,
        reviewId: review.id,
        ...(force ? { force: true, reason: forceReason } : {}),
      }).catch(outcome.reject);
      guard.dismiss();
      onDone();
    } catch (err) {
      onError(
        err instanceof ApiError ? err.message : "Periodelukning fejlede.",
      );
      setSubmitting(false);
    }
  };

  return (
    <Dialog title="Luk periode" onClose={onClose} busy={submitting}>
    {outcome.feedback}
      {guard.confirmation}


        <form onSubmit={submit}>
          <label>
            Start (YYYY-MM-DD)
            <Input disabled={outcome.blocked}
              type="date"
              value={periodStart}
              onChange={(e) => setPeriodStart(e.target.value)}
              required
            />
          </label>
          {packet && <>
            <p className="muted">Kontrolleret: {packet.blockers} blokeringer, {packet.warnings} advarsler. Gennemgå resultatet og vælg derefter “Gem review og luk”.</p>
            {packet.blockers > 0 && <>
              <div className="callout danger">Blokeringer: {packet.items.filter((item) => item.status === "blocked" || item.status === "unavailable").map((item) => item.code).join(", ")}</div>
              {can("company.period.force-close") && <label className="confirm-ack"><Input disabled={outcome.blocked} type="checkbox" checked={force} onChange={(e) => setForce(e.target.checked)} /> Anmod om force-lukning af alene fravigelige blokeringer</label>}
              {force && <label>Begrundelse for force-lukning<Textarea disabled={outcome.blocked} value={forceReason} onChange={(e) => setForceReason(e.target.value)} required rows={2} /></label>}
            </>}
          </>}
          <label>
            Slut (YYYY-MM-DD)
            <Input disabled={outcome.blocked}
              type="date"
              value={periodEnd}
              onChange={(e) => setPeriodEnd(e.target.value)}
              required
            />
          </label>
          <label>
            Type
            <Select disabled={outcome.blocked}
              value={kind}
              onChange={(e) => setKind(e.target.value as AccountingPeriodKind)}
            >
              <option value="vat_period">Momsperiode</option>
              <option value="fiscal_year">Regnskabsår</option>
              <option value="custom">Andet</option>
            </Select>
          </label>
          <label>
            Reference (valgfri)
            <Input disabled={outcome.blocked}
              type="text"
              value={reference}
              onChange={(e) => setReference(e.target.value)}
              placeholder="fx Q1 2026 momsangivelse"
            />
          </label>
          {/* #301: a future period-end means the period is not over yet. Warn
              clearly and require a second, explicit acknowledgement before the
              close can go through. */}
          {periodEndsInFuture && (
            <>
              <div className="callout danger" role="alert">
                Perioden er <strong>ikke afsluttet endnu</strong> — slutdatoen{" "}
                {periodEnd} ligger i fremtiden. Lukker du nu, blokeres bogføring
                med dato i perioden. Luk normalt først perioden, når den er
                forbi. En periode lukket ved en fejl kan genåbnes herfra.
              </div>
              <label className="confirm-ack">
                <Input disabled={outcome.blocked}
                  type="checkbox"
                  checked={futureEndAcknowledged}
                  onChange={(e) => setFutureEndAcknowledged(e.target.checked)}
                />
                Jeg forstår at perioden ikke er afsluttet, og vil lukke den
                alligevel.
              </label>
            </>
          )}
          <div className="row-actions">
            <Button requiredPermission={force ? "company.period.force-close" : "company.review"}
              type="submit"
              className="btn primary"
              disabled={
                outcome.blocked || (submitting || (periodEndsInFuture && !futureEndAcknowledged) || (packet?.blockers !== 0 && (!force || !forceReason.trim())))
              }
            >
              {submitting ? "Arbejder …" : packet ? "Gem review og luk" : "Kontrollér"}
            </Button>
            <Button variant="secondary" type="button" className="btn secondary" onClick={onClose}>
              Annullér
            </Button>
          </div>
        </form>

    </Dialog>
  );
}

function ReopenPeriodModal({
  slug,
  target,
  onClose: onDismiss,
  onRefresh,
  onDone,
  onError,
}: {
  slug: string;
  target: AccountingPeriodRow;
  onClose: () => void;
  onRefresh: () => void;
  onDone: () => void;
  onError: (msg: string) => void;
}) {
  const [reason, setReason] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const outcome = useMutationOutcome(onRefresh);
  const guard = useDiscardGuard(Boolean(reason), onDismiss);
  const { onClose } = guard;

  const submit = async (e: React.FormEvent) => {e.preventDefault(); if (outcome.isBlocked()) return;
    setSubmitting(true);
    try {
      await api.reopenPeriod(slug, {
        periodStart: target.periodStart,
        periodEnd: target.periodEnd,
        kind: target.kind,
        reason,
      }).catch(outcome.reject);
      guard.dismiss();
      onDone();
    } catch (err) {
      onError(
        err instanceof ApiError ? err.message : "Genåbning fejlede.",
      );
      setSubmitting(false);
    }
  };

  return (
    <Dialog title="Genåbn periode" onClose={onClose} busy={submitting}>
    {outcome.feedback}
      {guard.confirmation}


        <p className="muted">
          {target.periodStart} – {target.periodEnd} ({KIND_LABEL[target.kind]})
          . Din begrundelse gemmes ordret i revisionssporet.
        </p>
        <form onSubmit={submit}>
          <label>
            Begrundelse (påkrævet)
            <Textarea disabled={outcome.blocked}
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              rows={3}
              required
              placeholder="fx 'Bilag indlæst for sent — postering skal korrigeres'"
            />
          </label>
          <div className="row-actions">
            <Button requiredPermission="company.review"
              type="submit"
              className="btn primary"
              disabled={outcome.blocked || (submitting || !reason.trim())}
            >
              {submitting ? "Genåbner …" : "Genåbn periode"}
            </Button>
            <Button variant="secondary" type="button" className="btn secondary" onClick={onClose}>
              Annullér
            </Button>
          </div>
        </form>

    </Dialog>
  );
}
