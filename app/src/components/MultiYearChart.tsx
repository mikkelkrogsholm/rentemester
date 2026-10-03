import { designTokens } from "../../../src/design/tokens";
// Multi-year trend chart for the Flerårsoversigt (cockpit-redesign it. 4).
//
// A grouped bar chart of omsætning / udgifter / resultat across every fiscal
// year, oldest→newest. Chart.js is already registered by `PnlChart`; colours
// are pulled from the cockpit design tokens (DESIGN.md palette) so the chart
// stays consistent with the rest of the SPA.

import {
  type ChartData,
  type ChartOptions,
} from "chart.js";
import { Bar } from "react-chartjs-2";
import type { MultiYearRow } from "../lib/types";
import { useChartFonts } from "./useChartFonts";
import { CHART_AXIS_NUMBER, chartCurrency } from "./chart-format";

// DESIGN.md palette — kept in sync with app/src/styles.css tokens.
const INK_MUTED = designTokens.colors.inkMuted;
const INCOME = designTokens.colors.success; // --color-success
const EXPENSE = designTokens.colors.accent; // --color-accent
const RESULT = designTokens.colors.info; // --color-info (sober blue)
const BORDER = designTokens.colors.border; // --color-border

export function MultiYearChart({
  years,
  currentYear,
  currency = "DKK",
  dataTableId,
}: {
  years: MultiYearRow[];
  /** The live/current fiscal year — labelled "(år til dato)" as it is partial. */
  currentYear?: string | null;
  currency?: string;
  dataTableId?: string;
}) {
  const chartRef = useChartFonts<"bar">();
  const currencyFormat = chartCurrency(currency);
  const data: ChartData<"bar"> = {
    // The live year is a partial year next to the full archived ones — its
    // x-axis label says so (a two-line label), so the trend is not read as
    // like-for-like.
    labels: years.map((y) =>
      y.year === currentYear ? [y.year, "(år til dato)"] : y.year,
    ),
    datasets: [
      {
        label: "Omsætning",
        data: years.map((y) => y.omsaetning),
        backgroundColor: INCOME,
        borderRadius: 2,
      },
      {
        label: "Udgifter",
        data: years.map((y) => y.udgifter),
        backgroundColor: EXPENSE,
        borderRadius: 2,
      },
      {
        label: "Resultat",
        data: years.map((y) => y.resultat),
        backgroundColor: RESULT,
        borderRadius: 2,
      },
    ],
  };

  const options: ChartOptions<"bar"> = {
    responsive: true,
    animation: false,
    maintainAspectRatio: false,
    interaction: { mode: "index", intersect: false },
    plugins: {
      legend: {
        position: "top",
        align: "end",
        labels: {
          color: INK_MUTED,
          boxWidth: 12,
          boxHeight: 12,
          font: { family: designTokens.typography.bodyFamily, size: Number.parseInt(designTokens.typography.sizeSm) },
        },
      },
      tooltip: {
        callbacks: {
          label: (ctx) =>
            `${ctx.dataset.label}: ${currencyFormat.format(Number(ctx.parsed.y))}`,
        },
      },
    },
    scales: {
      x: {
        grid: { display: false },
        ticks: {
          color: INK_MUTED,
          font: { family: designTokens.typography.bodyFamily, size: Number.parseInt(designTokens.typography.sizeXs) },
        },
      },
      y: {
        beginAtZero: true,
        // A fixed gutter width so the axis labels never clip before the web
        // font loads — the same trick `PnlChart` uses.
        afterFit: (scale) => {
          scale.width = 76;
        },
        grid: { color: BORDER },
        ticks: {
          color: INK_MUTED,
          font: { family: designTokens.typography.monoFamily, size: Number.parseInt(designTokens.typography.sizeXs) },
          callback: (value) => CHART_AXIS_NUMBER.format(Number(value)),
        },
      },
    },
  };

  // A fixed-height wrapper gives Chart.js a stable box to fill at every
  // viewport width — no collapse on mobile, no unbounded growth on desktop.
  return (
    <div className="pnl-chart">
      <Bar ref={chartRef} role="img" aria-label={`Omsætning, udgifter og resultat pr. regnskabsår i ${currency}. Alle værdier findes i tabellen nedenfor.`} aria-details={dataTableId} data={data} options={options} />
    </div>
  );
}
