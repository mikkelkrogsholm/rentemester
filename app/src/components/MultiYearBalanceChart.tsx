import { designTokens } from "../../../src/design/tokens";
// Multi-year balance-sheet trend chart for the Flerårsoversigt (Runde 3, it. 11).
//
// A grouped bar chart of balancesum (total assets) and egenkapital (equity)
// across every fiscal year, oldest→newest — the balance-sheet companion to
// `MultiYearChart`'s P&L bars. Chart.js is already registered by `PnlChart`;
// colours are pulled from the cockpit design tokens (DESIGN.md palette) so the
// chart stays consistent with the rest of the SPA.

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
const ASSETS = designTokens.colors.info; // --color-info (sober blue)
const EQUITY = designTokens.colors.success; // --color-success
const BORDER = designTokens.colors.border; // --color-border

export function MultiYearBalanceChart({
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
    labels: years.map((y) =>
      y.year === currentYear ? [y.year, "(år til dato)"] : y.year,
    ),
    datasets: [
      {
        label: "Balancesum",
        data: years.map((y) => y.balancesum),
        backgroundColor: ASSETS,
        borderRadius: 2,
      },
      {
        label: "Egenkapital",
        data: years.map((y) => y.egenkapital),
        backgroundColor: EQUITY,
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
      <Bar ref={chartRef} role="img" aria-label={`Balancesum og egenkapital pr. regnskabsår i ${currency}. Alle værdier findes i tabellen nedenfor.`} aria-details={dataTableId} data={data} options={options} />
    </div>
  );
}
