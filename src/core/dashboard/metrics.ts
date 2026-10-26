import { documentAttr } from "../../design/document-html";
// Top-of-page metric tiles: åbne fakturaer, overforfaldne, bankposter uden
// bilag, åbne exceptions.

import { escapeHtml, formatDkk, type DashboardInput } from "./_shared";

export function metricCard(label: string, value: string, secondary?: string, accent?: "accent" | "danger" | null): string {
  const card = accent === "danger" ? "metricDanger" : accent === "accent" ? "metricAccent" : "metricCard";
  const secondaryHtml = secondary
    ? `<div ${documentAttr("secondary")}>${escapeHtml(secondary)}</div>`
    : `<div ${documentAttr("secondary")}>&nbsp;</div>`;
  return `<div ${documentAttr(card)}>
  <div ${documentAttr("amountLg")}>${escapeHtml(value)}</div>
  ${secondaryHtml}
  <div ${documentAttr("metricLabel")}>${escapeHtml(label)}</div>
</div>`;
}

export function metricsSection(input: DashboardInput): string {
  const openSum = input.invoices.rows.reduce((acc, r) => acc + r.openBalance, 0);
  const overdueOldest = input.overdueInvoices.rows.reduce((acc, r) => Math.max(acc, r.overdueDays), 0);
  return `<section ${documentAttr("metrics")}>
${metricCard("ÅBNE FAKTURAER", String(input.invoices.count), `${formatDkk(openSum)}`, null)}
${metricCard("OVERFORFALDNE", String(input.overdueInvoices.count), input.overdueInvoices.count > 0 ? `ældste ${overdueOldest} d` : "0 dage", input.overdueInvoices.count > 0 ? "accent" : null)}
${metricCard("BANKPOSTER UDEN BILAG", String(input.unlinkedBank.count), undefined, null)}
${metricCard("ÅBNE EXCEPTIONS", String(input.exceptions.count), undefined, input.exceptions.count > 0 ? "danger" : null)}
</section>`;
}
