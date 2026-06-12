import { isValidIsoDate as looksLikeIsoDate } from "./dates";
import { normalizeCurrency, roundDkk, roundRate6 } from "./money";
import { normalizeEanNumber } from "./ean";
export type InvoiceType = "full" | "simplified";
export type VatTreatment = "standard" | "domestic_reverse_charge" | "foreign_reverse_charge";
export type ReverseChargeBasis =
  | "DK_MOMSLOVEN_§46_STK_1_NR_3"
  | "DK_MOMSLOVEN_§46_STK_1_NR_6"
  | "DK_MOMSLOVEN_§46_STK_1_NR_7"
  | "EU_MOMSDIREKTIV_ART_196"
  | "EU_MOMSDIREKTIV_ART_199";

export type InvoiceBuyer = {
  name?: string;
  address?: string;
  vatOrCvr?: string;
  eanNumber?: string;
  publicRecipient?: boolean;
};

export type InvoicePayload = {
  invoiceType: InvoiceType;
  vatTreatment?: VatTreatment;
  issueDate?: string;
  invoiceNumber?: string;
  seller?: { name?: string; address?: string; vatOrCvr?: string };
  buyer?: InvoiceBuyer;
  lines?: Array<{ description?: string; quantity?: number; unitPriceExVat?: number; lineTotalExVat?: number }>;
  totals?: {
    netAmount?: number;
    vatRate?: number;
    vatAmount?: number;
    grossAmount?: number;
    fxRateToDkk?: number;
    netAmountDkk?: number;
    vatAmountDkk?: number;
    grossAmountDkk?: number;
    vatComputationBasis?: "VAT_20_OF_GROSS" | string;
  };
  reverseChargeBasis?: ReverseChargeBasis;
  reverseChargeNote?: string;
  currency?: string;
  dueDate?: string;
  deliveryDate?: string;
  deliveryPeriodStart?: string;
  deliveryPeriodEnd?: string;
};

export type InvoiceValidationResult = {
  ok: boolean;
  invoiceType: InvoiceType;
  vatTreatment: VatTreatment;
  appliedRules: string[];
  errors: string[];
};

const RULES = {
  FULL: "DK-INVOICE-FULL-001",
  SIMPLIFIED: "DK-INVOICE-SIMPLIFIED-001",
  REVERSE_CHARGE: "DK-INVOICE-REVERSE-CHARGE-001",
  REVERSE_CHARGE_BASIS: "DK-INVOICE-REVERSE-CHARGE-BASIS-001",
  DELIVERY_DATE: "DK-INVOICE-DELIVERY-DATE-001",
  ARITHMETIC: "DK-INVOICE-ARITHMETIC-001",
  VAT_SEPARATE_AMOUNT: "DK-VAT-SEPARATE-AMOUNT-001",
  PUBLIC_RECIPIENT: "DK-INVOICE-PUBLIC-RECIPIENT-001",
} as const;

const FOREIGN_REVERSE_CHARGE_BASES: ReverseChargeBasis[] = [
  "DK_MOMSLOVEN_§46_STK_1_NR_3",
  "EU_MOMSDIREKTIV_ART_196",
  "EU_MOMSDIREKTIV_ART_199",
];
const DOMESTIC_REVERSE_CHARGE_BASES: ReverseChargeBasis[] = [
  "DK_MOMSLOVEN_§46_STK_1_NR_6",
  "DK_MOMSLOVEN_§46_STK_1_NR_7",
];

function hasText(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function hasPositiveNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}


function hasLineDescriptions(lines: InvoicePayload["lines"]) {
  return Array.isArray(lines) && lines.length > 0 && lines.every((line) => hasText(line.description));
}

function normalizedCurrency(payload: InvoicePayload) {
  return normalizeCurrency(payload.currency);
}

