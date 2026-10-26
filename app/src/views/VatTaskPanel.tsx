import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { Link } from "react-router-dom";
import { api } from "../lib/api";
import type { CompanyVat, CompanyVatRegistered } from "../lib/types";
import { useAsync } from "../lib/useAsync";
import { Button } from "../components/ui";

const SOURCES: Record<string, [string, string]> = {
  BANK_UNRECONCILED: ["Bankposter mangler afstemning", "bank"],
  DOCUMENT_OUTSTANDING: ["Bilag mangler gennemgang eller bogføring", "bilag"],
  DOCUMENT_ORIGINAL_INTEGRITY: ["Originale bilag skal kontrolleres", "bilag"],
  BATCH_UNPOSTED_OR_FAILED: ["Bogføring er ikke afsluttet", "posteringer"],
  DKK_CONTROL_ACCOUNTS: ["Konti og kilder stemmer ikke eller kan ikke kontrolleres", "posteringer"],
  VAT_PREFLIGHT: ["Momsgrundlaget skal kontrolleres", "posteringer"],
  TRIAL_BALANCE: ["Regnskabets balance skal kontrolleres", "posteringer"],
  EXCEPTIONS_OPEN: ["Uafklarede forhold skal gennemgås", "bilag"],
};

type Props = { slug: string; vat: CompanyVat; verified: boolean; onRefresh: () => void };

export function VatTaskPanel(props: Props) {
  const { vat, verified } = props;
  if (!verified) return <TaskNotice state="unavailable" onRefresh={props.onRefresh}>Kan ikke verificere momsopgaven. Opdatér og kontrollér den aktuelle periode.</TaskNotice>;
  if (vat.archived) return <TaskNotice state="archived">Arkiveret år — momsgrundlaget kan ikke rekonstrueres her.</TaskNotice>;
  if (!vat.vatRegistered) return <TaskNotice state="unavailable">Virksomheden er ikke momsregistreret; der er ingen momsopgave eller SKAT-frist.</TaskNotice>;
  if (vat.periodActionsAvailable === false || vat.slug !== props.slug) return <TaskNotice state="unavailable" onRefresh={props.onRefresh}>Kan ikke verificere momsopgaven. Kontrollér virksomhed og perioder.</TaskNotice>;
  return <VerifiedTask {...props} vat={vat} />;
}

function TaskNotice({ state, children, onRefresh }: { state: string; children: React.ReactNode; onRefresh?: () => void }) {
  return <section {...stylex.props(cockpitStyles.card)} aria-label="Momsopgave" data-state={state}>
    <h3>Momsopgave</h3><p>{children}</p><p>Rentemester sender ikke til SKAT.</p>
    {onRefresh && <Button variant="secondary" onClick={onRefresh}>Opdatér momsopgave</Button>}
  </section>;
}

function VerifiedTask({ slug, vat: v, onRefresh }: Props & { vat: CompanyVatRegistered }) {
  const proof = useAsync(async (signal) => {
    const periods = await api.periods(slug, { signal });
    const packet = v.periodStatus === "open" ? await api.closeReadiness(slug, v.periodStart, v.periodEnd, { signal }) : null;
    return { periods, packet };
  }, [slug, v.periodStart, v.periodEnd], { resourceKey: v });
  const data = !proof.loading && !proof.error ? proof.data : null;
  const matches = data?.periods.periods.filter(p => p.periodStart === v.periodStart && p.periodEnd === v.periodEnd);
  // Match the server's effective kind precedence, never a different quarter/year.
  const rows = matches?.filter(p => p.kind === "vat_period");
  const candidates = rows?.length ? rows : matches?.filter(p => p.kind === "vat_quarter");
  const row = candidates?.length === 1 ? candidates[0] : undefined;
  const consistent = data?.periods.slug === slug && (candidates?.length ?? 0) <= 1 &&
    (row ? row.effectiveStatus === v.periodStatus : v.periodStatus === "open");
  const packet = data?.packet;
  let state = "unavailable";
  if (consistent && v.periodStatus === "open" && !v.momsangivelseReady && packet?.hash && Array.isArray(packet.items)) {
    state = v.vatReportErrors.length || packet.blockers || packet.items.some(i => i.status === "blocked" || i.status === "unavailable") ? "blocked" : "draft";
  } else if (consistent && v.momsangivelseReady && !v.vatReportErrors.length && row?.closePacketHash) {
    if (v.periodStatus === "closed") state = "reviewed";
    if (v.periodStatus === "reported" && row.reference?.trim()) state = "recorded";
  }
  const to = (path: string) => `/companies/${encodeURIComponent(slug)}/${path}?year=${encodeURIComponent(v.selectedYear)}`;
  const refresh = () => { proof.reload(); onRefresh(); };
  return <section {...stylex.props(cockpitStyles.card)} aria-label="Momsopgave" data-state={state}>
    <h3>Momsopgave</h3>
    <p>{v.periodLabel} · {v.periodStart} – {v.periodEnd} · Frist: {v.deadline}</p>
    <p>Grundlag: bogførte momskonti og bilag i den valgte periode.</p>
    <p>Rentemester sender ikke til SKAT.</p>
    {state === "unavailable" && <p>Kan ikke verificere momsopgaven. Opdatér og kontrollér den aktuelle periode og adgang i Periodelås.</p>}
    {state === "blocked" && <p>Momsopgaven er blokeret. Gennemgå kilderne før review og lukning.</p>}
    {state === "draft" && <p>Beregnet kladde — tallene er foreløbige. Gennemgå grundlaget og luk derefter perioden med review.</p>}
    {(state === "blocked" || state === "draft") && packet && <>
      <p>Kontroller: {packet.blockers} blokeringer, {packet.warnings} advarsler.</p>
      <ul>{packet.items.filter(i => i.status !== "passed").map(i => {
        const source = SOURCES[i.code];
        return <li key={i.code}>{source?.[0] ?? `Kontrol ${i.code}`} — {i.count} ({i.status === "unavailable" ? "kan ikke kontrolleres" : i.status === "blocked" ? "blokeret" : "advarsel"}). <Link to={to(source?.[1] ?? "periodelas")}>Gennemgå kilde</Link></li>;
      })}</ul>
      <Link to={to("bank")}>Gennemgå bank</Link>{" · "}<Link to={to("bilag")}>Gennemgå bilag</Link>
    </>}
    {state === "reviewed" && <p>Gennemgået og låst — indberetningsklar arbejdsrapport, ikke en registreret indberetning. Indberet manuelt i TastSelv Erhverv. Registrér derefter kvitteringen via “Registrér indberetning” i Periodelås.</p>}
    {state === "recorded" && <p>Ekstern indberetning registreret. Kvitteringsreference: <strong>{row!.reference}</strong>. Ingen ny indberetningshandling.</p>}
    <p><Link to={to("periodelas")}>{state === "recorded" ? "Se registreret indberetning i Periodelås" : "Åbn Periodelås"}</Link></p>
    <Button variant="secondary" onClick={refresh}>Opdatér momsopgave</Button>
  </section>;
}
