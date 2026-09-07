import { describe, expect, test } from "bun:test";
import { screen } from "@testing-library/react";
import { CompanyTaskNavigation } from "./CompanyNav";
import { renderAt } from "../test/render";

describe("CompanyTaskNavigation (#649)", () => {
  test("shows the approved daily labels as one navigation level", () => {
    renderAt(<CompanyTaskNavigation />, { route: "/companies/acme-aps/bank", path: "*" });
    expect(screen.getByRole("navigation", { name: "Daglige opgaver" })).toBeInTheDocument();
    expect(screen.getAllByRole("navigation")).toHaveLength(1);
    for (const label of ["Status", "Kræver opmærksomhed", "Penge og bilag", "Fakturaer", "Moms og frister", "Rapporter", "Viden", "Administration"])
      expect(screen.getByRole("link", { name: label })).toBeInTheDocument();
  });

  test("keeps primary destinations natural and only carries the fiscal year", () => {
    renderAt(<CompanyTaskNavigation />, { route: "/companies/acme-aps/bank?year=2025&q=netto", path: "*" });
    expect(screen.getByRole("link", { name: "Moms og frister" })).toHaveAttribute("href", "/companies/acme-aps/moms?year=2025");
    expect(screen.getByRole("link", { name: "Kræver opmærksomhed" })).toHaveAttribute("href", "/companies/acme-aps/opmaerksomhed?year=2025");
  });

  test("hides a task whose destination is not in the permission-filtered presentation", () => {
    renderAt(<CompanyTaskNavigation visibleRouteIds={["bank"]} />, { route: "/companies/acme-aps/bank", path: "*" });
    expect(screen.getByRole("link", { name: "Penge og bilag" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Rapporter" })).not.toBeInTheDocument();
  });
});
