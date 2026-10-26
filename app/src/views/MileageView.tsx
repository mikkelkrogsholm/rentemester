import { cockpitStyles } from "../design/cockpit.stylex";
import { MileagePostingPanel } from "./MileagePostingPanel";
import * as stylex from "@stylexjs/stylex";
import { ButtonLink, Button, PageHeader } from "../components/ui";
// Kørsel — the per-company mileage register (#335).
//
// Renders `/api/companies/:slug/mileage?year=`: a deterministic mileage log
// for the selected fiscal year. The page mirrors the cockpit's other
// company views — a `CompanyNav` + `useCompanyYear` chrome, a summary card
// strip ("Sum pr. periode") and a table of trips. A primary "Registrér
// kørsel" button opens `MileageRegisterModal` which POSTs through the same
// `createMileageEntry` core the CLI's `mileage add` command uses.
//
// The mileage register is documentation/audit data — Rentemester never
// posts it to the journal. The view therefore lives under the "Bogføring"
// sub-nav group as a reference register, not a posting screen. An archived
// regnskabsår hides the action button (same pattern as BankView /
// DocumentsView): historical mileage is read-only.

import { useState } from "react";
import { useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { CompanyMileage } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { MileageRegisterModal } from "../components/MileageRegisterModal";

export function MileageView() {
  const { slug = "" } = useParams();
  const { year, setYear } = useCompanyYear();
  const state = useAsync<CompanyMileage>(
    (signal) => api.mileage(slug, year, { signal }),
    [slug, year],
  );
  const [registering, setRegistering] = useState(false);

  if (state.loading && !state.data)
    return <Loading label="Henter kørselsregister…" />;
  if (state.error && !state.data)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const m = state.data!;
  const currency = m.company.currency || "DKK";
  const archived = m.archived;
  const hasEntries = m.entries.length > 0;

  return (
    <section {...stylex.props(cockpitStyles.statement)}>
      {state.error && <div {...stylex.props(cockpitStyles.banner)} role="alert">Status kunne ikke opdateres. Din formular er bevaret; oplysningerne bag den er fra den seneste gennemførte læsning.</div>}
      <PageHeader title="Kørsel" actions={<><div {...stylex.props(cockpitStyles.rowActions)}>
          {!archived && (
            <Button requiredPermission="company.ledger.post"
              type="button"

              onClick={() => setRegistering(true)} xstyle={[cockpitStyles.statementBtnComposition]}
            >
              Registrér kørsel
            </Button>
          )}
          <ButtonLink  to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>

          <p {...stylex.props(cockpitStyles.muted)}>
            {m.company.cvr ? `CVR ${m.company.cvr} · ` : ""}
            {m.company.country} · {currency} · Kørsel
          </p>
        </div>

      </PageHeader>

      <CompanyNav
        slug={slug}
        years={m.fiscalYears}
        selectedYear={m.selectedYear}
        onYearChange={setYear}
      />

      {archived ? (
        <ArchivedNotice year={m.selectedYear} />
      ) : !hasEntries ? (
        <EmptyState
          year={m.selectedYear}
          onRegister={() => setRegistering(true)}
        />
      ) : (
        <>
          <p {...stylex.props(cockpitStyles.statementAsof, cockpitStyles.muted)}>
            Kørselsregister — regnskabsår {m.selectedYear}
          </p>

          <SummaryCards mileage={m} currency={currency} />

          <MonthlyBreakdown mileage={m} currency={currency} />

          <div {...stylex.props(cockpitStyles.card, cockpitStyles.statementCard, cockpitStyles.tableScroll)}>
            <table {...stylex.props(...[cockpitStyles.tableData, cockpitStyles.tableStatementTable])}>
              <thead {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableThead,
								)}>
                <tr {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTr,
									)}>
                  <th {...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition5,
										)}>Bilag</th>
                  <th {...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition5,
										)}>Dato</th>
                  <th {...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition5,
										)}>Formål</th>
                  <th {...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition5,
										)}>Fra → Til</th>
                  <th {...stylex.props(cockpitStyles.num)}>Km</th>
                  <th {...stylex.props(cockpitStyles.num)}>Takst</th>
                  <th {...stylex.props(cockpitStyles.num)}>Grundlag</th>
                  <th {...stylex.props(
											cockpitStyles.statementTableScrollTableThComposition5,
										)}>Takst-basis</th>
                </tr>
              </thead>
              <tbody {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTbody,
								)}>
                {m.entries.map((row) => (
                  <tr key={row.id} {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
											cockpitStyles.responsiveTableTr,
										)}>
                    <td {...stylex.props(...[cockpitStyles.num])}>{row.entryNo}</td>
                    <td {...stylex.props(cockpitStyles.entryDate)}>{row.tripDate}</td>
                    <td {...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition7,
											)}>{row.purpose}</td>
                    <td {...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition7,
											)}>
                      {row.fromLocation} → {row.toLocation}
                    </td>
                    <td {...stylex.props(cockpitStyles.num)}>{row.kilometers}</td>
                    <td {...stylex.props(cockpitStyles.num)}>
                      {formatKroner(row.ratePerKm, currency)}/km
                    </td>
                    <td {...stylex.props(cockpitStyles.num)}>
                      {formatKroner(row.amountBasis, currency)}
                    </td>
                    <td {...stylex.props(
												cockpitStyles.statementTableScrollTableTdComposition7,
											)}>
                      {row.rateSource ? (
                        <a
                          href={row.rateSource}
                          target="_blank"
                          rel="noreferrer noopener" {...stylex.props(cockpitStyles.aComposition)}
                        >
                          {row.rateBasis}
                        </a>
                      ) : (
                        row.rateBasis
                      )}
                    </td>
                  </tr>
                ))}
                <tr {...stylex.props(...[cockpitStyles.statementResultTd])}>
                  <td colSpan={4} {...stylex.props(
											cockpitStyles.statementTableScrollTableTdComposition8,
										)}>I alt — {m.selectedYear}</td>
                  <td {...stylex.props(cockpitStyles.num)}>{m.totalKilometers}</td>
                  <td {...stylex.props(cockpitStyles.num)}>—</td>
                  <td {...stylex.props(cockpitStyles.num)}>
                    {formatKroner(m.totalAmountBasis, currency)}
                  </td>
                  <td {...stylex.props(
											cockpitStyles.statementTableScrollTableTdComposition8,
										)} />
                </tr>
              </tbody>
            </table>
          </div>
          <p {...stylex.props(cockpitStyles.statementCheck)}>
            Kørselsregisteret er dokumentation. Registrering bogfører aldrig
            kørselsgodtgørelse automatisk — vurdér med din rådgiver om turen er
            fradragsberettiget, og brug fx en udlægspostering hvis du vil
            udbetale eller fratrække beløbet. Officielle satser findes på{" "}
            <a
              href="https://skat.dk/erhverv/moms/regler-og-satser/satser-for-erhvervsmaessig-koersel"
              target="_blank"
              rel="noreferrer noopener" {...stylex.props(cockpitStyles.aComposition)}
            >
              skat.dk
            </a>
            .
          </p>
        </>
      )}

      <MileagePostingPanel key={`${slug}:${m.selectedYear}`} slug={slug} entries={m.entries} disabled={archived || state.loading || !!state.error} />

      {registering && (
        <MileageRegisterModal
          slug={slug}
          periodStart={m.periodStart}
          periodEnd={m.periodEnd}
          onRegistered={() => state.reload()}
          onClose={() => setRegistering(false)}
        />
      )}
    </section>
  );
}

