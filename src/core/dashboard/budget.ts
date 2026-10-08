import { documentAttr } from "../../design/document-html";
// Budget & liquidity card — budget-vs-actual for the current period plus the
// liquidity forecast for the coming months.

import type { BudgetVsActualReport } from "../budget";
import type { LiquidityForecastResult } from "../liquidity-forecast";
import { escapeHtml, formatDkk } from "./_shared";

/**
 * Budget & liquidity card — budget-vs-actual for the current period plus the
 * liquidity forecast for the coming months.
 */
export function budgetLiquiditySection(
  budget: BudgetVsActualReport | undefined,
  liquidity: LiquidityForecastResult | undefined,
): string {
  const parts: string[] = [];

  const hasBudget = budget && budget.ok && budget.lines.length > 0;
  if (hasBudget) {
    // The raw `totalVariance` (= totalActual − totalBudget) is NOT a
    // favourability signal — its sign depends on the account mix, so an
    // over-budget expense month yields a POSITIVE total. The per-line
    // `BudgetVsActualLine.variance` already encodes "positive = favourable"
    // for every account type (expense: budget − actual; income/other: actual
    // − budget). Their sum is therefore the correct favourability figure.
    const favourability = budget!.lines.reduce((sum, line) => sum + line.variance, 0);
    const pill = favourability >= 0
      ? `<span ${documentAttr("pillSuccess")}>budget overholdt</span>`
      : `<span ${documentAttr("pillWarning")}>budgetafvigelse</span>`;
    parts.push(`<div ${documentAttr("statusRow")}>
    <div>
      <div ${documentAttr("label")}>Budget vs. faktisk · ${escapeHtml(budget!.periodStart)}</div>
      <div ${documentAttr("detail")}>budget <span ${documentAttr("mono")}>${escapeHtml(formatDkk(budget!.totalBudget))}</span> · faktisk <span ${documentAttr("mono")}>${escapeHtml(formatDkk(budget!.totalActual))}</span></div>
    </div>
    <div>${pill} <span ${documentAttr("muted")}>${escapeHtml(formatDkk(favourability))}</span></div>
  </div>`);
  } else {
    parts.push(`<div ${documentAttr("statusRow")}>
    <div>
      <div ${documentAttr("label")}>Budget vs. faktisk</div>
      <div ${documentAttr("detail")}>Intet budget sat for perioden</div>
    </div>
    <div><span ${documentAttr("pillNeutral")}>—</span></div>
  </div>`);
  }

  const hasForecast = liquidity && liquidity.ok && liquidity.periods.length > 0;
  if (hasForecast) {
    const final = liquidity!.periods[liquidity!.periods.length - 1]!;
    const lowest = liquidity!.periods.reduce(
      (min, p) => (p.closingBalance < min ? p.closingBalance : min),
      liquidity!.periods[0]!.closingBalance,
    );
    // A projected balance dipping below zero is the one thing the owner must
    // see — surface it as a danger pill.
    const pill = lowest < 0
      ? `<span ${documentAttr("pillDanger")}>negativ likviditet</span>`
      : `<span ${documentAttr("pillSuccess")}>positiv</span>`;
    parts.push(`<div ${documentAttr("statusRow")}>
    <div>
      <div ${documentAttr("label")}>Likviditetsprognose · ${liquidity!.periods.length} måneder</div>
      <div ${documentAttr("detail")}>projiceret saldo ${escapeHtml(final.period)} <span ${documentAttr("mono")}>${escapeHtml(formatDkk(final.closingBalance))}</span>${lowest < 0 ? ` · laveste <span ${documentAttr("mono")}>${escapeHtml(formatDkk(lowest))}</span>` : ""}</div>
    </div>
    <div>${pill}</div>
  </div>`);
  } else {
    parts.push(`<div ${documentAttr("statusRow")}>
    <div>
      <div ${documentAttr("label")}>Likviditetsprognose</div>
      <div ${documentAttr("detail")}>Ingen prognosedata</div>
    </div>
    <div><span ${documentAttr("pillNeutral")}>—</span></div>
  </div>`);
  }

  return parts.join("\n");
}
