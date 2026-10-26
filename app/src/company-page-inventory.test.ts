import { describe, expect, test } from "bun:test";
import { COMPANY_ROUTE_REGISTRY } from "./company-route-registry";
import { COCKPIT_DETAIL_ROUTE_EVIDENCE, COCKPIT_PAGE_FAMILIES, COCKPIT_ROUTE_ACCEPTANCE_MATRIX, INVENTORIED_COMPANY_ROUTE_IDS } from "./company-page-inventory";

describe("Cockpit page-template inventory", () => {
  test("classifies every registry route exactly once by a reusable page family", () => {
    expect(new Set(INVENTORIED_COMPANY_ROUTE_IDS).size).toBe(INVENTORIED_COMPANY_ROUTE_IDS.length);
    expect([...INVENTORIED_COMPANY_ROUTE_IDS].sort()).toEqual(COMPANY_ROUTE_REGISTRY.map((route) => route.id).sort());
    expect(COCKPIT_PAGE_FAMILIES.every((family) => family.reason && family.routeIds.length)).toBe(true);
  });

  test("keeps party profile as explicit detail-route evidence rather than inventing a registry route", () => {
    expect(COCKPIT_DETAIL_ROUTE_EVIDENCE).toContainEqual(expect.objectContaining({ id: "party-profile", path: "/companies/:slug/parter/:partyId" }));
    expect(INVENTORIED_COMPANY_ROUTE_IDS).not.toContain("party-profile" as never);
  });

  test("gives every registry route an explicit #655 state, keyboard and viewport evidence classification", () => {
    expect(new Set(COCKPIT_ROUTE_ACCEPTANCE_MATRIX.map((entry) => entry.routeId)).size)
      .toBe(COCKPIT_ROUTE_ACCEPTANCE_MATRIX.length);
    expect(COCKPIT_ROUTE_ACCEPTANCE_MATRIX.map((entry) => entry.routeId).sort())
      .toEqual(COMPANY_ROUTE_REGISTRY.map((route) => route.id).sort());
    for (const entry of COCKPIT_ROUTE_ACCEPTANCE_MATRIX) {
      expect(entry.contracts).toEqual(expect.arrayContaining(["states", "keyboard", "viewport"]));
      expect(["synthetic", "mutation-required"]).toContain(entry.evidence);
    }
  });
});
