// Tests: src/core/money.ts
import { describe, expect, test } from "bun:test";
import { accrueInterestDkk, formatKronerDa, multiplyDkk, percentOfDkk, roundDkk, roundRate6, sumDkk } from "../../src/core/money";

describe("money helpers", () => {
  test("rounds half-up at øre precision instead of Number.toFixed quirks", () => {
    expect(roundDkk(1.005)).toBe(1.01);
    expect(roundDkk(2.335)).toBe(2.34);
    expect(roundDkk(-1.005)).toBe(-1.01);
  });

  test("formatKronerDa rounds through integer øre and never shows a negative zero", () => {
    // Half-up at øre — same as roundDkk/formatAmount, not float Math.round.
    expect(formatKronerDa(1.005)).toBe("1,01 kr.");
    expect(formatKronerDa(1.015)).toBe("1,02 kr.");
    expect(formatKronerDa(2.675)).toBe("2,68 kr.");
    // A single round over the FULL decimal tail — a >3-decimal value below the
    // half-øre must round DOWN (a toFixed(3) pre-round would double-round it up).
    expect(formatKronerDa(1.0049)).toBe("1,00 kr.");
    expect(formatKronerDa(99.9949)).toBe("99,99 kr.");
    expect(formatKronerDa(0.0049)).toBe("0,00 kr.");
    // A sub-øre negative rounds to zero — and must not keep a minus sign.
    expect(formatKronerDa(-0.001)).toBe("0,00 kr.");
    expect(formatKronerDa(-0.004)).toBe("0,00 kr.");
    // Ordinary values, thousands grouping and a real negative are unchanged.
    expect(formatKronerDa(1234.5)).toBe("1.234,50 kr.");
    expect(formatKronerDa(-1234.5)).toBe("-1.234,50 kr.");
    expect(formatKronerDa(null)).toBe("—");
  });

  test("sums through integer øre without float drift", () => {
    expect(sumDkk([0.1, 0.2, 0.3])).toBe(0.6);
    expect(sumDkk([1000.1, 2000.2, -0.3])).toBe(3000);
  });

  test("keeps VAT, FX, and interest calculations deterministic", () => {
    expect(percentOfDkk(596.8, 25)).toBe(149.2);
    expect(multiplyDkk(100, 7.46)).toBe(746);
    expect(roundRate6(7.4555555)).toBe(7.455556);
    expect(accrueInterestDkk(250, 10.2, 5)).toBe(0.35);
  });
});
