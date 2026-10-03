import { designTokens } from "../../../src/design/tokens";
// Liquidity chart for the Likviditet view (cockpit-redesign it. 8).
//
// A combined chart: monthly indbetalinger / udbetalinger as bars (left axis),
// and the real bank-balance trajectory as a line (right axis). Bar elements
// and scales are already registered by `PnlChart`; the line element and point
// element are registered here. Colours are pulled from the cockpit design
// tokens (DESIGN.md palette) so the chart stays consistent with the SPA.

import {
  BarController,
  BarElement,
  CategoryScale,
  Chart as ChartJS,
  Legend,
  LinearScale,
  LineController,
  LineElement,
  PointElement,
  Tooltip,
  type ChartData,
  type ChartOptions,
} from "chart.js";
import { Chart } from "react-chartjs-2";
import type { CashflowMonth } from "../lib/types";
import { useChartFonts } from "./useChartFonts";
import { CHART_AXIS_NUMBER, chartCurrency } from "./chart-format";

// The generic <Chart> component does not auto-register controllers, so this
// mixed bar+line chart registers everything it needs itself — self-contained,
// not relying on another component's registration side effects.
ChartJS.register(
  BarController,
  LineController,
  BarElement,
  LineElement,
  PointElement,
  CategoryScale,
  LinearScale,
  Tooltip,
  Legend,
);

// DESIGN.md palette — kept in sync with app/src/styles.css tokens.
const INK_MUTED = designTokens.colors.inkMuted;
const INCOME = designTokens.colors.success; // --color-success
const EXPENSE = designTokens.colors.accent; // --color-accent
const BALANCE = designTokens.colors.info; // --color-info (sober blue)
const BORDER = designTokens.colors.border; // --color-border

/**
 * `months` drives the ind/ud bars; `balanceByMonth` is the bank balance at the
 * end of each calendar month (null where no statement point falls in or before
 * that month) — drawn as the trajectory line.
 */
export function CashflowChart({
  months,
  balanceByMonth,
  currency = "DKK",
  dataTableId,
}: {
  months: CashflowMonth[];
  balanceByMonth: Array<number | null>;
  currency?: string;
  dataTableId?: string;
}) {
  const chartRef = useChartFonts<"bar" | "line">();
  const currencyFormat = chartCurrency(currency);
  const data: ChartData<"bar" | "line"> = {
    labels: months.map((m) => m.label),
    datasets: [
      {
        type: "bar" as const,
        label: "Indbetalinger",
        data: months.map((m) => m.indbetalinger),
        backgroundColor: INCOME,
        borderRadius: 2,
        yAxisID: "y",
        order: 2,
      },
      {
        type: "bar" as const,
        label: "Udbetalinger",
        data: months.map((m) => m.udbetalinger),
        backgroundColor: EXPENSE,
        borderRadius: 2,
        yAxisID: "y",
        order: 2,
      },
      {
        type: "line" as const,
        label: "Banksaldo",
        data: balanceByMonth,
        borderColor: BALANCE,
        backgroundColor: BALANCE,
        borderWidth: 2,
        pointRadius: 3,
        pointHoverRadius: 4,
        tension: 0.2,
        spanGaps: true,
        yAxisID: "yBalance",
        order: 1,
      },
    ],
  };

  const options: ChartOptions<"bar" | "line"> = {
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
          label: (ctx) => {
            const value = ctx.parsed.y;
            if (value === null || value === undefined) return "";
            return `${ctx.dataset.label}: ${currencyFormat.format(Number(value))}`;
          },
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
        position: "left",
        // A fixed gutter so the axis labels never clip before the web font
        // loads — the same trick `PnlChart` uses.
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
      yBalance: {
        position: "right",
        afterFit: (scale) => {
          scale.width = 76;
        },
        grid: { display: false },
        ticks: {
          color: BALANCE,
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
      <Chart ref={chartRef} type="bar" role="img" aria-label={`Månedlige indbetalinger, udbetalinger og banksaldo i ${currency}. Alle værdier findes i tabellen nedenfor.`} aria-details={dataTableId} data={data} options={options} />
    </div>
  );
}
