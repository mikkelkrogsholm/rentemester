import { expect, test, vi } from "bun:test";
import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useLocation } from "react-router-dom";
import { CockpitLayout } from "./AppChrome";
import { AuthProvider } from "../lib/auth-context";
import { api } from "../lib/api";
import { renderAt } from "../test/render";

function Location() {
  const location = useLocation();
  return <output data-testid="location">{location.pathname}{location.search}</output>;
}
async function mount(query: string) {
  vi.spyOn(api, "companies").mockResolvedValue([]);
  const result = renderAt(<AuthProvider hosted={false}><CockpitLayout><Location /></CockpitLayout></AuthProvider>, {
    route: `/companies/synthetic/posteringer?${query}`, path: "*",
  });
  await act(async () => {});
  return result;
}
const reports = [
  ["Overblik", ""], ["Posteringer", "/posteringer"],
  ["Saldobalance", "/saldobalance"], ["Resultatopgørelse", "/resultatopgorelse"], ["Balance", "/balance"],
];

test("actual shell retains archive source for all five reports and clicked colliding year", async () => {
  await mount("year=2024&source=archive&account=4863&q=private&from=2024-01-01");
  const nav = screen.getByRole("navigation", { name: "Virksomhedsnavigation", hidden: true });
  for (const [label, segment] of reports) {
    expect(within(nav).getByRole("link", { name: label, hidden: true }))
      .toHaveAttribute("href", `/companies/synthetic${segment}?year=2024&source=archive`);
  }
  await userEvent.click(within(nav).getByText("Rapporter og planlægning", { exact: true }));
  await userEvent.click(within(nav).getByRole("link", { name: "Balance" }));
  expect(screen.getByTestId("location")).toHaveTextContent("/companies/synthetic/balance?year=2024&source=archive");
});

test("shell keeps explicit live source but excludes unsupported source and view filters", async () => {
  const first = await mount("year=2024&source=live&account=4863");
  expect(screen.getByRole("link", { name: "Posteringer", hidden: true })).toHaveAttribute("href", "/companies/synthetic/posteringer?year=2024&source=live");
  first.unmount();
  await mount("year=2024&source=invalid&q=private");
  expect(screen.getByRole("link", { name: "Posteringer", hidden: true })).toHaveAttribute("href", "/companies/synthetic/posteringer?year=2024");
});
