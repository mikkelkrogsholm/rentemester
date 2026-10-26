// #UI-4 — the sub-navigation must thread ONLY the fiscal year between views, so
// per-view filters (Bank's q/from/to/status, a posting account=…) never leak
// onto sibling tabs.
import { describe, expect, test, vi } from "bun:test";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { CompanyTaskNavigation, YearSelector } from "./CompanyNav";
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
        renderAt(<CompanyTaskNavigation visibleRouteIds={["bank"]}/>, { route: "/companies/acme-aps/bank", path: "*" });
        expect(screen.getByRole("link", { name: "Penge og bilag" })).toBeInTheDocument();
        expect(screen.queryByRole("link", { name: "Rapporter" })).not.toBeInTheDocument();
    });
});
test("unsupported archive years stay visible but disabled while legacy years remain selectable", async () => {
    const changed = vi.fn();
    renderAt(<YearSelector years={[
            { label: "2024", source: "live", start: "2024-01-01", end: "2024-12-31" },
            { label: "2024", source: "archive", start: null, end: null, selectable: false },
            { label: "2023", source: "archive", start: null, end: null },
        ]} selected="2024" onChange={changed}/>);
    const select = screen.getByRole("combobox", { name: "Vælg regnskabsår" });
    const unsupported = screen.getByRole("option", { name: "2024 (se Arkiv)" });
    expect(unsupported).toBeDisabled();
    expect(screen.getByRole("option", { name: "2024 (aktiv)" })).not.toBeDisabled();
    expect(screen.getByRole("option", { name: "2023 (arkiv)" })).not.toBeDisabled();
    await userEvent.selectOptions(select, unsupported);
    expect(changed).not.toHaveBeenCalled();
    await userEvent.selectOptions(select, "archive:2023");
    expect(changed).toHaveBeenCalledWith("archive:2023");
});
test("year source is retained between company tasks while per-view filters are excluded", () => {
    renderAt(<CompanyTaskNavigation />, { route: "/companies/acme-aps/bank?year=2024&source=archive&account=9999&q=source", path: "*" });
    expect(screen.getByRole("link", { name: "Penge og bilag" })).toHaveAttribute("href", "/companies/acme-aps/bank?year=2024&source=archive");
    expect(screen.getByRole("link", { name: "Rapporter" })).toHaveAttribute("href", "/companies/acme-aps/resultatopgorelse?year=2024&source=archive");
});
