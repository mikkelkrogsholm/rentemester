import { describe, expect, test } from "bun:test";
import { listPagination, listReturnTo, workflowTo } from "./workflow-navigation";

describe("daily workflow navigation", () => {
  test("return context preserves only this company's supported list fields", () => {
    expect(listReturnTo("acme-aps", "bilag", "/companies/acme-aps/bilag?year=2026&q=software&sort=amount&dir=desc&page=2&debug=secret&returnTo=https://evil.test"))
      .toBe("/companies/acme-aps/bilag?year=2026&q=software&sort=amount&dir=desc&page=2");
    for (const raw of ["https://evil.test/companies/acme-aps/bilag", "//evil.test/companies/acme-aps/bilag", "javascript:alert(1)", "/companies/other-aps/bilag", "/companies/acme-aps/fakturaer"]) {
      expect(listReturnTo("acme-aps", "bilag", raw, "2026")).toBe("/companies/acme-aps/bilag?year=2026");
    }
  });
  test("nested detail-to-booking links retain the original filters", () => {
    const params = new URLSearchParams({ year: "2026", returnTo: "/companies/acme-aps/bilag?year=2026&q=software&page=2" });
    const url = new URL(workflowTo("acme-aps", "bilag", "7/bogfoer", params), "https://rentemester.invalid");
    expect(url.pathname).toBe("/companies/acme-aps/bilag/7/bogfoer");
    expect(url.searchParams.get("returnTo")).toBe("/companies/acme-aps/bilag?year=2026&q=software&page=2");
    expect(url.searchParams.get("year")).toBe("2026");
  });
  test("pagination defaults, clamps and rejects invalid sizes", () => {
    expect(listPagination(new URLSearchParams(), 120)).toEqual({ page: 1, pageSize: 50, offset: 0 });
    expect(listPagination(new URLSearchParams("pageSize=25&page=3"), 120)).toEqual({ page: 3, pageSize: 25, offset: 50 });
    expect(listPagination(new URLSearchParams("pageSize=100&page=99"), 120)).toEqual({ page: 2, pageSize: 100, offset: 100 });
    expect(listPagination(new URLSearchParams("pageSize=10000&page=-1"), 0)).toEqual({ page: 1, pageSize: 50, offset: 0 });
    expect(listPagination(new URLSearchParams("pageSize=50&page=1.5"), 100)).toEqual({ page: 1, pageSize: 50, offset: 0 });
  });
});