export function validateInvoice(payload: InvoicePayload): InvoiceValidationResult {
  const errors: string[] = [];
  const invoiceType = payload.invoiceType;
  const vatTreatment = payload.vatTreatment ?? "standard";
  const currency = normalizedCurrency(payload);
  const appliedRules = [invoiceType === "simplified" ? RULES.SIMPLIFIED : RULES.FULL, RULES.ARITHMETIC];

  if (!looksLikeIsoDate(payload.issueDate)) errors.push("issueDate must be present in YYYY-MM-DD format");
  if (payload.dueDate !== undefined && !looksLikeIsoDate(payload.dueDate)) errors.push("dueDate must be YYYY-MM-DD when present");
  if (looksLikeIsoDate(payload.issueDate) && looksLikeIsoDate(payload.dueDate) && payload.dueDate < payload.issueDate) {
    errors.push("dueDate cannot be earlier than issueDate");
  }
  appliedRules.push(RULES.DELIVERY_DATE);
  if (payload.deliveryDate !== undefined && !looksLikeIsoDate(payload.deliveryDate)) {
    errors.push("deliveryDate must be YYYY-MM-DD when present");
  }
  if (payload.deliveryPeriodStart !== undefined && !looksLikeIsoDate(payload.deliveryPeriodStart)) {
    errors.push("deliveryPeriodStart must be YYYY-MM-DD when present");
  }
  if (payload.deliveryPeriodEnd !== undefined && !looksLikeIsoDate(payload.deliveryPeriodEnd)) {
    errors.push("deliveryPeriodEnd must be YYYY-MM-DD when present");
  }
  const hasDeliveryPeriodStart = payload.deliveryPeriodStart !== undefined;
  const hasDeliveryPeriodEnd = payload.deliveryPeriodEnd !== undefined;
  if (hasDeliveryPeriodStart !== hasDeliveryPeriodEnd) {
    errors.push("deliveryPeriodStart and deliveryPeriodEnd must be provided together");
  }
  if (payload.deliveryDate !== undefined && (hasDeliveryPeriodStart || hasDeliveryPeriodEnd)) {
    errors.push("use either deliveryDate or deliveryPeriodStart/deliveryPeriodEnd, not both");
  }
  if (looksLikeIsoDate(payload.deliveryPeriodStart) && looksLikeIsoDate(payload.deliveryPeriodEnd) && payload.deliveryPeriodEnd < payload.deliveryPeriodStart) {
    errors.push("deliveryPeriodEnd cannot be earlier than deliveryPeriodStart");
  }
  if (payload.invoiceNumber !== undefined && !hasText(payload.invoiceNumber)) errors.push("invoiceNumber must not be blank when present");
  if (!hasText(payload.seller?.name)) errors.push("seller.name is required");
  if (!hasText(payload.seller?.address)) errors.push("seller.address is required");
  if (!hasText(payload.seller?.vatOrCvr)) errors.push("seller.vatOrCvr is required");
  if (!hasLineDescriptions(payload.lines)) errors.push("lines must contain at least one described good or service");
  if (!hasPositiveNumber(payload.totals?.grossAmount)) errors.push("totals.grossAmount is required");

  if (invoiceType === "full") {
    if (!hasText(payload.buyer?.name)) errors.push("buyer.name is required for full invoices");
    if (!hasText(payload.buyer?.address)) errors.push("buyer.address is required for full invoices");
    if (!hasPositiveNumber(payload.totals?.netAmount)) errors.push("totals.netAmount is required for full invoices");
  }

  const buyerEanNumber = normalizeEanNumber(payload.buyer?.eanNumber);
  const publicRecipient = payload.buyer?.publicRecipient === true || buyerEanNumber !== null;
  if (publicRecipient) {
    appliedRules.push(RULES.PUBLIC_RECIPIENT);
    if (invoiceType !== "full") {
      errors.push("public-recipient invoices must use invoiceType full");
    }
    if (!buyerEanNumber) {
      errors.push("public-recipient invoices must include buyer.eanNumber as 13 digits");
    }
  }

  if (invoiceType === "simplified") {
    if ((payload.totals?.grossAmount ?? Number.POSITIVE_INFINITY) > 3000) {
      errors.push("simplified invoices are only allowed up to DKK 3,000 gross");
    }
    const hasVatAmount = hasPositiveNumber(payload.totals?.vatAmount) && (payload.totals?.vatAmount ?? 0) > 0;
    const has20PctBasis = payload.totals?.vatComputationBasis === "VAT_20_OF_GROSS";
    if (!hasVatAmount && !has20PctBasis) {
      errors.push("simplified invoices must include vatAmount or VAT_20_OF_GROSS computation basis");
    }
  }

  if (vatTreatment === "standard") {
    appliedRules.push(RULES.VAT_SEPARATE_AMOUNT);
    if (!hasPositiveNumber(payload.totals?.vatRate) || (payload.totals?.vatRate ?? 0) <= 0) {
      errors.push("standard VAT invoices must include totals.vatRate");
    }
    if (!hasPositiveNumber(payload.totals?.vatAmount) || (payload.totals?.vatAmount ?? 0) <= 0) {
      errors.push("standard VAT invoices must include totals.vatAmount");
    }
  }

  if (vatTreatment === "domestic_reverse_charge" || vatTreatment === "foreign_reverse_charge") {
    appliedRules.push(RULES.REVERSE_CHARGE, RULES.REVERSE_CHARGE_BASIS);
    if (!hasText(payload.reverseChargeBasis)) {
      errors.push("reverse-charge invoices must include reverseChargeBasis");
    }
    if (payload.totals?.vatRate !== undefined) {
      errors.push("reverse-charge invoices must not include totals.vatRate");
    }
    if (payload.totals?.vatAmount !== undefined) {
      errors.push("reverse-charge invoices must not include totals.vatAmount");
    }
    if (vatTreatment === "foreign_reverse_charge") {
      if (!hasText(payload.buyer?.vatOrCvr)) {
        errors.push("foreign reverse-charge invoices must include buyer.vatOrCvr");
      }
      if (hasText(payload.reverseChargeBasis) && !FOREIGN_REVERSE_CHARGE_BASES.includes(payload.reverseChargeBasis)) {
        errors.push(`reverseChargeBasis ${payload.reverseChargeBasis} is not valid for foreign reverse-charge invoices`);
      }
    }
    if (vatTreatment === "domestic_reverse_charge" && hasText(payload.reverseChargeBasis) && !DOMESTIC_REVERSE_CHARGE_BASES.includes(payload.reverseChargeBasis)) {
      errors.push(`reverseChargeBasis ${payload.reverseChargeBasis} is not valid for domestic reverse-charge invoices`);
    }
  }

  if (Array.isArray(payload.lines)) {
    for (const [index, line] of payload.lines.entries()) {
      const qty = line.quantity;
      const unit = line.unitPriceExVat;
      const total = line.lineTotalExVat;
      if (typeof qty === "number" && typeof unit === "number" && typeof total === "number") {
        const expected = roundDkk(qty * unit);
        if (roundDkk(total) !== expected) {
          errors.push(`lines[${index}].lineTotalExVat must equal quantity * unitPriceExVat (${expected})`);
        }
      }
    }
  }

  const lineSum = Array.isArray(payload.lines)
    ? roundDkk(payload.lines.reduce((sum, line) => sum + Number(line.lineTotalExVat ?? 0), 0))
    : 0;
  const netAmount = roundDkk(Number(payload.totals?.netAmount ?? 0));
  const vatAmount = roundDkk(Number(payload.totals?.vatAmount ?? 0));
  const grossAmount = roundDkk(Number(payload.totals?.grossAmount ?? 0));
  const fxRateToDkk = roundRate6(Number(payload.totals?.fxRateToDkk ?? 0));
  const netAmountDkk = roundDkk(Number(payload.totals?.netAmountDkk ?? 0));
  const vatAmountDkk = roundDkk(Number(payload.totals?.vatAmountDkk ?? 0));
  const grossAmountDkk = roundDkk(Number(payload.totals?.grossAmountDkk ?? 0));

  if (invoiceType === "full" && Array.isArray(payload.lines) && payload.lines.every((line) => typeof line.lineTotalExVat === "number")) {
    if (netAmount !== lineSum) errors.push(`totals.netAmount must equal sum of lineTotalExVat (${lineSum})`);
  }

  if (vatTreatment === "standard" && (invoiceType === "full" || payload.totals?.netAmount !== undefined)) {
    const expectedGross = roundDkk(netAmount + vatAmount);
    if (grossAmount !== expectedGross) {
      errors.push(`totals.grossAmount must equal totals.netAmount + totals.vatAmount (${expectedGross})`);
    }
  }

  // A simplified standard-VAT invoice normally omits totals.netAmount, so the
  // gross = net + vat cross-check above is skipped and an internally
  // impossible vatAmount (e.g. 999 on a 1.000 kr gross) would otherwise be
  // accepted and booked verbatim as output VAT. When an explicit vatAmount is
  // given, derive the implied net (gross − vat) and require the VAT to be
  // consistent with the declared rate, so a nonsensical figure is rejected
  // before it ever reaches the ledger. (A basis-only simplified invoice with
  // VAT_20_OF_GROSS and no explicit vatAmount computes its VAT downstream and
  // cannot be inconsistent, so it is left untouched.)
  if (
    vatTreatment === "standard" &&
    invoiceType === "simplified" &&
    payload.totals?.netAmount === undefined &&
    hasPositiveNumber(payload.totals?.vatAmount)
  ) {
    const vatRate = roundRate6(Number(payload.totals?.vatRate ?? 0));
    if (!(grossAmount > vatAmount)) {
      errors.push("totals.vatAmount must be less than totals.grossAmount");
    } else if (vatRate > 0) {
      // The VAT CONTAINED in a gross-inclusive amount is `gross * rate/(1+rate)`
      // (= 20% of gross at the 25% rate — exactly the VAT_20_OF_GROSS basis a
      // forenklet faktura uses), NOT `(gross − vat) * rate`. The net-first
      // round-trip double-rounds and wrongly rejects ~20% of legitimate
      // simplified invoices at the øre boundary (e.g. gross 100,07 → correct
      // VAT 20,01). Accept any vatAmount within one øre of the canonical
      // figure; a nonsensical VAT (e.g. 999 on a 1.000 gross) is still far
      // outside the tolerance and rejected.
      const expectedVat = roundDkk((grossAmount * vatRate) / (1 + vatRate));
      if (roundDkk(Math.abs(vatAmount - expectedVat)) > 0.01) {
        errors.push(
          `totals.vatAmount must be the VAT contained in totals.grossAmount at totals.vatRate (≈ ${expectedVat})`,
        );
      }
    }
  }

  if ((vatTreatment === "domestic_reverse_charge" || vatTreatment === "foreign_reverse_charge") && payload.totals?.netAmount !== undefined) {
    if (grossAmount !== netAmount) {
      errors.push(`reverse-charge invoices must have totals.grossAmount equal totals.netAmount (${netAmount})`);
    }
  }

  if (currency.length !== 3) {
    errors.push("currency must be a 3-letter ISO code when present");
  }

  if (currency !== "DKK") {
    if (!(fxRateToDkk > 0)) errors.push("non-DKK invoices must include totals.fxRateToDkk");
    if (!hasPositiveNumber(payload.totals?.netAmountDkk)) errors.push("non-DKK invoices must include totals.netAmountDkk");
    if (vatTreatment === "standard" && !hasPositiveNumber(payload.totals?.vatAmountDkk)) errors.push("non-DKK invoices must include totals.vatAmountDkk for standard VAT");
    if (!hasPositiveNumber(payload.totals?.grossAmountDkk)) errors.push("non-DKK invoices must include totals.grossAmountDkk");

    if (fxRateToDkk > 0) {
      if (payload.totals?.netAmountDkk !== undefined) {
        const expectedNetDkk = roundDkk(netAmount * fxRateToDkk);
        if (netAmountDkk !== expectedNetDkk) errors.push(`totals.netAmountDkk must equal totals.netAmount * totals.fxRateToDkk (${expectedNetDkk})`);
      }
      if (payload.totals?.vatAmountDkk !== undefined) {
        const expectedVatDkk = roundDkk(vatAmount * fxRateToDkk);
        if (vatAmountDkk !== expectedVatDkk) errors.push(`totals.vatAmountDkk must equal totals.vatAmount * totals.fxRateToDkk (${expectedVatDkk})`);
      }
      if (payload.totals?.grossAmountDkk !== undefined) {
        const expectedGrossDkk = roundDkk(grossAmount * fxRateToDkk);
        if (grossAmountDkk !== expectedGrossDkk) errors.push(`totals.grossAmountDkk must equal totals.grossAmount * totals.fxRateToDkk (${expectedGrossDkk})`);
      }
    }

    if (vatTreatment === "standard" && (payload.totals?.netAmountDkk !== undefined || payload.totals?.vatAmountDkk !== undefined || payload.totals?.grossAmountDkk !== undefined)) {
      const expectedGrossDkk = roundDkk(netAmountDkk + vatAmountDkk);
      if (grossAmountDkk !== expectedGrossDkk) {
        errors.push(`totals.grossAmountDkk must equal totals.netAmountDkk + totals.vatAmountDkk (${expectedGrossDkk})`);
      }
    }

    if ((vatTreatment === "domestic_reverse_charge" || vatTreatment === "foreign_reverse_charge") && payload.totals?.netAmountDkk !== undefined) {
      if (grossAmountDkk !== netAmountDkk) {
        errors.push(`reverse-charge invoices must have totals.grossAmountDkk equal totals.netAmountDkk (${netAmountDkk})`);
      }
    }
  }

  return {
    ok: errors.length === 0,
    invoiceType,
    vatTreatment,
    appliedRules,
    errors,
  };
}

