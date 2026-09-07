import { describe, expect, test } from "bun:test";
import { COMPANY_ROUTE_REGISTRY } from "./company-route-registry";
import { COCKPIT_PAGE_FAMILIES, INVENTORIED_COMPANY_ROUTE_IDS } from "./company-page-inventory";

describe("Cockpit page-template inventory", () => {
  test("classifies every registry route exactly once by a reusable page family", () => {
    expect(new Set(INVENTORIED_COMPANY_ROUTE_IDS).size).toBe(INVENTORIED_COMPANY_ROUTE_IDS.length);
    expect([...INVENTORIED_COMPANY_ROUTE_IDS].sort()).toEqual(COMPANY_ROUTE_REGISTRY.map((route) => route.id).sort());
    expect(COCKPIT_PAGE_FAMILIES.every((family) => family.reason && family.routeIds.length)).toBe(true);
  });
});
