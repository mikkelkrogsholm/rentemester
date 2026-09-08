// Month-by-month income-vs-expense bar chart for the Overblik (P&L graph).
//
// Chart.js is registered once here. Colours are pulled from the cockpit
// design tokens (DESIGN.md palette) so the chart stays consistent with the
// rest of the SPA — no shadows, sober paper-near surfaces.

import {
  BarElement,
  CategoryScale,
  Chart as ChartJS,
  Legend,
  LinearScale,
  Tooltip,
  type ChartData,
  type ChartOptions,
} from "chart.js";
import { Bar } from "react-chartjs-2";
import type { OverviewMonth } from "../lib/types";
import { CHART_AXIS_NUMBER, CHART_CURRENCY, CHART_MONO_FONT, CHART_SANS_FONT } from "./chart-format";

ChartJS.register(CategoryScale, LinearScale, BarElement, Tooltip, Legend);

// DESIGN.md palette — kept in sync with app/src/styles.css tokens.
const INK_MUTED = "#4c4740";
const INCOME = "#2e5e4e"; // --color-success
const EXPENSE = "#a6332a"; // --color-accent
const BORDER = "#d8d2c6"; // --color-border

export function PnlChart({ months }: { months: OverviewMonth[] }) {
  const data: ChartData<"bar"> = {
    labels: months.map((m) => m.label),
    datasets: [
      {
        label: "Indtægter",
        data: months.map((m) => m.income),
        backgroundColor: INCOME,
        borderRadius: 2,
      },
      {
        label: "Udgifter",
        data: months.map((m) => m.expense),
        backgroundColor: EXPENSE,
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
        // Pin the axis gutter wide enough for a full 6-digit label ("18.000").
        // Chart.js auto-fits axis width by measuring labels, but that runs
        // with different local font metrics. A fixed width makes the gutter
        // deterministic.
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

  // The fixed-height wrapper gives Chart.js a stable box to fill at every
  // viewport width. With `responsive: true` + `maintainAspectRatio: false`
  // the canvas tracks this box exactly — no collapse on mobile, no
  // unbounded growth on desktop.
  return (
    <div className="pnl-chart">
      <Bar data={data} options={options} />
    </div>
  );
}
