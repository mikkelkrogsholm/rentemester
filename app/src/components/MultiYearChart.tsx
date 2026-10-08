import type { MultiYearRow } from "../lib/types";
import { CategoricalChart } from "./CategoricalChart";

export function MultiYearChart({ years, currentYear, currency = "DKK", dataTableId }: {
  years: MultiYearRow[];
  /** The live fiscal year is partial, alongside full archived years. */
  currentYear?: string | null;
  currency?: string;
  dataTableId?: string;
}) {
  return <CategoricalChart labels={years.map((year) => year.year === currentYear ? [year.year, "(år til dato)"] : year.year)} currency={currency}
    label={`Omsætning, udgifter og resultat pr. regnskabsår i ${currency}. Alle værdier findes i tabellen nedenfor.`}
    dataTableId={dataTableId} series={[
      { id: "income", label: "Omsætning", values: years.map((year) => year.omsaetning), tone: "success" },
      { id: "expense", label: "Udgifter", values: years.map((year) => year.udgifter), tone: "accent" },
      { id: "result", label: "Resultat", values: years.map((year) => year.resultat), tone: "info" },
    ]} />;
}
