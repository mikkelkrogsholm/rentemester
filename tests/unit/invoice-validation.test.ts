// Tests: src/core/invoice.ts (invoice validation)
import { describe, expect, test } from "bun:test";
import { validateInvoice } from "../../src/core/invoice";

describe("invoice validator", () => {
  test("accepts a compliant full Danish VAT invoice", () => {
    const result = validateInvoice({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0042",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9, 8000 Aarhus C" },
      lines: [{ description: "Bogføring og momsopsætning", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      deliveryDate: "2026-04-30",
      currency: "DKK",
    });

    expect(result.ok).toBe(true);
    expect(result.errors).toEqual([]);
    expect(result.appliedRules).toContain("DK-INVOICE-FULL-001");
    expect(result.appliedRules).toContain("DK-INVOICE-ARITHMETIC-001");
  });

  test("rejects public-recipient invoices without a valid 13-digit EAN/GLN number", () => {
    const result = validateInvoice({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0042-PUB",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      buyer: { name: "Aarhus Kommune", address: "Rådhuspladsen 2, 8000 Aarhus C", publicRecipient: true, eanNumber: "57900012" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK",
    });

    expect(result.ok).toBe(false);
    expect(result.errors).toContain("public-recipient invoices must include buyer.eanNumber as 13 digits");
    expect(result.appliedRules).toContain("DK-INVOICE-PUBLIC-RECIPIENT-001");
  });

  test("accepts full public-recipient invoices with EAN/GLN metadata", () => {
    const result = validateInvoice({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0042-PUB-OK",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      buyer: { name: "Aarhus Kommune", address: "Rådhuspladsen 2, 8000 Aarhus C", publicRecipient: true, eanNumber: "5790000000001" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK",
    });

    expect(result.ok).toBe(true);
    expect(result.errors).toEqual([]);
    expect(result.appliedRules).toContain("DK-INVOICE-PUBLIC-RECIPIENT-001");
  });

  test("rejects simplified invoice above the DKK 3,000 limit", () => {
    const result = validateInvoice({
      invoiceType: "simplified",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0043",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      lines: [{ description: "Kvitteringssalg" }],
      totals: { grossAmount: 3000.01, vatAmount: 600.01, vatRate: 0.25 },
      currency: "DKK",
    });

    expect(result.ok).toBe(false);
    expect(result.errors).toContain("simplified invoices are only allowed up to DKK 3,000 gross");
  });

  test("accepts simplified invoice when VAT is computable as 20 percent of gross", () => {
    const result = validateInvoice({
      invoiceType: "simplified",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0044",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      lines: [{ description: "Kontant salg" }],
      totals: { grossAmount: 1250, vatRate: 0.25, vatAmount: 250, vatComputationBasis: "VAT_20_OF_GROSS" },
      currency: "DKK",
    });

    expect(result.ok).toBe(true);
    expect(result.appliedRules).toContain("DK-INVOICE-SIMPLIFIED-001");
  });

  test("rejects a simplified standard-VAT invoice with an impossible vatAmount", () => {
    // gross 1.000, vatAmount 999 implies a net of 1 kr — a VAT of 999 on it is
    // impossible at 25%. Without the implied-net check this slipped through
    // (no totals.netAmount) and got booked verbatim as output VAT.
    const result = validateInvoice({
      invoiceType: "simplified",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0045",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      lines: [{ description: "Kontant salg" }],
      totals: { grossAmount: 1000, vatRate: 0.25, vatAmount: 999 },
      currency: "DKK",
    });

    expect(result.ok).toBe(false);
    expect(
      result.errors.some((e) => e.includes("VAT contained in")),
    ).toBe(true);
  });

  test("accepts a simplified standard-VAT invoice with a consistent explicit vatAmount", () => {
    // gross 1.000, VAT 200 = 20% of gross (the VAT contained at 25%). Consistent.
    const result = validateInvoice({
      invoiceType: "simplified",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0046",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      lines: [{ description: "Kontant salg" }],
      totals: { grossAmount: 1000, vatRate: 0.25, vatAmount: 200 },
      currency: "DKK",
    });

    expect(result.ok).toBe(true);
  });

  test("accepts a simplified invoice whose canonical 20%-of-gross VAT lands on an øre-rounding boundary", () => {
    // gross 100,07 incl. 25% VAT — the VAT contained is roundDkk(100,07 × 0,20)
    // = 20,01. A net-first round-trip (roundDkk((gross−vat) × 0,25)) double-
    // rounds to 20,02 and WRONGLY rejected this (and ~20% of all simplified
    // invoices). The gross-inclusive check accepts the correct figure.
    const result = validateInvoice({
      invoiceType: "simplified",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0047",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      lines: [{ description: "Kontant salg" }],
      totals: { grossAmount: 100.07, vatRate: 0.25, vatAmount: 20.01 },
      currency: "DKK",
    });

    expect(result.errors).toEqual([]);
    expect(result.ok).toBe(true);
  });

  test("rejects invoice with inconsistent line and gross totals", () => {
    const result = validateInvoice({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0045",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9, 8000 Aarhus C" },
      lines: [{ description: "Bogføring", quantity: 2, unitPriceExVat: 500, lineTotalExVat: 900 }],
      totals: { netAmount: 900, vatRate: 0.25, vatAmount: 225, grossAmount: 1200 },
      currency: "DKK",
    });

    expect(result.ok).toBe(false);
    expect(result.errors).toContain("lines[0].lineTotalExVat must equal quantity * unitPriceExVat (1000)");
    expect(result.errors).toContain("totals.grossAmount must equal totals.netAmount + totals.vatAmount (1125)");
    expect(result.appliedRules).toContain("DK-INVOICE-ARITHMETIC-001");
  });

  test("rejects due date before issue date", () => {
    const result = validateInvoice({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      dueDate: "2026-05-15",
      invoiceNumber: "2026-0046",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9, 8000 Aarhus C" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK",
    });

    expect(result.ok).toBe(false);
    expect(result.errors).toContain("dueDate cannot be earlier than issueDate");
  });

  test("rejects malformed delivery dates and delivery periods", () => {
    const result = validateInvoice({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0047",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9, 8000 Aarhus C" },
      lines: [{ description: "Maj 2026 drift", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      deliveryDate: "2026-02-30",
      deliveryPeriodStart: "2026-05-01",
      currency: "DKK",
    });

    expect(result.ok).toBe(false);
    expect(result.errors).toContain("deliveryDate must be YYYY-MM-DD when present");
    expect(result.errors).toContain("deliveryPeriodStart and deliveryPeriodEnd must be provided together");
    expect(result.errors).toContain("use either deliveryDate or deliveryPeriodStart/deliveryPeriodEnd, not both");
    expect(result.appliedRules).toContain("DK-INVOICE-DELIVERY-DATE-001");
  });

  test("rejects impossible issue and due dates even when they match the ISO shape", () => {
    const result = validateInvoice({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-02-30",
      dueDate: "2026-04-31",
      invoiceNumber: "2026-0047B",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde A/S", address: "Købervej 9, 8000 Aarhus C" },
      lines: [{ description: "Bogføring", quantity: 1, unitPriceExVat: 1000, lineTotalExVat: 1000 }],
      totals: { netAmount: 1000, vatRate: 0.25, vatAmount: 250, grossAmount: 1250 },
      currency: "DKK",
    });

    expect(result.ok).toBe(false);
    expect(result.errors).toContain("issueDate must be present in YYYY-MM-DD format");
    expect(result.errors).toContain("dueDate must be YYYY-MM-DD when present");
  });

  test("rejects reverse-charge invoice with invalid legal basis for direction", () => {
    const result = validateInvoice({
      invoiceType: "full",
      vatTreatment: "foreign_reverse_charge",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0048",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      buyer: { name: "EU Kunde GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      lines: [{ description: "AI consulting" }],
      totals: { netAmount: 8000, vatAmount: 2000, grossAmount: 8000 },
      reverseChargeBasis: "DK_MOMSLOVEN_§46_STK_1_NR_7",
      reverseChargeNote: "Byggemoms",
      currency: "DKK",
    });

    expect(result.ok).toBe(false);
    expect(result.errors).toContain("reverse-charge invoices must not include totals.vatAmount");
    expect(result.errors).toContain("reverseChargeBasis DK_MOMSLOVEN_§46_STK_1_NR_7 is not valid for foreign reverse-charge invoices");
    expect(result.appliedRules).toContain("DK-INVOICE-REVERSE-CHARGE-001");
    expect(result.appliedRules).toContain("DK-INVOICE-REVERSE-CHARGE-BASIS-001");
  });

  test("accepts reverse-charge invoice with explicit legal basis", () => {
    const result = validateInvoice({
      invoiceType: "full",
      vatTreatment: "foreign_reverse_charge",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0049",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      buyer: { name: "EU Kunde GmbH", address: "Berlin", vatOrCvr: "DE123456789" },
      lines: [{ description: "AI consulting", quantity: 1, unitPriceExVat: 8000, lineTotalExVat: 8000 }],
      totals: { netAmount: 8000, grossAmount: 8000 },
      reverseChargeBasis: "EU_MOMSDIREKTIV_ART_196",
      reverseChargeNote: "VAT reverse charge — VAT to be accounted by the recipient",
      deliveryPeriodStart: "2026-05-01",
      deliveryPeriodEnd: "2026-05-15",
      currency: "DKK",
    });

    expect(result.ok).toBe(true);
    expect(result.errors).toEqual([]);
    expect(result.appliedRules).toContain("DK-INVOICE-REVERSE-CHARGE-BASIS-001");
    expect(result.appliedRules).toContain("DK-INVOICE-DELIVERY-DATE-001");
  });

  test("accepts non-DKK invoices when deterministic DKK totals are supplied", () => {
    const result = validateInvoice({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0050",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde GmbH", address: "Berlin" },
      lines: [{ description: "Consulting", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100 }],
      totals: { netAmount: 100, vatRate: 0.25, vatAmount: 25, grossAmount: 125, fxRateToDkk: 7.46, netAmountDkk: 746, vatAmountDkk: 186.5, grossAmountDkk: 932.5 },
      currency: "EUR",
    });

    expect(result.ok).toBe(true);
    expect(result.errors).toEqual([]);
  });

  test("rejects non-DKK invoices without deterministic DKK conversion totals", () => {
    const result = validateInvoice({
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate: "2026-05-16",
      invoiceNumber: "2026-0051",
      seller: { name: "Rentemester ApS", address: "Testvej 1, 2100 København Ø", vatOrCvr: "DK12345678" },
      buyer: { name: "Kunde GmbH", address: "Berlin" },
      lines: [{ description: "Consulting", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100 }],
      totals: { netAmount: 100, vatRate: 0.25, vatAmount: 25, grossAmount: 125 },
      currency: "EUR",
    });

    expect(result.ok).toBe(false);
    expect(result.errors).toContain("non-DKK invoices must include totals.fxRateToDkk");
    expect(result.errors).toContain("non-DKK invoices must include totals.netAmountDkk");
    expect(result.errors).toContain("non-DKK invoices must include totals.grossAmountDkk");
  });
});
