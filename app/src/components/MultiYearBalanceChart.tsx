import type { MultiYearRow } from "../lib/types";
import { CategoricalChart } from "./CategoricalChart";

export function MultiYearBalanceChart({ years, currentYear, currency = "DKK", dataTableId }: {
  years: MultiYearRow[];
  currentYear?: string | null;
  currency?: string;
  dataTableId?: string;
}) {
  return <CategoricalChart labels={years.map((year) => year.year === currentYear ? [year.year, "(år til dato)"] : year.year)} currency={currency}
    label={`Balancesum og egenkapital pr. regnskabsår i ${currency}. Alle værdier findes i tabellen nedenfor.`}
    dataTableId={dataTableId} series={[
      { id: "assets", label: "Balancesum", values: years.map((year) => year.balancesum), tone: "info" },
      { id: "equity", label: "Egenkapital", values: years.map((year) => year.egenkapital), tone: "success" },
    ]} />;
}
