import { ButtonLink, Button, PageHeader } from "../components/ui";
// Overblik — the per-company overview dashboard (cockpit-redesign iteration 1).
//
// Renders `/api/companies/:slug/overview?year=`: three headline KPI cards
// (Omsætning / Udgifter / Resultat), a month-by-month P&L chart, and a row of
// status cards (Bank, Moms, Opgaver, Seneste posteringer). The per-company
// sub-navigation and fiscal-year selector live in `CompanyNav`; the chosen
// year is carried in the URL (`?year=`) so it follows the user across views.
// All `/overview` money fields are kroner, so `formatKroner` is used
// throughout (never `formatCurrency`, which expects minor units).

import { useEffect, useId, useState } from "react";
import * as stylex from "@stylexjs/stylex";
import { dashboardStyles as styles } from "./DashboardView.stylex";
import { CategoricalChart, chartStyles } from "../components/CategoricalChart";
import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatDateDa, formatKroner, formatPercent } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type {
  CompanyOverview,
  ChangesSince,
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
  const revenueDataId = useId();
  const { year, setYear } = useCompanyYear();
  const seenKey = `rentemester:changes:local:workspace:${slug}`;
  const [seen, setSeen] = useState(
    () => Number(window.localStorage.getItem(seenKey) ?? "0") || 0,
  );
  const state = useAsync<CompanyOverview>(
    (signal) => api.overview(slug, year, undefined, { signal }),
    [slug, year],
  );
  const changes = useAsync(() => api.changesSince(slug, seen), [slug, seen]);
  const [seenNotice, setSeenNotice] = useState(false);
  useEffect(() => {
    if (changes.error) {
      window.localStorage.removeItem(seenKey);
      setSeen(0);
    }
  }, [changes.error, seenKey]);

  if (state.loading && !state.data) return <Loading label="Henter overblik…" />;
  if (state.error && !state.data)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const o = state.data!;
  const markSeen = () => {
    if (changes.data) {
      window.localStorage.setItem(seenKey, String(changes.data.cursor));
      setSeen(changes.data.cursor);
    }
    setSeenNotice(true);
  };
  const currency = o.company.currency || "DKK";
  const positive = o.profitAndLoss.resultat >= 0;

  return (
    <section {...stylex.props(styles.overview)} data-cockpit-page="dashboard" data-evidence-issue="651">
      {state.error && <div {...stylex.props(styles.banner)} role="alert">Status kunne ikke opdateres. De tidligere hentede oplysninger vises fortsat.</div>}
      <PageHeader title="Overblik" actions={<><ButtonLink variant="secondary" to={`/companies/${slug}/opgaver`}>Opgaver</ButtonLink><div {...stylex.props(styles.actions)}>
          <ButtonLink variant="secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div>

          <p {...stylex.props(styles.muted)}>
            {o.company.cvr ? `CVR ${o.company.cvr} · ` : ""}
            {o.company.country} · {currency} · Overblik
          </p>
        </div>

      </PageHeader>

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
      <p {...stylex.props(styles.period, styles.muted)}>
        Regnskabsår {o.selectedYear} ·{" "}
        {o.lastPostedDate
          ? `Senest bogført pr. ${formatDateDa(o.lastPostedDate)}`
          : "Ingen posteringer bogført endnu"}
      </p>

      <section {...stylex.props(styles.card)} aria-label="Status og næste handling">
        <h3 {...stylex.props(styles.heading)}>{o.attention.status === "requires-attention" ? `${o.attention.count} forhold kræver opmærksomhed` : "Status: ingen åbne forhold"}</h3>
        <p {...stylex.props(styles.muted)}>{o.attention.status === "requires-attention" ? "Gennemgå de åbne forhold, før du vurderer nøgletallene." : "Regnskabsdataene er klar til gennemgang."}</p>
        {o.attention.status === "requires-attention" ? <ButtonLink variant="primary" to={`/companies/${slug}/opmaerksomhed`}>Se krævende handlinger</ButtonLink> : isFreshEmptyCompany(o) ? <ButtonLink variant="primary" to={`/companies/${slug}/bilag`}>Start med bilag</ButtonLink> : <ButtonLink variant="secondary" to={statementTo(slug, "posteringer", o.selectedYear)}>Se posteringer</ButtonLink>}
      </section>

      <section {...stylex.props(styles.section)} aria-labelledby="changes-heading">
        <h3 {...stylex.props(styles.heading)} id="changes-heading" data-evidence-heading>Siden sidst</h3>
        {changes.loading && <p {...stylex.props(styles.muted)} data-evidence-status="loading">Henter ændringer…</p>}
        {changes.error && <p {...stylex.props(styles.paragraph)} role="alert" data-evidence-status={/403|forbudt|adgang/i.test(changes.error) ? "warning-or-blocked" : "error"}>{/403|forbudt|adgang/i.test(changes.error) ? "Overblik kræver opmærksomhed" : "Virksomhedsoverblik kunne ikke hentes"}</p>}
        {changes.data && changes.data.events.length === 0 && <p {...stylex.props(styles.muted)} data-evidence-status="empty">Ingen nye data- eller statusændringer siden dit seneste besøg.</p>}
        {changes.data && changes.data.events.length > 0 && <ChangesSummary changes={changes.data} onSeen={markSeen} />}
        {seenNotice && <p {...stylex.props(styles.paragraph)} role="status" data-evidence-task-outcome>Ændringer markeret som set</p>}
        {changes.data && seen === 0 && <p {...stylex.props(styles.muted)}>Første besøg: ændringer vises fra begyndelsen af det tilgængelige revisionsspor.</p>}
      </section>

      <div {...stylex.props(styles.kpis)}>
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

      <div {...stylex.props(styles.section)}>
        <h3 {...stylex.props(styles.heading)}>{o.profitAndLoss.months.length > 0 ? `Omsætningen toppede i ${o.profitAndLoss.months.reduce((best, month) => month.income > best.income ? month : best, o.profitAndLoss.months[0]!).label}` : "Omsætningens udvikling kan endnu ikke vises"} — {o.selectedYear}</h3>
        <p {...stylex.props(styles.muted)}>Én pointe: sammenlign månedernes omsætning. Regnskabsår {o.selectedYear}.</p>
        <div {...stylex.props(styles.card, chartStyles.chartCard)}>
          {o.profitAndLoss.months.length > 0 ? <CategoricalChart labels={o.profitAndLoss.months.map((month) => month.label)} series={[{ id: "income", label: "Omsætning", values: o.profitAndLoss.months.map((month) => month.income), kind: "line", tone: "success" }]} currency={currency} label={`Omsætning pr. måned i ${currency}, regnskabsår ${o.selectedYear}`} dataTableId={revenueDataId} height="line" /> : <p {...stylex.props(styles.muted)}>Ingen måneder at vise endnu.</p>}
          <table id={revenueDataId} {...stylex.props(styles.table)} data-evidence-data><caption {...stylex.props(chartStyles.srOnly)}>Omsætning pr. måned ({currency}), regnskabsår {o.selectedYear}</caption><thead><tr><th {...stylex.props(styles.cell)}>Måned</th><th {...stylex.props(styles.cell)}>Beløb ({currency})</th></tr></thead><tbody>{o.profitAndLoss.months.map((month) => <tr key={month.month}><td {...stylex.props(styles.cell)}>{month.label}</td><td {...stylex.props(styles.cell, styles.amount)}>{formatKroner(month.income, currency)}</td></tr>)}</tbody></table>
          <Link {...stylex.props(styles.link)} to={statementTo(slug, "resultatopgorelse", o.selectedYear)} data-evidence-progressive>Se underliggende resultatopgørelse</Link>
          <PnlChart months={o.profitAndLoss.months} currency={currency} />
        </div>
      </div>

      <div {...stylex.props(styles.statusGrid)}>
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
              attentionCount={o.attention.count}
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
        <div {...stylex.props(styles.section)}>
          <h3 {...stylex.props(styles.heading)}>Eksport til revisor</h3>
          <AccountantExportCard slug={slug} />
        </div>
      )}
    </section>
  );
}

