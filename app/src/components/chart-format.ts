import { designTokens } from "../../../src/design/tokens";
// Shared Chart.js number formatters (#UI-11).
//
// CashflowChart, MultiYearChart, MultiYearBalanceChart and PnlChart each had a
// byte-identical copy of these two `Intl.NumberFormat` instances. One drift
// (e.g. a stray decimal) would have made one chart's tooltips disagree with the
// others, so they live here as the single source of truth.

/** Bundled font stacks shared with generated Cockpit typography. */
export const CHART_SANS_FONT = designTokens.typography.bodyFamily;
export const CHART_MONO_FONT = designTokens.typography.monoFamily;
/** Compatibility formatter for callers whose explicit report unit is DKK. */
export const CHART_CURRENCY = chartCurrency("DKK");

/** Currency formatting for chart tooltips, in the report's explicit unit. */
export function chartCurrency(currency: string) {
  return new Intl.NumberFormat("da-DK", {
    style: "currency",
    currency,
    maximumFractionDigits: 0,
  });
}

/**
 * Plain number with a Danish thousands separator ("18.000") — axis ticks. The
 * bare number (no " kr." suffix) keeps the tick labels narrow enough that
 * Chart.js does not clip them.
 */
export const CHART_AXIS_NUMBER = new Intl.NumberFormat("da-DK", {
  maximumFractionDigits: 0,
});
