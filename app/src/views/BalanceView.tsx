import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { ReportSourceNotice } from "../components/ReportSourceNotice";
import { ButtonLink, PageHeader } from "../components/ui";
// Balance — the per-company balance sheet (cockpit-redesign iteration 2).
//
// Renders `/api/companies/:slug/balance?year=`: assets, liabilities and equity
// sections with section totals, as of the fiscal year's end date. The fiscal
// year's result is folded into the equity section as an "Årets resultat" line
// so `equity.total` is the equity an owner reads and the sheet balances
// (assets = liabilities + equity). All money fields are kroner.
//
// Sammenligningstal (#400 / ÅRL § 24): every line and every section total
// also carries the prior year's figure, so the balance fulfils the same
// regnskabskrav as resultatopgørelsen. When the ledger has no foregående
// regnskabsår, the prior column shows «—» rather than 0.

import { Link, useParams } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import type { BalanceLine, CompanyBalance } from "../lib/types";
import { ErrorState, Loading } from "../components/Feedback";
import { ArchivedBanner } from "../components/ArchivedBanner";
import {
  CompanyNav,
  accountPostingsTo,
  useCompanyYear,
} from "../components/CompanyNav";

/** Render a prior-year amount cell — «—» when no prior year exists. */
function priorCell(amount: import("../lib/types/common").ReportMoney, currency: string) {
  if (amount === null) return "—";
  return formatKroner(amount, currency);
}

