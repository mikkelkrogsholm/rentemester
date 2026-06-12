// Overblik — the per-company overview dashboard (cockpit-redesign iteration 1).
//
// Renders `/api/companies/:slug/overview?year=`: three headline KPI cards
// (Omsætning / Udgifter / Resultat), a month-by-month P&L chart, and a row of
// status cards (Bank, Moms, Opgaver, Seneste posteringer). The per-company
// sub-navigation and fiscal-year selector live in `CompanyNav`; the chosen
// year is carried in the URL (`?year=`) so it follows the user across views.
// All `/overview` money fields are kroner, so `formatKroner` is used
// throughout (never `formatCurrency`, which expects minor units).

import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner, formatPercent } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type {
  CompanyOverview,
  OverviewExceptionRow,
  OverviewMonth,
} from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { ArchivedBanner } from "../components/ArchivedBanner";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { PnlChart } from "../components/PnlChart";
import { AccountantExportCard } from "../components/AccountantExportCard";

export function DashboardView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const state = useAsync<CompanyOverview>(
    () => api.overview(slug, year),
    [slug, year],
  );

  if (state.loading && !state.data) return <Loading label="Henter overblik…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const o = state.data!;
  const currency = o.company.currency || "DKK";
  const positive = o.profitAndLoss.resultat >= 0;

  return (
    <section className="overview">
      <div className="page-head">
        <div>
          <h2>{o.company.name}</h2>
          <p className="muted">
            {o.company.cvr ? `CVR ${o.company.cvr} · ` : ""}
            {o.company.country} · {currency} · Overblik
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
        years={o.fiscalYears}
        selectedYear={o.selectedYear}
        onYearChange={setYear}
      />

      {o.archived && (
        <ArchivedBanner year={o.selectedYear} source={o.archivedSource} />
      )}
      {isFreshEmptyCompany(o) && <GetStartedCard slug={slug} />}
      <p className="period-head muted">
        Regnskabsår {o.selectedYear} ·{" "}
        {o.lastPostedDate
          ? `Senest bogført pr. ${o.lastPostedDate}`
          : "Ingen posteringer bogført endnu"}
      </p>

      <div className="kpi-row">
        <KpiCard
          label="Omsætning"
          value={formatKroner(o.profitAndLoss.omsaetning, currency)}
          tone="neutral"
          to={statementTo(slug, "resultatopgorelse", o.selectedYear)}
        />
        <KpiCard
          label="Udgifter"
          value={formatKroner(o.profitAndLoss.udgifter, currency)}
          tone="neutral"
          to={statementTo(slug, "resultatopgorelse", o.selectedYear)}
        />
        <KpiCard
          label="Resultat"
          value={formatKroner(o.profitAndLoss.resultat, currency)}
          sub={`Regnskabsår ${o.selectedYear}`}
          tone={positive ? "result-positive" : "result-negative"}
          emphasised
          to={statementTo(slug, "resultatopgorelse", o.selectedYear)}
        />
      </div>

      <KeyFigures keyFigures={o.keyFigures} />

      <div className="section">
        <h3>Indtægter og udgifter — {o.selectedYear}</h3>
        <div className="card chart-card">
          <PnlChart months={o.profitAndLoss.months} />
        </div>
      </div>

      <div className="status-grid">
        {o.archived ? (
          // An archived year has no live bank / VAT / exception data — show an
          // honest "not available" card rather than faking a zero.
          <ArchivedUnavailableCard />
        ) : (
          <>
            <BankCard
              bank={o.bank}
              currency={currency}
              to={statementTo(slug, "bank", o.selectedYear)}
            />
            <VatCard
              vat={o.vat}
              currency={currency}
              to={statementTo(slug, "moms", o.selectedYear)}
            />
            <ReceivablesCard
              receivables={o.receivables}
              currency={currency}
              to={statementTo(slug, "fakturaer", o.selectedYear)}
            />
            <ExceptionsCard
              slug={slug}
              exceptions={o.exceptions}
              archived={o.archived}
              onResolved={state.reload}
            />
          </>
        )}
        <RecentEntriesCard entries={o.recentEntries} currency={currency} />
      </div>

      {/* #373 — Revisor-eksport is one of the headline year-end actions; it
          lives here on Overblik so the owner can find it without digging
          through "Administrér virksomhed". The card stays on the Administrér
          page too — this is purely about discoverability. Hidden for an
          archived (read-only) year, since the export reads the live ledger. */}
      {!o.archived && (
        <div className="section">
          <h3>Eksport til revisor</h3>
          <AccountantExportCard slug={slug} />
        </div>
      )}
    </section>
  );
}