const changeCategoryLabels: readonly [prefix: string, singular: string, plural: string][] = [
  ["journal_", "bogføringsændring", "bogføringsændringer"],
  ["document_", "bilagsændring", "bilagsændringer"],
  ["bank_", "bankændring", "bankændringer"],
  ["invoice_", "fakturaændring", "fakturaændringer"],
  ["exception_", "opgaveændring", "opgaveændringer"],
  ["vat_", "momsændring", "momsændringer"],
  ["period_", "periodeændring", "periodeændringer"],
  ["company_", "virksomhedsændring", "virksomhedsændringer"],
];

function ChangesSummary({
  changes,
  onSeen,
}: {
  changes: ChangesSince;
  onSeen: () => void;
}) {
  const counts = new Map<string, { count: number; singular: string; plural: string }>();
  for (const event of changes.events) {
    const category = changeCategoryLabels.find(([prefix]) => event.eventType.startsWith(prefix));
    const key = category?.[0] ?? "other";
    const current = counts.get(key) ?? {
      count: 0,
      singular: category?.[1] ?? "anden ændring",
      plural: category?.[2] ?? "andre ændringer",
    };
    current.count += 1;
    counts.set(key, current);
  }
  return <>
    <p {...stylex.props(styles.paragraph)} data-evidence-status="normal">{changes.events.length} {changes.events.length === 1 ? "ny ændring" : "nye ændringer"} siden dit seneste besøg.</p>
    <ul {...stylex.props(styles.list)} aria-label="Kort ændringsoversigt" data-evidence-data>
      {[...counts.values()].map((item) => <li key={item.singular}>{item.count} {item.count === 1 ? item.singular : item.plural}</li>)}
    </ul>
    <details {...stylex.props(styles.evidence)} data-evidence-progressive>
      <summary {...stylex.props(styles.summary)}>Evidens</summary>
      <ul {...stylex.props(styles.list)}>{changes.events.map((event) => <li key={event.id}><strong>{event.eventType}</strong>: {event.message} <span {...stylex.props(styles.muted)}>· {event.actor} · {event.createdAt}</span></li>)}</ul>
    </details>
    <Button variant="secondary" type="button" onClick={onSeen} data-evidence-core-action>Markér som set</Button>
  </>;
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
  const noBank = o.bank.actualBalance === null && o.bank.balance === 0 && o.bank.bankStatementStatus !== "ambiguous";
  return noEntries && noBank;
}

