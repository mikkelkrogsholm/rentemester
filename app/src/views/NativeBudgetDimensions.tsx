import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { Link } from "react-router-dom";
import { ErrorState, Loading } from "../components/Feedback";
import { Select } from "../components/ui";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type { CompanyBudgetDimensionActuals } from "../lib/types";
import { useAsync } from "../lib/useAsync";

/** Server projections preserve exact cents and current immutable assignment evidence. */
export function NativeBudgetDimensions({
  slug,
  initial,
  currency,
}: {
  slug: string;
  initial: CompanyBudgetDimensionActuals;
  currency: string;
}) {
  const [selection, setSelection] = useState("");
  const option = initial.dimensionOptions.find((o) => o.value === selection);
  const state = useAsync(
    (signal) =>
      option?.dimensionId
        ? api.budgetDimensionActuals(
            slug,
            initial.selectedYear,
            { signal },
            {
              dimension: option.dimensionId,
              ...(option.memberId ? { member: option.memberId } : {}),
            },
          )
        : Promise.resolve(initial),
    [slug, initial.selectedYear, selection, initial],
  );
  const data = state.data;
  return (
    <section {...stylex.props(cockpitStyles.card, cockpitStyles.statementCard)} aria-label="Dimensionssammenligning">
      <h3>Dimensioner mod konto-budget</h3>
      <p {...stylex.props(cockpitStyles.muted)}>
        Konto-budgettet gælder hele kontoen. Dimensionsbudget og variance er ukendte uden en
        reviewet budgetfordeling, der stemmer med seneste konto-budget. Ufordelt er kontoens
        faktiske beløb uden en aktuel tildeling til den valgte dimension; andre medlemmer vises
        særskilt.
      </p>
      <label>
        Filter dimension
        <Select
          aria-label="Filter dimension"
          value={selection}
          onChange={(e) => setSelection(e.target.value)}
        >
          <option value="">Vælg dimension eller medlem</option>
          {initial.dimensionOptions.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </Select>
      </label>
      {!initial.dimensionOptions.length && (
        <p>
          Ingen dimensioner oprettet.{" "}
          <Link to={`/companies/${slug}/dimensioner`}>Opret dimensioner</Link>
        </p>
      )}
      {state.error ? (
        <ErrorState message={state.error} onRetry={state.reload} />
      ) : state.loading ? (
        <Loading label="Henter dimensionsaktualer…" />
      ) : (
        selection &&
        data && (
          <>
            {data.notes?.map((note) => (
              <p {...stylex.props(cockpitStyles.muted)} key={note}>
                {note}
              </p>
            ))}
            <div {...stylex.props(cockpitStyles.tableScroll)}>
              <table {...stylex.props(...[cockpitStyles.tableData, cockpitStyles.tableStatementTable])}>
                <thead>
                  <tr>
                    <th>Konto</th>
                    <th>Måned</th>
                    <th>Dimensionsaktual</th>
                    <th>Dimensionsbudget</th>
                    <th>Variance</th>
                    <th>Andre medlemmer</th>
                    <th>Ufordelt</th>
                    <th>Kontoaktual</th>
                    <th>Konto-budget</th>
                    <th>Kilde</th>
                  </tr>
                </thead>
                <tbody>
                  {!data.comparison?.length && (
                    <tr>
                      <td colSpan={10}>
                        Ingen kontobudgetter eller faktiske beløb i regnskabsåret.
                      </td>
                    </tr>
                  )}
                  {data.comparison?.map((row) => (
                    <tr key={`${row.accountNo}:${row.period}`}>
                      <td>{row.accountNo}</td>
                      <td>{row.period}</td>
                      <td>{formatKroner(row.actual, currency)}</td>
                      <td>{row.budget === null ? "Ukendt" : formatKroner(row.budget, currency)}</td>
                      <td>
                        {row.variance === null ? "Ukendt" : formatKroner(row.variance, currency)}
                      </td>
                      <td>{formatKroner(row.otherMembersActual, currency)}</td>
                      <td>{formatKroner(row.unallocatedActual, currency)}</td>
                      <td>{formatKroner(row.accountActual, currency)}</td>
                      <td>{formatKroner(row.accountBudget, currency)}</td>
                      <td>
                        {row.journalLineIds.length
                          ? row.journalLineIds.map((id) => (
                              <div key={id}>
                                <Link
                                  to={`/companies/${slug}/posteringer?year=${encodeURIComponent(data.selectedYear)}&journalLineId=${id}`}
                                >
                                  Journal-linje {id}
                                </Link>
                              </div>
                            ))
                          : "Ingen tildelte journal-linjer"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )
      )}
    </section>
  );
}