// --------------------------------------------------------------------------
// #395 — "Sådan kommer du i gang" empty-state CTA
// --------------------------------------------------------------------------

/**
 * A freshly-onboarded company shows zeros across the board: nothing has been
 * booked, no bank statement imported. We surface a next-step CTA only in that
 * specific state — and never for an archived (read-only) year. The card is
 * removed automatically as soon as either the ledger or the bank gains any
 * data, so it can never become a permanent fixture.
 */
function isFreshEmptyCompany(o: CompanyOverview): boolean {
  if (o.archived) return false;
  const noEntries = o.lastPostedDate === null && o.recentEntries.length === 0;
  const noBank = o.bank.actualBalance === null && o.bank.balance === 0;
  return noEntries && noBank;
}

function GetStartedCard({ slug }: { slug: string }) {
  return (
    <div className="card get-started-card">
      <h3>Sådan kommer du i gang</h3>
      <p className="muted">
        Du er klar til at bogføre din første postering. Vælg én af de tre veje
        — du kan altid bruge agenten eller kommandolinjen i stedet.
      </p>
      <div className="get-started-actions">
        <Link className="btn primary" to={`/companies/${slug}/bilag`}>
          Indlæs dit første bilag
        </Link>
        <Link className="btn secondary" to={`/companies/${slug}/bank`}>
          Importér bankudtog
        </Link>
        <Link className="btn secondary" to={`/companies/${slug}/fakturaer`}>
          Udsted din første faktura
        </Link>
      </div>
    </div>
  );
}

/**
 * The Overblik status card shown for an archived year in place of the live
 * Bank / Moms / Opgaver cards: those rest on live-ledger data (bank
 * reconciliation, the VAT position, the exception queue) that simply does not
 * exist for a pre-cut-over year. An honest "ikke tilgængelig" beats a fake 0.
 */
function ArchivedUnavailableCard() {
  return (
    <div className="card status-card">
      <h3>Bank, moms og opgaver</h3>
      <p className="muted status-note">
        Bankafstemning, momsopgørelse og opgavekøen bygger på den aktive
        ledger og er ikke tilgængelige for et arkiveret regnskabsår.
        Resultatopgørelse, balance, saldobalance og posteringer vises ud fra
        arkivet.
      </p>
    </div>
  );
}

// --------------------------------------------------------------------------
// Drill-down routing — the Overblik cards link into the detailed views
// --------------------------------------------------------------------------

/** A per-company sub-view route that carries the selected fiscal year. */
function statementTo(slug: string, view: string, year: string): string {
  const suffix = year ? `?year=${encodeURIComponent(year)}` : "";
  return `/companies/${slug}/${view}${suffix}`;
}

// --------------------------------------------------------------------------
// KPI cards
// --------------------------------------------------------------------------

function KpiCard({
  label,
  value,
  sub,
  tone,
  emphasised,
  to,
}: {
  label: string;
  value: string;
  sub?: string;
  tone: "neutral" | "result-positive" | "result-negative";
  emphasised?: boolean;
  /** When given, the whole card is a drill-down link. */
  to?: string;
}) {
  const className = `kpi ${tone}${emphasised ? " emphasised" : ""}`;
  const body = (
    <>
      <div className="kpi-label">{label}</div>
      <div className="kpi-value">{value}</div>
      {sub && <div className="kpi-sub">{sub}</div>}
    </>
  );
  if (to) {
    return (
      <Link className={`${className} kpi-link`} to={to}>
        {body}
      </Link>
    );
  }
  return <div className={className}>{body}</div>;
}

// --------------------------------------------------------------------------
// Nøgletal — two key ratios read off the figures already on the page
// --------------------------------------------------------------------------

function KeyFigures({
  keyFigures,
}: {
  keyFigures: CompanyOverview["keyFigures"];
}) {
  return (
    <div className="key-figures">
      <div className="key-figure">
        {/* `keyFigures.bruttomargin` is computed as resultat ÷ omsætning —
            that is the profit margin (overskudsgrad/resultatgrad), not the
            gross margin. The label tracks what the figure actually measures
            so an owner never quotes the wrong term to a bank or accountant. */}
        <span className="key-figure-label">Overskudsgrad</span>
        <span className="key-figure-value">
          {formatPercent(keyFigures.bruttomargin)}
        </span>
        <span className="key-figure-note">resultat ÷ omsætning</span>
      </div>
      <div className="key-figure">
        <span className="key-figure-label">Egenkapitalandel</span>
        <span className="key-figure-value">
          {formatPercent(keyFigures.egenkapitalandel)}
        </span>
        <span className="key-figure-note">egenkapital ÷ balancesum</span>
      </div>
    </div>
  );
}