function SummaryCards({
  mileage,
  currency,
}: {
  mileage: CompanyMileage;
  currency: string;
}) {
  return (
    <div {...stylex.props(cockpitStyles.statusGrid, cockpitStyles.invoicesSummary)}>
      <div {...stylex.props(cockpitStyles.card, cockpitStyles.statusCard)}>
        <h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
						cockpitStyles.statusCardH3,
					)}>Antal ture</h3>
        <div {...stylex.props(cockpitStyles.statusFigure)}>{mileage.tripCount}</div>
        <p {...stylex.props(cockpitStyles.muted, cockpitStyles.statusNote)}>
          {mileage.tripCount === 1 ? "kørsel" : "kørsler"} i{" "}
          {mileage.selectedYear}
        </p>
      </div>
      <div {...stylex.props(cockpitStyles.card, cockpitStyles.statusCard)}>
        <h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
						cockpitStyles.statusCardH3,
					)}>Samlet km</h3>
        <div {...stylex.props(cockpitStyles.statusFigure)}>{mileage.totalKilometers}</div>
        <p {...stylex.props(cockpitStyles.muted, cockpitStyles.statusNote)}>km i regnskabsåret</p>
      </div>
      <div {...stylex.props(cockpitStyles.card, cockpitStyles.statusCard)}>
        <h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
						cockpitStyles.statusCardH3,
					)}>Godtgørelsesgrundlag</h3>
        <div {...stylex.props(cockpitStyles.statusFigure)}>
          {formatKroner(mileage.totalAmountBasis, currency)}
        </div>
        <p {...stylex.props(cockpitStyles.muted, cockpitStyles.statusNote)}>km × takst — dokumentation</p>
      </div>
    </div>
  );
}

