// Shared Chart.js number formatters (#UI-11).
//
// CashflowChart, MultiYearChart, MultiYearBalanceChart and PnlChart each had a
// byte-identical copy of these two `Intl.NumberFormat` instances. One drift
// (e.g. a stray decimal) would have made one chart's tooltips disagree with the
// others, so they live here as the single source of truth.

/** Local/system font stacks shared with the Cockpit typography tokens. */
export const CHART_SANS_FONT =
  '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif';
export const CHART_MONO_FONT = "ui-monospace, SFMono-Regular, Menlo, monospace";

/** Full currency formatting — chart tooltips ("18.000 kr."). */
export const CHART_CURRENCY = new Intl.NumberFormat("da-DK", {
  style: "currency",
  currency: "DKK",
  maximumFractionDigits: 0,
});

/**
 * Plain number with a Danish thousands separator ("18.000") — axis ticks. The
 * bare number (no " kr." suffix) keeps the tick labels narrow enough that
 * Chart.js does not clip them.
 */
export const CHART_AXIS_NUMBER = new Intl.NumberFormat("da-DK", {
  maximumFractionDigits: 0,
});
