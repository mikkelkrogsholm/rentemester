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
      ? `<span class="pill success">budget overholdt</span>`
      : `<span class="pill warning">budgetafvigelse</span>`;
    parts.push(`<div class="status-row">
    <div>
      <div class="label">Budget vs. faktisk · ${escapeHtml(budget!.periodStart)}</div>
      <div class="detail">budget <span class="mono">${escapeHtml(formatDkk(budget!.totalBudget))}</span> · faktisk <span class="mono">${escapeHtml(formatDkk(budget!.totalActual))}</span></div>
    </div>
    <div>${pill} <span class="muted">${escapeHtml(formatDkk(favourability))}</span></div>
  </div>`);
  } else {
    parts.push(`<div class="status-row">
    <div>
      <div class="label">Budget vs. faktisk</div>
      <div class="detail">Intet budget sat for perioden</div>
    </div>
    <div><span class="pill neutral">—</span></div>
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
      ? `<span class="pill danger">negativ likviditet</span>`
      : `<span class="pill success">positiv</span>`;
    parts.push(`<div class="status-row">
    <div>
      <div class="label">Likviditetsprognose · ${liquidity!.periods.length} måneder</div>
      <div class="detail">projiceret saldo ${escapeHtml(final.period)} <span class="mono">${escapeHtml(formatDkk(final.closingBalance))}</span>${lowest < 0 ? ` · laveste <span class="mono">${escapeHtml(formatDkk(lowest))}</span>` : ""}</div>
    </div>
    <div>${pill}</div>
  </div>`);
  } else {
    parts.push(`<div class="status-row">
    <div>
      <div class="label">Likviditetsprognose</div>
      <div class="detail">Ingen prognosedata</div>
    </div>
    <div><span class="pill neutral">—</span></div>
  </div>`);
  }

  return parts.join("\n");
}
