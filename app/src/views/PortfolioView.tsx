import {useState} from "react";
import { Button, ButtonLink, Input, PageHeader } from "../components/ui";
// Portfolio overview — the workspace-level landing page.
//
// A cross-company roll-up strip answers "how is the whole portfolio doing",
// and one card per company shows the headline health an owner judges a
// company on. Companies that need attention sort to the top and are flagged.

import { useOptionalAuth } from "../lib/auth-context";
import { useNavigate } from "react-router-dom";
import { api } from "../lib/api";
import { formatKroner, sortByAttention } from "../lib/format";
import { useAsync } from "../lib/useAsync";
import { CompanyCard } from "../components/CompanyCard";
import { ErrorState, Loading } from "../components/Feedback";
import { Onboarding } from "./Onboarding";

export function PortfolioView() {
  const auth = useOptionalAuth();
  if (auth?.backend === "rust") return <NativePortfolioOverview/>;
  return <PortfolioOverview />;
}

function PortfolioOverview() {
  const navigate = useNavigate();
  const state = useAsync((signal) => api.portfolio(undefined, { signal }), []);

  if (state.loading) return <Loading label="Henter portefølje…" />;
  if (state.error)
    return <ErrorState message={state.error} onRetry={state.reload} />;

  const portfolio = state.data!;

  // First run: an empty workspace drops straight into onboarding.
  if (portfolio.companies.length === 0) {
    return (
      <Onboarding onCreated={(slug) => navigate(`/companies/${slug}`)} />
    );
  }

  const ordered = sortByAttention(portfolio.companies);
  const needAttention = ordered.filter(
    (c) =>
      !c.archived &&
      (c.ledgerMissing ||
        !c.auditChainOk ||
        c.resultat < 0 ||
        c.openTaskCount > 0 ||
        (c.vat !== null &&
          c.vat.payable > 0 &&
          c.vat.daysRemaining <= 30)),
  ).length;

  const { rollup } = portfolio;

  return (
    <section>
      <PageHeader title="Portefølje" actions={<><ButtonLink className="btn" to="/companies/new">
          Tilføj virksomhed
        </ButtonLink></>}>
        <div>

          <p className="muted">
            {portfolio.companyCount} virksomhed
            {portfolio.companyCount === 1 ? "" : "er"} · {needAttention} kræver
            opmærksomhed · pr. {portfolio.asOf}
          </p>
        </div>

      </PageHeader>

      {rollup && (
        <div className="rollup-strip" role="group" aria-label="Tværgående overblik">
          <div
            className={`rollup-cell ${rollup.resultat < 0 ? "neg" : "pos"}`}
          >
            <span className="rollup-label">Samlet resultat</span>
            <span className="rollup-value">
              {formatKroner(rollup.resultat)}
            </span>
          </div>
          <div className="rollup-cell">
            <span className="rollup-label">Samlet likviditet</span>
            <span className="rollup-value">
              {formatKroner(rollup.liquidity)}
            </span>
          </div>
          <div className="rollup-cell">
            <span className="rollup-label">{rollup.vatPayable < 0 ? "Moms til gode" : "Moms at betale"}</span>
            <span className="rollup-value">
              {formatKroner(rollup.vatPayable)}
            </span>
          </div>
          <div
            className={`rollup-cell ${
              rollup.openTaskCount > 0 ? "warn" : ""
            }`}
          >
            <span className="rollup-label">Åbne opgaver</span>
            <span className="rollup-value">{rollup.openTaskCount}</span>
          </div>
        </div>
      )}

      <div className="company-grid">
        {ordered.map((c) => (
          <CompanyCard key={c.slug} company={c} />
        ))}
      </div>
    </section>
  );
}

function NativePortfolioOverview(){
 const [asOf,setAsOf]=useState("");
 const state=useAsync(signal=>api.nativePortfolio(asOf||undefined,{signal}),[asOf]);
 if(state.loading&&!state.data)return <Loading label="Henter portefølje…"/>;
 if(state.error)return <ErrorState message={state.error} onRetry={state.reload}/>;
 const data=state.data!;
 const amount=(value:number|null)=>value===null?"Ikke tilgængelig":formatKroner(value,"DKK");
 const count=(value:number|null)=>value===null?"Ikke tilgængelig":String(value);
 return <section><PageHeader title="Portefølje" actions={<ButtonLink className="btn" to="/companies/new">Tilføj virksomhed</ButtonLink>}/>
  <div className="row-actions"><label>Pr. dato<Input type="date" value={asOf||data.asOf} onChange={e=>setAsOf(e.target.value)}/></label><Button variant="secondary" onClick={state.reload}>Hent portefølje igen</Button></div>
  <p>{data.companyCount} virksomheder · pr. {data.asOf}</p><p className="muted">Opgaver viser aktuel åben status for kilder til og med den valgte dato.</p><p className="muted">Overblik over virksomheder, du aktuelt har adgang til. Beløbene er ikke et konsolideret koncernregnskab.</p>
  <div className="rollup-strip" role="group" aria-label="Tværgående overblik">
   {[['Samlet resultat',amount(data.rollup.resultat)],['Samlet likviditet',amount(data.rollup.liquidity)],['Momsposition',amount(data.rollup.vatPayable)],['Åbne opgaver',count(data.rollup.openTaskCount)]].map(([label,value])=><div className="rollup-cell" key={label}><span className="rollup-label">{label}</span><span className="rollup-value">{value}</span></div>)}
  </div>
  {!data.rollup.bankBalanceComplete&&<p role="status">Samlet banksaldo er ikke tilgængelig: {data.rollup.unknownBankCompanyCount} synlige virksomheder mangler et fuldt kendt kontoudtogsgrundlag.</p>}
  {data.companies.length===0?<Onboarding onCreated={()=>state.reload()}/>:<div className="company-grid">{data.companies.map(company=><article className="card" key={company.slug} aria-label={company.name}><h2>{company.name}</h2><p>{company.fiscalYear??"Regnskabsår ikke tilgængeligt"} · {company.periodStart??"Ukendt start"} – {company.periodEnd??"Ukendt slut"}</p>
   <dl>{[['Omsætning',amount(company.omsaetning)],['Resultat',amount(company.resultat)],['Banksaldo',amount(company.actualBankBalance)],['Momsposition',amount(company.vat?.payable??null)],['Åbne fakturaer',count(company.openInvoiceCount)],['Åbne opgaver',count(company.openTaskCount)]].map(([label,value])=><div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>
   {Object.entries(company.availability).filter(([key,value])=>key!=="tasks"&&value.reason).map(([key,value])=><p key={key}>{value.reason}</p>)}
   {company.taskGroups.length>0&&<ul>{company.taskGroups.map(group=><li key={group.type}>{group.link?<ButtonLink to={group.link}>{group.label} · {group.count}</ButtonLink>:<>{group.label} · {group.count}</>}</li>)}</ul>}
   {company.limitations.map(message=><p className="muted" key={message}>{message}</p>)}<p>Ledgerkontrol: {company.audit.status==="verified"?"Kontrolleret journalhashkæde":"Ikke tilgængelig"}</p>
   <div className="row-actions"><ButtonLink to={`/companies/${company.slug}/posteringer`}>Åbn bogføring</ButtonLink><ButtonLink to={`/companies/${company.slug}/likviditet`}>Se pengestrøm</ButtonLink></div>
  </article>)}</div>}
 </section>;
}