// --------------------------------------------------------------------------
// Status cards
// --------------------------------------------------------------------------

function StatusCard({
  title,
  children,
  to,
}: {
  title: string;
  children: React.ReactNode;
  /** When given, the whole card is a drill-down link. */
  to?: string;
}) {
  if (to) {
    return (
      <Link className="card status-card status-card-link" to={to}>
        <h3>{title}</h3>
        {children}
      </Link>
    );
  }
  return (
    <div className="card status-card">
      <h3>{title}</h3>
      {children}
    </div>
  );
}

function BankCard({
  bank,
  currency,
  to,
}: {
  bank: CompanyOverview["bank"];
  currency: string;
  to: string;
}) {
  const { balance, actualBalance, difference } = bank;
  // The actual statement balance is the headline figure when it is known —
  // it is what the owner's bank app shows. The booked balance and the gap
  // sit below it, clearly labelled, so a difference is never mistaken.
  const reconciled = difference !== null && Math.abs(difference) < 0.005;
  return (
    <StatusCard title="Bank" to={to}>
      <div className="status-figure">
        {formatKroner(actualBalance ?? balance, currency)}
      </div>
      {actualBalance === null ? (
        <p className="muted status-note">
          Bogført saldo på bank- og kassekonti — intet kontoudtog importeret
        </p>
      ) : (
        <p className="muted status-note">
          Kontoudtog {formatKroner(actualBalance, currency)} · Bogført{" "}
          {formatKroner(balance, currency)}
          {difference !== null && (
            <>
              {" · "}
              {reconciled ? (
                <span className="bank-diff ok">Afstemt</span>
              ) : (
                <span className="bank-diff alert">
                  Difference {formatKroner(difference, currency)} — ikke afstemt
                </span>
              )}
            </>
          )}
        </p>
      )}
    </StatusCard>
  );
}

function VatCard({
  vat,
  currency,
  to,
}: {
  vat: CompanyOverview["vat"];
  currency: string;
  to: string;
}) {
  // VAT is a live-ledger figure — never rendered for an archived year.
  if (vat === null) return null;
  // The momsangivelse is easy to forget — surface it right on the card. Two
  // dates that an owner must not conflate: the VAT period's own span, and the
  // SKAT filing/payment deadline (the 1st of the third month AFTER the period
  // ends). The countdown targets the deadline, so the card spells both out.
  const days = vat.daysRemaining;
  const countdown =
    days < 0
      ? `Fristen overskredet ${Math.abs(days)} ${
          Math.abs(days) === 1 ? "dag" : "dage"
        }`
      : days === 0
        ? "Frist i dag"
        : `${days} ${days === 1 ? "dag" : "dage"} tilbage`;
  const tone = days <= 30 ? (days < 0 ? "critical" : "warning") : "ok";
  return (
    <StatusCard title="Moms" to={to}>
      <div className="status-figure">
        {formatKroner(vat.payable, currency)}
      </div>
      <p className="muted status-note">
        Momsperiode {vat.periodLabel} ({vat.periodStart} – {vat.periodEnd}) ·{" "}
        {vat.payable >= 0 ? "at betale" : "tilgode"}
      </p>
      <p className="muted status-note">
        Indberettes og betales til SKAT senest {vat.deadline} ·{" "}
        <span className={`bank-diff ${tone === "ok" ? "ok" : "alert"}`}>
          {countdown}
        </span>
      </p>
    </StatusCard>
  );
}

function ReceivablesCard({
  receivables,
  currency,
  to,
}: {
  receivables: CompanyOverview["receivables"];
  currency: string;
  to: string;
}) {
  // "Hvem skylder mig" — the open balance of issued sales invoices. For a
  // company with no outstanding receivables (Helheim) this is a clean zero.
  const { openCount, openTotal } = receivables;
  return (
    <StatusCard title="Tilgodehavender" to={to}>
      <div
        className={`status-figure${openTotal > 0 ? " status-alert" : ""}`}
      >
        {formatKroner(openTotal, currency)}
      </div>
      <p className="muted status-note">
        {openCount === 0
          ? "Ingen udestående fakturaer — ingen skylder dig penge."
          : `${openCount} ${
              openCount === 1 ? "faktura" : "fakturaer"
            } afventer betaling fra kunder.`}
      </p>
    </StatusCard>
  );
}

// Maps a cockpit-relative `link` target from a grouped exception to its route.
function exceptionLinkTo(slug: string, link: string | null): string | null {
  if (link === "bank") return `/companies/${slug}/bank`;
  return null;
}

