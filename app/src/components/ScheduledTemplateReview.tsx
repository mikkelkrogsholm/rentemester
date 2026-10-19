import type { ScheduledPlan } from "../lib/api/scheduled-recurring";
const intervals: Record<string, string> = { weekly: "uge", monthly: "måned", quarterly: "kvartal", yearly: "år" };
export function ScheduledTemplateReview({ template: t }: { template: ScheduledPlan["template"] }) {
  return <section aria-label="Gennemgå fakturaskabelon"><h4>{t.name}</h4>
    <p>Sælger: {t.payload.seller?.name ?? "Ikke angivet"} · Kunde: {t.payload.buyer?.name ?? "Ikke angivet"}</p>
    <p>Sælgers adresse: {t.payload.seller?.address ?? "Ikke angivet"} · CVR/momsnummer: {t.payload.seller?.vatOrCvr ?? "Ikke angivet"}</p>
    <p>Kundens adresse: {t.payload.buyer?.address ?? "Ikke angivet"} · CVR/momsnummer: {t.payload.buyer?.vatOrCvr ?? "Ikke angivet"}</p>
    <p>Første fakturadato: {t.firstIssueDate} · Betalingsfrist: {t.paymentTermsDays} dage · Interval: {t.intervalCount} × {intervals[t.interval] ?? t.interval}</p>
    <p>Leveringsperiode: {t.deliveryPeriodMode === "issue_month" ? "Fakturadatoens måned" : t.deliveryPeriodMode === "previous_month" ? "Foregående måned" : t.deliveryPeriodMode === "none" ? "Ingen" : t.deliveryPeriodMode}</p>
    <p>Momssats i skabelonen: {t.payload.vatRatePercent == null ? "Ikke angivet" : `${t.payload.vatRatePercent} %`}</p>
    <table><thead><tr><th>Beskrivelse</th><th>Antal</th><th>Enhedspris ekskl. moms</th></tr></thead><tbody>{t.payload.lines.map((line, i) => <tr key={i}><td>{line.description}</td><td>{String(line.quantity)}</td><td>{String(line.unitPriceExVat)} {t.payload.currency ?? ""}</td></tr>)}</tbody></table>
    <p>Beløbene er skabelonens kildepriser. Endelige beløb valideres og beregnes ved udstedelse.</p>
  </section>;
}
