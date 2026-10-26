import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
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
    <section {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
      <PageHeader title="Portefølje" actions={<><ButtonLink  to="/companies/new" xstyle={[cockpitStyles.aComposition]}>
          Tilføj virksomhed
        </ButtonLink></>}>
        <div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>

          <p {...stylex.props(cockpitStyles.muted)}>
            {portfolio.companyCount} virksomhed
            {portfolio.companyCount === 1 ? "" : "er"} · {needAttention} kræver
            opmærksomhed · pr. {portfolio.asOf}
          </p>
        </div>

      </PageHeader>

      {rollup && (
        <div {...stylex.props(cockpitStyles.rollupStrip)} role="group" aria-label="Tværgående overblik">
          <div
            {...stylex.props(...[cockpitStyles.rollupCell, ...(rollup.resultat < 0 ? [cockpitStyles.rollupCellNeg] : [cockpitStyles.rollupCellPos])])}
          >
            <span {...stylex.props(...[cockpitStyles.rollupCellRollupLabel])}>Samlet resultat</span>
            <span {...stylex.props(...[cockpitStyles.rollupCellRollupValue])}>
              {formatKroner(rollup.resultat)}
            </span>
          </div>
          <div {...stylex.props(cockpitStyles.rollupCell)}>
            <span {...stylex.props(...[cockpitStyles.rollupCellRollupLabel])}>Samlet likviditet</span>
            <span {...stylex.props(...[cockpitStyles.rollupCellRollupValue])}>
              {formatKroner(rollup.liquidity)}
            </span>
          </div>
          <div {...stylex.props(cockpitStyles.rollupCell)}>
            <span {...stylex.props(...[cockpitStyles.rollupCellRollupLabel])}>{rollup.vatPayable < 0 ? "Moms til gode" : "Moms at betale"}</span>
            <span {...stylex.props(...[cockpitStyles.rollupCellRollupValue])}>
              {formatKroner(rollup.vatPayable)}
            </span>
          </div>
          <div
            {...stylex.props(...[cockpitStyles.rollupCell, ...(rollup.openTaskCount > 0 ? [cockpitStyles.rollupCellWarn] : [])])}
          >
            <span {...stylex.props(...[cockpitStyles.rollupCellRollupLabel])}>Åbne opgaver</span>
            <span {...stylex.props(...[cockpitStyles.rollupCellRollupValue])}>{rollup.openTaskCount}</span>
          </div>
        </div>
      )}

      <div {...stylex.props(cockpitStyles.companyGrid)}>
        {ordered.map((c) => (
          <CompanyCard key={c.slug} company={c} />
        ))}
      </div>
    </section>
  );
}

function availabilityMessage(reason:string){
 const messages:Record<string,string>={
  native_history_begins_at_explicit_cutover:"Tallene dækker kun perioden fra den dokumenterede overgang til Rentemester.",
  archive_or_cutover_basis_unavailable:"Det historiske regnskabsgrundlag er ikke tilgængeligt for denne dato.",
  native_invoices_only_imported_receivables_separate:"Fakturatal omfatter kun fakturaer udstedt i Rentemester. Importerede tilgodehavender følges særskilt.",
  unvalued_or_adjusted_receivable:"Tilgodehavenderne kræver afklaring af værdiansættelse eller korrektioner.",
  vat_reconciliation_errors:"Momsgrundlaget skal afstemmes, før beløbet kan anvendes til angivelse.",
  none:"Der mangler et kontoudtog med dokumenteret saldo.",
  "no-balance-column":"Kontoudtoget indeholder ingen saldi.",
  unavailable:"Oplysningerne er ikke tilgængelige på et entydigt grundlag.",
 };
 return messages[reason]??(reason.includes("_")?"Oplysningerne kræver yderligere dokumentation.":reason);
}