function ExceptionsCard({
  slug,
  exceptions,
  archived,
  onResolved,
}: {
  slug: string;
  exceptions: CompanyOverview["exceptions"];
  /** A pre-cut-over archived year is read-only — no write action is offered. */
  archived: boolean;
  /** Re-runs the overview load after an exception is resolved. */
  onResolved: () => void;
}) {
  // The exception under the open "Løs" modal — null when no modal is open.
  const [resolving, setResolving] = useState<OverviewExceptionRow | null>(null);

  return (
    <StatusCard title="Opgaver">
      <div
        className={`status-figure${
          exceptions.count > 0 ? " status-alert" : ""
        }`}
      >
        {exceptions.count}
      </div>
      {exceptions.count === 0 ? (
        <p className="muted status-note">Ingen åbne opgaver.</p>
      ) : (
        <>
          {/* The grouped summary lines — one Danish line per exception type. */}
          <ul className="status-list">
            {exceptions.groups.map((g) => {
              const to = exceptionLinkTo(slug, g.link);
              const body = (
                <>
                  <span
                    className={`flag ${
                      g.severity === "high" ? "critical" : "warning"
                    }`}
                  >
                    {g.count}
                  </span>{" "}
                  {g.label}
                </>
              );
              return (
                <li key={g.type}>
                  {to ? (
                    <Link className="status-link" to={to}>
                      {body}
                    </Link>
                  ) : (
                    body
                  )}
                </li>
              );
            })}
          </ul>
          {/* The first few individual exceptions, each with its concrete
              requiredAction guidance and a "Markér som gennemgået" action.
              The action is hidden for an archived (read-only) year. */}
          {!archived && exceptions.rows.length > 0 && (
            <ul className="status-list task-list">
              {exceptions.rows.map((row) => (
                <li key={row.id} className="task-row">
                  <div className="task-row-text">
                    <span className="task-row-message">
                      <span
                        className={`flag ${
                          row.severity === "high" ? "critical" : "warning"
                        }`}
                      >
                        !
                      </span>{" "}
                      {row.message}
                    </span>
                    {row.requiredAction && (
                      <p className="task-row-action">
                        <strong>Sådan løser du den:</strong>{" "}
                        {row.requiredAction}
                      </p>
                    )}
                  </div>
                  <button
                    type="button"
                    className="btn secondary"
                    onClick={() => setResolving(row)}
                  >
                    Markér som gennemgået
                  </button>
                </li>
              ))}
            </ul>
          )}
        </>
      )}

      {resolving && (
        <ConfirmDialog
          title="Markér opgave som gennemgået"
          body={
            <>
              <p>
                Dette markerer opgaven <em>{resolving.message}</em> som
                gennemgået, så den ikke længere står på listen.
              </p>
              <p className="dialog-warning">
                <strong>Bemærk:</strong> dette bogfører ikke noget. Selve
                posteringen — fx en bankindbetaling, en udgift eller moms —
                skal stadig bogføres
                {resolving.requiredAction ? (
                  <>
                    {" "}
                    som beskrevet:{" "}
                    <em>{resolving.requiredAction}</em>
                  </>
                ) : (
                  " via agenten eller kommandolinjen"
                )}
                . Markér først som gennemgået, når den underliggende postering
                rent faktisk er bogført — ellers viser cockpittet et grønt
                regnskab, der ikke er grønt.
              </p>
            </>
          }
          confirmLabel="Markér som gennemgået"
          noteLabel="Note — hvad blev bogført? (valgfri)"
          notePlaceholder="Fx: bogført som salg via agenten, bilag 2026-0042"
          onConfirm={async (note) => {
            await api.resolveException(slug, resolving.id, note || undefined);
            onResolved();
          }}
          onClose={() => setResolving(null)}
        />
      )}
    </StatusCard>
  );
}

function RecentEntriesCard({
  entries,
  currency,
}: {
  entries: CompanyOverview["recentEntries"];
  currency: string;
}) {
  return (
    <StatusCard title="Seneste posteringer">
      {entries.length === 0 ? (
        <p className="muted status-note">Ingen posteringer i året endnu.</p>
      ) : (
        <ul className="recent-entries">
          {entries.map((e) => (
            <li key={e.id} className="recent-entry">
              <span className="recent-entry-text">{e.text}</span>
              <span className="recent-entry-meta">
                <span className="entry-date">{e.date}</span>
                <span className="recent-entry-amount num">
                  {formatKroner(e.amount, currency)}
                </span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </StatusCard>
  );
}

export type { OverviewMonth };
