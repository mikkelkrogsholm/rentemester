import { documentAttr } from "../../design/document-html";
// Tax card — estimated corporate tax for the open fiscal year, or an
// "awaiting year-end" placeholder.

import { escapeHtml, formatDkk, type DashboardTaxStatus } from "./_shared";

/**
 * Tax card — estimated corporate tax for the open fiscal year, or a
 * "preparation available once the year is closed" state.
 */
export function taxSection(tax: DashboardTaxStatus): string {
  if (!tax.available) {
    return `<div ${documentAttr("statusRow")}>
    <div>
      <div ${documentAttr("label")}>Selskabsskat · regnskabsår ${escapeHtml(tax.fiscalYearLabel)}</div>
      <div ${documentAttr("detail")}>Forberedelse er klar, når regnskabsåret er lukket</div>
    </div>
    <div><span ${documentAttr("pillNeutral")}>afventer årsafslutning</span></div>
  </div>`;
  }
  const corporateTax = tax.corporateTax ?? null;
  const taxValue = corporateTax == null ? "—" : formatDkk(corporateTax);
  const reviewCount = tax.needsReviewCount ?? 0;
  const reviewPill = reviewCount > 0
    ? `<span ${documentAttr("pillWarning")}>${reviewCount} til gennemgang</span>`
    : `<span ${documentAttr("pillSuccess")}>✔ ingen åbne punkter</span>`;
  return `<div ${documentAttr("statusRow")}>
    <div>
      <div ${documentAttr("label")}>Estimeret selskabsskat · regnskabsår ${escapeHtml(tax.fiscalYearLabel)}</div>
      <div ${documentAttr("detail")}>årets resultat <span ${documentAttr("mono")}>${escapeHtml(formatDkk(tax.bookkeptResult ?? 0))}</span></div>
    </div>
    <div ${documentAttr("amountLg")}>${escapeHtml(taxValue)}</div>
  </div>
  <div ${documentAttr("statusRow")}>
    <div>
      <div ${documentAttr("label")}>Needs-review</div>
      <div ${documentAttr("detail")}>${reviewCount > 0 ? "poster Rentemester ikke beregner deterministisk" : "oplysningsskemaet kan forberedes"}</div>
    </div>
    <div>${reviewPill}</div>
  </div>`;
}