function MonthlyBreakdown({
  mileage,
  currency,
}: {
  mileage: CompanyMileage;
  currency: string;
}) {
  // Only show the breakdown when at least one month has activity — otherwise
  // it is just twelve rows of zero, which the summary card already conveys.
  if (mileage.months.every((m) => m.tripCount === 0)) return null;
  return (
    <div {...stylex.props(cockpitStyles.card, cockpitStyles.statementCard, cockpitStyles.tableScroll)}>
      <h3 {...stylex.props(viewStyles.site0)}>Sum pr. måned</h3>
      <table {...stylex.props(...[cockpitStyles.tableData, cockpitStyles.tableStatementTable])}>
        <thead {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
          <tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
            <th {...stylex.props(cockpitStyles.tableStatementTableThComposition3)}>Måned</th>
            <th {...stylex.props(cockpitStyles.num)}>Ture</th>
            <th {...stylex.props(cockpitStyles.num)}>Km</th>
            <th {...stylex.props(cockpitStyles.num)}>Grundlag</th>
          </tr>
        </thead>
        <tbody {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
          {mileage.months.map((row) => (
            <tr key={row.month} {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
              <td {...stylex.props(
									cockpitStyles.tableStatementTableTdComposition2,
								)}>{row.label}</td>
              <td {...stylex.props(cockpitStyles.num)}>{row.tripCount}</td>
              <td {...stylex.props(cockpitStyles.num)}>{row.kilometers}</td>
              <td {...stylex.props(cockpitStyles.num)}>
                {row.amountBasis > 0
                  ? formatKroner(row.amountBasis, currency)
                  : "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function EmptyState({
  year,
  onRegister,
}: {
  year: string;
  onRegister: () => void;
}) {
  return (
    <div {...stylex.props(cockpitStyles.card, cockpitStyles.archivedNotice)}>
      <h3 {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
					cockpitStyles.archivedNoticeH3,
				)}>Ingen kørsler registreret i {year}</h3>
      <p {...stylex.props(cockpitStyles.muted)}>
        Klik på <strong {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>Registrér kørsel</strong> for at logge en tur — dato,
        formål, fra/til, antal km og den officielle takst du har slået op.
        Rentemester gemmer registret som dokumentation og udregner aldrig en
        skattesats for dig.
      </p>
      <div {...stylex.props(...[cockpitStyles.rowActions, viewStyles.site1])} >
        <Button requiredPermission="company.ledger.post" type="button"  onClick={onRegister}>
          Registrér første kørsel
        </Button>
      </div>
    </div>
  );
}

function ArchivedNotice({ year }: { year: string }) {
  return (
    <div {...stylex.props(cockpitStyles.card, cockpitStyles.archivedNotice)}>
      <h3 {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
					cockpitStyles.archivedNoticeH3,
				)}>Kørsel er ikke tilgængelig for {year}</h3>
      <p {...stylex.props(cockpitStyles.muted)}>
        {year} er et arkiveret regnskabsår. Kørselsregisteret findes kun for
        den aktive ledger; arkiverede år vises som read-only historik på de
        øvrige skærmbilleder (Resultatopgørelse, Balance osv.).
      </p>
    </div>
  );
}

const viewStyles = stylex.create({
site0: { margin: "0.25rem 0 0.5rem" },
site1: { marginTop: "0.5rem" }
});
