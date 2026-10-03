import { designTokens } from "../../../src/design/tokens";
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
import { Amount, DataTable } from "./ui";
import { Bar } from "react-chartjs-2";
import type { OverviewMonth } from "../lib/types";
import { useChartFonts } from "./useChartFonts";
import { CHART_AXIS_NUMBER, chartCurrency } from "./chart-format";

ChartJS.register(CategoryScale, LinearScale, BarElement, Tooltip, Legend);

// DESIGN.md palette — kept in sync with app/src/styles.css tokens.
const INK_MUTED = designTokens.colors.inkMuted;
const INCOME = designTokens.colors.success; // --color-success
const EXPENSE = designTokens.colors.accent; // --color-accent
const BORDER = designTokens.colors.border; // --color-border

export function PnlChart({ months, currency = "DKK" }: { months: OverviewMonth[]; currency?: string }) {
  const chartRef = useChartFonts<"bar">();
  const currencyFormat = chartCurrency(currency);
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
        // Pin the axis gutter wide enough for a full 6-digit label ("18.000").
        // Chart.js auto-fits axis width by measuring labels, but that runs
        // before the web font loads — it under-reserves and clips the leading
        // digit. A fixed width makes the gutter deterministic.
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

  // The fixed-height wrapper gives Chart.js a stable box to fill at every
  // viewport width. With `responsive: true` + `maintainAspectRatio: false`
  // the canvas tracks this box exactly — no collapse on mobile, no
  // unbounded growth on desktop.
  return (
    <>
      <div className="pnl-chart">
        <Bar ref={chartRef} role="img" aria-label={`Månedlige indtægter og udgifter i ${currency}. Alle værdier findes i tabellen nedenfor.`} data={data} options={options} />
      </div>
      <details className="chart-data">
        <summary>Se indtægter og udgifter som tabel</summary>
        <DataTable caption={`Månedlige indtægter og udgifter (${currency})`} rows={months} rowKey={(month) => month.month} columns={[
          { id: "month", label: "Måned", render: (month) => month.label },
          { id: "income", label: "Indtægter", render: (month) => <Amount value={month.income} currency={currency} /> },
          { id: "expense", label: "Udgifter", render: (month) => <Amount value={month.expense} currency={currency} /> },
        ]} />
      </details>
    </>
  );
}
