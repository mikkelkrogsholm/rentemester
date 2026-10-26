// Shared Danish formatters for the SVG charts and their category details.

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
 * the SVG axis gutter remains easy to read.
 */
export const CHART_AXIS_NUMBER = new Intl.NumberFormat("da-DK", {
  maximumFractionDigits: 0,
});
