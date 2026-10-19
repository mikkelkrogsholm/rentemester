// #UI-4 — the sub-navigation must thread ONLY the fiscal year between views, so
// per-view filters (Bank's q/from/to/status, a posting account=…) never leak
// onto sibling tabs.

import { describe, expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useLocation } from "react-router-dom";
import { CompanyTaskNavigation, YearSelector } from "./CompanyNav";
import { renderAt } from "../test/render";

function renderNav(route: string) {
  return renderAt(
    <CompanyTaskNavigation />,
    { route, path: "*" },
  );
}

function LocationProbe() {
  const location = useLocation();
  return <output data-testid="location">{location.pathname}{location.search}</output>;
}

describe("CompanyNav query-string whitelist (#UI-4)", () => {
  test("threads ?year= onto the current area's destinations", () => {
    renderNav("/companies/acme-aps/bank?year=2025");
    const bookkeeping = screen.getByRole("button", { name: "Bogføring" });
    expect(bookkeeping).toHaveAttribute("aria-pressed", "true");
    const documents = screen.getByRole("link", { name: "Bilag" });
    expect(documents.getAttribute("href")).toBe("/companies/acme-aps/bilag?year=2025");
    expect(screen.getByRole("link", { name: "Bank" }).getAttribute("aria-current")).toBe("page");
    expect(screen.queryByRole("link", { name: "Resultatopgørelse" })).not.toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Opgaveområder" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Sider i Bogføring" })).toBeInTheDocument();
  });

  test("does NOT leak per-view filters (q/from/to/status/account) to sibling tabs", () => {
    renderNav(
      "/companies/acme-aps/bank?year=2025&q=netto&from=2025-01-01&to=2025-03-31&status=unmatched",
    );
    const postings = screen.getByRole("link", { name: "Posteringer" });
    const href = postings.getAttribute("href") ?? "";
    expect(href).toBe("/companies/acme-aps/posteringer?year=2025");
    expect(href).not.toMatch(/q=/);
    expect(href).not.toMatch(/from=/);
    expect(href).not.toMatch(/status=/);
  });

  test("omits the query string entirely when no year is set", () => {
    renderNav("/companies/acme-aps/bank?q=netto");
    const postings = screen.getByRole("link", { name: "Posteringer" });
    expect(postings.getAttribute("href")).toBe("/companies/acme-aps/posteringer");
  });

  test("hides empty permission-filtered task areas and preserves long labels", () => {
    renderAt(
      <CompanyTaskNavigation visibleRouteIds={["bank", "payables"]} />,
      { route: "/companies/acme-aps/bank", path: "*" },
    );
    expect(screen.getByRole("link", { name: "Leverandørfaktura" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Rapporter og planlægning" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Salg og debitorer" })).not.toBeInTheDocument();
  });

  test("uses wrap-safe, visible-focus navigation CSS", async () => {
    const css = await Bun.file(new URL("../styles.css", import.meta.url)).text();
    expect(css).toContain(".company-areas,\n.company-destinations");
    expect(css).toContain("overflow-wrap: anywhere");
    expect(css).toContain(".company-areas button:focus-visible");
  });

  test("renders on a deep-linked company page that has no fiscal-year control", () => {
    renderNav("/companies/acme-aps/manage");
    expect(screen.getByRole("button", { name: "Virksomhedsadministration" }))
      .toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("link", { name: "Virksomhedsoplysninger" }))
      .toHaveAttribute("aria-current", "page");
  });

  test("does not render company navigation on the create-company route", () => {
    renderNav("/companies/new");
    expect(screen.queryByRole("navigation", { name: "Opgaveområder" }))
      .not.toBeInTheDocument();
  });

  test("reveals an area without navigating or adding an intermediate history entry", async () => {
    renderAt(
      <><CompanyTaskNavigation /><LocationProbe /></>,
      { route: "/companies/acme-aps/bank?year=2025", path: "*" },
    );
    await userEvent.click(screen.getByRole("button", { name: "Moms og perioder" }));
    expect(screen.getByRole("link", { name: "Moms" })).toHaveAttribute(
      "href",
      "/companies/acme-aps/moms?year=2025",
    );
    expect(screen.getByTestId("location")).toHaveTextContent(
      "/companies/acme-aps/bank?year=2025",
    );
  });
});


test("unsupported archive years stay visible but disabled while legacy years remain selectable", async () => {
  const changed=vi.fn();
  renderAt(<YearSelector years={[
    {label:"2024",source:"live",start:"2024-01-01",end:"2024-12-31"},
    {label:"2024",source:"archive",start:null,end:null,selectable:false},
    {label:"2023",source:"archive",start:null,end:null},
  ]} selected="2024" onChange={changed}/>);
  const select=screen.getByRole("combobox",{name:"Vælg regnskabsår"});
  const unsupported=screen.getByRole("option",{name:"2024 (se Arkiv)"});
  expect(unsupported).toBeDisabled();
  expect(screen.getByRole("option",{name:"2024 (aktiv)"})).not.toBeDisabled();
  expect(screen.getByRole("option",{name:"2023 (arkiv)"})).not.toBeDisabled();
  await userEvent.selectOptions(select,unsupported);
  expect(changed).not.toHaveBeenCalled();
  await userEvent.selectOptions(select,"archive:2023");
  expect(changed).toHaveBeenCalledWith("archive:2023");
});


test("year source is retained between company tasks while per-view filters are excluded", () => {
 renderNav("/companies/acme-aps/bank?year=2024&source=archive&account=9999&q=source");
 expect(screen.getByRole("link",{name:"Bilag"})).toHaveAttribute("href","/companies/acme-aps/bilag?year=2024&source=archive");
 expect(screen.getByRole("link",{name:"Posteringer"})).toHaveAttribute("href","/companies/acme-aps/posteringer?year=2024&source=archive");
});
