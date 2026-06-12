// Shared constants + the `mockFetch` helper used by every fixture file.

import { vi } from "vitest";

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
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = typeof input === "string" ? input : input.toString();
      const path = url.replace(/^https?:\/\/[^/]+/, "").split("?")[0];
      const method = (init?.method ?? "GET").toUpperCase();
      const key =
        Object.keys(routes).find((k) => {
          const [m, p] = k.split(" ");
          return m === method && path === p;
        }) ?? `${method} ${path}`;
      const payload = routes[key];
      if (payload === undefined) {
        // #368: unified `{ ok:false, errors:[...], code }` envelope.
        return jsonResponse(
          { ok: false, errors: ["no route"], code: "not_found" },
          404,
        );
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
