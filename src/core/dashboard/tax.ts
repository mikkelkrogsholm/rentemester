// Tax card — estimated corporate tax for the open fiscal year, or an
// "awaiting year-end" placeholder.

import { escapeHtml, formatDkk, type DashboardTaxStatus } from "./_shared";

/**
 * Tax card — estimated corporate tax for the open fiscal year, or a
 * "preparation available once the year is closed" state.
 */
export function taxSection(tax: DashboardTaxStatus): string {
  if (!tax.available) {
    return `<div class="status-row">
    <div>
      <div class="label">Selskabsskat · regnskabsår ${escapeHtml(tax.fiscalYearLabel)}</div>
      <div class="detail">Forberedelse er klar, når regnskabsåret er lukket</div>
    </div>
    <div><span class="pill neutral">afventer årsafslutning</span></div>
  </div>`;
  }
  const corporateTax = tax.corporateTax ?? null;
  const taxValue = corporateTax == null ? "—" : formatDkk(corporateTax);
  const reviewCount = tax.needsReviewCount ?? 0;
  const reviewPill = reviewCount > 0
    ? `<span class="pill warning">${reviewCount} til gennemgang</span>`
    : `<span class="pill success">✔ ingen åbne punkter</span>`;
  return `<div class="status-row">
    <div>
      <div class="label">Estimeret selskabsskat · regnskabsår ${escapeHtml(tax.fiscalYearLabel)}</div>
      <div class="detail">årets resultat <span class="mono">${escapeHtml(formatDkk(tax.bookkeptResult ?? 0))}</span></div>
    </div>
    <div class="amount-lg">${escapeHtml(taxValue)}</div>
  </div>
  <div class="status-row">
    <div>
      <div class="label">Needs-review</div>
      <div class="detail">${reviewCount > 0 ? "poster Rentemester ikke beregner deterministisk" : "oplysningsskemaet kan forberedes"}</div>
    </div>
    <div>${reviewPill}</div>
  </div>`;
}
