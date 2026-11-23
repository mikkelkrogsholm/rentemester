import type { AccountRole } from "../account-roles";
import { normalizeCvr } from "../company";
import { removePathWithRetry } from "../fs-cleanup";
import { readDineroChart } from "./dinero";
import { resolveSource } from "./source";
import { DINERO_VAT_CONTROL_ACCOUNTS } from "../vat-account-semantics";

export type DineroVatRoleReview = {
  accountName: string;
  companyCvr: string;
  chartPath: string;
  chartSha256: string;
  inventorySha256: string;
};

/** Parse the actual export without running its ledger import. */
export function readDineroVatRoleReview(path: string, role: AccountRole, accountNo: string): DineroVatRoleReview {
  const source = resolveSource(path);
  try {
    const errors: string[] = [];
    const companyRows = source.files["Firmaoplysninger.csv"]?.text.split(/\r?\n/).filter((line) => line.trim());
    if (companyRows && companyRows.length !== 2)
      throw new Error("Dinero VAT review requires one company in Firmaoplysninger.csv");
    const chart = readDineroChart(source, errors);
    if (!chart || errors.length) throw new Error(errors.join("; "));
    if (!/^\d{4}\/Kontoplan\.csv$/i.test(chart.chartPath))
      throw new Error("Dinero VAT review requires a fiscal-year Kontoplan.csv");
    const control = DINERO_VAT_CONTROL_ACCOUNTS[accountNo as keyof typeof DINERO_VAT_CONTROL_ACCOUNTS];
    const matches = chart.accounts.filter((row) => row.accountNo === accountNo);
    const account = matches[0];
    // The shared reverse-charge role is used for services. The separate
    // Dinero goods control cannot substantiate that choice in a legacy chart.
    if (
      !control ||
      control.role !== role ||
      accountNo === "64020" ||
      matches.length !== 1 ||
      !account ||
      account.normalizedType !== "vat"
    ) {
      throw new Error(`Dinero chart does not identify account ${accountNo} as a unique ${role} control`);
    }
    const companyCvr = normalizeCvr(chart.companyMasterData?.cvr);
    if (!companyCvr) throw new Error("Dinero export requires a company CVR for account-role review");
    const chartEntry = source.sourceEvidence.entries.find((entry) => entry.path === chart.chartPath)!;
    return {
      accountName: account.name.trim(),
      companyCvr,
      chartPath: chart.chartPath,
      chartSha256: chartEntry.sha256,
      inventorySha256: source.sourceEvidence.canonicalInventorySha256,
    };
  } finally {
    if (source.sourceEvidence.sourceKind === "zip") removePathWithRetry(source.rootDir);
  }
}
