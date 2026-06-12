import { afterEach, describe, expect, test, vi } from "vitest";
import {
  attentionFlags,
  attentionLevel,
  formatCurrency,
  formatKroner,
  sortByAttention,
  todayIso,
} from "./format";
import { summary } from "../test/fixtures";

describe("todayIso", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  test("returns the LOCAL date, not the UTC one, late on a day", () => {
    // Pick a local instant of 23:30 — for any positive UTC offset (Danmark is
    // UTC+1/+2) the UTC clock has already rolled to the next day, so the naïve
    // `new Date().toISOString().slice(0,10)` mis-dates a booking by +1.
    // `todayIso` must return the LOCAL "2026-03-01". The instant is built from
    // local Y/M/D components so the test holds whatever the runner's timezone.
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 2, 1, 23, 30, 0)); // 1 Mar 2026, 23:30 local
    expect(todayIso()).toBe("2026-03-01");
  });
});

describe("formatCurrency", () => {
  test("renders øre as a Danish currency amount", () => {
    // 1.234,56 kr — non-breaking spaces, so just assert the digits + comma.
    const out = formatCurrency(123456);
    expect(out).toMatch(/1.234,56/);
  });

  test("handles zero", () => {
    expect(formatCurrency(0)).toMatch(/0,00/);
  });
});

describe("formatKroner (DKK) integer-øre rounding", () => {
  test("rounds half-up at øre like the server, and never shows a negative zero", () => {
    // Byte-identical to core/money.ts#formatKronerDa: 1.005 → "1,01 kr.", not
    // "1,00 kr." from float Math.round(num*100).
    expect(formatKroner(1.005)).toBe("1,01 kr.");
    expect(formatKroner(1.015)).toBe("1,02 kr.");
    expect(formatKroner(2.675)).toBe("2,68 kr.");
    // The byte-identical contract's load-bearing case: a >3-decimal value below
    // the half-øre must round DOWN, matching the server's single full-precision
    // round. A toFixed(3) pre-round double-rounded these to "1,01"/"99,99→100".
    expect(formatKroner(1.0049)).toBe("1,00 kr.");
    expect(formatKroner(99.9949)).toBe("99,99 kr.");
    expect(formatKroner(0.0049)).toBe("0,00 kr.");
    // A sub-øre negative rounds to zero and must drop the minus sign.
    expect(formatKroner(-0.001)).toBe("0,00 kr.");
    // Ordinary values and a real negative are unchanged.
    expect(formatKroner(1234.5)).toBe("1.234,50 kr.");
    expect(formatKroner(-1234.5)).toBe("-1.234,50 kr.");
  });
});