// ---------------------------------------------------------------------------
// Invoice arithmetic — compute line totals and invoice totals from the bare
// essentials a human can supply (description, quantity, unit price ex-VAT and
// a VAT rate). The point of #212: a bookkeeping product must do this maths so
// the human never hand-writes a number SKAT will hold them accountable for.
//
// All amounts are in DKK kroner (decimal). `vatRatePercent` is a percentage
// (e.g. 25 for standard Danish VAT), normalised to the 0..1 fraction the rest
// of the invoice pipeline (validation, totals.vatRate) expects.
// ---------------------------------------------------------------------------

export type InvoiceLineInput = {
  description: string;
  quantity: number;
  unitPriceExVat: number;
};

export type ComputedInvoiceLine = {
  description: string;
  quantity: number;
  unitPriceExVat: number;
  lineTotalExVat: number;
};

export type ComputedInvoiceTotals = {
  /** VAT rate as a 0..1 fraction, ready for totals.vatRate. */
  vatRate: number;
  netAmount: number;
  vatAmount: number;
  grossAmount: number;
};

export type ComputeInvoiceAmountsResult =
  | { ok: true; lines: ComputedInvoiceLine[]; totals: ComputedInvoiceTotals; errors: [] }
  | { ok: false; lines: ComputedInvoiceLine[]; totals?: undefined; errors: string[] };

