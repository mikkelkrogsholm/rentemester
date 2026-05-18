export type Ore = bigint;

type DecimalParts = {
  sign: 1 | -1;
  digits: string;
  scale: number;
};

function pow10(exp: number) {
  return 10n ** BigInt(exp);
}

function normalizeDigits(digits: string) {
  const trimmed = digits.replace(/^0+/, "");
  return trimmed.length > 0 ? trimmed : "0";
}

function parseDecimalParts(input: number | string): DecimalParts {
  const raw = String(input).trim().toLowerCase();
  if (!raw || raw === ".") return { sign: 1, digits: "0", scale: 0 };

  let sign: 1 | -1 = 1;
  let value = raw;
  if (value.startsWith("-")) {
    sign = -1;
    value = value.slice(1);
  } else if (value.startsWith("+")) {
    value = value.slice(1);
  }

  if (!/^\d*\.?\d*(e[+-]?\d+)?$/.test(value) || value === "" || value === ".") {
    throw new Error(`invalid decimal value: ${input}`);
  }

  const [coefficient, exponentText] = value.split("e");
  const exponent = exponentText ? Number(exponentText) : 0;
  if (!Number.isInteger(exponent)) throw new Error(`invalid exponent in decimal value: ${input}`);

  const [whole = "", fraction = ""] = coefficient.split(".");
  let digits = normalizeDigits(`${whole}${fraction}`);
  let scale = fraction.length - exponent;

  if (scale < 0) {
    digits = `${digits}${"0".repeat(-scale)}`;
    scale = 0;
  }

  return { sign, digits, scale };
}

function roundScaledInteger(value: bigint, currentScale: number, targetScale: number) {
  if (currentScale === targetScale) return value;
  if (currentScale < targetScale) return value * pow10(targetScale - currentScale);

  const divisor = pow10(currentScale - targetScale);
  const abs = value < 0n ? -value : value;
  const quotient = abs / divisor;
  const remainder = abs % divisor;
  const rounded = remainder * 2n >= divisor ? quotient + 1n : quotient;
  return value < 0n ? -rounded : rounded;
}

function scaledInt(input: number | string, scale: number) {
  const parsed = parseDecimalParts(input);
  const signed = BigInt(parsed.digits) * BigInt(parsed.sign);
  return roundScaledInteger(signed, parsed.scale, scale);
}

function fromScaledInt(value: bigint, scale: number) {
  return Number(value) / 10 ** scale;
}

export function roundDkk(value: number) {
  return Number.isFinite(value) ? fromScaledInt(scaledInt(value, 2), 2) : NaN;
}

/**
 * Format a monetary amount as a fixed-precision string with 2 decimals.
 *
 * Uses integer-ore math internally (no floating-point drift) and emits a plain
 * "1234.56" style string. Callers that need a currency suffix should append it
 * themselves (e.g. `${formatAmount(value)} ${currency}`).
 *
 * Returns null for null/undefined/empty/NaN inputs so PDF/JSON renderers can
 * keep their conditional formatting branches.
 */
export function formatAmount(value: number | string | bigint | null | undefined): string | null {
  if (value == null || value === "") return null;
  let ore: bigint;
  if (typeof value === "bigint") {
    ore = value;
  } else {
    const num = typeof value === "number" ? value : Number(value);
    if (!Number.isFinite(num)) return null;
    ore = toOre(num);
  }
  const negative = ore < 0n;
  const abs = negative ? -ore : ore;
  const whole = abs / 100n;
  const fraction = abs % 100n;
  const fractionText = fraction.toString().padStart(2, "0");
  return `${negative ? "-" : ""}${whole.toString()}.${fractionText}`;
}

/**
 * Format a DKK amount with currency suffix, e.g. "1234.56 DKK".
 *
 * Convenience wrapper around `formatAmount` that appends the (uppercased)
 * currency code. Returns null on invalid/empty input.
 */
export function formatDkk(value: number | string | bigint | null | undefined, currency = "DKK"): string | null {
  const amount = formatAmount(value);
  if (amount == null) return null;
  return `${amount} ${currency.trim().toUpperCase()}`;
}

export function roundRate6(value: number) {
  return Number.isFinite(value) ? fromScaledInt(scaledInt(value, 6), 6) : NaN;
}

export function toOre(value: number) {
  if (!Number.isFinite(value)) throw new Error(`invalid monetary amount: ${value}`);
  return scaledInt(value, 2);
}

export function fromOre(value: Ore) {
  return fromScaledInt(value, 2);
}

export function sumDkk(values: Array<number | null | undefined>) {
  let total = 0n;
  for (const value of values) {
    if (value == null) continue;
    total += toOre(value);
  }
  return fromOre(total);
}

export function addDkk(...values: number[]) {
  return sumDkk(values);
}

export function subtractDkk(left: number, ...rights: number[]) {
  let total = toOre(left);
  for (const value of rights) total -= toOre(value);
  return fromOre(total);
}

export function absDkk(value: number) {
  return fromOre(toOre(value) < 0n ? -toOre(value) : toOre(value));
}

export function compareDkk(left: number, right: number) {
  const l = toOre(left);
  const r = toOre(right);
  if (l === r) return 0;
  return l < r ? -1 : 1;
}

export function equalsDkk(left: number, right: number) {
  return compareDkk(left, right) === 0;
}

export function multiplyDkk(left: number, right: number) {
  const a = parseDecimalParts(left);
  const b = parseDecimalParts(right);
  const sign = a.sign * b.sign;
  const product = BigInt(a.digits) * BigInt(b.digits) * BigInt(sign);
  return fromScaledInt(roundScaledInteger(product, a.scale + b.scale, 2), 2);
}

export function percentOfDkk(amount: number, percent: number) {
  const amountOre = toOre(amount);
  const basisPoints = scaledInt(percent, 2);
  return fromOre(roundDiv(amountOre * basisPoints, 10000n));
}

export function accrueInterestDkk(principalAmount: number, annualRatePercent: number, days: number) {
  if (!Number.isInteger(days) || days <= 0) return 0;
  const principalOre = toOre(principalAmount);
  const basisPoints = scaledInt(annualRatePercent, 2);
  return fromOre(roundDiv(principalOre * basisPoints * BigInt(days), 10000n * 365n));
}

export function roundDiv(numerator: bigint, denominator: bigint) {
  if (denominator === 0n) throw new Error("division by zero");
  const sameSign = (numerator >= 0n && denominator > 0n) || (numerator <= 0n && denominator < 0n);
  const absNumerator = numerator < 0n ? -numerator : numerator;
  const absDenominator = denominator < 0n ? -denominator : denominator;
  const quotient = absNumerator / absDenominator;
  const remainder = absNumerator % absDenominator;
  const rounded = remainder * 2n >= absDenominator ? quotient + 1n : quotient;
  return sameSign ? rounded : -rounded;
}