describe("attentionFlags", () => {
  test("a healthy company has no flags", () => {
    expect(attentionFlags(summary())).toEqual([]);
    expect(attentionLevel(summary())).toBe("ok");
  });

  test("a missing ledger is the single critical flag", () => {
    const flags = attentionFlags(summary({ ledgerMissing: true }));
    expect(flags).toHaveLength(1);
    expect(flags[0]).toMatchObject({ level: "critical" });
  });

  test("a broken audit chain is critical", () => {
    expect(attentionLevel(summary({ auditChainOk: false }))).toBe("critical");
  });

  test("a broken audit chain flag links to /integritet (#420)", () => {
    // #420 — flaget skal ikke være en dødvej. Det skal pege på Integritet-
    // viewet (#333) hvor afvigelsen er forklaret med entry-nr + et konkret
    // næste skridt.
    const flags = attentionFlags(
      summary({ slug: "acme-aps", auditChainOk: false }),
    );
    const chainFlag = flags.find((f) => f.label === "Revisionskæde brudt");
    expect(chainFlag).toBeDefined();
    expect(chainFlag?.to).toBe("/companies/acme-aps/integritet");
  });

  test("non-critical-chain flags do not get a `to`-link (default)", () => {
    // Vi vil ikke pr. uagtsomhed linke andre flags. Kun audit-chain-flaget er
    // klikbart i denne PR.
    const flags = attentionFlags(summary({ resultat: -100, openTaskCount: 3 }));
    for (const f of flags) {
      expect(f.to).toBeUndefined();
    }
  });

  test("a negative result is critical", () => {
    expect(attentionLevel(summary({ resultat: -500 }))).toBe("critical");
  });

  test("open tasks are a warning, not critical", () => {
    expect(attentionLevel(summary({ openTaskCount: 2 }))).toBe("warning");
  });

  test("a VAT deadline within 30 days is a warning", () => {
    const c = summary({
      vat: { payable: 3371.2, deadline: "2026-06-01", daysRemaining: 12 },
    });
    expect(attentionLevel(c)).toBe("warning");
  });

  test("an overdue VAT deadline is critical", () => {
    const c = summary({
      vat: { payable: 3371.2, deadline: "2026-01-01", daysRemaining: -4 },
    });
    expect(attentionLevel(c)).toBe("critical");
  });

  test("the VAT flag names the filing deadline, not the period", () => {
    // The countdown targets the SKAT filing/payment deadline. The flag must
    // say so ("Momsfrist") — a bare "Moms om N dage" reads as the VAT period
    // itself ending, which is a different, earlier date.
    const soon = summary({
      vat: { payable: 3371.2, deadline: "2026-06-01", daysRemaining: 10 },
    });
    const soonFlag = attentionFlags(soon).find((f) =>
      f.label.toLowerCase().includes("moms"),
    );
    expect(soonFlag?.label).toBe("Momsfrist om 10 dage");

    const overdue = summary({
      vat: { payable: 3371.2, deadline: "2026-01-01", daysRemaining: -4 },
    });
    const overdueFlag = attentionFlags(overdue).find((f) =>
      f.label.toLowerCase().includes("moms"),
    );
    expect(overdueFlag?.label).toBe("Momsfrist overskredet");
  });

  test("the VAT flag inflects correctly for 1 and 0 days remaining", () => {
    // "Momsfrist om 1 dage" / "Momsfrist om 0 dage" is wrong Danish grammar;
    // `formatDeadline` gives "i morgen" / "i dag" instead.
    const tomorrow = summary({
      vat: { payable: 3371.2, deadline: "2026-06-01", daysRemaining: 1 },
    });
    const tomorrowFlag = attentionFlags(tomorrow).find((f) =>
      f.label.toLowerCase().includes("moms"),
    );
    expect(tomorrowFlag?.label).not.toBe("Momsfrist om 1 dage");
    expect(tomorrowFlag?.label).toBe("Momsfrist i morgen");

    const today = summary({
      vat: { payable: 3371.2, deadline: "2026-05-31", daysRemaining: 0 },
    });
    const todayFlag = attentionFlags(today).find((f) =>
      f.label.toLowerCase().includes("moms"),
    );
    expect(todayFlag?.label).not.toBe("Momsfrist om 0 dage");
    expect(todayFlag?.label).toBe("Momsfrist i dag");
  });

  test("a negative result outranks open tasks to critical", () => {
    const c = summary({ resultat: -100, openTaskCount: 3 });
    expect(attentionLevel(c)).toBe("critical");
  });
});

describe("sortByAttention", () => {
  test("orders critical, then warning, then ok", () => {
    const ok = summary({ slug: "ok-co", name: "Ok Co" });
    const warn = summary({
      slug: "warn-co",
      name: "Warn Co",
      openTaskCount: 1,
    });
    const crit = summary({
      slug: "crit-co",
      name: "Crit Co",
      auditChainOk: false,
    });
    const sorted = sortByAttention([ok, warn, crit]);
    expect(sorted.map((c) => c.slug)).toEqual(["crit-co", "warn-co", "ok-co"]);
  });

  test("archived companies sink within their level", () => {
    const active = summary({ slug: "a", name: "A" });
    const archived = summary({ slug: "b", name: "B", archived: true });
    expect(sortByAttention([archived, active]).map((c) => c.slug)).toEqual([
      "a",
      "b",
    ]);
  });
});