/**
 * Compute every derived invoice amount from minimal human input.
 *
 * The human supplies one or more lines (description, quantity, unit price
 * ex-VAT) and a single VAT rate in percent. Rentemester computes each line
 * total, the net amount (sum of line totals), the VAT amount and the gross
 * amount — using the same `roundDkk` ore-precise rounding the validator uses,
 * so the result always passes `validateInvoice`'s arithmetic checks.
 */
export function computeInvoiceAmounts(
  lines: InvoiceLineInput[],
  vatRatePercent: number,
): ComputeInvoiceAmountsResult {
  const errors: string[] = [];
  if (!Array.isArray(lines) || lines.length === 0) {
    errors.push("at least one invoice line is required");
  }
  if (!Number.isFinite(vatRatePercent) || vatRatePercent < 0) {
    errors.push("vatRatePercent must be a number greater than or equal to 0");
  }

  const computed: ComputedInvoiceLine[] = [];
  (lines ?? []).forEach((line, index) => {
    const description = typeof line.description === "string" ? line.description.trim() : "";
    if (description.length === 0) {
      errors.push(`lines[${index}].description must not be blank`);
    }
    if (!Number.isFinite(line.quantity) || line.quantity <= 0) {
      errors.push(`lines[${index}].quantity must be a number greater than 0`);
    }
    if (!Number.isFinite(line.unitPriceExVat) || line.unitPriceExVat < 0) {
      errors.push(`lines[${index}].unitPriceExVat must be a number greater than or equal to 0`);
    }
    computed.push({
      description,
      quantity: line.quantity,
      unitPriceExVat: line.unitPriceExVat,
      lineTotalExVat: roundDkk(Number(line.quantity) * Number(line.unitPriceExVat)),
    });
  });

  if (errors.length > 0) return { ok: false, lines: computed, errors };

  const netAmount = roundDkk(computed.reduce((sum, line) => sum + line.lineTotalExVat, 0));
  const vatRate = roundRate6(vatRatePercent / 100);
  const vatAmount = roundDkk(netAmount * vatRate);
  const grossAmount = roundDkk(netAmount + vatAmount);

  return {
    ok: true,
    lines: computed,
    totals: { vatRate, netAmount, vatAmount, grossAmount },
    errors: [],
  };
}
