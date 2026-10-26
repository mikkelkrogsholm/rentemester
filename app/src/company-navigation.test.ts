import { describe, expect, test } from "bun:test";
import {
  COMPANY_ROUTE_REGISTRY,
  COMPANY_TASK_AREAS,
  assertCompanyRouteRegistry,
  companyRouteForPath,
} from "./company-route-registry";
import { companyRouteForPath as matchCompanyRoutePath } from "./company-route-path";

describe("company route registry", () => {
  test("owns a unique, renderable route in all approved daily task areas", () => {
    expect(COMPANY_TASK_AREAS.map((area) => area.label)).toEqual(["Status", "Kræver opmærksomhed", "Penge og bilag", "Fakturaer", "Moms og frister", "Rapporter", "Viden", "Administration"]);
    expect(new Set(COMPANY_ROUTE_REGISTRY.map((route) => route.id)).size)
      .toBe(COMPANY_ROUTE_REGISTRY.length);
    expect(new Set(COMPANY_ROUTE_REGISTRY.map((route) => route.segment)).size)
      .toBe(COMPANY_ROUTE_REGISTRY.length);
    expect(COMPANY_ROUTE_REGISTRY.every((route) => route.element)).toBe(true);
    expect(new Set(COMPANY_ROUTE_REGISTRY.map((route) => route.area))).toEqual(
      new Set(COMPANY_TASK_AREAS.map((area) => area.id)),
    );
    expect(() => assertCompanyRouteRegistry()).not.toThrow();
    const administration = COMPANY_ROUTE_REGISTRY.filter((route) => route.area === "administration");
    expect(administration.every((route) => route.administrationGroup && route.administrationPurpose && route.administrationNextStep)).toBe(true);
  });

  test("resolves deep links without considering their query string", () => {
    expect(companyRouteForPath("/companies/acme-aps/bank")?.id).toBe("bank");
    expect(companyRouteForPath("/companies/acme-aps")?.id).toBe("dashboard");
    expect(matchCompanyRoutePath("/companies/acme-aps/bank", COMPANY_ROUTE_REGISTRY)?.id).toBe("bank");
  });

  test("does not treat the company creation route as a company dashboard", () => {
    expect(companyRouteForPath("/companies/new")).toBeUndefined();
  });

  test("derives routing and navigation from the same route registry without a runtime cycle", async () => {
    const appSource = await Bun.file(new URL("./App.tsx", import.meta.url)).text();
    const navigationSource = await Bun.file(new URL("./components/CompanyNav.tsx", import.meta.url)).text();
    expect(appSource).not.toMatch(/path\s*=\s*["'`]\/companies\/:slug/);
    expect(appSource).toContain("COMPANY_ROUTE_REGISTRY.map");
    expect(appSource).not.toContain("COMPANY_ROUTE_ELEMENTS");
    expect(navigationSource).toContain("navigation?.routes.filter");
    expect(navigationSource).toContain('from "../company-route-path"');
    expect(navigationSource).toContain('import type { CompanyRouteId }');
  });
});
