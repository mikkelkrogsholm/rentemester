import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
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
    <section {...stylex.props(...[cockpitStyles.statement])}>
      {outcome.feedback}
      <PageHeader title="GDPR-indsigt" actions={<><div {...stylex.props(cockpitStyles.rowActions)}>
          <ButtonLink  to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>

          <p {...stylex.props(cockpitStyles.muted)}>
            Find personoplysninger om en person eller virksomhed (kunde eller
            leverandør) og anonymisér dem hvor bogføringspligten ikke længere
            kræver dem.
          </p>
        </div>

      </PageHeader>

      <section {...stylex.props(cockpitStyles.card)}>
        <h3 {...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}>Søg person eller virksomhed</h3>
        <form onSubmit={runExport} {...stylex.props(cockpitStyles.filterBar)}>
          <label {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.filterBarLabel,
						)}>
            CVR
            <Input
              disabled={loading || erasing || outcome.blocked}
              type="text"
              value={cvr}
              onChange={(e) => { setCvr(e.target.value); setExportData(null); setReviewedSubject(null); setErasure(null); }}
              placeholder="DK…" xstyle={[cockpitStyles.filterBarInputComposition]}
            />
          </label>
          <label {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.filterBarLabel,
						)}>
            Navn
            <Input
              disabled={loading || erasing || outcome.blocked}
              type="text"
              value={name}
              onChange={(e) => { setName(e.target.value); setExportData(null); setReviewedSubject(null); setErasure(null); }}
              placeholder="fx 'Acme ApS'" xstyle={[cockpitStyles.filterBarInputComposition]}
            />
          </label>
          <Button requiredPermission="company.export"
            type="submit"

            disabled={outcome.blocked || loading || (!cvr.trim() && !name.trim())} xstyle={[cockpitStyles.gdprViewBtnComposition2]}
          >
            {loading ? "Søger …" : "Hent indsigtsrapport"}
          </Button>
        </form>
        <p {...stylex.props(cockpitStyles.muted)}>
          Mindst ét felt er påkrævet. Navnesøgning skelner mellem store og små
          bogstaver.
        </p>
      </section>

      <PrivacyActions key={slug + cvr + name} slug={slug} subject={{ cvr: cvr.trim() || undefined, name: name.trim() || undefined }} />

      {error && (
        <div {...stylex.props(...[cockpitStyles.banner, cockpitStyles.bannerError])} role="alert">
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
          title="Anonymisér personoplysninger"
          body={
            <p {...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}>
              Anonymisér det gennemgåede subjekt {reviewedSubject?.cvr || reviewedSubject?.name}. I GDPR-rapporten skjules de mulige felter med en markering. Originaler og journal ændres ikke. Rækker, der stadig er bogføringspligtige (5-års-fristen),
              springes over. Handlingen kan ikke fortrydes.
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
    <section {...stylex.props(cockpitStyles.card)}>
      <h3 {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
				)}>
        Indsigtsrapport ({records.length} række
        {records.length === 1 ? "" : "r"})
      </h3>
      {data.export.coverage && <p {...stylex.props(...[cockpitStyles.banner, cockpitStyles.statusChipWarning])}>{data.export.coverage.notice}</p>}
      <p {...stylex.props(cockpitStyles.muted)}>
        Hentet pr. {data.export.asOf}. Under bogføringspligt:{" "}
        {underRetention}. Allerede anonymiseret: {alreadyErased}. Kan
        anonymiseres nu: {erasable}.
      </p>
      {records.length === 0 ? (
        <p {...stylex.props(cockpitStyles.muted)}>
          Ingen personoplysninger fundet for det angivne navn/CVR.
        </p>
      ) : (
        <>
          <div {...stylex.props(cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData])}>
            <thead {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <tr {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Kilde</th>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Navn</th>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>CVR</th>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Email</th>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Adresse</th>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Opbevares til</th>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Status</th>
              </tr>
            </thead>
            <tbody {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              {records.map((r, i) => (
                <RecordRow key={`${r.source}-${r.sourceRowId}-${i}`} row={r} />
              ))}
            </tbody>
          </table></div>
          <div {...stylex.props(cockpitStyles.rowActions)}>
            <Button variant="danger"
              type="button"

              onClick={onErase}
              requiredPermission="company.admin" disabled={erasing || erasable === 0}
            >
              {erasing
                ? "Anonymiserer …"
                : `Anonymisér de ${erasable} mulige rækker`}
            </Button>
          </div>
        </>
      )}
    </section>
  );
}

function RecordRow({ row }: { row: GdprExportRecord }) {
  return (
    <tr {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>{SOURCE_LABEL[row.source] ?? row.source}</td>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>{row.personalData.name ?? "—"}</td>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>{row.personalData.vatOrCvr ?? "—"}</td>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>{row.personalData.email ?? "—"}</td>
      <td {...stylex.props(cockpitStyles.muted)}>{row.personalData.address ?? "—"}</td>
      <td {...stylex.props(cockpitStyles.muted)}>{row.retainUntil ?? "—"}</td>
      <td {...stylex.props(
					cockpitStyles.tableDataTd,
					cockpitStyles.element,
					cockpitStyles.focusVisible,
				)}>
        {row.erased ? (
          <span {...stylex.props(...[cockpitStyles.statusChip])}>Anonymiseret</span>
        ) : row.retentionUnknown ? (
          <span {...stylex.props(...[cockpitStyles.statusChip])}>Opbevaringsfrist skal afklares</span>
        ) : row.underRetention ? (
          <span {...stylex.props(...[cockpitStyles.statusChip, cockpitStyles.statusChipWarning])}>Under bogføringspligt</span>
        ) : !row.erasable ? (
          <span {...stylex.props(...[cockpitStyles.statusChip, cockpitStyles.statusChipWarning])}>Kan ikke anonymiseres</span>
        ) : (
          <span {...stylex.props(...[cockpitStyles.statusChip, cockpitStyles.statusChipSuccess])}>Kan anonymiseres</span>
        )}
      </td>
    </tr>
  );
}

function ErasureSummary({ result }: { result: GdprErasureResult }) {
  return (
    <section {...stylex.props(cockpitStyles.card)}>
      <h3 {...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.h3,
				)}>Anonymisering — resultat</h3>
      <p {...stylex.props(cockpitStyles.muted)}>
        Pr. {result.asOf}. Anonymiseret: {result.erasedCount}. Allerede
        anonymiseret: {result.alreadyErasedCount}. Afvist (under bogføringspligt):{" "}
        {result.refusedCount}.
      </p>
      {result.refused.length > 0 && (
        <>
          <h4 {...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h4,
						)}>Afviste rækker</h4>
          <div {...stylex.props(cockpitStyles.tableScroll)}><table {...stylex.props(...[cockpitStyles.tableData])}>
            <thead {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              <tr {...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Kilde</th>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Reference</th>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Opbevares til</th>
                <th {...stylex.props(
											cockpitStyles.tableDataTh,
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>Grund</th>
              </tr>
            </thead>
            <tbody {...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}>
              {result.refused.map((r, i) => (
                <tr key={i} {...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}>
                  <td {...stylex.props(
												cockpitStyles.tableDataTd,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>{SOURCE_LABEL[r.source] ?? r.source}</td>
                  <td {...stylex.props(
												cockpitStyles.tableDataTd,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>{r.label ?? `#${r.sourceRowId}`}</td>
                  <td {...stylex.props(cockpitStyles.muted)}>{r.retainUntil ?? "—"}</td>
                  <td {...stylex.props(
												cockpitStyles.tableDataTd,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}>{r.reason}</td>
                </tr>
              ))}
            </tbody>
          </table></div>
        </>
      )}
    </section>
  );
}

// Server owns current authority, legal hold history and trusted dates.
function PrivacyActions({ slug, subject }: { slug: string; subject: { cvr?: string; name?: string } }) {
  const { can } = useCapabilities(slug);
  const outcome = useMutationOutcome();
  const [holdId, setHoldId] = useState("");
  const [reason, setReason] = useState("");
  const [since, setSince] = useState("");
  const [until, setUntil] = useState("");
  const [result, setResult] = useState<unknown>(null);
  const [error, setError] = useState<string | null>(null);
  type Action = Parameters<typeof api.gdprAction>[1];
  type Body = Parameters<typeof api.gdprAction>[2];
  const [pending, setPending] = useState<{ action: Action; body: Body } | null>(null);
  const [busy, setBusy] = useState(false);
  const hasSubject = !!(subject.cvr || subject.name);
  const blocked = busy || outcome.blocked;
  const prepare = (action: Action, operation?: "place" | "release") => {
    if (blocked || !can(action === "legal-hold" ? "company.admin" : "company.export")) return;
    const body: Body = action === "audit-log" ? { since: since || undefined, until: until || undefined, confirm: true }
      : action === "legal-hold" ? { ...subject, holdId, reason, operation: operation!, confirm: true }
      : { ...subject, confirm: true };
    setPending({ action, body });
  };
  return <section {...stylex.props(cockpitStyles.card)}>
    {outcome.feedback}
    <h3>Opdagelse, juridisk hold og audit</h3>
    <p {...stylex.props(cockpitStyles.muted)}>Opdagelse og audit-eksport skriver auditspor. Hold ophæver aldrig andre opbevaringskrav. Ingen automatisk gentagelse ved ukendt udfald.</p>
    <Button requiredPermission="company.export" disabled={blocked || !hasSubject} onClick={() => prepare("discover")}>Opdag personoplysninger</Button>
    <div {...stylex.props(cockpitStyles.filterBar)}>
      <label>Hold-ID<Input value={holdId} maxLength={128} disabled={blocked} onChange={e => setHoldId(e.target.value)} /></label>
      <label>Begrundelse<Input value={reason} maxLength={2000} disabled={blocked} onChange={e => setReason(e.target.value)} /></label>
      <Button requiredPermission="company.admin" disabled={blocked || !hasSubject || !holdId.trim() || !reason.trim()} onClick={() => prepare("legal-hold", "place")}>Placér hold</Button>
      <Button requiredPermission="company.admin" disabled={blocked || !hasSubject || !holdId.trim() || !reason.trim()} onClick={() => prepare("legal-hold", "release")}>Frigiv hold</Button>
    </div>
    <div {...stylex.props(cockpitStyles.filterBar)}>
      <label>Audit fra<Input type="date" value={since} disabled={blocked} onChange={e => setSince(e.target.value)} /></label>
      <label>Audit til<Input type="date" value={until} disabled={blocked} onChange={e => setUntil(e.target.value)} /></label>
      <Button requiredPermission="company.export" disabled={blocked} onClick={() => prepare("audit-log")}>Eksportér GDPR-audit</Button>
    </div>
    <p {...stylex.props(cockpitStyles.muted)}>En indlejret signaturnøgle er ikke uafhængig verifikation. Kontrollér audit-signaturen med en særskilt betroet nøgle.</p>
    {error && <p {...stylex.props(...[cockpitStyles.banner, cockpitStyles.bannerError])} role="alert">{error}</p>}
    {result !== null && <><h4>Seneste serverresultat (ikke automatisk opdateret)</h4><pre>{JSON.stringify(result, null, 2)}</pre></>}
    {pending && <ConfirmDialog title="Bekræft GDPR-handling" confirmLabel="Udfør bekræftet handling" body={<><p>Gennemgå den præcise handling {pending.action}. Holdfrigivelse tillader ikke automatisk anonymisering.</p><pre>{JSON.stringify(pending.body, null, 2)}</pre></>} onClose={() => setPending(null)} onConfirm={async () => {
      if (outcome.isBlocked()) return;
      setBusy(true); setError(null); setResult(null);
      try { setResult(await api.gdprAction(slug, pending.action, pending.body).catch(outcome.reject)); }
      catch (e) { setError(e instanceof Error ? e.message : "GDPR-handlingen fejlede."); throw e; }
      finally { setBusy(false); }
    }} />}
  </section>;
}
