import { Link, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { CompanyNav, useCompanyYear } from "../components/CompanyNav";
import { DocumentBookExpenseForm } from "../components/DocumentBookExpenseModal";
import { ErrorState, Loading } from "../components/Feedback";
import { ButtonLink, PageHeader } from "../components/ui";
import { api } from "../lib/api";
import { useCapabilities } from "../lib/useCapabilities";
import { useAsync } from "../lib/useAsync";
import { groupDocuments } from "./DocumentsView";
import { listReturnTo } from "./workflow-navigation";

export function DocumentBookingView() {
  const { slug = "", documentId: rawId } = useParams();
  const { can } = useCapabilities(slug);
  const documentId = Number(rawId);
  const validId = Number.isSafeInteger(documentId) && documentId > 0;
  const { year, setYear } = useCompanyYear();
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const state = useAsync(async (signal) => {
    const [documents, fiscalYears] = await Promise.all([api.documents(slug, { signal }), api.fiscalYears(slug, { signal })]);
    return { documents, fiscalYears };
  }, [slug]);
  if (state.loading && !state.data) return <Loading label="Henter bilaget…" />;
  if (state.error) return <ErrorState message={state.error} onRetry={state.reload} />;
  const { documents, fiscalYears } = state.data!;
  const selectedYear = year ?? fiscalYears.find((entry) => entry.source === "live")?.label ?? fiscalYears[0]?.label ?? String(new Date().getFullYear());
  const archived = fiscalYears.find((entry) => entry.label === selectedYear)?.source === "archive";
  const document = validId ? groupDocuments(documents.documents).find((entry) => entry.id === documentId) : undefined;
  const returnTo = listReturnTo(slug, "bilag", params.get("returnTo"), selectedYear);
  return <section className="statement workflow-page">
    <PageHeader title="Bogfør bilag" description={`${documents.company.name} · ${document?.documentNo ?? "Bilag"} · Alle bilag`} actions={<ButtonLink className="btn secondary" to={returnTo}>Tilbage til bilag</ButtonLink>} />
    <CompanyNav slug={slug} years={fiscalYears} selectedYear={selectedYear} onYearChange={setYear} />
    {!can("company.ledger.post") ? <div className="card" role="status"><h2>Du kan ikke bogføre bilag</h2><p>Din rolle giver adgang til at læse virksomhedens oplysninger.</p></div> : !document ? <div className="card" role="status"><h2>Bilaget findes ikke</h2><p>Gå tilbage til bilagslisten og vælg et eksisterende bilag.</p></div> : archived ? <div className="card" role="status"><h2>Regnskabsåret er arkiveret</h2><p>Bilaget kan ikke bogføres i {selectedYear}. Vælg et aktivt regnskabsår.</p></div> : document.journalEntryNo ? <div className="card" role="status"><h2>Bilaget er allerede bogført</h2><p>Se den eksisterende postering {document.journalEntryNo} i bilagsdetaljerne.</p><Link to={`/companies/${encodeURIComponent(slug)}/bilag/${documentId}?${new URLSearchParams({ year: selectedYear, returnTo })}`}>Se bilaget</Link></div> :
      <div className="workflow-with-document">
        <aside className="workflow-document card" aria-label="Bilagsfil">
          <h2>Bilagsgrundlag</h2>
          <p>{document.supplierName ?? document.accountingRationale ?? document.documentNo}</p>
          {document.hasFile ? <><a href={api.documentFileUrl(slug, documentId)} target="_blank" rel="noreferrer">Åbn bilagsfil</a>
            {document.filename?.toLowerCase().endsWith(".pdf") && <details><summary>Vis bilaget</summary><object data={api.documentFileUrl(slug, documentId)} type="application/pdf" aria-label={`Bilag ${document.documentNo ?? documentId}`} className="document-preview"><p>Åbn bilagsfilen for at læse dokumentet.</p></object></details>}
          </> : <p className="muted">Der er ingen fil gemt på bilaget.</p>}
        </aside>
        <DocumentBookExpenseForm key={`${slug}:${documentId}:${selectedYear}`} slug={slug} documentId={documentId} presentation="page" onBooked={() => undefined} onClose={() => navigate(returnTo)} /></div>}
  </section>;
}
