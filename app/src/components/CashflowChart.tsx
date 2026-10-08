import type { CashflowMonth } from "../lib/types";
import { CategoricalChart } from "./CategoricalChart";

/** Bars are cash movements; the right axis shows only proven bank balances. */
export function CashflowChart({ months, balanceByMonth, currency = "DKK", dataTableId }: {
  months: CashflowMonth[];
  balanceByMonth: Array<number | null>;
  currency?: string;
  dataTableId?: string;
}) {
  return <CategoricalChart labels={months.map((month) => month.label)} currency={currency}
    label={`Månedlige indbetalinger, udbetalinger og banksaldo i ${currency}. Alle værdier findes i tabellen nedenfor.`}
    dataTableId={dataTableId} series={[
      { id: "income", label: "Indbetalinger", values: months.map((month) => month.indbetalinger), tone: "success" },
      { id: "expense", label: "Udbetalinger", values: months.map((month) => month.udbetalinger), tone: "accent" },
      { id: "balance", label: "Banksaldo", values: balanceByMonth, tone: "info", kind: "line", axis: "right" },
    ]} />;
}