function GetStartedCard({ slug }: { slug: string }) {
  return (
    <div {...stylex.props(styles.card, styles.getStarted)}>
      <h3 {...stylex.props(styles.heading, styles.getStartedTitle)}>Sådan kommer du i gang</h3>
      <p {...stylex.props(styles.muted)}>
        Du er klar til at bogføre din første postering. Vælg én af de tre veje
        — du kan altid bruge agenten eller kommandolinjen i stedet.
      </p>
      <div {...stylex.props(styles.actions)}>
        <ButtonLink variant="primary" to={`/companies/${slug}/bilag`}>
          Indlæs dit første bilag
        </ButtonLink>
        <ButtonLink variant="secondary" to={`/companies/${slug}/bank`}>
          Importér bankudtog
        </ButtonLink>
        <ButtonLink variant="secondary" to={`/companies/${slug}/fakturaer`}>
          Udsted din første faktura
        </ButtonLink>
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
    <div {...stylex.props(styles.card, styles.statusCard)} data-card="status">
      <h3 {...stylex.props(styles.heading)}>Bank, moms og opgaver</h3>
      <p {...stylex.props(styles.muted, styles.statusNote)}>
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
  const cardStyles = stylex.props(styles.kpi, emphasised && styles.emphasised, tone === "result-positive" && styles.positiveBorder, tone === "result-negative" && styles.negativeBorder);
  const body = (
    <>
      <div {...stylex.props(styles.kpiLabel)} data-kpi-label>{label}</div>
      <div {...stylex.props(styles.kpiValue, emphasised && styles.prominentValue, emphasised && tone === "result-positive" && styles.positive, emphasised && tone === "result-negative" && styles.negative)} data-kpi-value>{value}</div>
      {sub && <div {...stylex.props(styles.kpiSub)}>{sub}</div>}
    </>
  );
  if (to) {
    return (
      <Link {...stylex.props(styles.kpi, emphasised && styles.emphasised, tone === "result-positive" && styles.positiveBorder, tone === "result-negative" && styles.negativeBorder, styles.cardLink)} data-kpi to={to}>
        {body}
      </Link>
    );
  }
  return <div {...cardStyles} data-kpi>{body}</div>;
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
    <div {...stylex.props(styles.keyFigures)}>
      <div {...stylex.props(styles.card, styles.keyFigure)} data-key-figure>
        {/* `keyFigures.bruttomargin` is computed as resultat ÷ omsætning —
            that is the profit margin (overskudsgrad/resultatgrad), not the
            gross margin. The label tracks what the figure actually measures
            so an owner never quotes the wrong term to a bank or accountant. */}
        <span {...stylex.props(styles.keyLabel)}>Overskudsgrad</span>
        <span {...stylex.props(styles.keyValue)}>
          {formatPercent(keyFigures.bruttomargin)}
        </span>
        <span {...stylex.props(styles.keyNote)}>resultat ÷ omsætning</span>
      </div>
      <div {...stylex.props(styles.card, styles.keyFigure)} data-key-figure>
        <span {...stylex.props(styles.keyLabel)}>Egenkapitalandel</span>
        <span {...stylex.props(styles.keyValue)}>
          {formatPercent(keyFigures.egenkapitalandel)}
        </span>
        <span {...stylex.props(styles.keyNote)}>egenkapital ÷ balancesum</span>
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
      <Link {...stylex.props(styles.card, styles.statusCard, styles.cardLink)} data-card="status" to={to}>
        <h3 {...stylex.props(styles.statusTitle)}>{title}</h3>
        {children}
      </Link>
    );
  }
  return (
    <div {...stylex.props(styles.card, styles.statusCard)} data-card="status">
      <h3 {...stylex.props(styles.statusTitle)}>{title}</h3>
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
  const ambiguous = bank.bankStatementStatus === "ambiguous";
  // The actual statement balance is the headline figure when it is known —
  // it is what the owner's bank app shows. The booked balance and the gap
  // sit below it, clearly labelled, so a difference is never mistaken.
  const reconciled = difference !== null && Math.abs(difference) < 0.005;
  return (
    <StatusCard title="Bank" to={to}>
      <div {...stylex.props(styles.statusFigure)} data-figure>
        {actualBalance === null && ambiguous ? "—" : formatKroner(actualBalance ?? balance, currency)}
      </div>
      {actualBalance === null ? (
        <p {...stylex.props(styles.muted, styles.statusNote)}>
          {ambiguous
            ? "Kontoudtogets rækkefølge eller løbende saldo kan ikke bevises. Se Bank og kontrollér eksporten — Rentemester viser ingen gættet saldo."
            : "Bogført saldo på bank- og kassekonti — intet kontoudtog importeret"}
        </p>
      ) : (
        <p {...stylex.props(styles.muted, styles.statusNote)}>
          Kontoudtog {formatKroner(actualBalance, currency)} · Bogført{" "}
          {formatKroner(balance, currency)}
          {difference !== null && (
            <>
              {" · "}
              {reconciled ? (
                <span {...stylex.props(styles.success)}>Afstemt</span>
              ) : (
                <span {...stylex.props(styles.warning)}>
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
  // canonical SKAT filing/payment deadline for the registered cadence. The
  // countdown targets the deadline, so the card spells both dates out.
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
  const periodStatus =
    vat.periodStatus === "reported"
      ? "Indberettet"
      : vat.periodStatus === "closed"
        ? "Lukket – klar til indberetning"
        : "Åben";
  return (
    <StatusCard title="Moms" to={to}>
      <div {...stylex.props(styles.statusFigure)} data-figure>
        {formatKroner(vat.payable, currency)}
      </div>
      <p {...stylex.props(styles.muted, styles.statusNote)}>
        Momsperiode {vat.periodLabel} ({vat.periodStart} – {vat.periodEnd}) · {periodStatus} ·{" "}
        {vat.payable >= 0 ? "at betale" : "tilgode"}
      </p>
      <p {...stylex.props(styles.muted, styles.statusNote)}>
        Indberettes og betales til SKAT senest {vat.deadline} ·{" "}
        <span {...stylex.props(tone === "ok" ? styles.success : styles.warning)}>
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
        {...stylex.props(styles.statusFigure, openTotal > 0 && styles.negative)} data-figure
      >
        {formatKroner(openTotal, currency)}
      </div>
      <p {...stylex.props(styles.muted, styles.statusNote)}>
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
  attentionCount,
  archived,
  onResolved,
}: {
  slug: string;
  exceptions: CompanyOverview["exceptions"];
  attentionCount: number;
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
        {...stylex.props(styles.statusFigure, attentionCount > 0 && styles.negative)} data-figure
      >
        {attentionCount}
      </div>
      {attentionCount === 0 ? (
        <p {...stylex.props(styles.muted, styles.statusNote)}>Ingen åbne opgaver.</p>
      ) : (
        <>
          <p {...stylex.props(styles.paragraph)}><Link {...stylex.props(styles.link)} to={`/companies/${slug}/opmaerksomhed`}>Se alle opgaver</Link></p>
          {/* The grouped summary lines — one Danish line per exception type. */}
          <ul {...stylex.props(styles.statusList)}>
            {exceptions.groups.map((g) => {
              const to = exceptionLinkTo(slug, g.link);
              const body = (
                <>
                  <span
                    {...stylex.props(styles.flag, g.severity === "high" ? styles.criticalFlag : styles.warningFlag)}
                  >
                    {g.count}
                  </span>{" "}
                  {g.label}
                </>
              );
              return (
                <li key={g.type}>
                  {to ? (
                    <Link {...stylex.props(styles.link, styles.statusLink)} to={to}>
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
            <ul {...stylex.props(styles.statusList)}>
              {exceptions.rows.map((row) => (
                <li key={row.id} {...stylex.props(styles.taskRow)}>
                  <div {...stylex.props(styles.taskText)}>
                    <span {...stylex.props(styles.taskMessage)}>
                      <span
                        {...stylex.props(styles.flag, row.severity === "high" ? styles.criticalFlag : styles.warningFlag)}
                      >
                        !
                      </span>{" "}
                      {row.message}
                    </span>
                    {row.requiredAction && (
                      <p {...stylex.props(styles.taskAction)}>
                        <strong>Sådan løser du den:</strong>{" "}
                        {row.requiredAction}
                      </p>
                    )}
                  </div>
                  <Button requiredPermission="company.review" variant="secondary"
                    type="button"
                    onClick={() => setResolving(row)}
                  >
                    Markér som gennemgået
                  </Button>
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
              <p {...stylex.props(styles.paragraph)}>
                Dette markerer opgaven <em>{resolving.message}</em> som
                gennemgået, så den ikke længere står på listen.
              </p>
              <p {...stylex.props(styles.dialogWarning)}>
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
          onClose={() => setResolving(null)} onRefresh={onResolved}
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
        <p {...stylex.props(styles.muted, styles.statusNote)}>Ingen posteringer i året endnu.</p>
      ) : (
        <ul {...stylex.props(styles.recentEntries)}>
          {entries.map((e) => (
            <li key={e.id} {...stylex.props(styles.recentEntry)}>
              <span {...stylex.props(styles.recentText)}>{e.text}</span>
              <span {...stylex.props(styles.recentMeta)}>
                <span {...stylex.props(styles.entryDate)}>{e.date}</span>
                <span {...stylex.props(styles.recentAmount)}>
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
