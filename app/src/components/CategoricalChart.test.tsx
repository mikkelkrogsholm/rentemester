import { describe, expect, test } from "bun:test";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { CategoricalChart } from "./CategoricalChart";
import { CashflowChart } from "./CashflowChart";
import { PnlChart } from "./PnlChart";
import { MultiYearChart } from "./MultiYearChart";
import { MultiYearBalanceChart } from "./MultiYearBalanceChart";
import type { MultiYearRow } from "../lib/types";

const months = [
  { month: 1, label: "jan", income: 1234, expense: 200, indbetalinger: 1234, udbetalinger: 200, netto: 1034 },
  { month: 2, label: "feb", income: 0, expense: -150, indbetalinger: 0, udbetalinger: 150, netto: -150 },
  { month: 3, label: "mar", income: 500, expense: 0, indbetalinger: 500, udbetalinger: 0, netto: 500 },
];
const years: MultiYearRow[] = [
  { year: "2025/26", source: "archive", omsaetning: 100, udgifter: 120, resultat: -20, balancesum: 200, egenkapital: -30, bruttomargin: -0.2, egenkapitalandel: -0.15 },
  { year: "2026/27", source: "live", omsaetning: 0, udgifter: 0, resultat: 0, balancesum: 0, egenkapital: 0, bruttomargin: null, egenkapitalandel: null },
];

function expectFiniteGeometry(container: HTMLElement) {
  for (const element of container.querySelectorAll("rect,circle,line,path")) {
    for (const attribute of element.attributes) {
      if (["x", "y", "x1", "x2", "y1", "y2", "cx", "cy", "height", "width", "d"].includes(attribute.name)) {
        expect(attribute.value).not.toMatch(/NaN|Infinity/);
      }
    }
  }
}

