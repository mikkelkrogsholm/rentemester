// Bun unit fixtures import bun:test; Playwright executes in Node. Generate
// checked-in JSON so both runners share the same synthetic, typed corpus.
import { assets } from "../src/test/fixtures/assets";
import { bank } from "../src/test/fixtures/bank";
import { budget, budgetDimensionActuals, budgetVsActual } from "../src/test/fixtures/budget";
import { cashflow } from "../src/test/fixtures/cashflow";
import { archive, companySettings, fiscalYears, multiYear, summary } from "../src/test/fixtures/companies";
import { contacts } from "../src/test/fixtures/contacts";
import { overview } from "../src/test/fixtures/dashboard";
import { documents } from "../src/test/fixtures/documents";
import { invoices } from "../src/test/fixtures/invoices";
import { mileage } from "../src/test/fixtures/mileage";
import { obligations } from "../src/test/fixtures/obligations";
import { payables } from "../src/test/fixtures/payables";
import { balance, incomeStatement, journal, trialBalance } from "../src/test/fixtures/statements";
import { vat } from "../src/test/fixtures/vat";
import { STATEMENT_COMPANY } from "../src/test/fixtures/_shared";

const output = JSON.stringify({ assets: assets(), bank: bank(), budget: budget(), budgetDimensionActuals: budgetDimensionActuals(), budgetVsActual: budgetVsActual(), cashflow: cashflow(), archive: archive(), companySettings: companySettings(), fiscalYears: fiscalYears(), multiYear: multiYear(), summary: summary(), contacts: contacts(), overview: overview(), documents: documents(), invoices: invoices(), mileage: mileage(), obligations: obligations(), payables: payables(), balance: balance(), incomeStatement: incomeStatement(), journal: journal(), trialBalance: trialBalance(), vat: vat(), company: STATEMENT_COMPANY }, null, 2) + "\n";
const path = new URL("../e2e/data/core.json", import.meta.url);
if (process.argv.includes("--check")) {
  if (!(await Bun.file(path).exists()) || await Bun.file(path).text() !== output) {
    throw new Error("Browser fixtures are stale. Run bun scripts/browser-fixtures.ts");
  }
} else await Bun.write(path, output);
