import { COMPANY_ROUTE_REGISTRY, COMPANY_TASK_AREAS } from "../company-route-registry";
import { expect, test, vi } from "bun:test";
import { act, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useLocation } from "react-router-dom";
import { CockpitLayout } from "./AppChrome";
import { CompanyNav, CompanyNavigationShell } from "./CompanyNav";
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
  ["Status", ""], ["Posteringer", "/posteringer"],
  ["Saldobalance", "/saldobalance"], ["Resultatopgørelse", "/resultatopgorelse"], ["Balance", "/balance"],
];

test("actual shell retains archive source for all five reports and clicked colliding year", async () => {
  await mount("year=2024&source=archive&account=4863&q=private&from=2024-01-01");
  const nav = screen.getByRole("navigation", { name: "Virksomhedsnavigation", hidden: true });
  for (const [label, segment] of reports) {
    expect(within(nav).getByRole("link", { name: label, hidden: true }))
      .toHaveAttribute("href", `/companies/synthetic${segment}?year=2024&source=archive`);
  }
  await userEvent.click(within(nav).getByText("Sider i Rapporter", { exact: true }));
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

test("native cashflow exposes the registered shifted fiscal selector in the actual shell", async () => {
  vi.spyOn(api, "companies").mockResolvedValue([{slug:"synthetic",name:"Synthetic shifted",archived:false,createdAt:"2025-01-01T00:00:00Z"}]);
  const years = [
    {label:"2025/26",start:"2025-07-01",end:"2026-06-30",source:"live" as const,selectable:true},
    {label:"2026/27",start:"2026-07-01",end:"2027-06-30",source:"live" as const,selectable:true},
  ];
  renderAt(<AuthProvider hosted={false} backend="rust"><CompanyNavigationShell navigation={{routes: COMPANY_ROUTE_REGISTRY, areas: COMPANY_TASK_AREAS}} rendersNavigation><CockpitLayout>
    <CompanyNav slug="synthetic" years={years} selectedYear="2025/26" onYearChange={()=>{}}/><Location/>
  </CockpitLayout></CompanyNavigationShell></AuthProvider>,{route:"/companies/synthetic/likviditet?year=2025%2F26&source=live",path:"*"});
  const selector=await screen.findByLabelText("Vælg regnskabsår");
  expect(screen.getAllByLabelText("Vælg regnskabsår")).toHaveLength(1);
  expect(selector).toHaveValue("live:2025/26");
  await userEvent.selectOptions(selector,"live:2026/27");
  expect(screen.getByTestId("location")).toHaveTextContent("year=2026%2F27&source=live");
});
