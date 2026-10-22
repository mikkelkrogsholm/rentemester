// Regenerate synthetic OIOUBL compatibility evidence from the original formatter.
// Run: bun crates/rentemester-engine/src/public_einvoice/generate-source-fixtures.ts
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { tmpdir } from "node:os";
const root = resolve(import.meta.dir, "../../../..");
const original = resolve(root, "src/core/public-einvoice.ts");
const temporary = mkdtempSync(resolve(tmpdir(), "oioubl-source-"));
try {
  const module = readFileSync(original, "utf8")
    .replace(/from "\.\/([^\"]+)"/g, (_, p) => `from ${JSON.stringify(resolve(dirname(original), p))}`)
    + '\nexport { buildPublicEInvoiceOioUblXml, validateOioUblPayload };\n';
  const path = resolve(temporary, "source.ts");
  writeFileSync(path, module);
  const { buildPublicEInvoiceOioUblXml, validateOioUblPayload } = await import(path);
  const base = { invoiceType: "full", status: "issued", invoiceNumber: "SYN-2026-1", vatTreatment: "standard", issueDate: "2026-10-20", dueDate: "2026-11-03", currency: "DKK", seller: { name: "Synthetic Seller", address: "Synthetic Street 1", vatOrCvr: "DK12345678" }, buyer: { name: "Synthetic Buyer", address: "Synthetic Street 2", eanNumber: "5790000000000", publicRecipient: true }, lines: [{ description: "Synthetic service", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100 }], totals: { netAmount: 100, vatRate: 0.25, vatAmount: 25, grossAmount: 125 } };
  const line = (taxClassification: string, vatRate?: number, reverseChargeBasis?: string) => ({ description: `Synthetic ${taxClassification}`, quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100, taxClassification, ...(vatRate === undefined ? {} : { vatRate }), ...(reverseChargeBasis ? { reverseChargeBasis } : {}) });
  const cases: any[] = [];
  const add = (name: string, modify: (p: any) => void) => {
    const input = structuredClone(base); modify(input);
    const errors = validateOioUblPayload(input.invoiceNumber, input, input.buyer.eanNumber);
    if (errors.length) throw new Error(`${name}: ${errors.join("; ")}`);
    cases.push({ name, input, xml: buildPublicEInvoiceOioUblXml(input.invoiceNumber, input) });
  };
  add("native standard DKK bytes", () => {});
  add("mixed classifications with first-seen grouped tax rates", p => { p.lines = [line("taxable", 0.25), line("exempt"), line("taxable", 0.125), line("reverse_charge", undefined, "EU_MOMSDIREKTIV_ART_196"), line("taxable", 0.25)]; p.totals = { netAmount: 500, vatRate: 0.25, vatAmount: 62.5, grossAmount: 562.5 }; });
  add("legacy uniform exempt zero rate", p => { p.totals = { netAmount: 100, vatRate: 0, vatAmount: 0, grossAmount: 100 }; });
  add("explicit exempt zero rate", p => { p.lines = [line("exempt")]; p.totals = { netAmount: 100, vatRate: 0, vatAmount: 0, grossAmount: 100 }; });
  for (const [treatment, basis] of [["domestic_reverse_charge", "DK_MOMSLOVEN_§46_STK_1_NR_6"], ["foreign_reverse_charge", "EU_MOMSDIREKTIV_ART_196"]]) {
    add(treatment, p => { p.vatTreatment = treatment; p.reverseChargeBasis = basis; p.buyer.vatOrCvr = "DE123456789"; p.totals = { netAmount: 100, grossAmount: 100 }; });
  }
  add("original EUR currency and delivery period evidence", p => { p.currency = " eur "; p.deliveryPeriodStart = "2026-10-01"; p.deliveryPeriodEnd = "2026-10-19"; p.totals = { ...p.totals, fxRateToDkk: 7.5, netAmountDkk: 750, vatAmountDkk: 187.5, grossAmountDkk: 937.5 }; p.buyer.orderReference = "PO & synthetic"; p.lines[0].unitCode = "HUR"; });
  add("alternate positive rate and delivery date evidence", p => { p.deliveryDate = "2026-10-19"; p.totals = { netAmount: 100, vatRate: 0.2, vatAmount: 20, grossAmount: 120 }; });
  add("original EUR export requires no DKK bookkeeping FX", p => { p.currency = "EUR"; });
  writeFileSync(resolve(import.meta.dir, "original_cases.json"), JSON.stringify(cases, null, 2) + "\n");
} finally { rmSync(temporary, { recursive: true }); }