describe("StyleX SVG financial charts", () => {
  test("series toggles hide a series without changing its accessible source table", async () => {
    const { container } = render(<PnlChart months={months} currency="EUR" />);
    const graph = screen.getByRole("img");
    const details = document.getElementById(graph.getAttribute("aria-details")!)!;
    expect(details).toBeInTheDocument();
    const table = within(details).getByRole("table", { hidden: true });
    expect(table.textContent).toContain("EUR");
    expect(table.textContent).toContain("jan");
    const income = screen.getByRole("button", { name: "Indtægter" });
    expect(income).toHaveAttribute("aria-pressed", "true");
    expect(container.querySelector('[data-series="income"]')).toBeInTheDocument();
    await userEvent.click(income);
    expect(income).toHaveAttribute("aria-pressed", "false");
    expect(container.querySelector('[data-series="income"]')).toBeNull();
    expect(container.querySelector('[data-series="expense"]')).toBeInTheDocument();
    expect(within(details).getByRole("table", { hidden: true }).textContent).toBe(table.textContent);
    await userEvent.click(income);
    expect(container.querySelector('[data-series="income"]')).toBeInTheDocument();
    expectFiniteGeometry(container);
  });

  test("keyboard navigation announces the categorical period and the report currency", () => {
    render(<PnlChart months={months} currency="EUR" />);
    const graph = screen.getByRole("img");
    fireEvent.focus(graph);
    const values = document.getElementById(graph.getAttribute("aria-describedby")!.split(" ")[1]!)!;
    expect(values.textContent).toMatch(/jan.*Indtægter: 1\.234.*€/);
    fireEvent.keyDown(graph, { key: "ArrowRight" });
    expect(values.textContent).toMatch(/feb.*Indtægter: 0.*Udgifter: -150/);
    fireEvent.keyDown(graph, { key: "End" });
    expect(values.textContent).toMatch(/mar.*500/);
    fireEvent.keyDown(graph, { key: "ArrowRight" });
    expect(values.textContent).toMatch(/^mar/);
    fireEvent.keyDown(graph, { key: "Home" });
    expect(values.textContent).toMatch(/^jan/);
  });

  test("touch and hover select the category under the pointer", () => {
    render(<PnlChart months={months} />);
    const graph = screen.getByRole("img");
    graph.getBoundingClientRect = () => ({ left: 0, top: 0, width: 640, height: 320, right: 640, bottom: 320, x: 0, y: 0, toJSON() {} });
    const values = document.getElementById(graph.getAttribute("aria-describedby")!.split(" ")[1]!)!;
    fireEvent.pointerMove(graph, { clientX: 100, pointerType: "mouse" });
    expect(values.textContent).toMatch(/^jan/);
    fireEvent.pointerLeave(graph);
    expect(values.textContent).toMatch(/^Peg eller tryk/);
    fireEvent.pointerDown(graph, { clientX: 620, pointerType: "touch" });
    fireEvent.pointerLeave(graph);
    expect(values.textContent).toMatch(/^mar/);
  });

  test("unknown balances have no points, remain unknown on focus, and connect known months", () => {
    const { container } = render(<CashflowChart months={months} balanceByMonth={[90000, null, -12000]} dataTableId="cashflow-table" />);
    const graph = screen.getByRole("img");
    expect(graph).toHaveAttribute("aria-details", "cashflow-table");
    const balance = container.querySelector('[data-series="balance"]')!;
    expect(balance.querySelectorAll("circle")).toHaveLength(2);
    expect(balance.querySelector('[data-category="1"]')).toBeNull();
    expect(balance.querySelector("path")!.getAttribute("d")).toContain("C");
    fireEvent.keyDown(graph, { key: "Home" });
    fireEvent.keyDown(graph, { key: "ArrowRight" });
    const values = document.getElementById(graph.getAttribute("aria-describedby")!.split(" ")[1]!)!;
    expect(values.textContent).toMatch(/feb.*Banksaldo: Ukendt/);
    expect(values.textContent).not.toMatch(/Banksaldo: 0/);
    // Balance and movements retain independent scales.
    const leftLabels = [...graph.querySelectorAll("g text")].map((node) => node.textContent);
    expect(leftLabels).not.toContain("100.000");
    expect(graph.textContent).toContain("100.000");
    expectFiniteGeometry(container);
  });

  test("negative results, zero data, and partial fiscal years keep their category labels", () => {
    const { container } = render(<><MultiYearChart years={years} currentYear="2026/27" /><MultiYearBalanceChart years={years} currentYear="2026/27" /></>);
    expect(screen.getAllByRole("img")).toHaveLength(2);
    expect(screen.getAllByText("(år til dato)")).toHaveLength(2);
    const result = container.querySelector('[data-series="result"] [data-value="-20"]')!;
    const equity = container.querySelector('[data-series="equity"] [data-value="-30"]')!;
    expect(Number(result.getAttribute("height"))).toBeGreaterThan(0);
    expect(Number(equity.getAttribute("height"))).toBeGreaterThan(0);
    expectFiniteGeometry(container);
  });

  test("empty, all-zero and all-unknown data have finite geometry and honest values", () => {
    const { container, rerender } = render(<CategoricalChart labels={[]} series={[]} label="Tom graf" />);
    expect(screen.getByText("Ingen data at vise endnu.")).toBeInTheDocument();
    rerender(<CategoricalChart labels={["jan", "feb"]} series={[{ id: "zero", label: "Resultat", values: [0, 0], tone: "info" }]} label="Nul graf" />);
    expectFiniteGeometry(container);
    rerender(<CategoricalChart labels={["jan", "feb"]} series={[{ id: "balance", label: "Banksaldo", values: [null, null], tone: "info", kind: "line", axis: "right" }]} label="Ukendt graf" />);
    expect(container.querySelectorAll("circle,path")).toHaveLength(0);
    const graph = screen.getByRole("img");
    fireEvent.focus(graph);
    expect(document.getElementById(graph.getAttribute("aria-describedby")!.split(" ")[1]!)!.textContent).toMatch(/Banksaldo: Ukendt/);
    expectFiniteGeometry(container);
  });

  test("hiding every series gives a visible recovery instruction", async () => {
    render(<CategoricalChart labels={["jan"]} series={[{ id: "income", label: "Omsætning", values: [200], tone: "success", kind: "line" }]} label="Omsætning" height="line" />);
    await userEvent.click(screen.getByRole("button", { name: "Omsætning" }));
    expect(screen.getByText("Vælg en serie for at vise grafen.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Omsætning" }));
    expect(screen.queryByText("Vælg en serie for at vise grafen.")).toBeNull();
  });
});
