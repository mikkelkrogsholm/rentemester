import { describe, expect, test } from "bun:test";
import {
  config,
  get,
  makeWorkspace,
  postBadDebtWriteoff,
  postPnlEntry,
  rmSync,
} from "./_shared";

// #272: the cockpit must surface the VAT quarter that is currently due — the
// one with real activity — not a later, near-empty quarter a bad-debt
// write-off happens to touch. It must agree with the static dashboard, which
// keys off the quarter containing the as-of date.
describe("cockpit API — VAT period selection (#272)", () => {
  // The genuine activity is in Q2 2026 — the current quarter (today is in
  // May 2026). A bad-debt write-off lands in the next quarter, Q3; the
  // future-date ceiling is widened so the later-quarter posting is accepted.
  const originalMaxFuture = process.env.RENTEMESTER_MAX_FUTURE_DAYS;
  function withWideFutureWindow<T>(fn: () => T): T {
    process.env.RENTEMESTER_MAX_FUTURE_DAYS = "120";
    try {
      return fn();
    } finally {
      if (originalMaxFuture === undefined) {
        delete process.env.RENTEMESTER_MAX_FUTURE_DAYS;
      } else {
        process.env.RENTEMESTER_MAX_FUTURE_DAYS = originalMaxFuture;
      }
    }
  }

  test("surfaces the active quarter, not a later quarter holding only a write-off", async () => {
    const ws = makeWorkspace("vat-period", ["Acme ApS"]);
    try {
      // The genuine activity is in Q2 2026 (today, May 2026, is in Q2).
      postPnlEntry(ws, "acme-aps", "2026-05-15", 1000, 400);
      // A bad-debt write-off lands in Q3 2026 — a later, otherwise-empty
      // quarter. It must NOT pull the surfaced VAT period forward to Q3.
      withWideFutureWindow(() =>
        postBadDebtWriteoff(ws, "acme-aps", "2026-07-15", 800),
      );

      const vatRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(vatRes.status).toBe(200);
      // Q2 2026 (Apr–Jun) is the period that is currently due.
      expect(vatRes.body.vat.periodLabel).toBe("Q2 2026");
      expect(vatRes.body.vat.periodStart).toBe("2026-04-01");
      expect(vatRes.body.vat.periodEnd).toBe("2026-06-30");
      // Q2 → momsangivelse due 1 September 2026.
      expect(vatRes.body.vat.deadline).toBe("2026-09-01");

      // The Overblik VAT card must agree with the dedicated VAT view.
      const ovRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(ovRes.status).toBe(200);
      expect(ovRes.body.overview.vat.periodLabel).toBe("Q2 2026");
      expect(ovRes.body.overview.vat.periodEnd).toBe("2026-06-30");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("the cockpit VAT period agrees with the static dashboard", async () => {
    const ws = makeWorkspace("vat-period-parity", ["Acme ApS"]);
    try {
      postPnlEntry(ws, "acme-aps", "2026-05-15", 1000, 400);
      withWideFutureWindow(() =>
        postBadDebtWriteoff(ws, "acme-aps", "2026-07-15", 800),
      );

      // The static dashboard's VAT period is keyed off the as-of date.
      const dashRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/dashboard?asOf=2026-05-22",
      );
      expect(dashRes.status).toBe(200);
      // Static dashboard: Q2 (the as-of date's quarter).
      expect(dashRes.body.dashboard.vat.periodStart).toBe("2026-04-01");
      expect(dashRes.body.dashboard.vat.periodEnd).toBe("2026-06-30");

      // The cockpit VAT view must land on the same period.
      const vatRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(vatRes.body.vat.periodStart).toBe(
        dashRes.body.dashboard.vat.periodStart,
      );
      expect(vatRes.body.vat.periodEnd).toBe(
        dashRes.body.dashboard.vat.periodEnd,
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  // #281: the dashboard VAT block must point at the earliest unreported
  // quarter (the one `selectVatQuarter` picks — what the Overblik card and
  // `vat momsangivelse` use), NOT the calendar quarter of the as-of date.
  // When activity lives only in Q1 but the as-of date is in Q2, the old
  // `quarterPeriodForDate` path wrongly surfaced an empty Q2.
  test("dashboard VAT points at the earliest unreported quarter, not the as-of quarter", async () => {
    const ws = makeWorkspace("vat-dash-earliest", ["Acme ApS"]);
    try {
      // The only booked activity is in Q1 2026.
      postPnlEntry(ws, "acme-aps", "2026-02-15", 1000, 400);

      // As-of date is in Q2 — but Q2 has no activity at all.
      const dashRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/dashboard?asOf=2026-05-22",
      );
      expect(dashRes.status).toBe(200);
      // Must surface Q1 2026 — the quarter that is actually due.
      expect(dashRes.body.dashboard.vat.periodStart).toBe("2026-01-01");
      expect(dashRes.body.dashboard.vat.periodEnd).toBe("2026-03-31");

      // And it must agree with the dedicated VAT view + the Overblik card.
      const vatRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/vat?year=2026",
      );
      expect(vatRes.body.vat.periodStart).toBe(
        dashRes.body.dashboard.vat.periodStart,
      );
      expect(vatRes.body.vat.periodEnd).toBe(
        dashRes.body.dashboard.vat.periodEnd,
      );
      const ovRes = await get(
        config({ workspaceRoot: ws }),
        "/api/companies/acme-aps/overview?year=2026",
      );
      expect(ovRes.body.overview.vat.periodEnd).toBe(
        dashRes.body.dashboard.vat.periodEnd,
      );
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
