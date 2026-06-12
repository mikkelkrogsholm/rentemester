// One company in the portfolio overview. Renders the headline health an owner
// judges a company on — resultat, faktisk banksaldo, omsætning, moms (beløb +
// frist) and opgaver — plus the derived "needs attention" flags. The whole
// card is a link into the company's Overblik.

import { Link } from "react-router-dom";
import type { CompanySummary } from "../lib/types";
import {
  attentionFlags,
  attentionLevel,
  formatDeadline,
  formatKroner,
} from "../lib/format";

export function CompanyCard({ company }: { company: CompanySummary }) {
  const level = attentionLevel(company);
  const flags = attentionFlags(company);

  return (
    <article
      className={`company-card level-${level}${company.archived ? " archived" : ""}`}
    >
      <div className="cc-head">
        <div>
          <h3>
            <Link to={`/companies/${company.slug}`}>{company.name}</Link>
          </h3>
          <div className="cc-cvr">
            {company.cvr ? `CVR ${company.cvr}` : "CVR ikke angivet"}
            {company.fiscalYear ? ` · regnskabsår ${company.fiscalYear}` : ""}
          </div>
        </div>
        {company.archived && <span className="badge">Arkiveret</span>}
      </div>

      {company.ledgerMissing ? (
        <p className="empty-inline">
          Virksomheden er registreret, men har endnu intet regnskab på disken.
        </p>
      ) : (
        <div className="cc-metrics">
          <div className={`cc-metric ${company.resultat < 0 ? "neg" : "pos"}`}>
            <span className="m-label">Resultat (år til dato)</span>
            <span className="m-value">{formatKroner(company.resultat)}</span>
          </div>
          <div className="cc-metric">
            <span className="m-label">Faktisk banksaldo</span>
            <span className="m-value">
              {company.actualBankBalance === null
                ? "—"
                : formatKroner(company.actualBankBalance)}
            </span>
            {company.actualBankBalance === null && (
              <span className="m-sub">intet kontoudtog importeret</span>
            )}
          </div>
          <div className="cc-metric">
            <span className="m-label">Omsætning</span>
            <span className="m-value">{formatKroner(company.omsaetning)}</span>
          </div>
          <div className="cc-metric">
            <span className="m-label">Moms at betale</span>
            <span className="m-value">
              {company.vat ? formatKroner(company.vat.payable) : "—"}
            </span>
            {company.vat && (
              <span
                className={`m-sub${
                  company.vat.payable > 0 && company.vat.daysRemaining <= 30
                    ? company.vat.daysRemaining < 0
                      ? " sub-critical"
                      : " sub-warning"
                    : ""
                }`}
              >
                frist {company.vat.deadline} ·{" "}
                {formatDeadline(company.vat.daysRemaining)}
              </span>
            )}
          </div>
        </div>
      )}

      {!company.ledgerMissing && (
        <div className="cc-tasks">
          {company.openTaskCount === 0 ? (
            <span className="cc-task-none">Ingen åbne opgaver</span>
          ) : (
            <>
              <span className="cc-task-count">
                {company.openTaskCount} åbne opgaver
              </span>
              {company.taskGroups.slice(0, 2).map((g) => (
                <span key={g.type} className={`cc-task-line sev-${g.severity}`}>
                  {g.label}
                </span>
              ))}
            </>
          )}
        </div>
      )}

      <div className="flags">
        {flags.length === 0 ? (
          <span className="flag ok">Sund drift</span>
        ) : (
          flags.map((f) =>
            // #420 — flag med et `to`-felt er klikbare: ejeren får et konkret
            // næste skridt (fx Integritet-viewet) i stedet for et skræmmende
            // dødt label.
            f.to ? (
              <Link
                key={f.label}
                to={f.to}
                className={`flag flag-link ${f.level}`}
              >
                {f.label}
              </Link>
            ) : (
              <span key={f.label} className={`flag ${f.level}`}>
                {f.label}
              </span>
            ),
          )
        )}
      </div>
    </article>
  );
}