export function BalanceView() {
  const { slug = "" } = useParams();
  const { year, source, setYear } = useCompanyYear();
  const state = useAsync<CompanyBalance>(
    (signal) => api.balance(slug, year, { signal, ...(source ? {source} : {}) }),
    [slug, year, source],
  );

  if (state.loading && !state.data) return <Loading label="Henter balance…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const b = state.data!;
  const currency = b.declaredCurrency ?? b.company.currency ?? "DKK";
  const selectedSource = b.selectedSource ?? source;
  const priorYear = String(parseInt(b.selectedYear, 10) - 1);
  const priorHeading = b.priorAsOfDate === null ? "Sammenligning ikke tilgængelig" : `Pr. ${b.priorAsOfDate ?? `${priorYear}-12-31`}`;

  return (
    <section {...stylex.props(cockpitStyles.statement)}>
      <PageHeader title="Balance" actions={<><div {...stylex.props(cockpitStyles.rowActions)}>
          {/* #372 — "Hent CSV". #463 — "Hent PDF" som ren printbar version. */}
          {b.completeness && b.completeness.status !== "ready" ? <p>CSV og PDF kræver et fuldstændigt rapportgrundlag.</p> : <>
          <a
            {...stylex.props(...[cockpitStyles.buttonComposition, cockpitStyles.element])}
            href={api.statementCsvUrl(slug, "balance", b.selectedYear, selectedSource)}
            download
          >
            Hent CSV
          </a>
          <a
            {...stylex.props(...[cockpitStyles.buttonComposition, cockpitStyles.element])}
            href={api.statementPdfUrl(slug, "balance", b.selectedYear, selectedSource)}
            download
          >
            Hent PDF
          </a>
          </>}
          <ButtonLink  to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>

          <p {...stylex.props(cockpitStyles.muted)}>
            {b.company.cvr ? `CVR ${b.company.cvr} · ` : ""}
            {b.company.country} · {currency} · Balance
          </p>
        </div>

      </PageHeader>

      <CompanyNav
        slug={slug}
        years={b.fiscalYears}
        selectedYear={b.selectedYear}
        selectedSource={selectedSource}
        onYearChange={setYear}
      />

      <ReportSourceNotice report={b} slug={slug}/>
      {b.archived && (
        <ArchivedBanner year={b.selectedYear} source={b.archivedSource} />
      )}
      <p {...stylex.props(cockpitStyles.statementAsof, cockpitStyles.muted)}>{b.asOfDate ? `Pr. ${b.asOfDate}` : "Afslutningsdato ikke oplyst"}</p>
      <div {...stylex.props(cockpitStyles.card, cockpitStyles.statementCard)}>
        <div {...stylex.props(cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData, cockpitStyles.tableStatementTable])}>
          <thead {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
            <tr {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <th {...stylex.props(
										cockpitStyles.tableStatementTableThComposition3,
									)}>Konto</th>
              <th {...stylex.props(
										cockpitStyles.tableStatementTableThComposition3,
									)}>Navn</th>
              <th {...stylex.props(cockpitStyles.num)}>{b.asOfDate ? `Pr. ${b.asOfDate}` : "Afslutningsdato ikke oplyst"}</th>
              <th {...stylex.props(cockpitStyles.num)}>{priorHeading}</th>
            </tr>
          </thead>
          <BalanceSection
            heading="Aktiver"
            lines={b.assets.lines}
            total={b.assets.total}
            priorTotal={b.assets.priorTotal}
            totalLabel="Aktiver i alt"
            currency={currency}
            slug={slug}
            year={b.selectedYear}
            source={selectedSource}
          />
          <BalanceSection
            heading="Passiver"
            lines={b.liabilities.lines}
            total={b.liabilities.total}
            priorTotal={b.liabilities.priorTotal}
            totalLabel="Gæld i alt"
            currency={currency}
            slug={slug}
            year={b.selectedYear}
            source={selectedSource}
          />
          <BalanceSection
            heading="Egenkapital"
            lines={b.equity.lines}
            total={b.equity.total}
            priorTotal={b.equity.priorTotal}
            totalLabel="Egenkapital i alt"
            currency={currency}
            slug={slug}
            year={b.selectedYear}
            source={selectedSource}
          />
          <tbody {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
            <tr {...stylex.props(...[cockpitStyles.statementResultTd])}>
              <td colSpan={2} {...stylex.props(
										cockpitStyles.tableStatementTableTdComposition3,
									)}>Passiver og egenkapital i alt</td>
              <td {...stylex.props(cockpitStyles.num)}>
                {formatKroner(b.totalLiabilitiesAndEquity, currency)}
              </td>
              <td {...stylex.props(cockpitStyles.num, cockpitStyles.muted)}>
                {priorCell(b.priorTotalLiabilitiesAndEquity, currency)}
              </td>
            </tr>
          </tbody>
        </table></div>
      </div>
      <BalanceCheck balanced={b.balanced} />
    </section>
  );
}

function BalanceSection({
  heading,
  lines,
  total,
  priorTotal,
  totalLabel,
  currency,
  slug,
  year,
  source,
}: {
  heading: string;
  lines: BalanceLine[];
  total: import("../lib/types/common").ReportMoney;
  priorTotal: import("../lib/types/common").ReportMoney;
  totalLabel: string;
  currency: string;
  slug: string;
  year: string;
  source?: "live" | "archive";
}) {
  return (
    <tbody {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
      <tr {...stylex.props(...[cockpitStyles.statementSubhead])}>
        <th colSpan={4} {...stylex.props(cockpitStyles.statementSectionHeadThComposition)}>{heading}</th>
      </tr>
      {lines.length === 0 ? (
        <tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
          <td colSpan={4} {...stylex.props(cockpitStyles.emptyInline)}>
            Ingen konti.
          </td>
        </tr>
      ) : (
        lines.map((line) => (
          <tr key={line.accountNo} {...stylex.props(...[cockpitStyles.trAccountRowHover])}>
            <td {...stylex.props(...[cockpitStyles.num])}>
              {/* A synthetic line (e.g. "Årets resultat") has no real
                  account number — render it plain rather than as a link. */}
              {line.accountNo === "—" ? (
                "—"
              ) : (
                <Link
                  {...stylex.props(...[cockpitStyles.aComposition])}
                  to={accountPostingsTo(slug, year, line.accountNo, source)}
                >
                  {line.accountNo}
                </Link>
              )}
            </td>
            <td {...stylex.props(cockpitStyles.tableStatementTableTdComposition4)}>{line.name}</td>
            <td {...stylex.props(cockpitStyles.num)}>{formatKroner(line.amount, currency)}</td>
            <td {...stylex.props(cockpitStyles.num, cockpitStyles.muted)}>
              {priorCell(line.priorAmount, currency)}
            </td>
          </tr>
        ))
      )}
      <tr {...stylex.props(...[cockpitStyles.statementSubtotalTd])}>
        <td colSpan={2} {...stylex.props(cockpitStyles.statementSubtotalTdComposition)}>{totalLabel}</td>
        <td {...stylex.props(cockpitStyles.num)}>{formatKroner(total, currency)}</td>
        <td {...stylex.props(cockpitStyles.num, cockpitStyles.muted)}>{priorCell(priorTotal, currency)}</td>
      </tr>
    </tbody>
  );
}

function BalanceCheck({ balanced }: { balanced: boolean | null }) {
  return (
    <p {...stylex.props(...[cockpitStyles.statementCheck, ...(balanced ? [cockpitStyles.statusChipSuccess] : [cockpitStyles.statementCheckAlert])])}>
      {balanced === null ? "Balancens kontrol kan ikke opgøres med det tilgængelige grundlag." : balanced
        ? "Balancen stemmer — aktiver = passiver + egenkapital."
        : "Balancen stemmer ikke. Kontrollér ledgeren."}
    </p>
  );
}
