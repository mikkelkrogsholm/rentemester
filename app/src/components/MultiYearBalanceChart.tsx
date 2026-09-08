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
import { CHART_AXIS_NUMBER, CHART_CURRENCY, CHART_MONO_FONT, CHART_SANS_FONT } from "./chart-format";

// DESIGN.md palette — kept in sync with app/src/styles.css tokens.
const INK_MUTED = "#4c4740";
const ASSETS = "#2d5673"; // --color-info (sober blue)
const EQUITY = "#2e5e4e"; // --color-success
const BORDER = "#d8d2c6"; // --color-border

export function MultiYearBalanceChart({
  years,
  currentYear,
}: {
  years: MultiYearRow[];
  /** The live/current fiscal year — labelled "(år til dato)" as it is partial. */
  currentYear?: string | null;
}) {
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
          font: { family: CHART_SANS_FONT, size: 13 },
        },
      },
      tooltip: {
        callbacks: {
          label: (ctx) =>
            `${ctx.dataset.label}: ${CHART_CURRENCY.format(Number(ctx.parsed.y))}`,
        },
      },
    },
    scales: {
      x: {
        grid: { display: false },
        ticks: {
          color: INK_MUTED,
          font: { family: CHART_SANS_FONT, size: 12 },
        },
      },
      y: {
        beginAtZero: true,
        // A fixed gutter width keeps axis labels from clipping with local font
        // metrics — the same trick `PnlChart` uses.
        afterFit: (scale) => {
          scale.width = 76;
        },
        grid: { color: BORDER },
        ticks: {
          color: INK_MUTED,
          font: { family: CHART_MONO_FONT, size: 11 },
          callback: (value) => CHART_AXIS_NUMBER.format(Number(value)),
        },
      },
    },
  };

  // A fixed-height wrapper gives Chart.js a stable box to fill at every
  // viewport width — no collapse on mobile, no unbounded growth on desktop.
  return (
    <div className="pnl-chart">
      <Bar data={data} options={options} />
    </div>
  );
}
