// Generate only synthetic XML for independent offline XSD/Schematron checks.
// Usage: bun run scripts/verify-oioubl-synthetic.ts /path/to/empty/output-dir
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildPublicEInvoiceOioUblXml } from "../src/core/public-einvoice";

const outputDir = process.argv[2];
if (!outputDir) throw new Error("Provide an output directory for synthetic XML");
mkdirSync(outputDir, { recursive: true });
const base = {
  invoiceType: "full" as const, issueDate: "2026-05-16", dueDate: "2026-06-16", currency: "DKK",
  seller: { name: "Synthetic Seller", address: "Synthetic Road 1", vatOrCvr: "DK12345678" },
  buyer: { name: "Synthetic Buyer", address: "Synthetic Road 2", eanNumber: "5790000000013", publicRecipient: true, buyerReference: "BUYER-ABC", orderReference: "ORDER-789" },
  lines: [{ description: "Synthetic work", quantity: 1, unitPriceExVat: 100, lineTotalExVat: 100 }],
  totals: { netAmount: 100, vatRate: 0.25, vatAmount: 25, grossAmount: 125 },
};
for (const version of ["2.02", "2.1"] as const) for (const exempt of [false, true]) for (const iban of [false, true]) {
  const payload = {
    ...base,
    payment: iban ? { iban: "DK6712345678901234", bic: "SYNFDKKK" } : { registrationNo: "1234", accountNo: "5678901234" },
    ...(exempt ? { totals: { netAmount: 100, vatRate: 0, vatAmount: 0, grossAmount: 100 } } : {}),
  };
  const name = `${version}-${exempt ? "exempt" : "vat"}-${iban ? "iban" : "bank"}.xml`;
  writeFileSync(join(outputDir, name), buildPublicEInvoiceOioUblXml("2026-0001", payload, undefined, version));
}