function NativePortfolioOverview(){
 const [asOf,setAsOf]=useState("");
 const state=useAsync(signal=>api.nativePortfolio(asOf||undefined,{signal}),[asOf]);
 if(state.loading&&!state.data)return <Loading label="Henter portefølje…"/>;
 if(state.error)return <ErrorState message={state.error} onRetry={state.reload}/>;
 const data=state.data!;
 const amount=(value:number|null)=>value===null?"Ikke tilgængelig":formatKroner(value,"DKK");
 const count=(value:number|null)=>value===null?"Ikke tilgængelig":String(value);
 return <section><PageHeader title="Portefølje" actions={<ButtonLink  to="/companies/new">Tilføj virksomhed</ButtonLink>}/>
  <div {...stylex.props(cockpitStyles.rowActions)}><label>Pr. dato<Input type="date" value={asOf||data.asOf} onChange={e=>setAsOf(e.target.value)}/></label><Button variant="secondary" onClick={state.reload}>Hent portefølje igen</Button></div>
  <p>{data.companyCount} virksomheder · pr. {data.asOf}</p><p {...stylex.props(cockpitStyles.muted)}>Opgaver viser aktuel åben status for kilder til og med den valgte dato.</p><p {...stylex.props(cockpitStyles.muted)}>Overblik over virksomheder, du aktuelt har adgang til. Beløbene er ikke et konsolideret koncernregnskab.</p>
  <div {...stylex.props(cockpitStyles.rollupStrip)} role="group" aria-label="Tværgående overblik">
   {[['Samlet resultat',amount(data.rollup.resultat)],['Samlet likviditet',amount(data.rollup.liquidity)],['Moms i aktuelle perioder',amount(data.rollup.vatPayable)],['Åbne opgaver',count(data.rollup.openTaskCount)]].map(([label,value])=><div {...stylex.props(cockpitStyles.rollupCell)} key={label}><span {...stylex.props(...[cockpitStyles.rollupCellRollupLabel])}>{label}</span><span {...stylex.props(...[cockpitStyles.rollupCellRollupValue])}>{value}</span></div>)}
  </div>
  {!data.rollup.bankBalanceComplete&&<p role="status">Samlet banksaldo er ikke tilgængelig: {data.rollup.unknownBankCompanyCount} synlige virksomheder mangler et fuldt kendt kontoudtogsgrundlag.</p>}
  {data.companies.length===0?<Onboarding onCreated={()=>state.reload()}/>:<div {...stylex.props(cockpitStyles.companyGrid)}>{data.companies.map(company=><article {...stylex.props(cockpitStyles.card)} key={company.slug} aria-label={company.name}><h2>{company.name}</h2><p>{company.fiscalYear??"Regnskabsår ikke tilgængeligt"} · {company.periodStart??"Ukendt start"} – {company.periodEnd??"Ukendt slut"}</p>
   <dl>{[['Omsætning',amount(company.omsaetning)],['Resultat',amount(company.resultat)],['Banksaldo',amount(company.actualBankBalance)],['Moms i perioden',amount(company.vat?.payable??null)],['Åbne fakturaer',count(company.openInvoiceCount)],['Åbne opgaver',count(company.openTaskCount)]].map(([label,value])=><div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>
   {company.vat?.periodStart&&company.vat.periodEnd&&<p {...stylex.props(cockpitStyles.muted)}>Momsperiode {company.vat.periodStart} – {company.vat.periodEnd}, opgjort til {company.vat.cutoff??data.asOf}.</p>}
   {Object.entries(company.availability).filter(([key,value])=>key!=="tasks"&&value.reason).map(([key,value])=><p key={key}>{availabilityMessage(value.reason??"unavailable")}</p>)}
   {company.taskGroups.length>0&&<ul>{company.taskGroups.map(group=><li key={group.type}>{group.link?<ButtonLink to={group.link}>{group.label} · {group.count}</ButtonLink>:<>{group.label} · {group.count}</>}</li>)}</ul>}
   {company.limitations.map(message=><p {...stylex.props(cockpitStyles.muted)} key={message}>{message}</p>)}<p>Ledgerkontrol: {company.audit.status==="verified"?"Kontrolleret journalhashkæde":"Ikke tilgængelig"}</p>
   <div {...stylex.props(cockpitStyles.rowActions)}><ButtonLink to={`/companies/${company.slug}/posteringer`}>Åbn bogføring</ButtonLink><ButtonLink to={`/companies/${company.slug}/likviditet`}>Se pengestrøm</ButtonLink></div>
  </article>)}</div>}
 </section>;
}
