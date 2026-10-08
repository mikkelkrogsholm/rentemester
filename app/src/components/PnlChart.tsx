import { useId } from "react";
import * as stylex from "@stylexjs/stylex";
import { Amount, DataTable } from "./ui";
import type { OverviewMonth } from "../lib/types";
import { CategoricalChart, chartStyles } from "./CategoricalChart";

export function PnlChart({ months, currency = "DKK" }: { months: OverviewMonth[]; currency?: string }) {
  const dataTableId = useId();
  return <>
    <CategoricalChart labels={months.map((month) => month.label)} currency={currency}
      label={`Månedlige indtægter og udgifter i ${currency}. Alle værdier findes i tabellen nedenfor.`}
      dataTableId={dataTableId} series={[
        { id: "income", label: "Indtægter", values: months.map((month) => month.income), tone: "success" },
        { id: "expense", label: "Udgifter", values: months.map((month) => month.expense), tone: "accent" },
      ]} />
    <details {...stylex.props(chartStyles.chartData)} id={dataTableId}>
      <summary {...stylex.props(chartStyles.summary)}>Se indtægter og udgifter som tabel</summary>
      <DataTable caption={`Månedlige indtægter og udgifter (${currency})`} rows={months} rowKey={(month) => month.month} columns={[
        { id: "month", label: "Måned", render: (month) => month.label },
        { id: "income", label: "Indtægter", render: (month) => <Amount value={month.income} currency={currency} /> },
        { id: "expense", label: "Udgifter", render: (month) => <Amount value={month.expense} currency={currency} /> },
      ]} />
    </details>
  </>;
}
