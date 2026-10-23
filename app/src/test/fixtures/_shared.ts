// Shared constants + the `mockFetch` helper used by every fixture file.

import { vi } from "bun:test";
import { stubGlobal } from "../globals";
import { protocolResponse } from "../protocol";

export const MONTHS = [
  "jan", "feb", "mar", "apr", "maj", "jun",
  "jul", "aug", "sep", "okt", "nov", "dec",
];

export const STATEMENT_COMPANY = {
  name: "Acme ApS",
  cvr: "DK12345678",
  country: "DK",
  currency: "DKK",
  fiscalYearStartMonth: 1,
  fiscalYearLabelStrategy: "end-year",
};

export const STATEMENT_FISCAL_YEARS = [
  { label: "2026", start: "2026-01-01", end: "2026-12-31", source: "live" as const },
  { label: "2025", start: null, end: null, source: "archive" as const },
];

export const FY2026_PERIODS = [
  "2026-01", "2026-02", "2026-03", "2026-04", "2026-05", "2026-06",
  "2026-07", "2026-08", "2026-09", "2026-10", "2026-11", "2026-12",
];

export const MILEAGE_MONTH_LABELS = [
  "jan", "feb", "mar", "apr", "maj", "jun",
  "jul", "aug", "sep", "okt", "nov", "dec",
];

export const CASHFLOW_MONTHS = [
  "jan", "feb", "mar", "apr", "maj", "jun",
  "jul", "aug", "sep", "okt", "nov", "dec",
];

type RouteMap = Record<string, unknown>;

/**
 * Installs a `fetch` mock. `routes` maps a `METHOD path` (path matched by
 * prefix, query stripped) to either a success payload, or `{__error}` to make
 * the route fail with the cockpit error envelope.
 */
export function mockFetch(routes: RouteMap) {
  stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0];
      const method = (init?.method ?? "GET").toUpperCase();
      if (method === "GET" && path === "/api/protocol") {
        const keys = [...new Set([...Object.keys(routes), "GET /api/health"].map(key => key.split("?")[0]!))];
        return protocolResponse({ operations: keys.map(key => {
          const [method, route] = key.split(" ");
          const path = route!.split("?")[0];
          return { method, path, capability: `${method} ${path}` };
        }) });
      }
      const key =
        Object.keys(routes).find((k) => {
          const [m, p] = k.split(" ");
          return m === method && path === p;
        }) ?? `${method} ${path}`;
      const payload = routes[key];
      if (payload === undefined && method === "GET" && path === "/api/health") {
        return jsonResponse({
          ok: true, service: "rentemester-cockpit", workspace: "/test", authRequired: false,
          deploymentProfile: "local", build: {}, provenance: {}, routes: [],
        });
      }
      if (payload === undefined) {
        throw new Error(`Unexpected fetch request: ${method} ${path}`);
      }
      if (
        payload &&
        typeof payload === "object" &&
        "__error" in (payload as Record<string, unknown>)
      ) {
        const e = (payload as { __error: { code: string; message: string } })
          .__error;
        return jsonResponse(
          { ok: false, errors: [e.message], code: e.code },
          e.code === "conflict" ? 409 : 400,
        );
      }
      return jsonResponse({ ok: true, ...(payload as object) });
    }),
  );
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
