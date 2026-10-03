import { ButtonLink, Button, Input, PageHeader } from "../components/ui";
// GDPR export + forget UI (#334).
//
// Per-virksomhed view der hjælper ejeren med at besvare en indsigtsanmodning
// (Persondataforordningens art. 15) og udføre en sletning (art. 17). Begge
// flows er tynde skaller over kernens buildGdprSubjectExport og
// eraseGdprSubject — kernen håndterer 5-års retention og audit-log'ing.

import { useState } from "react";
import { useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import type {
  CompanyGdpr,
  GdprErasureResult,
  GdprExportRecord,
} from "../lib/types";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useCapabilities } from "../lib/useCapabilities";

const SOURCE_LABEL: Record<string, string> = {
  customers: "Kunde",
  vendors: "Leverandør",
  documents: "Bilag",
  bank_transactions: "Banktransaktion",
  journal_entries: "Postering",
  journal_lines: "Posteringslinje",
  audit_log: "Audit-log",
};

export function GdprView() {
  const { slug = "" } = useParams();
  const { can } = useCapabilities(slug);
  const [cvr, setCvr] = useState("");
  const [name, setName] = useState("");
  const [exportData, setExportData] = useState<CompanyGdpr | null>(null);
  const [erasure, setErasure] = useState<GdprErasureResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [erasing, setErasing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pendingErase, setPendingErase] = useState(false);
  const [reviewedSubject, setReviewedSubject] = useState<{ cvr?: string; name?: string } | null>(null);
  const refreshExport = async () => {
    if (!reviewedSubject) return;
    const data = await api.gdprExport(slug, reviewedSubject);
    setExportData(data);
  };
  // Even a GDPR lookup appends an attributed audit event. It is never used as
  // a read-only retry/status button after an interrupted write.
  const outcome = useMutationOutcome();

  const runExport = async (e: React.FormEvent) => {
    e.preventDefault();
    if (loading || outcome.isBlocked() || !can("company.export")) return;
    const subject = { cvr: cvr.trim() || undefined, name: name.trim() || undefined };
    setError(null);
    setErasure(null);
    setExportData(null); setReviewedSubject(null);
    setLoading(true);
    try {
      const data = await api.gdprExport(slug, subject).catch(outcome.reject);
      setReviewedSubject(subject);
      setExportData(data);
    } catch (err) {
      setError(err instanceof ApiError ? err.message : "Indsigtsopslag fejlede.");
      setExportData(null);
    } finally {
      setLoading(false);
    }
  };

  const runErase = async () => {
    if (!exportData || !reviewedSubject || outcome.isBlocked() || !can("company.admin")) return;
    setError(null);
    setErasing(true);
    try {
      const result = await api.gdprErase(slug, reviewedSubject).catch(outcome.reject);
      setErasure(result);
      // Re-run export så ejeren ser den opdaterede status.
      try { await refreshExport(); }
      catch { setError("Anonymiseringen er gennemført, men den opdaterede indsigt kunne ikke hentes."); }
    } catch (err) {
      setError(
        err instanceof ApiError ? err.message : "Anonymisering fejlede.",
      );
      throw err;
    } finally {
      setErasing(false);
    }
  };

  return (
    <section className="gdpr-view" data-cockpit-page="gdpr" data-evidence-issue="655">
      {outcome.feedback}
      <PageHeader title="GDPR-indsigt" actions={<><div className="row-actions">
          <ButtonLink className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div>

          <p className="muted">
            Find personoplysninger om en person eller virksomhed (kunde eller
            leverandør) og anonymisér dem hvor bogføringspligten ikke længere
            kræver dem.
          </p>
        </div>

      </PageHeader>

      <section className="card">
        <h3>Find oplysninger</h3>
        <form onSubmit={runExport} className="filter-bar">
          <label>
            CVR
            <Input
              disabled={loading || erasing || outcome.blocked}
              type="text"
              value={cvr}
              onChange={(e) => { setCvr(e.target.value); setExportData(null); setReviewedSubject(null); setErasure(null); }}
              placeholder="DK…"
            />
          </label>
          <label>
            Navn
            <Input
              disabled={loading || erasing || outcome.blocked}
              type="text"
              value={name}
              onChange={(e) => { setName(e.target.value); setExportData(null); setReviewedSubject(null); setErasure(null); }}
              placeholder="fx 'Acme ApS'"
            />
          </label>
          <Button requiredPermission="company.export"
            type="submit"
            className="btn primary"
            disabled={outcome.blocked || loading || (!cvr.trim() && !name.trim())}
          >
            {loading ? "Søger …" : "Find oplysninger"}
          </Button>
        </form>
        <p className="muted">
          Mindst ét felt er påkrævet. Navnesøgning skelner mellem store og små
          bogstaver.
        </p>
      </section>

      {error && (
        <div className="callout danger" role="alert">
          {error}
        </div>
      )}

      {exportData && (
        <ExportPanel
          data={exportData}
          erasing={erasing || outcome.blocked}
          onErase={() => setPendingErase(true)}
        />
      )}

      {erasure && <ErasureSummary result={erasure} />}

      {pendingErase && (
        <ConfirmDialog
          title="Bekræft anonymisering"
          body={
            <p>
              Anonymisér det gennemgåede subjekt {reviewedSubject?.cvr || reviewedSubject?.name}. Konsekvens: Anonymisering erstatter de viste, tilladte
              personoplysninger med en spærret markering. Rækker, der stadig er
              bogføringspligtige, springes over. Handlingen kan ikke fortrydes.
            </p>
          }
          confirmLabel="Anonymisér nu"
          confirmKind="danger"
          onConfirm={async () => {
            await runErase();
          }}
          onClose={() => setPendingErase(false)}
        />
      )}
    </section>
  );
}

function ExportPanel({
  data,
  erasing,
  onErase,
}: {
  data: CompanyGdpr;
  erasing: boolean;
  onErase: () => void;
}) {
  const { records } = data.export;
  const underRetention = records.filter((r) => r.underRetention).length;
  const erasable = records.filter((r) => r.erasable).length;
  const alreadyErased = records.filter((r) => r.erased).length;

  return (
    <section className="card">
      <h3>
        Indsigtsrapport ({records.length} række
        {records.length === 1 ? "" : "r"})
      </h3>
      <p className="muted">
        Hentet pr. {data.export.asOf}. Under bogføringspligt:{" "}
        {underRetention}. Allerede anonymiseret: {alreadyErased}. Kan
        anonymiseres nu: {erasable}.
      </p>
      {records.length === 0 ? (
        <p className="muted">
          Ingen personoplysninger fundet for det angivne navn/CVR.
        </p>
      ) : (
        <>
          <div className="table-scroll"><table className="table">
            <thead>
              <tr>
                <th>Kilde</th>
                <th>Navn</th>
                <th>CVR</th>
                <th>Email</th>
                <th>Adresse</th>
                <th>Opbevares til</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {records.map((r, i) => (
                <RecordRow key={`${r.source}-${r.sourceRowId}-${i}`} row={r} />
              ))}
            </tbody>
          </table></div>
          <section className="card" aria-labelledby="anonymisering-heading">
            <h4 id="anonymisering-heading">Anonymisering</h4>
            <p>
              Resultat: {erasable} række{erasable === 1 ? " kan" : "r kan"} anonymiseres nu, mens {underRetention} fortsat skal opbevares.
            </p>
            <p className="muted">Konsekvens: Tilladte personoplysninger erstattes permanent med en spærret markering. Gennemgå resultatet ovenfor før du fortsætter.</p>
            <div className="row-actions">
              <Button
                type="button"
                variant="danger" requiredPermission="company.admin" className="btn danger"
                onClick={onErase}
                disabled={erasing || erasable === 0}
              >
                {erasing
                  ? "Anonymiserer …"
                  : `Anonymisér de ${erasable} mulige rækker`}
              </Button>
            </div>
          </section>
        </>
      )}
    </section>
  );
}

function RecordRow({ row }: { row: GdprExportRecord }) {
  return (
    <tr>
      <td>{SOURCE_LABEL[row.source] ?? row.source}</td>
      <td>{row.personalData.name ?? "—"}</td>
      <td>{row.personalData.vatOrCvr ?? "—"}</td>
      <td>{row.personalData.email ?? "—"}</td>
      <td className="muted">{row.personalData.address ?? "—"}</td>
      <td className="muted">{row.retainUntil ?? "—"}</td>
      <td>
        {row.erased ? (
          <span className="pill">Anonymiseret</span>
        ) : row.underRetention ? (
          <span className="pill warn">Under bogføringspligt</span>
        ) : !row.erasable ? (
          <span className="pill warn">Kan ikke anonymiseres</span>
        ) : (
          <span className="pill ok">Kan anonymiseres</span>
        )}
      </td>
    </tr>
  );
}

function ErasureSummary({ result }: { result: GdprErasureResult }) {
  return (
    <section className="card">
      <h3>Anonymisering — resultat</h3>
      <p className="muted">
        Pr. {result.asOf}. Anonymiseret: {result.erasedCount}. Allerede
        anonymiseret: {result.alreadyErasedCount}. Afvist (under bogføringspligt):{" "}
        {result.refusedCount}.
      </p>
      {result.refused.length > 0 && (
        <>
          <h4>Afviste rækker</h4>
          <div className="table-scroll"><table className="table">
            <thead>
              <tr>
                <th>Kilde</th>
                <th>Reference</th>
                <th>Opbevares til</th>
                <th>Grund</th>
              </tr>
            </thead>
            <tbody>
              {result.refused.map((r, i) => (
                <tr key={i}>
                  <td>{SOURCE_LABEL[r.source] ?? r.source}</td>
                  <td>{r.label ?? `#${r.sourceRowId}`}</td>
                  <td className="muted">{r.retainUntil ?? "—"}</td>
                  <td>{r.reason}</td>
                </tr>
              ))}
            </tbody>
          </table></div>
        </>
      )}
    </section>
  );
}
